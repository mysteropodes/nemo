use super::*;
use crate::contract::NativeApplicationRequest;
use serde_json::json;

fn examples() -> Vec<(&'static str, Value)> {
    vec![
        (INSERT, json!({"command":"object.frames.insert","cell":3})),
        (
            EXTEND,
            json!({"command":"object.frames.extend","cell":3,"count":2}),
        ),
        (
            DUPLICATE,
            json!({"command":"object.frames.duplicate","layerUid":"layer-a","cell":3}),
        ),
        (
            REMOVE,
            json!({"command":"object.frames.remove","from":2,"to":4}),
        ),
        (
            KEY_PROMOTE,
            json!({"command":"object.layer.key.promote","cells":[3,5]}),
        ),
        (
            KEY_CLEAR,
            json!({"command":"object.layer.key.clear","layerUid":"layer-a","cell":3}),
        ),
        (
            KEY_CLEAR_CELLS,
            json!({"command":"object.layer.key.clear_cells","targets":[{"layerUid":"layer-a","frame":3}]}),
        ),
        (
            TWEEN_REMOVE,
            json!({"command":"object.layer.tween.remove","layerUid":"layer-a","from":2,"to":6}),
        ),
        (
            KEY_MOVE,
            json!({"command":"object.layer.key.move","layerUid":"layer-a","from":2,"to":4}),
        ),
        (
            SELECTION_DUPLICATE,
            json!({"command":"object.frames.clipboard.duplicate","layerUid":"layer-a",
                "selection":[{"layerUid":"layer-a","frame":2}]}),
        ),
        (
            REPEAT,
            json!({"command":"object.frames.repeat","fromLayerUid":"layer-a","toLayerUid":"layer-b",
                "fromFrame":2,"toFrame":3,"times":2}),
        ),
    ]
}
fn envelope(operation: &str, payload: Value) -> NativeApplicationRequest {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":"frame-edit","instanceId":"instance",
        "documentId":"document","expectedRevision":7,"operation":operation,"payload":payload}),
    )
    .unwrap()
}

#[test]
fn every_declared_operation_accepts_its_literal_payload_and_is_registered() {
    assert_eq!(examples().len(), OPERATIONS.len());
    for (operation, payload) in examples() {
        assert!(declares(operation), "{operation}");
        assert!(request(operation, &payload), "{operation}");
        assert!(
            envelope(operation, payload).validate().is_ok(),
            "{operation}"
        );
        let descriptor = crate::capabilities::native_catalog()
            .capability_for_operation(operation)
            .unwrap();
        assert_eq!(descriptor["id"], "native.frame-edit");
        assert_eq!(descriptor["authority"]["fallback"], "none");
    }
}

#[test]
fn shapes_are_closed_and_commands_must_match_their_operation() {
    for (operation, payload) in examples() {
        let keys: Vec<String> = payload.as_object().unwrap().keys().cloned().collect();
        for key in keys {
            let mut missing = payload.clone();
            missing.as_object_mut().unwrap().remove(&key);
            assert!(!request(operation, &missing), "{operation} without {key}");
            let mut nulled = payload.clone();
            nulled[key.as_str()] = Value::Null;
            assert!(!request(operation, &nulled), "{operation} null {key}");
        }
        let mut extra = payload.clone();
        extra["extra"] = json!(1);
        assert!(!request(operation, &extra), "{operation} extra member");
        assert!(envelope(operation, extra).validate_input().is_err());
        for (other, other_payload) in examples() {
            if other != operation {
                assert!(
                    !request(operation, &other_payload),
                    "{operation} takes {other}'s payload"
                );
            }
        }
        assert!(!request(operation, &json!([operation])));
        let mut no_revision = envelope(operation, payload);
        no_revision.expected_revision = None;
        assert!(no_revision.validate_input().is_err(), "{operation}");
    }
    assert!(!request(
        "command.document.object.layer.policy.edit",
        &json!({})
    ));
}

