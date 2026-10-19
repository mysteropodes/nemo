//! Copy through the real public command dispatcher, existing read-only producer and retained receipts.
use super::{build, dispatch, owner, request, rows, state};
use crate::commands::{cut_clipboard_payload, CellRef, ObjectFillOwner};
use crate::object_codec::decode_storage_project;
use crate::transaction::{CompactResult, RetainedDisposition};
use serde_json::{json, Value};

const COPY: &str = "command.document.object.frames.clipboard.copy";
const CUT: &str = "command.document.object.frames.clipboard.cut";
fn payload(selection: Value) -> Value {
    json!({"command":"object.frames.clipboard.copy","selection":selection})
}
fn one(frame: u32) -> Value {
    payload(json!([{"layerUid":"L0","frame":frame}]))
}
fn small() -> crate::object_document::ObjectDocument {
    build::document(
        &rows::parse_layers("K=a.1.10;.;K=b.2.20;.|K;.;.;K"),
        build::Build::V1,
    )
}
fn result(clipboard: Value) -> Value {
    json!({"applied":false,"historyEntriesAdded":0,"clipboard":clipboard})
}

#[test]
fn copy_preserves_selection_order_and_normalizes_key_interpolated_held_and_unexposed_cells() {
    for form in build::Build::ALL {
        let input = build::document(
            &rows::parse_layers(".;K=a.1.10;I=b.2.20;K;.;.|K=c.3.30;.;.;.;K=d.4.40;."),
            form,
        );
        let mut o = owner(input);
        let before = state(&o);
        let q = request(
            &o,
            "ordered-selection",
            COPY,
            payload(json!([
                {"layerUid":"L1","frame":1}, {"layerUid":"L0","frame":2},
                {"layerUid":"L0","frame":0}, {"layerUid":"L0","frame":1},
                {"layerUid":"L0","frame":3}
            ])),
        );
        let mut cells = vec![
            json!({"relLayer":1,"relFrame":1,"keyframe":false,"records":[]}),
            json!({"relLayer":0,"relFrame":2,"keyframe":true,
                "records":[build::record_json("scene-root","authored","L0","b",2,2,20)]}),
            json!({"relLayer":0,"relFrame":0,"keyframe":false,"records":[]}),
            json!({"relLayer":0,"relFrame":1,"keyframe":true,
                "records":[build::record_json("scene-root","authored","L0","a",1,1,10)]}),
            json!({"relLayer":0,"relFrame":3,"keyframe":true,"records":[]}),
        ];
        if form.v2() {
            for cell in &mut cells {
                cell["orderElements"] = json!([]);
            }
            cells[1]["orderElements"] = json!([build::element_json(
                "L0",
                &build::Stroke {
                    id: "b".into(),
                    z: 2,
                    g: 20
                },
                0
            )]);
            cells[3]["orderElements"] = json!([build::element_json(
                "L0",
                &build::Stroke {
                    id: "a".into(),
                    z: 1,
                    g: 10
                },
                0
            )]);
        }
        let expected =
            json!({"instanceId":o.instance_id(),"documentId":o.document_id(),"cells":cells});
        let reply = dispatch(&mut o, &q);
        assert_eq!(reply["ok"], true, "{form:?}: {reply}");
        assert_eq!(reply["result"], result(expected), "{form:?}");
        assert_eq!(
            state(&o),
            before,
            "Copy must not publish, change history or snapshot identity"
        );
    }
}

