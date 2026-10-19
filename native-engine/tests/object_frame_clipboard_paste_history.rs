//! Frame-clipboard transactions through the real history owner: every oracle row (oracle.txt; the verbatim baseline
//! editor at fbe4b8a3) is rebuilt in two native document forms, applied as the operation the baseline function stands
//! for (typed paste, typed cut, dispatched duplicate selection), compared back to the baseline, undone to the original
//! bytes, redone and reopened from storage. Ordered V2/V2Extras retain explicit authored elements. Hand-computed cases, replay, no-op/redo,
//! preflight, sealing and a chain of edits are in object_frame_clipboard_paste_edges.rs; negative controls in
//! object_frame_clipboard_paste_controls.rs.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
pub(crate) mod build;
#[path = "object_frame_clipboard_paste_controls.rs"]
mod controls;
#[path = "object_frame_clipboard_cut_dispatch.rs"]
mod cut_dispatch;
#[path = "object_frame_clipboard_paste_edges.rs"]
mod edges;
#[cfg(feature = "evaluation")]
#[path = "object_frame_clipboard_paste_exposure.rs"]
mod exposure;
#[path = "object_frame_clipboard_paste_rows.rs"]
pub(crate) mod rows;
#[path = "object_frame_clipboard_paste_apply.rs"]
pub(crate) mod runner;

use build::{document, Build};
use rows::{compare, packet_matches, rows, Row, FORMS};
use runner::{apply, Mistake};

pub(crate) fn owner(document: ObjectDocument) -> NativeObjectHistory {
    NativeObjectHistory::new("frame-clipboard", document).unwrap()
}
pub(crate) fn head(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
pub(crate) fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(head(o).document()).unwrap()
}
pub(crate) fn state(o: &NativeObjectHistory) -> (u64, (usize, usize), String, Vec<u8>) {
    (
        o.content_revision(),
        o.history_depths(),
        head(o).snapshot_id().into(),
        bytes(o),
    )
}
pub(crate) fn request(o: &NativeObjectHistory, id: &str, operation: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":operation,"payload":payload})
}
pub(crate) fn dispatch(o: &mut NativeObjectHistory, request: &Value) -> Value {
    let typed = serde_json::from_value(request.clone()).unwrap();
    serde_json::to_value(o.try_dispatch_object_command(&typed).unwrap().unwrap()).unwrap()
}

/// The locked variant of a row's input (`locked: true` authored on the marked layers; F06 admits it).
fn locked_input(row: &Row, form: Build) -> ObjectDocument {
    let mut value = build::document_json(&row.input, form);
    for (index, locked) in row.locked.iter().enumerate() {
        if *locked {
            value["layers"][index]["locked"] = json!(true);
        }
    }
    decode_storage_project(value.to_string().as_bytes()).expect("F06 admits locked layers")
}

/// One row through the history owner: applied, compared with the baseline, undone to the original bytes, redone and
/// reopened. The same checks for the unlocked and the locked rows.
fn run_row(row: &Row, form: Build, input: ObjectDocument) {
    let who = format!("{} {form:?}", row.label());
    let mut o = owner(input.clone());
    let before = bytes(&o);
    let packet = apply(row, &mut o, Mistake::None).unwrap_or_else(|error| panic!("{who}: {error}"));
    let out = head(&o).document().clone();
    if let Err(message) = compare(row, &input, &out) {
        panic!("{who}: {message}");
    }
    if row.op == "cut" {
        let packet = packet.expect("a cut returns its packet");
        if let Err(message) = packet_matches(row, &packet) {
            panic!("{who}: {message}");
        }
    }
    if row.chg {
        assert_eq!(
            (o.content_revision(), o.history_depths()),
            (1, (1, 0)),
            "{who}"
        );
        assert_ne!(bytes(&o), before, "{who}");
        let undone = o.undo(1).unwrap();
        assert_eq!(
            bytes(&o),
            before,
            "{who}: one Undo restores the complete bytes"
        );
        assert_eq!(o.history_depths(), (0, 1), "{who}");
        o.redo(undone).unwrap();
        assert_eq!(*head(&o).document(), out, "{who}: Redo reproduces the edit");
    } else {
        assert_eq!(
            (o.content_revision(), o.history_depths()),
            (0, (0, 0)),
            "{who}: a no-op publishes nothing"
        );
        assert_eq!(bytes(&o), before, "{who}");
    }
    assert_eq!(
        decode_storage_project(&bytes(&o)).unwrap(),
        *head(&o).document(),
        "{who}: save and reopen"
    );
}

