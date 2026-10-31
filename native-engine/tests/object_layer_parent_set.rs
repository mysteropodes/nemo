//! #2372 U12: Parent A set / clear through the actual owner, history and dispatcher. The fixture is the L02-F
//! three-layer document (L0 position [0,0], L2 position [2,0]); expectations are hand-derived literals read back
//! through plain `serde_json::Value` projections.
use super::object_layer_lifecycle_tests::{base, bytes, dispatch, owner_of, request, send, value};
use super::*;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

fn parent(layer: &str, parent: Value) -> Value {
    json!({"command":"object.layer.parent.set","contextId":"scene-root","layerUid":layer,
        "parentLayerUid":parent,"frame":0})
}
fn position(document: &Value, layer: &str) -> Vec<f64> {
    document["propertyHolders"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["target"]["layerUid"] == layer && h["target"].get("strokeId").is_none())
        .and_then(|h| {
            h["properties"]
                .as_array()?
                .iter()
                .find(|p| p["property"] == "position")
        })
        .map(|p| {
            p["static"]
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_f64().unwrap())
                .collect()
        })
        .unwrap_or_default()
}
fn refused(owner: &mut NativeObjectHistory, id: &str, payload: &Value, code: &str) {
    let (before, depths, revision) = (
        bytes(owner),
        owner.history_depths(),
        owner.content_revision(),
    );
    let reply = send(owner, id, "parent.set", payload);
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
fn set_parent_keeps_the_world_pose_as_one_exact_undoable_entry() {
    for version in [2] {
        let mut owner = owner_of(&base(version));
        let before = bytes(&owner);
        let req = request(&owner, "p1", "parent.set", &parent("L0", json!("L2")));
        let reply = dispatch(&mut owner, &req);
        assert_eq!(
            reply["result"],
            json!({"applied":true,"historyEntriesAdded":1}),
            "v{version}: {reply}"
        );
        let after = value(&owner);
        assert_eq!(after["layers"][0]["parentLayerUid"], "L2");
        // world = own translation + parent translation: 0 + 0 before, -2 + 2 after
        assert_eq!(position(&after, "L0"), vec![-2.0, 0.0], "v{version}");
        assert_eq!(position(&after, "L2"), vec![2.0, 0.0]);
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (1, (1, 0))
        );
        let kept = bytes(&owner);
        assert_eq!(owner.undo(1).unwrap(), 2);
        assert_eq!(bytes(&owner), before, "v{version} undo");
        assert_eq!(owner.redo(2).unwrap(), 3);
        assert_eq!(bytes(&owner), kept, "v{version} redo");
        // clearing returns the child to its own pose: the shift is the exact inverse
        send(&mut owner, "p2", "parent.set", &parent("L0", Value::Null));
        let cleared = value(&owner);
        assert!(cleared["layers"][0]["parentLayerUid"].is_null());
        assert_eq!(position(&cleared, "L0"), vec![0.0, 0.0]);
        assert_eq!(owner.history_depths().0, 2);
    }
}

#[test]
fn set_parent_creates_the_child_position_holder_and_survives_save_and_reopen() {
    let mut document = base(2);
    document["propertyHolders"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    let mut owner = owner_of(&document);
    send(&mut owner, "p1", "parent.set", &parent("L0", json!("L2")));
    assert_eq!(position(&value(&owner), "L0"), vec![-2.0, 0.0]);
    let reopened =
        NativeObjectHistory::new("reopened", decode_storage_project(&bytes(&owner)).unwrap())
            .unwrap();
    assert_eq!(value(&reopened)["layers"][0]["parentLayerUid"], "L2");
    assert_eq!(bytes(&reopened), bytes(&owner));
}

#[test]
fn a_v1_document_has_no_parent_links_so_the_edit_is_refused_typed() {
    let mut owner = owner_of(&base(1));
    refused(
        &mut owner,
        "v1",
        &parent("L0", json!("L2")),
        "invalid_request",
    );
}

#[test]
fn an_unchanged_parent_publishes_nothing() {
    let mut owner = owner_of(&base(2));
    let reply = send(&mut owner, "p0", "parent.set", &parent("L0", Value::Null));
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0})
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (0, (0, 0))
    );
    send(&mut owner, "p1", "parent.set", &parent("L0", json!("L2")));
    let reply = send(&mut owner, "p2", "parent.set", &parent("L0", json!("L2")));
    assert_eq!(reply["result"]["applied"], false);
    assert_eq!(owner.history_depths(), (1, 0));
}

