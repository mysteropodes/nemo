//! Strict Cut payload, immutable packet DTO and original byte-bound controls.
use super::*;

fn cut_value() -> Value {
    json!({"command":"object.frames.clipboard.cut",
        "selection":[{"layerUid":"L0","frame":0},{"layerUid":"L1","frame":3}]})
}

#[test]
fn cut_selection_only_parses_real_typed_and_empty_operations() {
    assert_eq!(
        parse(&cut_value()).unwrap(),
        FrameClipboardOperation::Cut {
            selection: vec![
                CellRef {
                    layer_uid: "L0".into(),
                    frame: 0
                },
                CellRef {
                    layer_uid: "L1".into(),
                    frame: 3
                }
            ],
        }
    );
    assert_eq!(
        parse(&json!({"command":"object.frames.clipboard.cut","selection":[]})).unwrap(),
        FrameClipboardOperation::Cut { selection: vec![] }
    );
    let opaque = json!({"command":"object.frames.clipboard.cut",
        "selection":[{"layerUid":" 親:雪/opaque ","frame":4294967295_u32}]});
    assert_eq!(
        parse(&opaque).unwrap(),
        FrameClipboardOperation::Cut {
            selection: vec![CellRef {
                layer_uid: " 親:雪/opaque ".into(),
                frame: u32::MAX
            }],
        }
    );
}

#[test]
fn cut_rejects_wrong_command_fields_and_all_missing_null_or_wrong_types() {
    for field in [
        "instanceId",
        "documentId",
        "layerUid",
        "frame",
        "cells",
        "extra",
    ] {
        let mut value = cut_value();
        value[field] = match field {
            "frame" => json!(0),
            "cells" => json!([]),
            _ => json!("not-admitted"),
        };
        assert_eq!(
            parse(&value).unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{field}"
        );
        value[field] = Value::Null;
        assert_eq!(
            parse(&value).unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "null {field}"
        );
    }
    for value in [
        json!({"command":"object.frames.clipboard.cut"}),
        json!({"selection":[]}),
        json!({"command":"object.frames.clipboard.Cut","selection":[]}),
        json!({"command":"object.frames.clipboard.paste","selection":[]}),
        json!({"command":"object.frames.clipboard.duplicate","selection":[]}),
    ] {
        assert_eq!(
            parse(&value).unwrap_err(),
            DispatchErrorCode::InvalidRequest
        );
    }
    for selection in [
        Value::Null,
        json!(1),
        json!("cells"),
        json!({}),
        json!([null]),
        json!([{"layerUid":"L0"}]),
        json!([{"frame":0}]),
        json!([{"layerUid":"","frame":0}]),
        json!([{"layerUid":null,"frame":0}]),
        json!([{"layerUid":5,"frame":0}]),
        json!([{"layerUid":"L0","frame":null}]),
        json!([{"layerUid":"L0","frame":"0"}]),
        json!([{"layerUid":"L0","frame":-1}]),
        json!([{"layerUid":"L0","frame":0.5}]),
        json!([{"layerUid":"L0","frame":4294967296_u64}]),
        json!([{"layerUid":"L0","frame":0,"extra":true}]),
    ] {
        let value = json!({"command":"object.frames.clipboard.cut","selection":selection});
        assert_eq!(
            parse(&value).unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{value}"
        );
    }
}