#[test]
fn every_oracle_row_matches_through_the_history_owner_undo_redo_and_reopen() {
    let free: Vec<Row> = rows().into_iter().filter(|row| !row.any_locked()).collect();
    assert_eq!(free.len(), 323);
    for row in &free {
        for form in FORMS {
            run_row(row, form, document(&row.input, form));
        }
    }
}

/// The baseline skips a locked layer on paste and keeps it in the cut packet without blanking it. The 170 locked rows
/// execute through the history owner on the batch-5 head, where the codec admits `locked: true` (F06).
#[test]
fn locked_rows_follow_the_baseline_policy_once_the_codec_admits_locked_layers() {
    let locked: Vec<Row> = rows().into_iter().filter(|row| row.any_locked()).collect();
    assert_eq!(locked.len(), 170);
    for row in &locked {
        for form in FORMS {
            run_row(row, form, locked_input(row, form));
        }
    }
}

#[test]
fn oracle_reaches_every_branch() {
    let all = rows();
    let count = |op: &str| all.iter().filter(|row| row.op == op).count();
    assert_eq!(
        (count("paste"), count("cut"), count("dupsel")),
        (223, 130, 140)
    );
    let changed = |op: &str| all.iter().filter(|row| row.op == op && row.chg).count();
    assert_eq!(
        (changed("paste"), changed("cut"), changed("dupsel")),
        (155, 86, 80)
    );
    let (mut empty, mut dropped, mut all_dropped, mut multi, mut multi_changed) = (0, 0, 0, 0, 0);
    let (mut in_between, mut plain, mut blank_key) = (0, 0, 0);
    let (mut cut_locked, mut dup_not_min, mut dup_past_end, mut dup_single) = (0, 0, 0, 0);
    for row in &all {
        let rows_n = row.input.len() as u32;
        match row.op.as_str() {
            "paste" => {
                let (bl, bf) = row.paste_base();
                let outside = |rl: u32, rf: u32| bl as u32 + rl >= rows_n || bf + rf >= row.total;
                empty += usize::from(row.clip.is_empty());
                dropped += usize::from(row.clip.iter().any(|e| outside(e.rl, e.rf)));
                all_dropped += usize::from(
                    !row.clip.is_empty() && row.clip.iter().all(|e| outside(e.rl, e.rf)),
                );
                let layered = row.clip.iter().any(|e| e.rl > 0);
                multi += usize::from(layered);
                multi_changed += usize::from(layered && row.chg);
                in_between += usize::from(row.clip.iter().any(|e| "IMN".contains(e.cell.flag)));
                plain += usize::from(row.clip.iter().any(|e| e.cell.flag == '.'));
                blank_key += usize::from(
                    row.clip
                        .iter()
                        .any(|e| e.cell.flag == 'K' && e.cell.strokes.is_empty()),
                );
            }
            "cut" => cut_locked += usize::from(row.selection().iter().any(|c| row.locked[c.0])),
            _ => {
                let last = row.selection().iter().map(|c| c.1).max().unwrap();
                dup_past_end += usize::from(last + 1 >= row.total);
                dup_not_min += usize::from(row.dup_anchor() != row.min_selected_layer());
                dup_single += usize::from(row.selection().len() == 1);
            }
        }
    }
    assert_eq!((empty, dropped, all_dropped), (3, 99, 6));
    assert_eq!((multi, multi_changed), (78, 49));
    assert_eq!((in_between, plain, blank_key), (147, 134, 42));
    assert_eq!(cut_locked, 45);
    assert_eq!((dup_not_min, dup_past_end, dup_single), (52, 24, 50));
}