#[test]
fn copy_preserves_authored_order_dtos_and_opaque_layer_identity_without_a_new_owner() {
    for form in [build::Build::V2, build::Build::V2Extras] {
        let mut o = owner(rows::ordered_document(form));
        let before = state(&o);
        let q = request(&o, "order-dtos", COPY, one(0));
        let reply = dispatch(&mut o, &q);
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(
            reply["result"]["clipboard"]["cells"][0]["orderElements"],
            rows::ordered_elements("L0")
        );
        assert_eq!(state(&o), before);
    }
    let text = serde_json::to_string(&small())
        .unwrap()
        .replace("\"L0\"", "\"layer:opaque-0/a_b.9\"");
    let document = decode_storage_project(text.as_bytes()).unwrap();
    let mut o = crate::history::NativeObjectHistory::new("copy:owner/opaque", document).unwrap();
    let before = state(&o);
    let q = request(
        &o,
        "opaque",
        COPY,
        payload(json!([{"layerUid":"layer:opaque-0/a_b.9","frame":0}])),
    );
    let reply = dispatch(&mut o, &q);
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(
        reply["result"]["clipboard"]["instanceId"],
        "copy:owner/opaque"
    );
    assert_eq!(
        reply["result"]["clipboard"]["cells"][0]["records"][0]["target"]["layerUid"],
        "layer:opaque-0/a_b.9"
    );
    assert_eq!(state(&o), before);
}