#[test]
fn cut_raw_duplicate_fields_and_original_utf8_byte_boundary_are_strict() {
    for raw in [
        r#"{"command":"object.frames.clipboard.cut","command":"object.frames.clipboard.cut","selection":[]}"#,
        r#"{"command":"object.frames.clipboard.cut","selection":[],"selection":[]}"#,
        r#"{"command":"object.frames.clipboard.cut","selection":[{"layerUid":"L0","layerUid":"L1","frame":0}]}"#,
        r#"{"command":"object.frames.clipboard.cut","selection":[{"layerUid":"L0","frame":0,"frame":1}]}"#,
    ] {
        assert_eq!(
            parse_clipboard_payload(raw.as_bytes()).unwrap_err(),
            DispatchErrorCode::InvalidRequest
        );
    }
    let mut value = cut_value();
    value["selection"][0]["layerUid"] = json!("雪".repeat(1000));
    let mut bytes = value.to_string().into_bytes();
    assert!(bytes.len() < 4096);
    let limit = crate::request_receipts::frame_clipboard_transport::MESSAGE_BYTES;
    assert_eq!(limit, 5_246_976);
    for size in [4096, 4097, limit - 1, limit] {
        bytes.resize(size, b' ');
        assert!(
            parse_clipboard_payload(&bytes).is_ok(),
            "original UTF8 bytes {size}"
        );
    }
    bytes.push(b' ');
    assert_eq!(
        parse_clipboard_payload(&bytes).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    // Duplicate is not one of the three clipboard pairing operations.
    value["command"] = json!("object.frames.clipboard.duplicate");
    value["layerUid"] = json!("L0");
    let mut duplicate = value.to_string().into_bytes();
    duplicate.resize(4096, b' ');
    assert!(parse_clipboard_payload(&duplicate).is_ok());
    duplicate.push(b' ');
    assert_eq!(
        parse_clipboard_payload(&duplicate).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
}

#[test]
fn prepared_cut_changes_document_is_readonly_and_noop_still_has_a_packet() {
    let owner = ObjectFillOwner::new("wire", doc("K=a.1.10;.;K=b.2.20;.")).unwrap();
    for (selection, changed, has_packet) in [
        (vec![json!({"layerUid":"L0","frame":0})], true, true),
        (vec![json!({"layerUid":"L0","frame":1})], false, true),
        (vec![], false, false),
    ] {
        let raw =
            json!({"command":"object.frames.clipboard.cut","selection":selection}).to_string();
        let candidate = owner.prepare_frame_clipboard_json(raw.as_bytes()).unwrap();
        assert_eq!(candidate.command(), "object.frames.clipboard.cut");
        assert_eq!(candidate.changes_document(), changed);
        assert_eq!(candidate.packet().is_some(), has_packet);
        assert_eq!(
            owner.content_revision(),
            0,
            "read-only planning cannot publish"
        );
        if !has_packet {
            assert_eq!(cut_clipboard_payload(candidate.packet()), Value::Null);
        }
    }
    let duplicate = json!({"command":"object.frames.clipboard.cut",
        "selection":[{"layerUid":"L0","frame":0},{"layerUid":"L0","frame":0}]})
    .to_string();
    assert_eq!(
        owner
            .prepare_frame_clipboard_json(duplicate.as_bytes())
            .unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(
        owner
            .copy_frame_clipboard(&[CellRef {
                layer_uid: "L0".into(),
                frame: 0
            }])
            .unwrap()
            .cells()[0]
            .records()[0]
            .target()
            .stroke_id(),
        "a"
    );
}

#[test]
fn actual_cut_result_has_only_source_identity_and_existing_normalized_paste_cells() {
    for form in [Build::V1, Build::V2, Build::V2Extras] {
        let document = if form == Build::V1 {
            doc("K=a.1.10,b.2.20;.;.;.|K=c.3.30;.;.;.")
        } else {
            rows::ordered_document(form)
        };
        let owner = ObjectFillOwner::new("wire", document).unwrap();
        let raw = json!({"command":"object.frames.clipboard.cut",
            "selection":[{"layerUid":"L0","frame":0},{"layerUid":"L0","frame":1}]})
        .to_string();
        let prepared = owner.prepare_frame_clipboard_json(raw.as_bytes()).unwrap();
        let packet = prepared.packet().unwrap();
        let result = cut_clipboard_payload(Some(packet));
        assert_eq!(
            result
                .as_object()
                .unwrap()
                .keys()
                .map(String::as_str)
                .collect::<Vec<_>>(),
            ["cells", "documentId", "instanceId"]
        );
        assert_eq!(result["instanceId"], "wire");
        assert_eq!(result["documentId"], owner.document_id());
        assert_eq!(result["cells"], paste_payload(packet, "L1", 2)["cells"]);
        assert_eq!(result["cells"][0]["relLayer"], 0);
        assert_eq!(result["cells"][0]["relFrame"], 0);
        assert_eq!(result["cells"][0]["keyframe"], true);
        let ids: Vec<_> = result["cells"][0]["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["target"]["strokeId"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["a", "b"]);
        assert_eq!(result["cells"][1]["relFrame"], 1);
        assert_eq!(result["cells"][1]["keyframe"], false);
        assert_eq!(result["cells"][1]["records"], json!([]));
        if form == Build::V1 {
            assert!(result["cells"][0].get("orderElements").is_none());
            assert!(result["cells"][1].get("orderElements").is_none());
        } else {
            assert_eq!(
                result["cells"][0]["orderElements"],
                rows::ordered_elements("L0")
            );
            assert_eq!(result["cells"][1]["orderElements"], json!([]));
        }
        assert_eq!(owner.content_revision(), 0);
    }
}

#[test]
fn oversized_cut_dto_is_visible_to_precommit_envelope_admission_without_mutation() {
    let owner = ObjectFillOwner::new(
        "wire",
        doc("K=a.1.1,b.1.2,c.1.3,d.1.4,e.1.5,f.1.6,g.1.7,h.1.8;.;.;."),
    )
    .unwrap();
    let prepared = owner
        .prepare_frame_clipboard_json(
            json!({"command":"object.frames.clipboard.cut",
        "selection":[{"layerUid":"L0","frame":0}]})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    assert!(prepared.changes_document());
    let value = cut_clipboard_payload(prepared.packet());
    assert!(
        value.to_string().len() > 4096,
        "dispatcher must refuse complete reply BEFORE commit"
    );
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(
        owner
            .copy_frame_clipboard(&[CellRef {
                layer_uid: "L0".into(),
                frame: 0
            }])
            .unwrap()
            .cells()[0]
            .records()
            .len(),
        8
    );
}