#[test]
fn ordered_documents_copy_duplicate_cut_and_paste_preserve_authored_dtos_atomically() {
    use crate::commands::{packet_paste_cells, CellRef, FrameClipboardOperation, PasteRequest};
    for form in [Build::V2, Build::V2Extras] {
        let input = rows::ordered_document(form);
        let mut o = owner(input.clone());
        let before = bytes(&o);
        let selected = vec![CellRef {
            layer_uid: "L0".into(),
            frame: 0,
        }];
        let packet = o.copy_frame_clipboard(&selected).unwrap();
        assert_eq!(
            serde_json::to_value(packet.cells()[0].order_elements().value().unwrap()).unwrap(),
            rows::ordered_elements("L0")
        );
        let submitted = request(
            &o,
            "ordered-dup",
            rows::OP_DUPLICATE,
            json!({"command":"object.frames.clipboard.duplicate","layerUid":"L1",
                "selection":[{"layerUid":"L0","frame":0}]}),
        );
        let reply = dispatch(&mut o, &submitted);
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(
            rows::scoped_elements(head(&o).document(), "L1", 1),
            rows::ordered_elements("L1")
        );
        assert_eq!(o.history_depths(), (1, 0));
        let after = state(&o);
        assert_eq!(dispatch(&mut o, &submitted), reply);
        assert_eq!(state(&o), after);
        assert_eq!(
            build::untouched(&input),
            build::untouched(head(&o).document())
        );
        let saved = bytes(&o);
        assert_eq!(
            encode_storage_project(&decode_storage_project(&saved).unwrap()).unwrap(),
            saved
        );
        o.undo(1).unwrap();
        assert_eq!(bytes(&o), before);
        o.redo(o.content_revision()).unwrap();
        assert_eq!(bytes(&o), saved);
        let cut = o.cut_frame_cells(&selected).unwrap().unwrap();
        assert_eq!(cut.cells(), packet.cells());
        assert_eq!(
            rows::scoped_elements(head(&o).document(), "L0", 0),
            json!([])
        );
        let operation = FrameClipboardOperation::Paste(PasteRequest {
            instance_id: o.instance_id().into(),
            document_id: o.document_id().into(),
            anchor_layer: "L0".into(),
            anchor_frame: 0,
            cells: packet_paste_cells(&cut),
        });
        let candidate = o.prepare_frame_clipboard(&operation).unwrap();
        o.commit_frame_clipboard(candidate).unwrap();
        let restored = head(&o).document().clone();
        assert_eq!(
            rows::scoped_elements(&restored, "L0", 0),
            rows::ordered_elements("L0")
        );
        assert_eq!(
            rows::scoped_elements(&restored, "L1", 1),
            rows::ordered_elements("L1")
        );
        assert_eq!(build::render_layer(&restored, 0), "K=a.1.10,b.2.20;.;.;.");
        assert_eq!(
            build::render_layer(&restored, 1),
            "K=c.3.30;K=a.1.10,b.2.20;.;."
        );
        assert_eq!(build::untouched(&restored), build::untouched(&input));
        let restored_bytes = bytes(&o);
        assert_eq!(
            encode_storage_project(&decode_storage_project(&restored_bytes).unwrap()).unwrap(),
            restored_bytes
        );
        assert_eq!(o.history_depths(), (3, 0));
        let candidate = o.prepare_frame_clipboard(&operation).unwrap();
        let stable = state(&o);
        o.commit_frame_clipboard(candidate).unwrap();
        assert_eq!(state(&o), stable, "ordered exact paste is a semantic no-op");
    }
}
