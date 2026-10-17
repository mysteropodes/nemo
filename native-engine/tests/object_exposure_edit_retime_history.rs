//! Key-retime transaction through the real history owner: every oracle row (retime.txt, the verbatim baseline
//! `moveKeyframe` with its tween-span retime at fbe4b8a3) is rebuilt in four native document forms, dispatched as ONE
//! request, compared back to the baseline (cells, strokes, exposure, shown strokes, v2 order elements, the landing frame
//! and the key pairs the baseline handed the arc owner), undone to the original bytes, redone and reopened from
//! storage. Hand-computed cases cover replay, no-op/redo, preflight refusals, sealed candidates and a chain of moves.
//! Negative controls: object_exposure_edit_retime_controls.rs. Rows that need a locked layer need the accepted F06
//! admission (2435379d).
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
pub(crate) mod build;
#[path = "object_exposure_edit_retime_controls.rs"]
mod controls;
#[cfg(feature = "evaluation")]
#[path = "object_exposure_edit_retime_exposure.rs"]
mod exposure;
#[path = "object_exposure_edit_retime_reach.rs"]
mod reach;
#[path = "object_exposure_edit_retime_rows.rs"]
pub(crate) mod rows;

use build::{document, exposure_string, render_layer, Build};
use rows::{compare, input_document, parse_layers, rows, Row, OP_MOVE};

pub(crate) fn owner(document: ObjectDocument) -> NativeObjectHistory {
    NativeObjectHistory::new("exposure-retime", document).unwrap()
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
pub(crate) fn request(o: &NativeObjectHistory, id: &str, payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),
        "documentId":o.document_id(),"expectedRevision":o.content_revision(),
        "operation":OP_MOVE,"payload":payload})
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
fn mv(layer: &str, from: u32, to: u32) -> Value {
    json!({"command":"object.layer.key.move","layerUid":layer,"from":from,"to":to})
}
fn code_of(text: &str) -> DispatchErrorCode {
    match text {
        "unavailable" => DispatchErrorCode::Unavailable,
        "invalid_request" => DispatchErrorCode::InvalidRequest,
        other => panic!("unexpected refusal code {other}"),
    }
}
/// Two layers, five frames: L0 has a stroke at each of its two keys, L1 has keys at 0 and 3.
const SMALL: &str = "K=a.1.10;.;K=b.2.20;.;.|K;.;.;K;.";

/// One oracle row in one form: the prepared numbers, dispatch, the typed outcome, the baseline comparison, undo / redo / reopen.
fn run_row(row: &Row, form: Build) {
    let input = input_document(row, form);
    let mut o = owner(input.clone());
    let before = bytes(&o);
    let who = format!("{} {form:?}", row.label());
    let prepared = o.prepare_exposure_retime_json(row.payload().to_string().as_bytes());
    match row.refusal() {
        Some(code) => assert_eq!(prepared.unwrap_err(), code_of(code), "{who}"),
        None => {
            let prepared = prepared.unwrap_or_else(|error| panic!("{who}: {error:?}"));
            let pairs: Vec<[u32; 4]> = prepared
                .pairs()
                .iter()
                .map(|pair| [pair.from_a, pair.from_b, pair.to_a, pair.to_b])
                .collect();
            if row.chg {
                assert_eq!(prepared.applied_to(), row.applied, "{who}: landing frame");
                assert_eq!(pairs, row.pairs, "{who}: arc owner pairs");
            } else {
                assert_eq!((prepared.applied_to(), pairs.len()), (None, 0), "{who}");
            }
        }
    }
    let submitted = request(&o, "row", row.payload());
    let reply = dispatch(&mut o, &submitted);
    if let Some(code) = row.refusal() {
        assert_eq!(reply["error"]["code"], code, "{who}: {reply}");
    } else {
        assert_eq!(reply["ok"], true, "{who}: {reply}");
        assert_eq!(
            applied(&reply),
            json!({"applied":row.chg,"historyEntriesAdded":u64::from(row.chg)}),
            "{who}"
        );
    }
    let out = head(&o).document().clone();
    if let Err(message) = compare(row, form, &input, &out) {
        panic!("{who}: {message}");
    }
    if row.chg {
        assert_eq!(row.outcome, "changed", "{who}");
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
        assert_eq!(bytes(&o), before, "{who}: nothing published");
    }
    assert_eq!(
        decode_storage_project(&bytes(&o)).unwrap(),
        *head(&o).document(),
        "{who}: save and reopen"
    );
}