#[test]
fn numbers_and_lists_follow_the_owner_parsers() {
    let bad_cells = [
        json!(-1),
        json!(1.5),
        json!(4294967296u64),
        json!("3"),
        json!(null),
    ];
    for cell in bad_cells {
        assert!(!request(
            INSERT,
            &json!({"command":"object.frames.insert","cell":cell})
        ));
    }
    assert!(request(
        INSERT,
        &json!({"command":"object.frames.insert","cell":4294967295u64})
    ));
    for (count, ok) in [(0, true), (999, true), (1000, false)] {
        let payload = json!({"command":"object.frames.extend","cell":0,"count":count});
        assert_eq!(request(EXTEND, &payload), ok, "count {count}");
    }
    for layers in [
        json!([]),
        json!(["a", "a"]),
        json!([""]),
        json!([1]),
        json!("a"),
    ] {
        let payload = json!({"command":"object.frames.insert","cell":1,"layerUids":layers});
        assert!(!request(INSERT, &payload));
    }
    assert!(request(
        INSERT,
        &json!({"command":"object.frames.insert","cell":1,"layerUids":["a","b"]})
    ));
    let promote = |cells: Value| json!({"command":"object.layer.key.promote","cells":cells});
    assert!(!request(KEY_PROMOTE, &promote(json!([]))));
    assert!(!request(KEY_PROMOTE, &promote(json!([2, 2]))));
    assert!(!request(KEY_PROMOTE, &promote(json!([-1]))));
    let long: Vec<u32> = (0..4097).collect();
    assert!(!request(KEY_PROMOTE, &promote(json!(long))));
    let full: Vec<u32> = (0..4096).collect();
    assert!(request(KEY_PROMOTE, &promote(json!(full))));
    let cells =
        |targets: Value| json!({"command":"object.layer.key.clear_cells","targets":targets});
    assert!(!request(KEY_CLEAR_CELLS, &cells(json!([]))));
    assert!(!request(
        KEY_CLEAR_CELLS,
        &cells(json!([{"layerUid":"a","frame":1},{"layerUid":"a","frame":1}]))
    ));
    assert!(request(
        KEY_CLEAR_CELLS,
        &cells(json!([{"layerUid":"a","frame":1},{"layerUid":"b","frame":1}]))
    ));
    assert!(!request(
        KEY_CLEAR_CELLS,
        &cells(json!([{"layerUid":"","frame":1}]))
    ));
    assert!(!request(
        KEY_CLEAR_CELLS,
        &cells(json!([{"layerUid":"a","frame":1,"x":0}]))
    ));
    let repeat = |times: Value| {
        json!({"command":"object.frames.repeat","fromLayerUid":"a","toLayerUid":"a",
            "fromFrame":0,"toFrame":1,"times":times})
    };
    for (times, ok) in [
        (json!(0), true),
        (json!(-3), true),
        (json!(9007199254740991i64), true),
        (json!(9007199254740992i64), false),
        (json!(1.5), false),
        (json!("2"), false),
    ] {
        assert_eq!(request(REPEAT, &repeat(times.clone())), ok, "times {times}");
    }
}

#[test]
fn a_data_prefixed_layer_uid_is_an_opaque_name() {
    let payload = json!({"command":"object.layer.key.clear","layerUid":"data:opaque authored layer","cell":0});
    assert!(request(KEY_CLEAR, &payload));
    assert!(envelope(KEY_CLEAR, payload).validate_input().is_ok());
}

#[test]
fn clipboard_cut_copy_and_paste_are_not_declared() {
    for operation in [
        "command.document.object.frames.clipboard.paste",
        "command.document.object.frames.clipboard.cut",
        "command.document.object.frames.clipboard.copy",
    ] {
        assert!(!declares(operation));
        let payload =
            json!({"command":"object.frames.clipboard.paste","layerUid":"a","frame":0,"cells":[]});
        assert!(envelope(operation, payload).validate_input().is_err());
    }
}

#[test]
fn results_correlate_applied_history_and_exact_revision() {
    let changed = json!({"applied":true,"historyEntriesAdded":1});
    let noop = json!({"applied":false,"historyEntriesAdded":0});
    assert!(revision_matches(Some(7), 8, &changed));
    assert!(revision_matches(Some(7), 7, &noop));
    assert!(!revision_matches(Some(7), 7, &changed));
    assert!(!revision_matches(Some(7), 8, &noop));
    assert!(!revision_matches(None, 8, &changed));
    assert!(!revision_matches(Some(u64::MAX), 0, &changed));
    for bad in [
        json!({"applied":true,"historyEntriesAdded":0}),
        json!({"applied":false,"historyEntriesAdded":1}),
        json!({"applied":true,"historyEntriesAdded":1,"document":{}}),
        json!({"applied":1,"historyEntriesAdded":1}),
    ] {
        assert!(!result(&bad));
    }
}
