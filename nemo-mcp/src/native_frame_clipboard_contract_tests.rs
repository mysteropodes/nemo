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
        // F12 admits the full4096-cell selection, independently of the unchanged cell cap.
        let large = (0..4096)
            .map(|frame| json!({"layerUid":"a","frame":frame}))
            .collect::<Vec<_>>();
        let complete = envelope(operation, selection(operation, json!(large)));
        let bytes = serde_json::to_vec(&complete).unwrap().len();
        assert!(bytes > 4096 && bytes < crate::wire::native_request_byte_limit(operation));
        assert!(complete.validate_input().is_ok());
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
    // Count cells separately from record/order shells; native storage still owns record grammar.
    assert_eq!(MAX_DATA_ITEMS, 131_072);
    for field in ["records", "orderElements"] {
        for count in [4096, 4097, MAX_DATA_ITEMS] {
            let mut at_limit = cell.clone();
            at_limit[field] = json!(vec![json!({}); count]);
            assert!(request(CLIPBOARD_PASTE, &paste(json!([at_limit]))));
        }
        let mut over = cell.clone();
        over[field] = json!(vec![json!({}); MAX_DATA_ITEMS + 1]);
        assert!(!request(CLIPBOARD_PASTE, &paste(json!([over]))));
    }
    // Count admission does not waive aggregate serialized record+order bytes.
    let data_bytes = crate::wire::native_clipboard_transport::DATA_BYTES;
    assert_eq!(data_bytes, 4_194_304);
    let mut data = cell.clone();
    data["records"] = json!([{"padding":""}]);
    let fixed = serde_json::to_vec(&data["records"][0]).unwrap().len()
        + serde_json::to_vec(&data["orderElements"]).unwrap().len();
    data["records"][0]["padding"] = json!("x".repeat(data_bytes - fixed));
    assert!(request(CLIPBOARD_PASTE, &paste(json!([data.clone()]))));
    data["records"][0]["padding"] = json!("x".repeat(data_bytes - fixed + 1));
    assert!(!request(CLIPBOARD_PASTE, &paste(json!([data]))));
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
fn clipboard_complete_request_bound_is_the_declared_full_envelope_not_the_data_budget() {
    let limit = crate::wire::native_clipboard_transport::MESSAGE_BYTES;
    assert_eq!(limit, 5_246_976);
    for operation in [CLIPBOARD_COPY, CLIPBOARD_CUT, CLIPBOARD_PASTE] {
        let payload = if operation == CLIPBOARD_PASTE {
            paste(json!([]))
        } else {
            selection(operation, json!([{"layerUid":"a","frame":0}]))
        };
        assert_eq!(crate::wire::native_request_byte_limit(operation), limit);
        let original = envelope(operation, payload);
        let fixed = serde_json::to_vec(&original).unwrap().len();
        for length in [4096, 4097, limit] {
            let mut value = original.clone();
            let key = if operation == CLIPBOARD_PASTE {
                &mut value.payload["layerUid"]
            } else {
                &mut value.payload["selection"][0]["layerUid"]
            };
            *key = json!(key.as_str().unwrap().to_owned() + &"x".repeat(length - fixed));
            assert_eq!(serde_json::to_vec(&value).unwrap().len(), length);
            assert!(value.validate_input().is_ok());
            if length == limit {
                let key = if operation == CLIPBOARD_PASTE {
                    &mut value.payload["layerUid"]
                } else {
                    &mut value.payload["selection"][0]["layerUid"]
                };
                *key = json!(key.as_str().unwrap().to_owned() + "x");
                assert_eq!(serde_json::to_vec(&value).unwrap().len(), limit + 1);
                assert!(value.validate_input().is_err());
            }
        }
    }
    // The F12 message budget never enlarges another frame-edit operation.
    let mut other = envelope(
        KEY_BLANK,
        json!({"command":"object.layer.key.blank","layerUid":"a","cell":0}),
    );
    let fixed = serde_json::to_vec(&other).unwrap().len();
    other.payload["layerUid"] = json!("a".to_owned() + &"x".repeat(4096 - fixed));
    assert_eq!(crate::wire::native_request_byte_limit(KEY_BLANK), 4096);
    assert_eq!(serde_json::to_vec(&other).unwrap().len(), 4096);
    assert!(other.validate_input().is_ok());
    other.payload["layerUid"] = json!(other.payload["layerUid"].as_str().unwrap().to_owned() + "x");
    assert_eq!(serde_json::to_vec(&other).unwrap().len(), 4097);
    assert!(other.validate_input().is_err());
}

#[path = "native_frame_clipboard_response_schema_tests.rs"]
mod response_schema;