#[test]
fn every_unlocked_oracle_row_matches_through_dispatch_undo_redo_and_reopen() {
    let all: Vec<Row> = rows().into_iter().filter(|row| !row.any_lock()).collect();
    assert_eq!(all.len(), 557);
    for row in &all {
        for form in Build::ALL {
            run_row(row, form);
        }
    }
}

#[test]
fn locked_oracle_rows_follow_the_baseline_lock_policy() {
    let all: Vec<Row> = rows().into_iter().filter(Row::any_lock).collect();
    assert_eq!(all.len(), 66);
    for row in &all {
        for form in Build::ALL {
            run_row(row, form);
        }
    }
}

#[test]
fn replay_never_executes_twice_even_after_undo() {
    let mut o = owner(doc(SMALL));
    let req = request(&o, "move", mv("L0", 0, 1));
    let first = dispatch(&mut o, &req);
    assert_eq!(first["ok"], true);
    assert_eq!(
        applied(&first),
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(layer_text(&o, 0), ".;K=a.1.10;.;K=b.2.20;.");
    let after = state(&o);
    assert_eq!(dispatch(&mut o, &req), first);
    assert_eq!(
        state(&o),
        after,
        "an exact replay returns the receipt and executes nothing"
    );
    let mut altered = req.clone();
    altered["payload"]["to"] = json!(2);
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
    let mut o = owner(doc("K=a.1.10;.;K=b.2.20;.;.|K;.;.;K;.|K;.;.;.;K"));
    let first = request(&o, "edit", mv("L0", 0, 1));
    assert_eq!(dispatch(&mut o, &first)["ok"], true);
    o.undo(1).unwrap();
    assert_eq!(o.history_depths(), (0, 1));
    // the same frame, and a drag whose ripple clamps to nothing (L2's only later key sits on the last frame)
    for (index, payload) in [mv("L0", 2, 2), mv("L1", 3, 3), mv("L2", 0, 3)]
        .into_iter()
        .enumerate()
    {
        let before = state(&o);
        let submitted = request(&o, &format!("noop-{index}"), payload);
        let reply = dispatch(&mut o, &submitted);
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(
            applied(&reply),
            json!({"applied":false,"historyEntriesAdded":0}),
            "{index}"
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
    let first = request(&o, "first", mv("L0", 0, 1));
    assert_eq!(dispatch(&mut o, &first)["ok"], true);
    o.undo(1).unwrap();
    assert_eq!(o.history_depths(), (0, 1));
    // the drag moves a key and ripples another: one request, ONE entry, and the redo it replaces is gone
    let second = request(&o, "second", mv("L0", 0, 2));
    assert_eq!(dispatch(&mut o, &second)["ok"], true);
    assert_eq!(o.history_depths(), (1, 0));
    assert_eq!(layer_text(&o, 0), ".;.;K=a.1.10;.;K=b.2.20");
    assert_eq!(
        layer_text(&o, 1),
        "K;.;.;K;.",
        "the other layer is untouched"
    );
    o.undo(o.content_revision()).unwrap();
    assert_eq!(layer_text(&o, 0), "K=a.1.10;.;K=b.2.20;.;.");
}

#[test]
fn preflight_and_payload_refusals_publish_nothing_and_are_retained() {
    let mut o = owner(doc(SMALL));
    let before = state(&o);
    let good = mv("L0", 0, 1);
    for (index, (pointer, value, code)) in [
        ("/instanceId", json!("other"), "wrong_instance"),
        ("/documentId", json!("other"), "wrong_document"),
        ("/expectedRevision", json!(7), "stale_revision"),
        ("/apiVersion", json!(1), "invalid_request"),
        ("/payload", json!([]), "invalid_request"),
        (
            "/payload/command",
            json!("object.layer.key.clear"),
            "invalid_request",
        ),
        ("/payload/extra", json!(true), "invalid_request"),
        ("/payload/to", json!(5), "invalid_request"),
        ("/payload/from", json!(1), "invalid_request"),
        ("/payload/to", json!(-1), "invalid_request"),
        ("/payload/layerUid", json!("nope"), "not_found"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut submitted = request(&o, &format!("preflight-{index}"), good.clone());
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
    let mut cancelled = request(&o, "cancelled", good.clone());
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        dispatch(&mut o, &cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    let mut missing = request(&o, "missing", good.clone());
    missing.as_object_mut().unwrap().remove("expectedRevision");
    assert_eq!(
        dispatch(&mut o, &missing)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), before);
}

#[test]
fn prepared_candidates_are_sealed_to_their_owner_and_snapshot() {
    let payload = mv("L0", 0, 1).to_string();
    let mut source = owner(doc(SMALL));
    let foreign_candidate = source
        .prepare_exposure_retime_json(payload.as_bytes())
        .unwrap();
    assert_eq!(foreign_candidate.applied_to(), Some(1));
    let mut other = owner(doc(SMALL));
    let before = state(&other);
    assert_eq!(
        other.commit_exposure_retime(foreign_candidate).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(state(&other), before);
    let wrong_instance = source
        .prepare_exposure_retime_json(payload.as_bytes())
        .unwrap();
    let mut foreign = NativeObjectHistory::new("foreign", doc(SMALL)).unwrap();
    assert_eq!(
        foreign.commit_exposure_retime(wrong_instance).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(foreign.content_revision(), 0);
    let first = source
        .prepare_exposure_retime_json(payload.as_bytes())
        .unwrap();
    let second = source
        .prepare_exposure_retime_json(payload.as_bytes())
        .unwrap();
    assert_eq!(source.commit_exposure_retime(first), Ok(1));
    let after = state(&source);
    assert_eq!(
        source.commit_exposure_retime(second).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(state(&source), after);
    assert_eq!(source.history_depths(), (1, 0));
}

#[test]
fn a_chain_of_moves_reopens_at_every_step_and_undoes_back_to_the_original_bytes() {
    let mut o = owner(doc(
        "K=a.1.1;I=s.3.3;I=t.4.4;K=b.2.2;.;K=c.3.3;.|K;.;.;.;.;.;.",
    ));
    let steps = [
        (
            mv("L0", 0, 1),
            ".;K=a.1.1;I=s.3.3;I=t.4.4;K=b.2.2;.;K=c.3.3",
            "K;.;.;.;.;.;.",
        ),
        (
            mv("L0", 4, 3),
            ".;K=a.1.1;I=t.4.4;K=b.2.2;.;K=c.3.3;.",
            "K;.;.;.;.;.;.",
        ),
        (
            mv("L1", 0, 3),
            ".;K=a.1.1;I=t.4.4;K=b.2.2;.;K=c.3.3;.",
            ".;.;.;K;.;.;.",
        ),
        (
            mv("L0", 5, 6),
            ".;K=a.1.1;I=t.4.4;K=b.2.2;.;.;K=c.3.3",
            ".;.;.;K;.;.;.",
        ),
    ];
    let mut saved = vec![bytes(&o)];
    for (index, (payload, l0, l1)) in steps.into_iter().enumerate() {
        let submitted = request(&o, &format!("step-{index}"), payload);
        assert_eq!(dispatch(&mut o, &submitted)["ok"], true, "step {index}");
        assert_eq!(
            (layer_text(&o, 0), layer_text(&o, 1)),
            (l0.to_owned(), l1.to_owned()),
            "step {index}"
        );
        assert_eq!(
            decode_storage_project(&bytes(&o)).unwrap(),
            *head(&o).document(),
            "step {index}: save and reopen"
        );
        saved.push(bytes(&o));
    }
    assert_eq!(o.history_depths(), (4, 0));
    assert_eq!(head(&o).document().total_frames(), 7);
    assert_eq!(exposure_string(head(&o).document(), 0), "-*~*33*");
    assert_eq!(exposure_string(head(&o).document(), 1), "---*333");
    for back in (0..4).rev() {
        o.undo(o.content_revision()).unwrap();
        assert_eq!(bytes(&o), saved[back], "undo to step {back}");
    }
    for forward in 1..=4 {
        o.redo(o.content_revision()).unwrap();
        assert_eq!(bytes(&o), saved[forward], "redo to step {forward}");
    }
}
