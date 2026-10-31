//! #2372: Link time (`layer.timeLink.set`) and Follow path (`layer.followPath.set`) through the actual owner, history and
//! dispatcher. The fixture is the L02-F three-layer document with explicit ranges L0 = [4,10] and L2 = [6,20]; expectations
//! are hand-derived literals read back through plain `serde_json::Value` projections, and the effective range is read
//! with the same immutable range evaluator the layer bars use.
use super::object_layer_lifecycle_tests::{base, bytes, dispatch, owner_of, request, send, value};
use super::*;
use crate::object_codec::decode_storage_project;
use crate::object_document::RangeEvaluator;
use serde_json::{json, Value};

fn fixture() -> Value {
    let mut document = base(2);
    document["layers"][0]["range"] = json!({"inPoint":4,"outPoint":10});
    document["layers"][2]["range"] = json!({"inPoint":6,"outPoint":20});
    document
}
fn link(layer: &str, source: Value) -> Value {
    json!({"command":"object.layer.timeLink.set","contextId":"scene-root","layerUid":layer,
        "sourceLayerUid":source,"frame":0})
}
fn edges(owner: &NativeObjectHistory, uid: &str) -> (f64, f64) {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let range = RangeEvaluator::for_document(snapshot.document(), "scene-root", 0)
        .unwrap()
        .resolve(uid, true)
        .unwrap();
    (range.in_point, range.out_point)
}
fn offset(document: &Value, layer: &str, property: &str) -> Option<f64> {
    document["propertyHolders"]
        .as_array()?
        .iter()
        .find(|h| h["target"]["layerUid"] == layer && h["target"].get("strokeId").is_none())?
        ["properties"]
        .as_array()?
        .iter()
        .find(|p| p["property"] == property)?["static"][0]
        .as_f64()
}
fn refused(owner: &mut NativeObjectHistory, op: &str, id: &str, payload: &Value, code: &str) {
    let (before, depths, revision) = (
        bytes(owner),
        owner.history_depths(),
        owner.content_revision(),
    );
    let reply = send(owner, id, op, payload);
    assert_eq!(reply["ok"], false, "{id}: {reply}");
    assert_eq!(reply["error"]["code"], code, "{id}: {reply}");
    assert_eq!(bytes(owner), before, "{id} bytes");
    assert_eq!(
        (owner.history_depths(), owner.content_revision()),
        (depths, revision),
        "{id}"
    );
}

#[test]
fn link_time_seeds_the_offsets_so_nothing_moves_as_one_exact_undoable_entry() {
    let mut owner = owner_of(&fixture());
    let before = bytes(&owner);
    assert_eq!(edges(&owner, "L0"), (4.0, 10.0));
    let req = request(&owner, "t1", "timeLink.set", &link("L0", json!("L2")));
    let reply = dispatch(&mut owner, &req);
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1}),
        "{reply}"
    );
    let after = value(&owner);
    assert_eq!(
        after["layers"][0]["range"]["timeLink"],
        json!({"uid":"L2","mode":"both"})
    );
    // seeded from the current gap: 4 - 6 and 10 - 20
    assert_eq!(offset(&after, "L0", "timeLinkInOffset"), Some(-2.0));
    assert_eq!(offset(&after, "L0", "timeLinkOutOffset"), Some(-10.0));
    assert_eq!(
        after["layers"][0]["range"]["inPoint"], 4,
        "the hard edges are kept"
    );
    assert_eq!(
        edges(&owner, "L0"),
        (4.0, 10.0),
        "linking never makes the layer jump"
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
    let kept = bytes(&owner);
    assert_eq!(owner.undo(1).unwrap(), 2);
    assert_eq!(bytes(&owner), before, "undo");
    assert_eq!(owner.redo(2).unwrap(), 3);
    assert_eq!(bytes(&owner), kept, "redo");
}

#[test]
fn the_linked_layer_follows_the_leader_and_survives_save_and_reopen() {
    let mut owner = owner_of(&fixture());
    send(&mut owner, "t1", "timeLink.set", &link("L0", json!("L2")));
    let mut moved = value(&owner);
    moved["layers"][2]["range"] = json!({"inPoint":8,"outPoint":22});
    let mut reopened = NativeObjectHistory::new(
        "reopened",
        decode_storage_project(moved.to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(
        edges(&reopened, "L0"),
        (6.0, 12.0),
        "the leader's new range plus the seeded offsets"
    );
    // a plain save and reopen keeps the link, mode and offsets byte for byte
    let saved =
        NativeObjectHistory::new("again", decode_storage_project(&bytes(&owner)).unwrap()).unwrap();
    assert_eq!(bytes(&saved), bytes(&owner));
    assert_eq!(value(&saved)["layers"][0]["range"]["timeLink"]["uid"], "L2");
    // unlinking bakes what the link resolved to, whatever the leader does afterwards
    let reply = send(
        &mut reopened,
        "u1",
        "timeLink.set",
        &link("L0", Value::Null),
    );
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1}),
        "{reply}"
    );
    let unlinked = value(&reopened);
    assert!(unlinked["layers"][0]["range"].get("timeLink").is_none());
    assert_eq!(unlinked["layers"][0]["range"]["inPoint"], 6);
    assert_eq!(unlinked["layers"][0]["range"]["outPoint"], 12);
    assert_eq!(
        edges(&reopened, "L0"),
        (6.0, 12.0),
        "Time: None keeps the position"
    );
    assert_eq!(reopened.history_depths(), (1, 0));
    assert_eq!(reopened.undo(1).unwrap(), 2);
    assert_eq!(
        edges(&reopened, "L0"),
        (6.0, 12.0),
        "undo restores the link, which resolves to the same edges"
    );
    assert_eq!(
        value(&reopened)["layers"][0]["range"]["timeLink"]["uid"],
        "L2"
    );
}

