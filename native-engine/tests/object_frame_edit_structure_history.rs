//! Frame-structure transactions through the real history owner: every oracle row (L05-B documents.txt insert-frame
//! and remove rows, this leaf's extend and duplicate rows; the verbatim baseline editor at fbe4b8a3) is rebuilt in
//! four native document forms, dispatched as ONE request, compared back to the baseline, undone to the original
//! bytes, redone and reopened from storage. Hand-computed cases cover replay, no-op/redo, preflight refusals,
//! sealed candidates and a chain of edits. Negative controls: object_frame_edit_structure_controls.rs.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
pub(crate) mod build;
#[path = "object_frame_edit_structure_controls.rs"]
mod controls;
#[cfg(feature = "evaluation")]
#[path = "object_frame_edit_structure_exposure.rs"]
mod exposure;
#[path = "object_frame_edit_structure_rows.rs"]
pub(crate) mod rows;

use build::{document, exposure_string, render_layer, Build};
use rows::{compare, parse_layers, rows, OP_DUPLICATE, OP_EXTEND, OP_INSERT, OP_REMOVE};

pub(crate) fn owner(document: ObjectDocument) -> NativeObjectHistory {
    NativeObjectHistory::new("frame-structure", document).unwrap()
}
pub(crate) fn head(o: &NativeObjectHistory) -> ObjectSnapshot {
    o.acquire_snapshot(o.content_revision()).unwrap()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(head(o).document()).unwrap()
}
fn state(o: &NativeObjectHistory) -> (u64, (usize, usize), String, Vec<u8>) {
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
fn doc(text: &str) -> ObjectDocument {
    document(&parse_layers(text), Build::V1)
}
fn layer_text(o: &NativeObjectHistory, layer: usize) -> String {
    render_layer(head(o).document(), layer)
}
fn applied(reply: &Value) -> Value {
    json!({"applied":reply["result"]["applied"],"historyEntriesAdded":reply["result"]["historyEntriesAdded"]})
}
/// Two layers, four frames: L0 has a stroke at each key, L1 is a bare key track.
const SMALL: &str = "K=a.1.10;.;K=b.2.20;.|K;.;.;K";

#[test]
fn every_oracle_row_matches_through_dispatch_undo_redo_and_reopen() {
    let all = rows();
    assert_eq!(all.len(), 890);
    for row in &all {
        for form in Build::ALL {
            let input = document(&row.input, form);
            let mut o = owner(input.clone());
            let before = bytes(&o);
            let submitted = request(&o, "row", row.operation(), row.payload());
            let reply = dispatch(&mut o, &submitted);
            let who = format!("{} {form:?}", row.label());
            assert_eq!(reply["ok"], true, "{who}: {reply}");
            assert_eq!(
                applied(&reply),
                json!({"applied":row.chg,"historyEntriesAdded":u64::from(row.chg)}),
                "{who}"
            );
            let out = head(&o).document().clone();
            if let Err(message) = compare(row, form, &input, &out) {
                panic!("{who}: {message}");
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
                    "{who}"
                );
                assert_eq!(bytes(&o), before, "{who}");
            }
            assert_eq!(
                decode_storage_project(&bytes(&o)).unwrap(),
                *head(&o).document(),
                "{who}: save and reopen"
            );
        }
    }
}

#[test]
fn oracle_reaches_every_branch() {
    let (mut gap_all, mut gap_part, mut rm_changed, mut rm_refused) = (0, 0, 0, 0);
    let (mut n1, mut n2, mut n3) = (0, 0, 0);
    let (mut key, mut held, mut sample, mut unexposed, mut empty) = (0, 0, 0, 0, 0);
    for row in &rows() {
        match row.op.as_str() {
            "gap" if row.roles.contains('P') => gap_part += 1,
            "gap" => gap_all += 1,
            "rm" if row.chg => rm_changed += 1,
            "rm" => rm_refused += 1,
            "extend" => match row.args.split_once(':').unwrap().1 {
                "1" => n1 += 1,
                "2" => n2 += 1,
                _ => n3 += 1,
            },
            "dup" => {
                let cell: usize = row.args.parse().unwrap();
                let shown = exposure_string(&document(&row.input, Build::V1), 0)
                    .chars()
                    .nth(cell)
                    .unwrap();
                match (row.chg, shown) {
                    (false, '-') => unexposed += 1,
                    (false, _) => empty += 1,
                    (true, '*') => key += 1,
                    (true, '~' | '!') => sample += 1,
                    (true, _) => held += 1,
                }
            }
            other => panic!("unexpected op {other}"),
        }
    }
    assert_eq!(
        (gap_all, gap_part, rm_changed, rm_refused),
        (16, 223, 193, 43)
    );
    assert_eq!((n1, n2, n3), (99, 97, 35));
    assert_eq!((key, held, sample, unexposed, empty), (61, 37, 48, 29, 9));
}