#[test]
fn self_cycle_unknown_context_and_payload_refusals_change_nothing() {
    let mut owner = owner_of(&base(2));
    refused(
        &mut owner,
        "self",
        &parent("L0", json!("L0")),
        "invalid_request",
    );
    refused(
        &mut owner,
        "ghost-parent",
        &parent("L0", json!("nope")),
        "not_found",
    );
    refused(
        &mut owner,
        "ghost-child",
        &parent("nope", json!("L0")),
        "not_found",
    );
    let mut elsewhere = parent("L0", json!("L1"));
    elsewhere["contextId"] = json!("unknown-context");
    refused(&mut owner, "context", &elsewhere, "invalid_request");
    let mut extra = parent("L0", json!("L1"));
    extra["extra"] = json!(1);
    refused(&mut owner, "extra", &extra, "invalid_request");
    refused(
        &mut owner,
        "empty",
        &parent("L0", json!("")),
        "invalid_request",
    );
    send(&mut owner, "ok", "parent.set", &parent("L1", json!("L0")));
    send(&mut owner, "ok2", "parent.set", &parent("L2", json!("L1")));
    // L0 under L2 would close L0 <- L1 <- L2 <- L0
    refused(
        &mut owner,
        "cycle",
        &parent("L0", json!("L2")),
        "invalid_request",
    );
}

#[test]
fn a_folder_cannot_be_a_parent_but_the_same_layer_unfoldered_can() {
    for folder in [true, false] {
        let mut document = base(2);
        document["layers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":"F","name":"F",
            "visible":true,"locked":false,"color":"#5fa875","parentLayerUid":null,
            "isFolderLayer":folder,"timing":super::object_layer_lifecycle_tests::timing()}));
        for context in document["compositionOrder"]["contexts"]
            .as_array_mut()
            .unwrap()
        {
            context["layers"]
                .as_array_mut()
                .unwrap()
                .push(json!({"layerUid":"F","naturalPosition":3,"order":{}}));
        }
        let mut owner = owner_of(&document);
        if folder {
            refused(
                &mut owner,
                "folder",
                &parent("L0", json!("F")),
                "invalid_request",
            );
        } else {
            let reply = send(&mut owner, "plain", "parent.set", &parent("L0", json!("F")));
            assert_eq!(reply["result"]["applied"], true, "{reply}");
        }
    }
}

#[test]
fn a_pose_that_cannot_be_kept_exactly_is_refused_not_jumped() {
    // a rotated ancestor needs its evaluated pivot
    let mut rotated = base(2);
    rotated["propertyHolders"][2]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"rotation","static":[30]}));
    let mut owner = owner_of(&rotated);
    refused(
        &mut owner,
        "rotated",
        &parent("L0", json!("L2")),
        "unavailable",
    );
    // a keyed child Position cannot absorb a static shift
    let mut keyed = base(2);
    keyed["propertyHolders"][0]["properties"] = json!([{"property":"position","track":{"keys":[
        {"frame":0,"v":[0,0]},{"frame":4,"v":[8,0]}]}}]);
    let mut owner = owner_of(&keyed);
    refused(
        &mut owner,
        "keyed",
        &parent("L0", json!("L2")),
        "unavailable",
    );
    // but a parent that adds no translation needs no shift, so the keyed child may be (un)parented
    let mut owner = owner_of(&keyed);
    let reply = send(
        &mut owner,
        "plain",
        "parent.set",
        &parent("L0", json!("L1")),
    );
    assert_eq!(reply["result"]["applied"], true);
}

#[test]
fn a_keyed_parent_is_sampled_at_the_playhead_frame() {
    let mut document = base(2);
    document["propertyHolders"][2]["properties"] = json!([{"property":"position","track":{"keys":[
        {"frame":0,"v":[0,0]},{"frame":4,"v":[8,0]}]}}]);
    let mut owner = owner_of(&document);
    let target: crate::object_document::PropertyHolderTarget =
        serde_json::from_value(json!({"contextId":"scene-root","layerUid":"L2"})).unwrap();
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let at_two = snapshot
        .sample_property_raw(&target, "position", 2.0)
        .unwrap()[0];
    assert!(at_two > 0.0 && at_two < 8.0, "{at_two}");
    let mut payload = parent("L0", json!("L2"));
    payload["frame"] = json!(2);
    send(&mut owner, "keyed", "parent.set", &payload);
    // the child sits where the parent's pose at frame 2 cancels: world = own + parent = 0 at that frame
    let own = position(&value(&owner), "L0")[0];
    assert!((own + at_two).abs() < 1e-9, "{own} vs {at_two}");
}
