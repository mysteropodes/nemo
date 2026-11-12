//! Hand-computed frame-clipboard cases through the real history owner: duplicate selection as copy plus paste, replay,
//! no-op/redo, one entry per change, preflight and payload refusals, sealed candidates, cut, foreign packets and a chain
//! of edits that pastes a packet copied at an earlier revision. The oracle rows are in
//! object_frame_clipboard_paste_history.rs.
use super::build::{document, exposure_string, render_layer, Build};
use super::rows::{parse_layers, record_of, OP_DUPLICATE, OP_PASTE};
use super::{bytes, dispatch, head, owner, request, state};
use crate::commands::{
    packet_paste_cells, paste_payload, CellRef, FrameClipboardOperation, PasteCell, PasteRequest,
};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::ObjectDocument;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

/// Two layers, four frames: L0 has a stroke at each of its two keys, L1 is a bare key track (blank keys at 0 and 3).
const SMALL: &str = "K=a.1.10;.;K=b.2.20;.|K;.;.;K";

fn doc(text: &str) -> ObjectDocument {
    document(&parse_layers(text), Build::V1)
}
fn layer_text(o: &NativeObjectHistory, layer: usize) -> String {
    render_layer(head(o).document(), layer)
}
fn applied(reply: &Value) -> Value {
    json!({"applied":reply["result"]["applied"],"historyEntriesAdded":reply["result"]["historyEntriesAdded"]})
}
fn refs(cells: &[(&str, u32)]) -> Vec<CellRef> {
    cells
        .iter()
        .map(|(layer, frame)| CellRef {
            layer_uid: (*layer).to_owned(),
            frame: *frame,
        })
        .collect()
}
fn selection_json(cells: &[(&str, u32)]) -> Value {
    Value::Array(
        cells
            .iter()
            .map(|(layer, frame)| json!({"layerUid":layer,"frame":frame}))
            .collect(),
    )
}
fn duplicate_payload(layer: &str, cells: &[(&str, u32)]) -> Value {
    json!({"command":"object.frames.clipboard.duplicate","layerUid":layer,
        "selection":selection_json(cells)})
}
fn stroke_cell(rel_layer: u32, rel_frame: u32, id: &str, z: i64, g: i64) -> PasteCell {
    let stroke = super::build::Stroke {
        id: id.to_owned(),
        z,
        g,
    };
    PasteCell {
        items: Default::default(),
        order_elements: crate::object_document::Presence::Missing,
        rel_layer,
        rel_frame,
        keyframe: true,
        records: vec![record_of(&stroke)],
    }
}
fn paste_of(
    o: &NativeObjectHistory,
    layer: &str,
    frame: u32,
    cells: Vec<PasteCell>,
) -> PasteRequest {
    PasteRequest {
        instance_id: o.instance_id().to_owned(),
        document_id: o.document_id().to_owned(),
        anchor_layer: layer.to_owned(),
        anchor_frame: frame,
        cells,
    }
}
fn paste(o: &mut NativeObjectHistory, request: PasteRequest) -> Result<u64, DispatchErrorCode> {
    let candidate = o.prepare_frame_clipboard(&FrameClipboardOperation::Paste(request))?;
    Ok(o.commit_frame_clipboard(candidate)?.revision)
}

#[test]
fn duplicate_selection_is_copy_plus_paste_on_the_active_layer_after_the_selection() {
    let cells = [("L0", 0), ("L0", 1)];
    let mut o = owner(doc(SMALL));
    let before = bytes(&o);
    let submitted = request(&o, "dup", OP_DUPLICATE, duplicate_payload("L1", &cells));
    let reply = dispatch(&mut o, &submitted);
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(
        applied(&reply),
        json!({"applied":true,"historyEntriesAdded":1})
    );
    // The key copies onto L1 at frame 2 (selection bounds end at 1), the held cell clears the blank key at frame 3.
    assert_eq!(layer_text(&o, 1), "K;.;K=a.1.10;.");
    assert_eq!(layer_text(&o, 0), "K=a.1.10;.;K=b.2.20;.");
    assert_eq!(exposure_string(head(&o).document(), 1), "*0*2");
    // The same bytes as an explicit copy followed by a paste at (L1, 2), the baseline's own definition.
    let mut manual = owner(doc(SMALL));
    let packet = manual.copy_frame_clipboard(&refs(&cells)).unwrap();
    assert_eq!(packet.origin(), (0, 0));
    assert_eq!(packet.extent(), (1, 2));
    let copy_paste = paste_of(&manual, "L1", 2, packet_paste_cells(&packet));
    paste(&mut manual, copy_paste).unwrap();
    assert_eq!(bytes(&manual), bytes(&o));
    o.undo(1).unwrap();
    assert_eq!(bytes(&o), before);
}