#[test]
fn replay_never_executes_twice_even_after_undo() {
    let mut o = owner(doc(SMALL));
    let extend = json!({"command":"object.frames.extend","cell":0,"count":1});
    let req = request(&o, "extend", OP_EXTEND, extend);
    let first = dispatch(&mut o, &req);
    assert_eq!(first["ok"], true);
    assert_eq!(
        applied(&first),
        json!({"applied":true,"historyEntriesAdded":1})
    );
    let after = state(&o);
    assert_eq!(dispatch(&mut o, &req), first);
    assert_eq!(
        state(&o),
        after,
        "an exact replay returns the receipt and executes nothing"
    );
    let mut altered = req.clone();
    altered["payload"]["count"] = json!(2);
    assert_eq!(
        dispatch(&mut o, &altered)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), after);
    o.undo(1).unwrap();
    let undone = state(&o);
    assert_eq!(
        dispatch(&mut o, &req),
        first,
        "replay cannot execute after undo"
    );
    assert_eq!(state(&o), undone);
    assert_eq!(o.history_depths(), (0, 1));
}

#[test]
fn a_no_op_adds_no_history_publishes_nothing_and_keeps_redo() {
    let mut o = owner(doc("K=a.1.10;.;K=b.2.20;.|K;.;.;K|K;.;.;."));
    let first = request(
        &o,
        "edit",
        OP_EXTEND,
        json!({"command":"object.frames.extend","cell":0,"count":1}),
    );
    assert_eq!(dispatch(&mut o, &first)["ok"], true);
    o.undo(1).unwrap();
    assert_eq!(o.history_depths(), (0, 1));
    let total = head(&o).document().total_frames();
    let no_ops = [
        (
            OP_EXTEND,
            json!({"command":"object.frames.extend","cell":1,"count":0}),
        ),
        (
            OP_REMOVE,
            json!({"command":"object.frames.remove","from":0,"to":total - 1}),
        ),
        (
            OP_DUPLICATE,
            json!({"command":"object.frames.duplicate","layerUid":"L1","cell":0}),
        ),
        (
            OP_DUPLICATE,
            json!({"command":"object.frames.duplicate","layerUid":"L2","cell":3}),
        ),
    ];
    for (index, (operation, payload)) in no_ops.into_iter().enumerate() {
        let before = state(&o);
        let submitted = request(&o, &format!("noop-{index}"), operation, payload);
        let reply = dispatch(&mut o, &submitted);
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(
            applied(&reply),
            json!({"applied":false,"historyEntriesAdded":0})
        );
        assert_eq!(
            state(&o),
            before,
            "no revision, no history entry, redo kept, bytes unchanged"
        );
        assert_eq!(o.history_depths(), (0, 1));
    }
}

#[test]
fn a_changed_edit_is_one_history_entry_and_clears_redo() {
    let mut o = owner(doc(SMALL));
    let three = request(
        &o,
        "three",
        OP_EXTEND,
        json!({"command":"object.frames.extend","cell":1,"count":3}),
    );
    assert_eq!(dispatch(&mut o, &three)["ok"], true);
    assert_eq!(
        o.history_depths(),
        (1, 0),
        "three inserted frames are ONE entry"
    );
    assert_eq!(head(&o).document().total_frames(), 7);
    o.undo(1).unwrap();
    assert_eq!(o.history_depths(), (0, 1));
    let insert = request(
        &o,
        "insert",
        OP_INSERT,
        json!({"command":"object.frames.insert","cell":0}),
    );
    assert_eq!(dispatch(&mut o, &insert)["ok"], true);
    assert_eq!(o.history_depths(), (1, 0), "a changed edit clears redo");
}

