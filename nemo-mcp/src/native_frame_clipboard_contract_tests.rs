// Public clipboard shell controls. Blank cells are real empty key payloads; objects below test
// container admission only. Native storage admission, not this shell, validates record/order DTOs.
use super::*;

fn packet() -> Value {
    json!({"instanceId":"instance","documentId":"document","cells":[
        {"relLayer":0,"relFrame":0,"keyframe":true,"records":[],"orderElements":[]},
        {"relLayer":1,"relFrame":2,"keyframe":false,"records":[]}]})
}
fn selection(operation: &str, cells: Value) -> Value {
    let command = if operation == CLIPBOARD_COPY {
        "object.frames.clipboard.copy"
    } else {
        "object.frames.clipboard.cut"
    };
    json!({"command":command,"selection":cells})
}
fn paste(cells: Value) -> Value {
    json!({"command":"object.frames.clipboard.paste","instanceId":"instance","documentId":"document",
        "layerUid":"data:opaque authored target","frame":3,"cells":cells})
}
fn clipboard_result(applied: bool, clipboard: Value) -> Value {
    json!({"applied":applied,"historyEntriesAdded":if applied {1} else {0},"clipboard":clipboard})
}

#[test]
fn clipboard_selection_is_closed_unique_bounded_and_allows_empty_selection() {
    for operation in [CLIPBOARD_COPY, CLIPBOARD_CUT] {
        for cells in [
            json!([]),
            json!([{"layerUid":"data:opaque","frame":u32::MAX}]),
        ] {
            assert!(request(operation, &selection(operation, cells.clone())));
            assert!(envelope(operation, selection(operation, cells))
                .validate_input()
                .is_ok());
        }
        for cells in [
            Value::Null,
            json!({}),
            json!([null]),
            json!([[]]),
            json!([{"layerUid":"","frame":0}]),
            json!([{"layerUid":"a","frame":-1}]),
            json!([{"layerUid":"a","frame":0.5}]),
            json!([{"layerUid":"a","frame":4294967296u64}]),
            json!([{"layerUid":"a","frame":0,"extra":true}]),
            json!([{"layerUid":"a","frame":0},{"layerUid":"a","frame":0}]),
        ] {
            assert!(
                !request(operation, &selection(operation, cells.clone())),
                "{operation}: {cells}"
            );
        }
        let full: Vec<Value> = (0..4096)
            .map(|frame| json!({"layerUid":"a","frame":frame}))
            .collect();
        assert!(request(operation, &selection(operation, json!(full))));
        let over: Vec<Value> = (0..4097)
            .map(|frame| json!({"layerUid":"a","frame":frame}))
            .collect();
        assert!(!request(operation, &selection(operation, json!(over))));
        // Structural list bound does not grant a larger complete transport envelope.
        let large = (0..4096)
            .map(|frame| json!({"layerUid":"a","frame":frame}))
            .collect::<Vec<_>>();
        assert!(envelope(operation, selection(operation, json!(large)))
            .validate_input()
            .is_err());
    }
}

#[test]
fn clipboard_paste_accepts_blank_and_plain_empty_cells_with_closed_order_shells_only() {
    assert!(envelope(CLIPBOARD_PASTE, paste(json!([])))
        .validate_input()
        .is_ok());
    assert!(envelope(CLIPBOARD_PASTE, paste(packet()["cells"].clone()))
        .validate_input()
        .is_ok());
    let shell =
        json!([{"relLayer":0,"relFrame":0,"keyframe":true,"records":[{}],"orderElements":[{}]}]);
    assert!(
        request(CLIPBOARD_PASTE, &paste(shell)),
        "structural objects only, not a claim of native DTO validity"
    );
    let cell = packet()["cells"][0].clone();
    for field in ["relLayer", "relFrame", "keyframe", "records"] {
        let mut missing = cell.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            !request(CLIPBOARD_PASTE, &paste(json!([missing]))),
            "missing {field}"
        );
    }
    for (field, value) in [
        ("relLayer", json!(-1)),
        ("relFrame", json!(4294967296u64)),
        ("relFrame", json!(0.5)),
        ("keyframe", json!(0)),
        ("records", Value::Null),
        ("records", json!([null])),
        ("records", json!([[]])),
        ("orderElements", Value::Null),
        ("orderElements", json!([null])),
        ("orderElements", json!([[]])),
        ("extra", json!(true)),
    ] {
        let mut bad = cell.clone();
        bad[field] = value;
        assert!(
            !request(CLIPBOARD_PASTE, &paste(json!([bad]))),
            "cell {field}"
        );
    }
    for field in ["instanceId", "documentId", "layerUid", "frame", "cells"] {
        let mut bad = paste(json!([]));
        bad[field] = Value::Null;
        assert!(!request(CLIPBOARD_PASTE, &bad), "paste {field}");
    }
    for field in ["records", "orderElements"] {
        let mut at_limit = cell.clone();
        at_limit[field] = json!(vec![json!({}); 4096]);
        assert!(request(CLIPBOARD_PASTE, &paste(json!([at_limit]))));
        let mut over = cell.clone();
        over[field] = json!(vec![json!({}); 4097]);
        assert!(!request(CLIPBOARD_PASTE, &paste(json!([over]))));
    }
    let full = (0..4096)
        .map(|frame| json!({"relLayer":0,"relFrame":frame,"keyframe":false,"records":[]}))
        .collect::<Vec<_>>();
    assert!(request(CLIPBOARD_PASTE, &paste(json!(full))));
    let over = (0..4097)
        .map(|frame| json!({"relLayer":0,"relFrame":frame,"keyframe":false,"records":[]}))
        .collect::<Vec<_>>();
    assert!(!request(CLIPBOARD_PASTE, &paste(json!(over))));
}