#[test]
fn replay_never_executes_twice_even_after_undo() {
    let mut o = owner(doc(SMALL));
    let dup = request(
        &o,
        "dup",
        OP_DUPLICATE,
        duplicate_payload("L1", &[("L0", 0), ("L0", 1)]),
    );
    let first = dispatch(&mut o, &dup);
    assert_eq!(first["ok"], true);
    let after = state(&o);
    assert_eq!(dispatch(&mut o, &dup), first);
    assert_eq!(
        state(&o),
        after,
        "an exact replay returns the receipt and executes nothing"
    );
    let mut altered = dup.clone();
    altered["payload"]["layerUid"] = json!("L0");
    assert_eq!(
        dispatch(&mut o, &altered)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), after);
    o.undo(1).unwrap();
    let undone = state(&o);
    assert_eq!(
        dispatch(&mut o, &dup),
        first,
        "replay cannot execute after undo"
    );
    assert_eq!(state(&o), undone);
    assert_eq!(o.history_depths(), (0, 1));
}

#[test]
fn a_paste_through_dispatch_keeps_the_original_stroke_id_on_another_layer_and_replays_once() {
    let mut o = owner(doc(SMALL));
    let small = owner(doc(SMALL));
    let packet = small.copy_frame_clipboard(&refs(&[("L0", 0)])).unwrap();
    let payload = paste_payload(&packet, "L1", 1);
    // Foreign identities: the packet came from `small`, a different document of the same instance string.
    assert_eq!(payload["documentId"], json!(small.document_id()));
    let foreign = request(&o, "paste", OP_PASTE, payload.clone());
    let refusal = dispatch(&mut o, &foreign);
    assert_eq!(refusal["error"]["code"], "wrong_document", "{refusal}");
    assert_eq!(o.content_revision(), 0);
    // The same packet with this document's identity is accepted.
    let mut own = payload;
    own["documentId"] = json!(o.document_id());
    let submitted = request(&o, "paste-own", OP_PASTE, own);
    let reply = dispatch(&mut o, &submitted);
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(
        applied(&reply),
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(
        layer_text(&o, 1),
        "K;K=a.1.10;.;K",
        "the original id on another layer"
    );
    assert_eq!(layer_text(&o, 0), "K=a.1.10;.;K=b.2.20;.");
    let after = state(&o);
    assert_eq!(dispatch(&mut o, &submitted), reply);
    assert_eq!(state(&o), after);
}

#[test]
fn a_no_op_adds_no_history_publishes_nothing_and_keeps_redo() {
    let mut o = owner(doc(SMALL));
    let first = request(
        &o,
        "edit",
        OP_DUPLICATE,
        duplicate_payload("L1", &[("L0", 0)]),
    );
    assert_eq!(dispatch(&mut o, &first)["ok"], true);
    o.undo(1).unwrap();
    assert_eq!(o.history_depths(), (0, 1));
    let key_a = stroke_cell(0, 0, "a", 1, 10);
    let typed: Vec<PasteRequest> = vec![
        // exactly what the target cell already holds
        paste_of(&o, "L0", 0, vec![key_a.clone()]),
        // every cell outside the layers or the frames
        paste_of(
            &o,
            "L1",
            3,
            vec![stroke_cell(5, 0, "z", 1, 1), stroke_cell(0, 1, "z", 1, 1)],
        ),
        // an empty clipboard
        paste_of(&o, "L0", 0, Vec::new()),
    ];
    for (index, one) in typed.into_iter().enumerate() {
        let before = state(&o);
        assert_eq!(paste(&mut o, one), Ok(before.0), "paste no-op {index}");
        assert_eq!(
            state(&o),
            before,
            "paste no-op {index}: nothing published, redo kept"
        );
    }
    let hold_cut = refs(&[("L0", 1), ("L0", 3)]);
    let before = state(&o);
    let packet = o
        .cut_frame_cells(&hold_cut)
        .unwrap()
        .expect("the packet is returned");
    assert_eq!(packet.cells().len(), 2);
    assert_eq!(state(&o), before, "a cut that blanks nothing keeps redo");
    let empty = request(&o, "empty", OP_DUPLICATE, duplicate_payload("L1", &[]));
    let reply = dispatch(&mut o, &empty);
    assert_eq!(
        applied(&reply),
        json!({"applied":false,"historyEntriesAdded":0}),
        "{reply}"
    );
    assert_eq!(state(&o), before);
    let revision = o.content_revision();
    o.redo(revision).unwrap();
    assert_eq!(
        layer_text(&o, 1),
        "K;K=a.1.10;.;K",
        "redo still reproduces the duplicate"
    );
}

#[test]
fn a_changed_edit_is_one_history_entry_and_clears_redo() {
    let mut o = owner(doc(SMALL));
    let cells = [("L0", 0), ("L0", 1), ("L0", 2)];
    let first = request(&o, "first", OP_DUPLICATE, duplicate_payload("L1", &cells));
    assert_eq!(dispatch(&mut o, &first)["ok"], true);
    assert_eq!(o.history_depths(), (1, 0), "a multi-cell edit is ONE entry");
    o.undo(1).unwrap();
    assert_eq!(o.history_depths(), (0, 1));
    let fresh = paste_of(&o, "L1", 1, vec![stroke_cell(0, 0, "n", 4, 40)]);
    paste(&mut o, fresh).unwrap();
    assert_eq!(o.history_depths(), (1, 0), "a changed edit clears redo");
    assert_eq!(layer_text(&o, 1), "K;K=n.4.40;.;K");
}

#[test]
fn preflight_and_payload_refusals_publish_nothing_and_are_retained() {
    let mut o = owner(doc(SMALL));
    let before = state(&o);
    let good = duplicate_payload("L1", &[("L0", 0)]);
    let cases = [
        ("/instanceId", json!("other"), "wrong_instance"),
        ("/documentId", json!("other"), "wrong_document"),
        ("/expectedRevision", json!(7), "stale_revision"),
        ("/apiVersion", json!(1), "invalid_request"),
        ("/payload", json!([]), "invalid_request"),
        (
            "/payload/command",
            json!("object.frames.clipboard.paste"),
            "invalid_request",
        ),
        ("/payload/extra", json!(true), "invalid_request"),
        ("/payload/frame", json!(1), "invalid_request"),
        ("/payload/layerUid", json!("nope"), "not_found"),
        (
            "/payload/selection",
            selection_json(&[("L0", 9)]),
            "invalid_request",
        ),
        (
            "/payload/selection",
            selection_json(&[("zz", 0)]),
            "not_found",
        ),
        (
            "/payload/selection",
            selection_json(&[("L0", 0), ("L0", 0)]),
            "invalid_request",
        ),
    ];
    for (index, (pointer, value, code)) in cases.into_iter().enumerate() {
        let mut submitted = request(
            &o,
            &format!("preflight-{index}"),
            OP_DUPLICATE,
            good.clone(),
        );
        if let Some(key) = pointer.strip_prefix("/payload/") {
            submitted["payload"][key] = value;
        } else {
            *submitted.pointer_mut(pointer).unwrap() = value;
        }
        let first = dispatch(&mut o, &submitted);
        assert_eq!(first["error"]["code"], code, "{pointer}: {first}");
        assert_eq!(state(&o), before, "{pointer}");
        assert_eq!(
            dispatch(&mut o, &submitted),
            first,
            "{pointer}: the refusal is retained"
        );
    }
    let mut cancelled = request(&o, "cancelled", OP_DUPLICATE, good.clone());
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        dispatch(&mut o, &cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    let mut missing = request(&o, "missing", OP_DUPLICATE, good.clone());
    missing.as_object_mut().unwrap().remove("expectedRevision");
    assert_eq!(
        dispatch(&mut o, &missing)["error"]["code"],
        "invalid_request"
    );
    let mismatched = request(&o, "mismatch", OP_PASTE, good);
    assert_eq!(
        dispatch(&mut o, &mismatched)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), before);
}

#[test]
fn prepared_candidates_are_sealed_to_their_owner_and_snapshot() {
    let duplicate = FrameClipboardOperation::Duplicate {
        selection: refs(&[("L0", 0)]),
        anchor_layer: "L1".to_owned(),
    };
    let mut source = owner(doc(SMALL));
    let candidate = source.prepare_frame_clipboard(&duplicate).unwrap();
    assert_eq!(candidate.command(), "object.frames.clipboard.duplicate");
    let mut other = owner(doc(SMALL));
    let before = state(&other);
    assert_eq!(
        other.commit_frame_clipboard(candidate).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(state(&other), before);
    let wrong_instance = source.prepare_frame_clipboard(&duplicate).unwrap();
    let mut foreign = NativeObjectHistory::new("foreign", doc(SMALL)).unwrap();
    assert_eq!(
        foreign.commit_frame_clipboard(wrong_instance).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(foreign.content_revision(), 0);
    let first = source.prepare_frame_clipboard(&duplicate).unwrap();
    let second = source.prepare_frame_clipboard(&duplicate).unwrap();
    assert_eq!(source.commit_frame_clipboard(first).unwrap().revision, 1);
    let after = state(&source);
    assert_eq!(
        source.commit_frame_clipboard(second).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(state(&source), after);
    assert_eq!(source.history_depths(), (1, 0));
}

#[test]
fn cut_blanks_the_unlocked_cells_in_one_entry_and_returns_the_pre_edit_packet() {
    let mut o = owner(doc(SMALL));
    let before = bytes(&o);
    let cells = refs(&[("L0", 0), ("L0", 2)]);
    let copied = o.copy_frame_clipboard(&cells).unwrap();
    assert_eq!(state(&o).0, 0, "copy publishes nothing");
    let cut = o
        .cut_frame_cells(&cells)
        .unwrap()
        .expect("the packet is returned");
    assert_eq!(
        cut, copied,
        "the cut packet is the copy of the pre-edit document"
    );
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(layer_text(&o, 0), ".;.;.;.");
    assert_eq!(exposure_string(head(&o).document(), 0), "----");
    assert_eq!(
        cut.cells()[1].records().len(),
        1,
        "the packet still holds the cut strokes"
    );
    o.undo(1).unwrap();
    assert_eq!(bytes(&o), before);
}

#[test]
fn a_foreign_packet_is_refused_by_instance_and_by_document_and_publishes_nothing() {
    let mut a = owner(doc(SMALL));
    let packet = a.copy_frame_clipboard(&refs(&[("L0", 0)])).unwrap();
    let cells = packet_paste_cells(&packet);
    let mut same_instance = owner(doc(SMALL));
    let before = state(&same_instance);
    let from_a = PasteRequest {
        instance_id: packet.instance_id().to_owned(),
        document_id: packet.document_id().to_owned(),
        anchor_layer: "L1".to_owned(),
        anchor_frame: 1,
        cells: cells.clone(),
    };
    assert_eq!(
        paste(&mut same_instance, from_a.clone()),
        Err(DispatchErrorCode::WrongDocument)
    );
    let mut other_instance = NativeObjectHistory::new("foreign", doc(SMALL)).unwrap();
    assert_eq!(
        paste(&mut other_instance, from_a.clone()),
        Err(DispatchErrorCode::WrongInstance)
    );
    assert_eq!(state(&same_instance), before);
    assert_eq!(other_instance.content_revision(), 0);
    assert!(
        paste(&mut a, from_a).is_ok(),
        "the owner of the packet accepts it"
    );
}

#[test]
fn a_chain_pastes_a_packet_copied_at_an_earlier_revision_and_reopens_at_every_step() {
    let mut o = owner(doc(SMALL));
    let early = o
        .copy_frame_clipboard(&refs(&[("L0", 0), ("L0", 1), ("L0", 2)]))
        .unwrap();
    assert_eq!(early.content_revision(), 0);
    let mut saved = vec![bytes(&o)];
    // revision 1: duplicate the blank key of L1 onto frame 1
    let step = request(
        &o,
        "s1",
        OP_DUPLICATE,
        duplicate_payload("L1", &[("L1", 0)]),
    );
    assert_eq!(dispatch(&mut o, &step)["ok"], true);
    assert_eq!(layer_text(&o, 1), "K;K;.;K");
    saved.push(bytes(&o));
    // revision 2: the packet copied at revision 0 still pastes (a clipboard outlives the revision it came from)
    let stale_pin = paste_of(&o, "L1", 0, packet_paste_cells(&early));
    let request_from_early = PasteRequest {
        instance_id: early.instance_id().to_owned(),
        document_id: early.document_id().to_owned(),
        ..stale_pin
    };
    assert_eq!(paste(&mut o, request_from_early), Ok(2));
    assert_eq!(layer_text(&o, 1), "K=a.1.10;.;K=b.2.20;K");
    assert_eq!(exposure_string(head(&o).document(), 1), "*0**");
    saved.push(bytes(&o));
    for (index, snapshot) in saved.iter().enumerate().skip(1) {
        assert!(
            decode_storage_project(snapshot).is_ok(),
            "step {index} reopens"
        );
    }
    assert_eq!(o.history_depths(), (2, 0));
    for back in (0..2).rev() {
        o.undo(o.content_revision()).unwrap();
        assert_eq!(bytes(&o), saved[back], "undo to step {back}");
    }
    for forward in 1..=2 {
        o.redo(o.content_revision()).unwrap();
        assert_eq!(bytes(&o), saved[forward], "redo to step {forward}");
    }
}