#[test]
fn preflight_and_payload_refusals_publish_nothing_and_are_retained() {
    let mut o = owner(doc(SMALL));
    let before = state(&o);
    let good = json!({"command":"object.frames.insert","cell":1});
    for (index, (pointer, value, code)) in [
        ("/instanceId", json!("other"), "wrong_instance"),
        ("/documentId", json!("other"), "wrong_document"),
        ("/expectedRevision", json!(7), "stale_revision"),
        ("/apiVersion", json!(1), "invalid_request"),
        ("/payload", json!([]), "invalid_request"),
        (
            "/payload/command",
            json!("object.frames.remove"),
            "invalid_request",
        ),
        ("/payload/extra", json!(true), "invalid_request"),
        ("/payload/cell", json!(4), "invalid_request"),
        ("/payload/layerUids", json!(["nope"]), "not_found"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut submitted = request(&o, &format!("preflight-{index}"), OP_INSERT, good.clone());
        if pointer.starts_with("/payload/") {
            let key = &pointer["/payload/".len()..];
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
    let mut cancelled = request(&o, "cancelled", OP_INSERT, good.clone());
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        dispatch(&mut o, &cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    let mut missing = request(&o, "missing", OP_INSERT, good.clone());
    missing.as_object_mut().unwrap().remove("expectedRevision");
    assert_eq!(
        dispatch(&mut o, &missing)["error"]["code"],
        "invalid_request"
    );
    let mismatched = request(&o, "mismatch", OP_REMOVE, good);
    assert_eq!(
        dispatch(&mut o, &mismatched)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), before);
}

#[test]
fn prepared_candidates_are_sealed_to_their_owner_and_snapshot() {
    let payload = json!({"command":"object.frames.extend","cell":0,"count":1}).to_string();
    let mut source = owner(doc(SMALL));
    let foreign_candidate = source
        .prepare_frame_structure_json(payload.as_bytes())
        .unwrap();
    assert_eq!(foreign_candidate.command(), "object.frames.extend");
    let mut other = owner(doc(SMALL));
    let before = state(&other);
    assert_eq!(
        other.commit_frame_structure(foreign_candidate).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(state(&other), before);
    let wrong_instance = source
        .prepare_frame_structure_json(payload.as_bytes())
        .unwrap();
    let mut foreign = NativeObjectHistory::new("foreign", doc(SMALL)).unwrap();
    assert_eq!(
        foreign.commit_frame_structure(wrong_instance).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(foreign.content_revision(), 0);
    let first = source
        .prepare_frame_structure_json(payload.as_bytes())
        .unwrap();
    let second = source
        .prepare_frame_structure_json(payload.as_bytes())
        .unwrap();
    assert_eq!(source.commit_frame_structure(first), Ok(1));
    let after = state(&source);
    assert_eq!(
        source.commit_frame_structure(second).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(state(&source), after);
    assert_eq!(source.history_depths(), (1, 0));
}

#[test]
fn a_chain_of_edits_reopens_at_every_step_and_undoes_back_to_the_original_bytes() {
    let mut o = owner(doc(SMALL));
    let steps = [
        (
            OP_INSERT,
            json!({"command":"object.frames.insert","cell":1}),
        ),
        (
            OP_EXTEND,
            json!({"command":"object.frames.extend","cell":0,"count":2}),
        ),
        (
            OP_DUPLICATE,
            json!({"command":"object.frames.duplicate","layerUid":"L0","cell":0}),
        ),
        (
            OP_REMOVE,
            json!({"command":"object.frames.remove","from":1,"to":2}),
        ),
    ];
    let mut saved = vec![bytes(&o)];
    for (index, (operation, payload)) in steps.into_iter().enumerate() {
        let submitted = request(&o, &format!("step-{index}"), operation, payload);
        assert_eq!(dispatch(&mut o, &submitted)["ok"], true, "step {index}");
        assert_eq!(
            decode_storage_project(&bytes(&o)).unwrap(),
            *head(&o).document(),
            "step {index}: save and reopen"
        );
        saved.push(bytes(&o));
    }
    assert_eq!(o.history_depths(), (4, 0));
    assert_eq!(head(&o).document().total_frames(), 6);
    // insert@1: K a|.|.|K b|.  extend@0 x2 adds two holds  duplicate L0@0 adds a key copy  remove 1..2 drops the copy again.
    assert_eq!(layer_text(&o, 0), "K=a.1.10;.;.;.;K=b.2.20;.");
    assert_eq!(layer_text(&o, 1), "K;.;.;.;.;K");
    for back in (0..4).rev() {
        o.undo(o.content_revision()).unwrap();
        assert_eq!(bytes(&o), saved[back], "undo to step {back}");
    }
    for forward in 1..=4 {
        o.redo(o.content_revision()).unwrap();
        assert_eq!(bytes(&o), saved[forward], "redo to step {forward}");
    }
    assert_eq!(exposure_string(head(&o).document(), 0), "*000*4");
}
