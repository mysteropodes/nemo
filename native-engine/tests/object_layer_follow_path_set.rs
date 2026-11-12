//! #2372: Follow path (`layer.followPath.set`) through the actual owner, history and dispatcher, on the L02-F three-layer
//! document. Expectations are hand-derived literals read back through plain `serde_json::Value` projections; the motion
//! at two frames is read through the same snapshot sampler the effector layers use.
use super::object_layer_lifecycle_tests::{base, bytes, owner_of, send, value};
use super::*;
use crate::evaluation::object_component_kernels::object_duplicator_evaluate::snapshot::SnapshotMotion;
use crate::evaluation::object_component_kernels::object_effector_layers::MotionPort;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

fn fixture() -> Value {
    let mut document = base(2);
    document["layers"][0]["range"] = json!({"inPoint":4,"outPoint":10});
    document["layers"][2]["range"] = json!({"inPoint":6,"outPoint":20});
    document
}
fn follow(layer: &str, target: Value) -> Value {
    json!({"command":"object.layer.followPath.set","contextId":"scene-root","layerUid":layer,
        "targetLayerUid":target})
}
fn refused(owner: &mut NativeObjectHistory, id: &str, payload: &Value, code: &str) {
    let (before, depths, revision) = (
        bytes(owner),
        owner.history_depths(),
        owner.content_revision(),
    );
    let reply = send(owner, id, "followPath.set", payload);
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
fn follow_path_sets_clears_and_carries_align_as_one_exact_undoable_entry() {
    let mut owner = owner_of(&fixture());
    let before = bytes(&owner);
    let reply = send(
        &mut owner,
        "f1",
        "followPath.set",
        &follow("L0", json!("L2")),
    );
    assert_eq!(
        reply["result"],
        json!({"applied":true,"historyEntriesAdded":1}),
        "{reply}"
    );
    assert_eq!(
        value(&owner)["layers"][0]["followPath"],
        json!({"targetLayerUid":"L2","align":false})
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
    // the same target publishes nothing
    let reply = send(
        &mut owner,
        "f2",
        "followPath.set",
        &follow("L0", json!("L2")),
    );
    assert_eq!(reply["result"]["applied"], false);
    assert_eq!(owner.history_depths(), (1, 0));
    // Chemin : Aucun
    send(
        &mut owner,
        "f3",
        "followPath.set",
        &follow("L0", Value::Null),
    );
    assert!(value(&owner)["layers"][0]["followPath"].is_null());
    assert_eq!(owner.history_depths(), (2, 0));
    let reply = send(
        &mut owner,
        "f4",
        "followPath.set",
        &follow("L0", Value::Null),
    );
    assert_eq!(reply["result"]["applied"], false, "nothing to clear");
    assert_eq!(owner.history_depths(), (2, 0));
    // a stored `align` survives a retarget, like the baseline setter
    let mut document = fixture();
    document["layers"][0]["followPath"] = json!({"targetLayerUid":"L1","align":true});
    let mut aligned = owner_of(&document);
    send(
        &mut aligned,
        "f5",
        "followPath.set",
        &follow("L0", json!("L2")),
    );
    assert_eq!(
        value(&aligned)["layers"][0]["followPath"],
        json!({"targetLayerUid":"L2","align":true})
    );
    let reopened = NativeObjectHistory::new(
        "reopened",
        decode_storage_project(&bytes(&aligned)).unwrap(),
    )
    .unwrap();
    assert_eq!(bytes(&reopened), bytes(&aligned), "save and reopen");
}

fn follow_align(layer: &str, target: Value, align: Value) -> Value {
    let mut payload = follow(layer, target);
    payload["align"] = align;
    payload
}

#[test]
fn follow_path_align_is_authored_kept_dropped_and_undoable() {
    let mut owner = owner_of(&fixture());
    let path = |owner: &NativeObjectHistory| value(owner)["layers"][0]["followPath"].clone();
    // omitted keeps the stored align (false for a fresh follow)
    send(&mut owner, "a0", "followPath.set", &follow("L0", json!("L2")));
    assert_eq!(path(&owner), json!({"targetLayerUid":"L2","align":false}));
    // align:true on the same target is a real edit: one entry, one revision
    let (revision, depths) = (owner.content_revision(), owner.history_depths());
    let reply = send(&mut owner, "a1", "followPath.set", &follow_align("L0", json!("L2"), json!(true)));
    assert_eq!(reply["result"], json!({"applied":true,"historyEntriesAdded":1}), "{reply}");
    assert_eq!(path(&owner), json!({"targetLayerUid":"L2","align":true}));
    assert_eq!(owner.content_revision(), revision + 1);
    assert_eq!(owner.history_depths(), (depths.0 + 1, 0));
    // the same align again publishes nothing
    let kept = bytes(&owner);
    let reply = send(&mut owner, "a2", "followPath.set", &follow_align("L0", json!("L2"), json!(true)));
    assert_eq!(reply["result"]["applied"], false);
    assert_eq!(bytes(&owner), kept);
    // omitted align on the same target is a no-op and a retarget keeps the authored align
    let reply = send(&mut owner, "a3", "followPath.set", &follow("L0", json!("L2")));
    assert_eq!(reply["result"]["applied"], false);
    send(&mut owner, "a4", "followPath.set", &follow("L0", json!("L1")));
    assert_eq!(path(&owner), json!({"targetLayerUid":"L1","align":true}));
    // align:false is authored too
    send(&mut owner, "a5", "followPath.set", &follow_align("L0", json!("L1"), json!(false)));
    assert_eq!(path(&owner), json!({"targetLayerUid":"L1","align":false}));
    // one call may retarget and author at once
    send(&mut owner, "a6", "followPath.set", &follow_align("L0", json!("L2"), json!(true)));
    assert_eq!(path(&owner), json!({"targetLayerUid":"L2","align":true}));
    // undo restores the previous align, redo reapplies it
    let aligned = bytes(&owner);
    let at = owner.content_revision();
    assert_eq!(owner.undo(at).unwrap(), at + 1);
    assert_eq!(path(&owner), json!({"targetLayerUid":"L1","align":false}));
    owner.redo(at + 1).unwrap();
    assert_eq!(bytes(&owner), aligned, "redo");
    // a clear drops the align; an align beside a null target is ignored (nothing to clear: no edit)
    send(&mut owner, "a7", "followPath.set", &follow_align("L0", Value::Null, json!(true)));
    assert!(path(&owner).is_null());
    let reply = send(&mut owner, "a8", "followPath.set", &follow_align("L0", Value::Null, json!(false)));
    assert_eq!(reply["result"]["applied"], false);
    // a fresh follow after the clear starts unaligned unless authored
    send(&mut owner, "a9", "followPath.set", &follow("L0", json!("L2")));
    assert_eq!(path(&owner), json!({"targetLayerUid":"L2","align":false}));
    // a non-boolean align is invalid
    refused(&mut owner, "bad-align", &follow_align("L0", json!("L2"), json!("yes")), "invalid_request");
    // authoring on a fresh follow, then undo back to the untouched bytes
    let mut fresh = owner_of(&fixture());
    let untouched = bytes(&fresh);
    send(&mut fresh, "u0", "followPath.set", &follow_align("L0", json!("L2"), json!(true)));
    assert_eq!(path(&fresh), json!({"targetLayerUid":"L2","align":true}));
    let at = fresh.content_revision();
    fresh.undo(at).unwrap();
    assert_eq!(bytes(&fresh), untouched);
}

#[test]
fn follow_path_refusals_are_typed_and_a_target_must_carry_a_path() {
    let mut owner = owner_of(&fixture());
    refused(
        &mut owner,
        "self",
        &follow("L0", json!("L0")),
        "invalid_request",
    );
    refused(
        &mut owner,
        "ghost-target",
        &follow("L0", json!("nope")),
        "not_found",
    );
    refused(
        &mut owner,
        "ghost-layer",
        &follow("nope", json!("L0")),
        "not_found",
    );
    let mut foreign = follow("L0", json!("L1"));
    foreign["contextId"] = json!("unused-symbol");
    refused(&mut owner, "context", &foreign, "invalid_request");
    let mut extra = follow("L0", json!("L1"));
    extra["extra"] = json!(1);
    refused(&mut owner, "extra", &extra, "invalid_request");
    refused(
        &mut owner,
        "empty",
        &follow("L0", json!("")),
        "invalid_request",
    );
    for (flag, plain) in [
        ("isFolderLayer", true),
        ("isNullLayer", true),
        ("isEffectLayer", true),
    ] {
        let mut document = fixture();
        let mut extra_layer = json!({"layerUid":"F","name":"F","visible":true,"locked":false,"color":"#5fa875",
            "parentLayerUid":null,"timing":super::object_layer_lifecycle_tests::timing()});
        extra_layer[flag] = json!(plain);
        document["layers"].as_array_mut().unwrap().push(extra_layer);
        for context in document["compositionOrder"]["contexts"]
            .as_array_mut()
            .unwrap()
        {
            context["layers"]
                .as_array_mut()
                .unwrap()
                .push(json!({"layerUid":"F","naturalPosition":3,"order":{}}));
        }
        let decoded = decode_storage_project(document.to_string().as_bytes());
        let mut guarded =
            NativeObjectHistory::new("guard", decoded.unwrap_or_else(|e| panic!("{flag}: {e:?}")))
                .unwrap();
        refused(
            &mut guarded,
            flag,
            &follow("L0", json!("F")),
            "invalid_request",
        );
    }
}

#[test]
fn a_follow_path_layer_moves_along_the_guide_at_two_frames() {
    let mut document = base(2); // no explicit ranges: both layers show at both frames
                                // pathPercent keyed 0% at frame 0 and 50% at frame 12 on the follower
    document["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"pathPercent",
        "track":{"keys":[{"frame":0,"v":[0]},{"frame":12,"v":[50]}]}}));
    let mut owner = owner_of(&document);
    let motion = |owner: &NativeObjectHistory, frame: f64| {
        let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
        SnapshotMotion::new(&snapshot, "scene-root", frame)
            .motion_delta("L0")
            .unwrap()
            .map(|d| [d.dx.operand(), d.dy.operand()])
    };
    assert_eq!(motion(&owner, 0.0), None, "no follow path, no contribution");
    send(
        &mut owner,
        "f1",
        "followPath.set",
        &follow("L0", json!("L2")),
    );
    // the guide L2 is the closed segment 0..10 (length 20) at x + 2; the follower's own centre is x = 5
    let (a, b) = (motion(&owner, 0.0).unwrap(), motion(&owner, 12.0).unwrap());
    assert!((a[0] + 3.0).abs() < 1e-9 && a[1].abs() < 1e-9, "{a:?}");
    assert!((b[0] - 7.0).abs() < 1e-9 && b[1].abs() < 1e-9, "{b:?}");
    send(
        &mut owner,
        "f2",
        "followPath.set",
        &follow("L0", Value::Null),
    );
    assert_eq!(
        motion(&owner, 12.0),
        None,
        "clearing restores the plain pose"
    );
}

#[test]
fn a_v1_document_stores_no_layer_links_so_the_edit_is_refused_typed() {
    let mut owner = owner_of(&base(1));
    refused(
        &mut owner,
        "v1",
        &follow("L0", json!("L2")),
        "invalid_request",
    );
}