#[test]
fn copy_retains_original_packet_after_a_later_edit_and_preserves_redo() {
    let mut o = owner(small());
    let cut = request(
        &o,
        "seed",
        CUT,
        json!({"command":"object.frames.clipboard.cut","selection":[{"layerUid":"L0","frame":0}]}),
    );
    assert_eq!(dispatch(&mut o, &cut)["ok"], true);
    o.undo(1).unwrap();
    let before = state(&o);
    assert_eq!(o.history_depths(), (0, 1));
    let q = request(&o, "retained-copy", COPY, one(0));
    let reply = dispatch(&mut o, &q);
    assert_eq!(reply["ok"], true);
    assert!(!reply["result"]["clipboard"]["cells"][0]["records"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(state(&o), before);
    o.redo(before.0).unwrap();
    let after_edit = state(&o);
    assert_ne!(after_edit.3, before.3);
    assert_eq!(
        dispatch(&mut o, &q),
        reply,
        "replay returns the original packet/revision, not a new read"
    );
    assert_eq!(state(&o), after_edit);
    let fresh = request(&o, "fresh-copy", COPY, one(0));
    assert_ne!(
        dispatch(&mut o, &fresh)["result"]["clipboard"],
        reply["result"]["clipboard"]
    );
    assert_eq!(state(&o), after_edit);
    let mut changed = q.clone();
    changed["payload"] = one(2);
    assert_eq!(
        dispatch(&mut o, &changed)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), after_edit);
}

#[test]
fn empty_copy_returns_no_new_packet_and_preserves_existing_history_and_content() {
    let mut o = owner(small());
    let before = state(&o);
    let q = request(&o, "empty", COPY, payload(json!([])));
    let reply = dispatch(&mut o, &q);
    assert_eq!(reply["ok"], true);
    assert_eq!(reply["result"], result(Value::Null));
    assert_eq!(dispatch(&mut o, &q), reply);
    assert_eq!(state(&o), before);
}

#[test]
fn copy_refuses_invalid_stale_wrong_owner_cancelled_and_duplicate_cells_atomically() {
    let mut o = owner(small());
    let before = state(&o);
    let base = request(&o, "base", COPY, one(0));
    let mut cases = Vec::new();
    for (key, value, code) in [
        ("expectedRevision", json!(1), "stale_revision"),
        ("instanceId", json!("other-owner"), "wrong_instance"),
        ("documentId", json!("other-document"), "wrong_document"),
        (
            "cancelledBeforeDispatch",
            json!(true),
            "cancelled_before_dispatch",
        ),
    ] {
        let mut q = base.clone();
        q[key] = value;
        cases.push((q, code));
    }
    for selection in [
        json!([{"layerUid":"L0","frame":0},{"layerUid":"L0","frame":0}]),
        json!([{"layerUid":"L0","frame":4}]),
        json!([{"layerUid":"","frame":0}]),
        json!(null),
        json!([{"layerUid":"L0","frame":null}]),
        json!([{"layerUid":"L0","frame":0,"unknown":1}]),
    ] {
        let mut q = base.clone();
        q["payload"] = payload(selection);
        cases.push((q, "invalid_request"));
    }
    let mut missing_revision = base.clone();
    missing_revision
        .as_object_mut()
        .unwrap()
        .remove("expectedRevision");
    cases.push((missing_revision, "invalid_request"));
    let mut unknown = base.clone();
    unknown["payload"] = payload(json!([{"layerUid":"absent:layer","frame":0}]));
    cases.push((unknown, "not_found"));
    for extra in [
        "instanceId",
        "documentId",
        "layerUid",
        "frame",
        "cells",
        "unknown",
    ] {
        let mut q = base.clone();
        q["payload"][extra] = Value::Null;
        cases.push((q, "invalid_request"));
    }
    let mut mismatch = base.clone();
    mismatch["payload"]["command"] = json!("object.frames.clipboard.cut");
    cases.push((mismatch, "invalid_request"));
    for (index, (mut q, code)) in cases.into_iter().enumerate() {
        q["requestId"] = json!(format!("refusal-{index}"));
        let reply = dispatch(&mut o, &q);
        assert_eq!(reply["error"]["code"], code, "{q}: {reply}");
        assert_eq!(state(&o), before);
    }
}

#[test]
fn copy_raw_wire_refuses_duplicate_unknown_null_malformed_and_oversized_fields() {
    let o = ObjectFillOwner::new("wire-owner", small()).unwrap();
    let good =
        r#"{"command":"object.frames.clipboard.copy","selection":[{"layerUid":"L0","frame":0}]}"#;
    let mut raw = vec![
        "null".to_string(),
        "[]".to_string(),
        "{}".to_string(),
        good.replace("\"selection\":[", "\"selection\":[],\"selection\":["),
        good.replace("\"frame\":0", "\"frame\":0,\"frame\":1"),
        good.replace("\"layerUid\":\"L0\"", "\"layerUid\":null"),
        good.replace("\"frame\":0", "\"frame\":-1"),
        good.replace("\"frame\":0", "\"frame\":0.5"),
        good.replace("\"frame\":0", "\"frame\":4294967296"),
        good.replace("\"frame\":0", "\"frame\":\"0\""),
        good.replace("\"command\":", "\"unknown\":true,\"command\":"),
    ];
    raw.push(format!(
        "{good}{}",
        " ".repeat(crate::request_receipts::frame_clipboard_transport::MESSAGE_BYTES + 1)
    ));
    for text in raw {
        assert_eq!(
            o.copy_frame_clipboard_json(text.as_bytes()).unwrap_err(),
            crate::request_receipts::DispatchErrorCode::InvalidRequest,
            "{text}"
        );
    }
}

#[test]
fn formerly_4096_limited_copy_reply_is_retained_without_effect() {
    let d = build::document(
        &rows::parse_layers("K=a.1.1,b.1.2,c.1.3,d.1.4,e.1.5,f.1.6,g.1.7,h.1.8;.;.;."),
        build::Build::V1,
    );
    let mut o = owner(d);
    let before = state(&o);
    let q = request(&o, "too-large", COPY, one(0));
    let reply = dispatch(&mut o, &q);
    assert_eq!(reply["ok"], true, "{reply}");
    assert!(serde_json::to_vec(&reply).unwrap().len() > 4096);
    assert_eq!(dispatch(&mut o, &q), reply);
    assert_eq!(state(&o), before);
}

#[test]
fn copy_preflights_full_correlated_envelope_even_when_packet_alone_fits() {
    for count in 1..=8 {
        for padding in 0..=120 {
            let strokes = (0..count)
                .map(|i| format!("a{}{}.1.1", i, "x".repeat(padding)))
                .collect::<Vec<_>>()
                .join(",");
            let d = build::document(
                &rows::parse_layers(&format!("K={strokes};.;.;.")),
                build::Build::V1,
            );
            let mut o = owner(d);
            let packet = o
                .copy_frame_clipboard(&[CellRef {
                    layer_uid: "L0".into(),
                    frame: 0,
                }])
                .unwrap();
            let clipboard = cut_clipboard_payload(Some(&packet));
            let q = request(&o, &"q".repeat(128), COPY, one(0));
            let typed = serde_json::from_value(q.clone()).unwrap();
            let full = RetainedDisposition::Exact {
                content_revision: 0,
                outcome: Ok(CompactResult::FrameClipboardCut {
                    applied: false,
                    history_entries_added: 0,
                    clipboard: clipboard.clone(),
                }),
            }
            .response(&typed, o.instance_id(), o.document_id())
            .unwrap();
            if serde_json::to_vec(&clipboard).unwrap().len() <= 4096
                && serde_json::to_vec(&full).unwrap().len() > 4096
            {
                let before = state(&o);
                let reply = dispatch(&mut o, &q);
                assert_eq!(reply["ok"], true, "{reply}");
                assert_eq!(reply["result"]["clipboard"], clipboard);
                assert_eq!(dispatch(&mut o, &q), reply);
                assert_eq!(state(&o), before);
                return;
            }
        }
    }
    panic!("must exercise complete-header overflow with a legal packet");
}

#[test]
fn original_dense_cut_copy_packets_match_public_copy_without_editing_their_source() {
    let mut visited = 0;
    let mut admitted = 0;
    for row in rows::rows().into_iter().filter(|row| row.op == "cut") {
        for form in [build::Build::V1, build::Build::V1Ref] {
            let mut value = build::document_json(&row.input, form);
            for (layer, locked) in row.locked.iter().enumerate() {
                if *locked {
                    value["layers"][layer]["locked"] = json!(true);
                }
            }
            let mut o = owner(decode_storage_project(value.to_string().as_bytes()).unwrap());
            let selection = row.selection();
            let cells: Vec<Value> = row.clip.iter().zip(&selection).map(|(entry, (layer, frame))| {
                let records: Vec<Value> = if entry.cell.flag == '.' { Vec::new() } else {
                    entry.cell.strokes.iter().map(|stroke| build::record_json("scene-root", "authored", &build::uid(*layer), &stroke.id, *frame, stroke.z, stroke.g)).collect()
                };
                json!({"relLayer":entry.rl,"relFrame":entry.rf,"keyframe":entry.cell.flag!='.',"records":records})
            }).collect();
            assert_eq!(cells.len(), selection.len());
            let clipboard =
                json!({"instanceId":o.instance_id(),"documentId":o.document_id(),"cells":cells});
            let selected: Vec<Value> = selection
                .iter()
                .map(|(layer, frame)| json!({"layerUid":build::uid(*layer),"frame":frame}))
                .collect();
            let q = request(&o, "original-copy", COPY, payload(json!(selected)));
            let typed = serde_json::from_value(q.clone()).unwrap();
            let prospective = RetainedDisposition::Exact {
                content_revision: 0,
                outcome: Ok(CompactResult::FrameClipboardCut {
                    applied: false,
                    history_entries_added: 0,
                    clipboard: clipboard.clone(),
                }),
            }
            .response(&typed, o.instance_id(), o.document_id())
            .unwrap();
            let before = state(&o);
            let reply = dispatch(&mut o, &q);
            if serde_json::to_vec(&q).unwrap().len() <= 4096
                && serde_json::to_vec(&prospective).unwrap().len() <= 4096
            {
                assert_eq!(reply["ok"], true, "{} {form:?}: {reply}", row.label());
                assert_eq!(
                    reply["result"],
                    result(clipboard),
                    "{} {form:?}",
                    row.label()
                );
                admitted += 1;
            } else {
                assert_eq!(
                    reply["error"]["code"],
                    "invalid_request",
                    "{} {form:?}: {reply}",
                    row.label()
                );
            }
            assert_eq!(state(&o), before, "{} {form:?}", row.label());
            visited += 1;
        }
    }
    assert_eq!(
        visited, 260,
        "all130 original cut-copy packets in both admitted legacy forms"
    );
    assert!(admitted > 0);
}
