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
            KEY_BLANK,
            json!({"command":"object.layer.key.blank","layerUid":"layer-a","cell":3}),
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
            CLIPBOARD_COPY,
            json!({"command":"object.frames.clipboard.copy","selection":[]}),
        ),
        (
            CLIPBOARD_CUT,
            json!({"command":"object.frames.clipboard.cut","selection":[{"layerUid":"layer-a","frame":2}]}),
        ),
        (
            CLIPBOARD_PASTE,
            json!({"command":"object.frames.clipboard.paste","instanceId":"instance","documentId":"document",
            "layerUid":"layer-a","frame":3,"cells":[{"relLayer":0,"relFrame":0,"keyframe":true,"records":[],"orderElements":[]}]}),
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
fn clipboard_copy_cut_and_paste_are_declared_without_legacy_aliases() {
    assert_eq!(OPERATIONS.len(), 15);
    for (operation, payload) in examples().into_iter().filter(|(operation, _)| {
        [CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE].contains(operation)
    }) {
        assert!(declares(operation));
        assert!(envelope(operation, payload).validate_input().is_ok());
    }
    for operation in [
        "command.document.object.frames.copy",
        "command.document.object.frames.cut",
        "command.document.object.frames.paste",
    ] {
        assert!(!declares(operation));
        assert!(envelope(operation, json!({})).validate_input().is_err());
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

#[test]
fn actual_catalog_repeat_result_and_other_receipts_remain_operation_closed() {
    let catalog = crate::capabilities::native_catalog();
    let descriptor = catalog.capability_for_operation(REPEAT).unwrap();
    let examples = descriptor["examples"].as_array().unwrap();
    assert_eq!(examples.len(), OPERATIONS.len());
    for example in examples {
        let request: NativeApplicationRequest =
            serde_json::from_value(example["request"].clone()).unwrap();
        assert!(request.validate_input().is_ok());
        let receipt = &example["response"]["result"];
        assert!(crate::native_contract::validate_result(
            &request.operation,
            &request.document_id,
            &request.payload,
            receipt,
        ));
        assert!(revision_matches_for_operation(
            &request.operation,
            request.expected_revision,
            example["response"]["contentRevision"].as_u64().unwrap(),
            receipt,
        ));
        let compact = json!({"applied":true,"historyEntriesAdded":1});
        let repeat = json!({"applied":true,"historyEntriesAdded":1,"cycledLayers":1,"skippedComponentLayers":0});
        assert_eq!(
            result_for_operation(&request.operation, &compact),
            ![REPEAT, CLIPBOARD_COPY, CLIPBOARD_CUT].contains(&request.operation.as_str())
        );
        assert_eq!(
            result_for_operation(&request.operation, &repeat),
            request.operation == REPEAT
        );
    }
    assert!(!result_for_operation(
        "command.document.object.frames.clipboard.cut",
        &json!({"applied":false,"historyEntriesAdded":0})
    ));
}

#[test]
fn repeat_counts_are_required_closed_u32_without_inventing_cycle_to_applied_rules() {
    let valid =
        json!({"applied":true,"historyEntriesAdded":1,"cycledLayers":0,"skippedComponentLayers":2});
    assert!(
        result_for_operation(REPEAT, &valid),
        "global growth may cycle no drawing layer"
    );
    assert!(revision_matches_for_operation(REPEAT, Some(7), 8, &valid));
    assert!(!revision_matches_for_operation(REPEAT, Some(7), 7, &valid));
    assert!(!revision_matches_for_operation(REPEAT, None, 8, &valid));
    assert!(!revision_matches_for_operation(
        REPEAT,
        Some(u64::MAX),
        0,
        &valid
    ));
    let noop = json!({"applied":false,"historyEntriesAdded":0,"cycledLayers":1,"skippedComponentLayers":0});
    assert!(revision_matches_for_operation(REPEAT, Some(7), 7, &noop));
    assert!(!revision_matches_for_operation(REPEAT, Some(7), 8, &noop));
    for field in [
        "applied",
        "historyEntriesAdded",
        "cycledLayers",
        "skippedComponentLayers",
    ] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(!result_for_operation(REPEAT, &missing), "missing {field}");
    }
    for field in ["cycledLayers", "skippedComponentLayers"] {
        for bad in [
            json!(-1),
            json!(0.5),
            json!(4294967296u64),
            json!(null),
            json!(true),
            json!("1"),
        ] {
            let mut value = valid.clone();
            value[field] = bad;
            assert!(!result_for_operation(REPEAT, &value), "{field}: {value}");
        }
        let mut maximum = valid.clone();
        maximum[field] = json!(u32::MAX);
        assert!(result_for_operation(REPEAT, &maximum));
    }
    for (field, bad) in [
        ("applied", json!(1)),
        ("historyEntriesAdded", json!(0)),
        ("historyEntriesAdded", json!(2)),
    ] {
        let mut value = valid.clone();
        value[field] = bad;
        assert!(!result_for_operation(REPEAT, &value));
    }
    let mut extra = valid;
    extra["clipboard"] = json!(null);
    assert!(!result_for_operation(REPEAT, &extra));
}

#[test]
fn blank_key_uses_closed_opaque_active_layer_payload_and_public_byte_bound() {
    let payload = json!({"command":"object.layer.key.blank","layerUid":"data:opaque authored layer","cell":0});
    assert!(request(KEY_BLANK, &payload));
    assert!(envelope(KEY_BLANK, payload.clone())
        .validate_input()
        .is_ok());
    for cell in [
        json!(-1),
        json!(1.5),
        json!(4294967296u64),
        json!("0"),
        Value::Null,
    ] {
        let mut bad = payload.clone();
        bad["cell"] = cell;
        assert!(!request(KEY_BLANK, &bad));
    }
    let mut maximum = payload.clone();
    maximum["cell"] = json!(u32::MAX);
    assert!(request(KEY_BLANK, &maximum));
    for layer in [json!(""), json!(1), json!(["layer-a"]), Value::Null] {
        let mut bad = payload.clone();
        bad["layerUid"] = layer;
        assert!(!request(KEY_BLANK, &bad));
    }
    for field in ["layerUids", "cells", "targets", "blankOverride", "document"] {
        let mut bad = payload.clone();
        bad[field] = json!([]);
        assert!(envelope(KEY_BLANK, bad).validate_input().is_err());
    }
    let mut full = envelope(KEY_BLANK, payload);
    let padding =
        crate::contract::NATIVE_MAX_MESSAGE_BYTES - serde_json::to_vec(&full).unwrap().len();
    let layer = full.payload["layerUid"].as_str().unwrap().to_owned();
    full.payload["layerUid"] = json!(format!("{layer}{}", "x".repeat(padding)));
    assert_eq!(serde_json::to_vec(&full).unwrap().len(), 4096);
    assert!(full.validate_input().is_ok());
    full.payload["layerUid"] = json!(format!("{}x", full.payload["layerUid"].as_str().unwrap()));
    assert_eq!(serde_json::to_vec(&full).unwrap().len(), 4097);
    assert!(full.validate_input().is_err());
}

#[test]
fn blank_key_success_requires_one_checkpoint_and_revision_even_if_already_blank() {
    let changed = json!({"applied":true,"historyEntriesAdded":1});
    assert!(result_for_operation(KEY_BLANK, &changed));
    assert!(revision_matches_for_operation(
        KEY_BLANK,
        Some(7),
        8,
        &changed
    ));
    assert!(!revision_matches_for_operation(
        KEY_BLANK,
        Some(7),
        7,
        &changed
    ));
    assert!(!revision_matches_for_operation(
        KEY_BLANK, None, 8, &changed
    ));
    assert!(!revision_matches_for_operation(
        KEY_BLANK,
        Some(u64::MAX),
        0,
        &changed
    ));
    for bad in [
        json!({"applied":false,"historyEntriesAdded":0}),
        json!({"applied":true,"historyEntriesAdded":0}),
        json!({"applied":false,"historyEntriesAdded":1}),
        json!({"applied":true,"historyEntriesAdded":1,"cycledLayers":0,"skippedComponentLayers":0}),
        json!({"applied":true,"historyEntriesAdded":1,"document":{}}),
    ] {
        assert!(!result_for_operation(KEY_BLANK, &bad));
        assert!(!crate::native_contract::validate_result(
            KEY_BLANK,
            "document",
            &json!({}),
            &bad
        ));
        assert!(!revision_matches_for_operation(KEY_BLANK, Some(7), 7, &bad));
    }
    // Other compact operations retain their existing no-op semantics.
    assert!(revision_matches_for_operation(
        KEY_CLEAR,
        Some(7),
        7,
        &json!({"applied":false,"historyEntriesAdded":0})
    ));
}

#[cfg(test)]
#[path = "native_frame_clipboard_contract_tests.rs"]
mod clipboard;

#[test]
fn key_move_optional_active_layer_is_typed_and_operation_specific() {
    let base = json!({"command":"object.layer.key.move","layerUid":"target","from":0,"to":2});
    assert!(request(KEY_MOVE, &base));
    let mut with_active = base.clone();
    with_active["activeLayerUid"] = json!("different-active");
    assert!(request(KEY_MOVE, &with_active));
    assert!(envelope(KEY_MOVE, with_active.clone()).validate().is_ok());
    for invalid in [
        Value::Null,
        json!(""),
        json!(1),
        json!(false),
        json!([]),
        json!({}),
    ] {
        with_active["activeLayerUid"] = invalid;
        assert!(!request(KEY_MOVE, &with_active));
        assert!(envelope(KEY_MOVE, with_active.clone()).validate().is_err());
    }
    with_active["activeLayerUid"] = json!("x".repeat(257));
    assert!(request(KEY_MOVE, &with_active));
    assert!(envelope(KEY_MOVE, with_active).validate().is_ok());
    let mut other =
        json!({"command":"object.layer.tween.remove","layerUid":"target","from":0,"to":2});
    other["activeLayerUid"] = json!("different-active");
    assert!(!request(TWEEN_REMOVE, &other));
}
