//! Bounded Cut through the actual owner, retained receipts and original packet DTO.
use super::{build, bytes, dispatch, head, owner, request, rows, state};
use crate::commands::{cut_clipboard_payload, CellRef, FrameClipboardOperation};
use crate::transaction::{CompactResult, RetainedDisposition};
use serde_json::{json, Value};
const OP: &str = "command.document.object.frames.clipboard.cut";
fn payload(frame: u32) -> Value {
    json!({"command":"object.frames.clipboard.cut","selection":[{"layerUid":"L0","frame":frame}]})
}
fn small() -> crate::object_document::ObjectDocument {
    build::document(
        &rows::parse_layers("K=a.1.10;.;K=b.2.20;.|K;.;.;K"),
        build::Build::V1,
    )
}
#[test]
fn cut_returns_pre_edit_ordered_dtos_once_and_survives_undo_redo_reopen() {
    for form in [build::Build::V2, build::Build::V2Extras] {
        let mut o = owner(rows::ordered_document(form));
        let before = bytes(&o);
        let typed = o
            .copy_frame_clipboard(&[CellRef {
                layer_uid: "L0".into(),
                frame: 0,
            }])
            .unwrap();
        let expected = cut_clipboard_payload(Some(&typed));
        assert_eq!(
            expected["cells"][0]["orderElements"],
            rows::ordered_elements("L0")
        );
        let q = request(&o, "public-cut", OP, payload(0));
        let reply = dispatch(&mut o, &q);
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(
            reply["result"],
            json!({"applied":true,"historyEntriesAdded":1,"clipboard":expected})
        );
        assert_eq!(o.history_depths(), (1, 0));
        let cut = bytes(&o);
        assert_ne!(cut, before);
        assert_eq!(
            rows::scoped_elements(head(&o).document(), "L0", 0),
            json!([])
        );
        let undone = o.undo(1).unwrap();
        assert_eq!(bytes(&o), before);
        o.redo(undone).unwrap();
        assert_eq!(bytes(&o), cut);
        let reopened = crate::object_codec::decode_storage_project(&cut).unwrap();
        assert_eq!(
            crate::object_codec::encode_storage_project(&reopened).unwrap(),
            cut
        );
    }
}
#[test]
fn retained_cut_reply_is_exact_after_undo_and_changed_body_never_reexecutes() {
    let mut o = owner(small());
    let q = request(&o, "same-id", OP, payload(0));
    let reply = dispatch(&mut o, &q);
    assert_eq!(reply["ok"], true);
    o.undo(1).unwrap();
    let after_undo = state(&o);
    assert_eq!(dispatch(&mut o, &q), reply);
    assert_eq!(state(&o), after_undo);
    let mut changed = q.clone();
    changed["payload"] = payload(2);
    let refused = dispatch(&mut o, &changed);
    assert_eq!(refused["error"]["code"], "invalid_request");
    assert_eq!(state(&o), after_undo);
}
#[test]
fn held_noop_cut_returns_normalized_packet_and_keeps_existing_redo() {
    let mut o = owner(small());
    let changed = request(&o, "first", OP, payload(0));
    assert_eq!(dispatch(&mut o, &changed)["ok"], true);
    o.undo(1).unwrap();
    let before = state(&o);
    assert_eq!(o.history_depths(), (0, 1));
    let q = request(&o, "hold-noop", OP, payload(1));
    let reply = dispatch(&mut o, &q);
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(reply["result"]["applied"], false);
    assert_eq!(reply["result"]["historyEntriesAdded"], 0);
    assert_eq!(reply["result"]["clipboard"]["cells"][0]["keyframe"], false);
    assert_eq!(
        reply["result"]["clipboard"]["cells"][0]["records"],
        json!([])
    );
    assert_eq!(state(&o), before);
}
#[test]
fn empty_cut_is_null_clipboard_and_never_publishes() {
    let mut o = owner(small());
    let before = state(&o);
    let q = request(
        &o,
        "empty",
        OP,
        json!({"command":"object.frames.clipboard.cut","selection":[]}),
    );
    assert_eq!(
        dispatch(&mut o, &q)["result"],
        json!({"applied":false,"historyEntriesAdded":0,"clipboard":null})
    );
    assert_eq!(state(&o), before);
}
#[test]
fn formerly_4096_limited_cut_is_retained_and_commits_exactly_once() {
    let d = build::document(
        &rows::parse_layers("K=a.1.1,b.1.2,c.1.3,d.1.4,e.1.5,f.1.6,g.1.7,h.1.8;.;.;."),
        build::Build::V1,
    );
    let mut o = owner(d);
    let before = state(&o);
    let q = request(&o, "too-large", OP, payload(0));
    let reply = dispatch(&mut o, &q);
    assert_eq!(reply["ok"], true, "{reply}");
    assert!(serde_json::to_vec(&reply).unwrap().len() > 4096);
    assert_ne!(state(&o), before);
    let committed = state(&o);
    assert_eq!(dispatch(&mut o, &q), reply);
    assert_eq!(state(&o), committed);
}
#[test]
fn complete_header_is_preflighted_even_when_the_clipboard_alone_fits() {
    for count in 1..=8 {
        for padding in 0..=120 {
            let strokes = (0..count)
                .map(|i| format!("a{}{}.1.1", i, "x".repeat(padding)))
                .collect::<Vec<_>>()
                .join(",");
            let drawing = build::document(
                &rows::parse_layers(&format!("K={strokes};.;.;.")),
                build::Build::V1,
            );
            let mut o = owner(drawing);
            let candidate = o
                .prepare_frame_clipboard(&FrameClipboardOperation::Cut {
                    selection: vec![CellRef {
                        layer_uid: "L0".into(),
                        frame: 0,
                    }],
                })
                .unwrap();
            let clipboard = cut_clipboard_payload(candidate.packet());
            let q = request(&o, &"q".repeat(128), OP, payload(0));
            let typed = serde_json::from_value(q.clone()).unwrap();
            let response = RetainedDisposition::Exact {
                content_revision: 1,
                outcome: Ok(CompactResult::FrameClipboardCut {
                    applied: true,
                    history_entries_added: 1,
                    clipboard: clipboard.clone(),
                }),
            }
            .response(&typed, o.instance_id(), o.document_id())
            .unwrap();
            if serde_json::to_vec(&clipboard).unwrap().len() <= 4096
                && serde_json::to_vec(&response).unwrap().len() > 4096
            {
                let before = state(&o);
                let reply = dispatch(&mut o, &q);
                assert_eq!(reply["ok"], true, "{reply}");
                assert_eq!(reply["result"]["clipboard"], clipboard);
                assert_ne!(state(&o), before);
                let committed = state(&o);
                assert_eq!(dispatch(&mut o, &q), reply);
                assert_eq!(state(&o), committed);
                return;
            }
        }
    }
    panic!("control must exercise header overflow with a clipboard that fits");
}
#[test]
fn cut_mismatch_cancel_stale_and_wrong_owner_refuse_without_publication() {
    let mut o = owner(small());
    let before = state(&o);
    let base = request(&o, "base", OP, payload(0));
    let mut cases = Vec::new();
    let mut q = base.clone();
    q["requestId"] = json!("mismatch");
    q["payload"] =
        json!({"command":"object.frames.clipboard.duplicate","layerUid":"L0","selection":[]});
    cases.push((q, "invalid_request"));
    let mut q = base.clone();
    q["requestId"] = json!("stale");
    q["expectedRevision"] = json!(1);
    cases.push((q, "stale_revision"));
    let mut q = base.clone();
    q["requestId"] = json!("wrong-owner");
    q["instanceId"] = json!("another-owner");
    cases.push((q, "wrong_instance"));
    let mut q = base.clone();
    q["requestId"] = json!("cancel");
    q["cancelledBeforeDispatch"] = json!(true);
    cases.push((q, "cancelled_before_dispatch"));
    for (q, code) in cases {
        let reply = dispatch(&mut o, &q);
        assert_eq!(reply["error"]["code"], code, "{reply}");
        assert_eq!(state(&o), before);
    }
}
