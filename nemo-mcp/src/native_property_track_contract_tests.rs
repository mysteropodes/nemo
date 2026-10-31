//! Shape checks only; semantic duplicate stable keys remain native-owned refusals.
use super::*;
use crate::contract::NativeApplicationRequest;
use serde_json::json;
fn payload(action: Value) -> Value {
    json!({"command":"object.property.track.edit","selections":[{
        "target":{"contextId":"scene-root","layerUid":"authored layer:α / opaque"},
        "property":"opacity","frame":-1.25}],"action":action})
}
fn envelope(payload: Value) -> NativeApplicationRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":"track","instanceId":"instance","documentId":"document",
        "expectedRevision":7,"operation":OP,"payload":payload}),
    )
    .unwrap()
}
#[test]
fn track_actions_preserve_native_numeric_and_opaque_string_domains() {
    let mut actions = vec![json!({"kind":"subdivide"})];
    for preset in ["linear", "ease", "easeIn", "easeOut", "hold"] {
        actions.push(json!({"kind":"preset","preset":preset}));
    }
    for mode in [json!(null), json!("in"), json!("out"), json!("layer")] {
        actions.push(json!({"kind":"lockTo","mode":mode}));
    }
    for side in ["in", "out"] {
        for percent in [-1000.5, 0.0, 100.0, 1000.5] {
            actions.push(json!({"kind":"influence","side":side,"percent":percent}));
        }
    }
    for action in actions {
        let mut p = payload(action);
        p["selections"][0]["target"]["strokeId"] = json!("stroke:α / raw");
        assert!(request(&p));
        assert!(envelope(p).validate().is_ok());
    }
    let mut duplicate = payload(json!({"kind":"preset","preset":"hold"}));
    let s = duplicate["selections"][0].clone();
    duplicate["selections"] = json!([s.clone(), s]);
    assert!(
        envelope(duplicate).validate().is_ok(),
        "shape admission must not resolve native stable keys"
    );
    let descriptor = crate::capabilities::native_catalog()
        .capability_for_operation(OP)
        .unwrap();
    assert_eq!(descriptor["id"], "native.property-track-edit");
    assert_eq!(descriptor["authority"]["fallback"], "none");
}
#[test]
fn track_payload_selection_target_and_action_are_closed_at_every_level() {
    let valid = payload(json!({"kind":"preset","preset":"ease"}));
    for pointer in ["", "/selections/0", "/selections/0/target", "/action"] {
        let object = valid.pointer(pointer).unwrap().as_object().unwrap();
        for key in object.keys() {
            let mut bad = valid.clone();
            bad.pointer_mut(pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(!request(&bad), "missing {pointer}/{key}");
            let mut bad = valid.clone();
            bad.pointer_mut(pointer).unwrap()[key] = Value::Null;
            assert!(!request(&bad), "null {pointer}/{key}");
        }
        let mut bad = valid.clone();
        bad.pointer_mut(pointer).unwrap()["extra"] = json!(false);
        assert!(!request(&bad), "extra {pointer}");
    }
    for (pointer, value) in [
        ("/command", json!("object.property.edit")),
        ("/selections/0/frame", json!("1")),
        ("/selections/0/property", json!("")),
        ("/selections/0/target/contextId", json!("")),
        ("/selections/0/target/layerUid", json!("")),
        ("/selections/0/target/strokeId", json!(null)),
        ("/selections/0/target/strokeId", json!("")),
        ("/selections", json!([])),
        ("/selections", json!({})),
        ("/action", json!({"kind":"preset","preset":"elastic"})),
        ("/action", json!({"kind":"subdivide","preset":"linear"})),
        ("/action", json!({"kind":"lockTo"})),
        ("/action", json!({"kind":"lockTo","mode":"both"})),
        ("/action", json!({"kind":"lockTo","mode":""})),
        ("/action", json!({"kind":"lockTo","mode":true})),
        ("/action", json!({"kind":"lockTo","mode":["in"]})),
        ("/action", json!({"kind":"lockTo","mode":"in","extra":0})),
        (
            "/action",
            json!({"kind":"influence","side":"both","percent":50}),
        ),
        (
            "/action",
            json!({"kind":"influence","side":"in","percent":"50"}),
        ),
        (
            "/action",
            json!({"kind":"influence","side":"in","percent":null}),
        ),
        (
            "/action",
            json!({"kind":"influence","side":"in","percent":50,"extra":0}),
        ),
    ] {
        let mut bad = valid.clone();
        // strokeId is optional and absent in this source fixture.
        if let Some(v) = bad.pointer_mut(pointer) {
            *v = value;
        } else {
            bad["selections"][0]["target"]["strokeId"] = value;
        }
        assert!(!request(&bad), "{pointer}: {bad}");
        assert!(envelope(bad).validate_input().is_err());
    }
    let mut p = valid.clone();
    p["selections"] = json!(vec![valid["selections"][0].clone(); 32]);
    assert!(request(&p));
    p["selections"]
        .as_array_mut()
        .unwrap()
        .push(valid["selections"][0].clone());
    assert!(!request(&p));
    let mut missing = envelope(valid);
    missing.expected_revision = None;
    assert!(missing.validate_input().is_err());
}
#[test]
fn track_receipt_is_closed_and_exactly_correlates_one_or_zero_history_revisions() {
    let yes = json!({"applied":true,"historyEntriesAdded":1});
    let no = json!({"applied":false,"historyEntriesAdded":0});
    assert!(revision_matches(Some(7), 8, &yes));
    assert!(revision_matches(Some(7), 7, &no));
    assert!(revision_matches(
        Some(MAX_SAFE_REVISION),
        MAX_SAFE_REVISION,
        &no
    ));
    for (before, after, value) in [
        (Some(7), 7, yes.clone()),
        (Some(7), 8, no),
        (None, 8, yes.clone()),
        (Some(u64::MAX), 0, yes.clone()),
        (Some(MAX_SAFE_REVISION), MAX_SAFE_REVISION + 1, yes.clone()),
    ] {
        assert!(!revision_matches(before, after, &value));
    }
    for bad in [
        json!({"applied":true,"historyEntriesAdded":0}),
        json!({"applied":false,"historyEntriesAdded":1}),
        json!({"applied":true,"historyEntriesAdded":2}),
        json!({"applied":1,"historyEntriesAdded":1}),
        json!({"applied":true,"historyEntriesAdded":1,"privateOwner":{}}),
        json!({"applied":true}),
        json!([true, 1]),
    ] {
        assert!(!result(&bad));
    }
}