#[test]
fn clipboard_results_correlate_copy_cut_history_revision_and_operation_shape() {
    for clipboard in [Value::Null, packet()] {
        let copy = clipboard_result(false, clipboard.clone());
        assert!(result_for_operation(CLIPBOARD_COPY, &copy));
        assert!(revision_matches_for_operation(
            CLIPBOARD_COPY,
            Some(7),
            7,
            &copy
        ));
        assert!(!revision_matches_for_operation(
            CLIPBOARD_COPY,
            Some(7),
            8,
            &copy
        ));
        assert!(!result_for_operation(
            CLIPBOARD_COPY,
            &clipboard_result(true, clipboard.clone())
        ));
        for applied in if clipboard.is_null() {
            vec![false]
        } else {
            vec![true, false]
        } {
            let cut = clipboard_result(applied, clipboard.clone());
            assert!(result_for_operation(CLIPBOARD_CUT, &cut));
            assert!(revision_matches_for_operation(
                CLIPBOARD_CUT,
                Some(7),
                7 + u64::from(applied),
                &cut
            ));
            assert!(!revision_matches_for_operation(
                CLIPBOARD_CUT,
                Some(7),
                8 - u64::from(applied),
                &cut
            ));
        }
        for operation in [CLIPBOARD_COPY, CLIPBOARD_CUT] {
            assert!(!revision_matches_for_operation(operation, None, 7, &copy));
            let mut wrong_history = copy.clone();
            wrong_history["historyEntriesAdded"] = json!(1);
            assert!(!result_for_operation(operation, &wrong_history));
            let mut extra = copy.clone();
            extra["cycledLayers"] = json!(0);
            assert!(!result_for_operation(operation, &extra));
            let mut missing = copy.clone();
            missing.as_object_mut().unwrap().remove("clipboard");
            assert!(!result_for_operation(operation, &missing));
        }
    }
    assert!(
        !result_for_operation(CLIPBOARD_CUT, &clipboard_result(true, Value::Null)),
        "changed Cut must retain its nonempty-selection packet"
    );
    let changed = json!({"applied":true,"historyEntriesAdded":1});
    let repeat =
        json!({"applied":true,"historyEntriesAdded":1,"cycledLayers":0,"skippedComponentLayers":1});
    for operation in [CLIPBOARD_COPY, CLIPBOARD_CUT] {
        assert!(!result_for_operation(operation, &changed));
        assert!(!result_for_operation(operation, &repeat));
    }
    assert!(result_for_operation(CLIPBOARD_PASTE, &changed));
    assert!(!result_for_operation(
        CLIPBOARD_PASTE,
        &clipboard_result(true, packet())
    ));
    assert!(!result_for_operation(
        KEY_BLANK,
        &clipboard_result(true, packet())
    ));
    assert!(revision_matches_for_operation(
        CLIPBOARD_PASTE,
        Some(7),
        7,
        &json!({"applied":false,"historyEntriesAdded":0})
    ));
    assert!(!revision_matches_for_operation(
        CLIPBOARD_CUT,
        Some(u64::MAX),
        0,
        &clipboard_result(true, Value::Null)
    ));
}

#[test]
fn clipboard_inner_identity_and_packet_structure_cannot_be_forged() {
    for operation in [CLIPBOARD_COPY, CLIPBOARD_CUT] {
        let good = clipboard_result(false, packet());
        assert!(clipboard_identity_matches(
            operation, "instance", "document", &good
        ));
        for field in ["instanceId", "documentId"] {
            let mut wrong = good.clone();
            wrong["clipboard"][field] = json!("foreign");
            assert!(!clipboard_identity_matches(
                operation, "instance", "document", &wrong
            ));
        }
        assert!(clipboard_identity_matches(
            operation,
            "instance",
            "document",
            &clipboard_result(false, Value::Null)
        ));
        for malformed in [
            json!({}),
            json!([]),
            json!({"instanceId":"instance","documentId":"document","cells":null}),
            json!({"instanceId":"instance","documentId":"document","cells":[],"extra":0}),
        ] {
            assert!(!result_for_operation(
                operation,
                &clipboard_result(false, malformed)
            ));
        }
    }
    assert!(clipboard_identity_matches(
        KEY_CLEAR,
        "instance",
        "document",
        &json!({"applied":true,"historyEntriesAdded":1})
    ));
}

#[test]
fn clipboard_complete_request_bound_is_4096_not_the_typed_packet_budget() {
    for operation in [CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE] {
        let payload = if operation == CLIPBOARD_PASTE {
            paste(json!([]))
        } else {
            selection(operation, json!([{"layerUid":"a","frame":0}]))
        };
        let mut value = envelope(operation, payload);
        let size = serde_json::to_vec(&value).unwrap().len();
        let key = if operation == CLIPBOARD_PASTE {
            &mut value.payload["layerUid"]
        } else {
            &mut value.payload["selection"][0]["layerUid"]
        };
        let original = key.as_str().unwrap().to_owned();
        *key = json!(original + &"x".repeat(4096 - size));
        assert_eq!(serde_json::to_vec(&value).unwrap().len(), 4096);
        assert!(value.validate_input().is_ok());
        let key = if operation == CLIPBOARD_PASTE {
            &mut value.payload["layerUid"]
        } else {
            &mut value.payload["selection"][0]["layerUid"]
        };
        *key = json!(key.as_str().unwrap().to_owned() + "x");
        assert_eq!(serde_json::to_vec(&value).unwrap().len(), 4097);
        assert!(value.validate_input().is_err());
    }
}

#[path = "native_frame_clipboard_response_schema_tests.rs"]
mod response_schema;