#[test]
fn a_single_driven_edge_can_read_the_other_edge_of_its_source() {
    let mut owner = owner_of(&fixture());
    let mut payload = link("L0", json!("L2"));
    payload["mode"] = json!("in");
    payload["srcAnchor"] = json!("out");
    send(&mut owner, "t1", "timeLink.set", &payload);
    let after = value(&owner);
    assert_eq!(
        after["layers"][0]["range"]["timeLink"],
        json!({"uid":"L2","mode":"in","srcAnchor":"out"})
    );
    assert_eq!(
        offset(&after, "L0", "timeLinkInOffset"),
        Some(-16.0),
        "4 - the source's out edge 20"
    );
    assert_eq!(edges(&owner, "L0"), (4.0, 10.0));
    // the same link again changes nothing
    let reply = send(&mut owner, "t2", "timeLink.set", &payload);
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(owner.history_depths(), (1, 0));
    // a whole-layer link ignores an anchor, like the baseline
    let mut whole = link("L1", json!("L2"));
    whole["srcAnchor"] = json!("out");
    send(&mut owner, "t3", "timeLink.set", &whole);
    assert_eq!(
        value(&owner)["layers"][1]["range"]["timeLink"],
        json!({"uid":"L2","mode":"both"})
    );
}

#[test]
fn unlinking_an_unlinked_layer_publishes_nothing() {
    let mut owner = owner_of(&fixture());
    let reply = send(&mut owner, "u0", "timeLink.set", &link("L0", Value::Null));
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
}

#[test]
fn time_link_refusals_are_typed_and_change_nothing() {
    let mut owner = owner_of(&fixture());
    refused(
        &mut owner,
        "timeLink.set",
        "self",
        &link("L0", json!("L0")),
        "invalid_request",
    );
    refused(
        &mut owner,
        "timeLink.set",
        "ghost-source",
        &link("L0", json!("nope")),
        "not_found",
    );
    refused(
        &mut owner,
        "timeLink.set",
        "ghost-layer",
        &link("nope", json!("L0")),
        "not_found",
    );
    for context in ["unused-symbol", "elsewhere"] {
        let mut foreign = link("L0", json!("L1"));
        foreign["contextId"] = json!(context);
        refused(
            &mut owner,
            "timeLink.set",
            context,
            &foreign,
            "invalid_request",
        );
    }
    let mut mode = link("L0", json!("L1"));
    mode["mode"] = json!("sideways");
    refused(&mut owner, "timeLink.set", "mode", &mode, "invalid_request");
    let mut anchor = link("L0", json!("L1"));
    anchor["srcAnchor"] = json!("middle");
    refused(
        &mut owner,
        "timeLink.set",
        "anchor",
        &anchor,
        "invalid_request",
    );
    let mut orphan = link("L0", Value::Null);
    orphan["mode"] = json!("in");
    refused(
        &mut owner,
        "timeLink.set",
        "orphan-mode",
        &orphan,
        "invalid_request",
    );
    let mut extra = link("L0", json!("L1"));
    extra["extra"] = json!(1);
    refused(
        &mut owner,
        "timeLink.set",
        "extra",
        &extra,
        "invalid_request",
    );
    refused(
        &mut owner,
        "timeLink.set",
        "empty",
        &link("L0", json!("")),
        "invalid_request",
    );
    let mut late = link("L0", json!("L1"));
    late["frame"] = json!(24);
    refused(
        &mut owner,
        "timeLink.set",
        "frame",
        &late,
        "invalid_request",
    );
    // L0 <- L1 <- L2: linking L2 to... the chain below closes a loop when L0 follows L2
    send(&mut owner, "ok1", "timeLink.set", &link("L2", json!("L1")));
    send(&mut owner, "ok2", "timeLink.set", &link("L1", json!("L0")));
    refused(
        &mut owner,
        "timeLink.set",
        "cycle",
        &link("L0", json!("L2")),
        "invalid_request",
    );
    refused(
        &mut owner,
        "timeLink.set",
        "cycle2",
        &link("L0", json!("L1")),
        "invalid_request",
    );
}

#[test]
fn a_v1_document_stores_no_layer_links_so_the_edit_is_refused_typed() {
    let mut owner = owner_of(&base(1));
    refused(
        &mut owner,
        "timeLink.set",
        "v1-link",
        &link("L0", json!("L2")),
        "invalid_request",
    );
}

#[test]
fn a_range_that_cannot_be_read_exactly_is_refused_unavailable() {
    let mut document = fixture();
    // an enabled expression on an existing offset: the evaluator will not substitute a raw value
    document["layers"][1]["range"] = json!({"timeLink":{"uid":"L2","mode":"both"}});
    document["propertyHolders"].as_array_mut().unwrap().push(json!({
        "target":{"contextId":"scene-root","layerUid":"L1"},
        "properties":[{"property":"timeLinkInOffset","static":[0],"expression":{"enabled":true,"code":"1"}}]}));
    let mut owner = owner_of(&document);
    refused(
        &mut owner,
        "timeLink.set",
        "expr",
        &link("L1", Value::Null),
        "unavailable",
    );
    refused(
        &mut owner,
        "timeLink.set",
        "expr2",
        &link("L1", json!("L0")),
        "unavailable",
    );
}
