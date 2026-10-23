//! Hand-computed Repeat Selection cases through the real history owner and dispatcher: one history entry and one Undo,
//! replay that never executes twice, no-op/redo, preflight and payload refusals, sealed candidates, typed and dispatched
//! agreement and a chain of cycles that reopens at every step. The oracle rows are in
//! object_frame_clipboard_repeat_history.rs.
use super::build::{document, exposure_string, render_layer, Build};
use super::rows::{parse_layers, OP_REPEAT};
use super::{apply, bytes, dispatch, head, owner, request, run_request, state};
use crate::commands::{repeat_payload, RepeatRequest};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::ObjectDocument;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

/// L0 has a stroke at each of its two keys; L1 is a bare key track (blank keys at 0 and 3). Four frames.
const SMALL: &str = "K=a.1.10;.;K=b.2.20;.|K;.;.;K";

fn doc(text: &str) -> ObjectDocument {
    document(&parse_layers(text), Build::V1)
}
fn layer_text(o: &NativeObjectHistory, layer: usize) -> String {
    render_layer(head(o).document(), layer)
}
fn cycle(from: usize, to: usize, first: u32, last: u32, times: i64) -> RepeatRequest {
    RepeatRequest {
        from_layer: format!("L{from}"),
        to_layer: format!("L{to}"),
        from_frame: first,
        to_frame: last,
        times,
    }
}
fn applied(reply: &Value) -> Value {
    json!({"applied":reply["result"]["applied"],"historyEntriesAdded":reply["result"]["historyEntriesAdded"]})
}
/// L0 frames 0..=1 once: frame 2 (key b) becomes a copy of the key at 0, frame 3 stays a hold. No growth.
fn first_edit() -> RepeatRequest {
    cycle(0, 0, 0, 1, 1)
}

#[test]
fn a_cycle_is_one_history_entry_and_one_undo_restores_the_complete_bytes() {
    let mut o = owner(doc(SMALL));
    let before = bytes(&o);
    let reply = run_request(&mut o, &first_edit(), "edit");
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(
        applied(&reply),
        json!({"applied":true,"historyEntriesAdded":1})
    );
    assert_eq!(layer_text(&o, 0), "K=a.1.10;.;K=a.1.10;.");
    assert_eq!(exposure_string(head(&o).document(), 0), "*0*2");
    assert_eq!(layer_text(&o, 1), "K;.;.;K", "the other layer is untouched");
    assert_eq!(o.history_depths(), (1, 0));
    let after = head(&o).document().clone();
    let undone = o.undo(1).unwrap();
    assert_eq!(bytes(&o), before, "one Undo restores the complete bytes");
    assert_eq!(o.history_depths(), (0, 1));
    o.redo(undone).unwrap();
    assert_eq!(*head(&o).document(), after, "Redo reproduces the cycle");
    assert_eq!(
        decode_storage_project(&bytes(&o)).unwrap(),
        after,
        "save and reopen"
    );
}

#[test]
fn replay_never_executes_twice_even_after_undo() {
    let mut o = owner(doc(SMALL));
    let first_request = request(&o, "edit", OP_REPEAT, repeat_payload(&first_edit()));
    let first = dispatch(&mut o, &first_request);
    assert_eq!(first["ok"], true);
    let after = state(&o);
    assert_eq!(dispatch(&mut o, &first_request), first);
    assert_eq!(
        state(&o),
        after,
        "an exact replay returns the receipt and executes nothing"
    );
    let mut altered = first_request.clone();
    altered["payload"]["times"] = json!(2);
    assert_eq!(
        dispatch(&mut o, &altered)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), after);
    o.undo(1).unwrap();
    let undone = state(&o);
    assert_eq!(
        dispatch(&mut o, &first_request),
        first,
        "replay cannot execute after undo"
    );
    assert_eq!(state(&o), undone);
    assert_eq!(o.history_depths(), (0, 1));
}

#[test]
fn a_no_op_adds_no_history_publishes_nothing_and_keeps_redo() {
    let mut o = owner(doc(SMALL));
    assert_eq!(run_request(&mut o, &first_edit(), "edit")["ok"], true);
    o.undo(1).unwrap();
    assert_eq!(o.history_depths(), (0, 1));
    // A hold copied over a hold inside the timeline: L1 frame 1 once lands on frame 2, which already is a hold.
    let before = state(&o);
    let reply = run_request(&mut o, &cycle(1, 1, 1, 1, 1), "hold");
    assert_eq!(
        applied(&reply),
        json!({"applied":false,"historyEntriesAdded":0}),
        "{reply}"
    );
    assert_eq!(state(&o), before, "nothing published, redo kept");
    assert_eq!(
        apply(&mut o, &cycle(1, 1, 1, 1, 0)),
        Ok(before.0),
        "times 0 runs once: the same no-op"
    );
    assert_eq!(state(&o), before);
    let revision = o.content_revision();
    o.redo(revision).unwrap();
    assert_eq!(
        layer_text(&o, 0),
        "K=a.1.10;.;K=a.1.10;.",
        "redo still reproduces the cycle"
    );
}

#[test]
fn a_changed_edit_is_one_entry_and_clears_redo() {
    let mut o = owner(doc(SMALL));
    assert_eq!(
        run_request(&mut o, &cycle(0, 1, 0, 3, 1), "big")["ok"],
        true
    );
    assert_eq!(
        o.history_depths(),
        (1, 0),
        "a multi-layer growing cycle is ONE entry"
    );
    o.undo(1).unwrap();
    assert_eq!(o.history_depths(), (0, 1));
    apply(&mut o, &first_edit()).unwrap();
    assert_eq!(o.history_depths(), (1, 0), "a changed edit clears redo");
}

#[test]
fn preflight_and_payload_refusals_publish_nothing_and_are_retained() {
    let mut o = owner(doc(SMALL));
    let before = state(&o);
    let good = repeat_payload(&first_edit());
    let cases = [
        ("/instanceId", json!("other"), "wrong_instance"),
        ("/documentId", json!("other"), "wrong_document"),
        ("/expectedRevision", json!(7), "stale_revision"),
        ("/apiVersion", json!(1), "invalid_request"),
        ("/payload", json!([]), "invalid_request"),
        (
            "/payload/command",
            json!("object.frames.clipboard.duplicate"),
            "invalid_request",
        ),
        ("/payload/extra", json!(true), "invalid_request"),
        ("/payload/fromLayerUid", json!("nope"), "not_found"),
        ("/payload/toLayerUid", json!("nope"), "not_found"),
        ("/payload/fromLayerUid", json!("L1"), "invalid_request"),
        ("/payload/toFrame", json!(9), "invalid_request"),
        ("/payload/fromFrame", json!(2), "invalid_request"),
        ("/payload/times", json!("2"), "invalid_request"),
        ("/payload/times", json!(1.5), "invalid_request"),
    ];
    for (index, (pointer, value, code)) in cases.into_iter().enumerate() {
        let mut submitted = request(&o, &format!("preflight-{index}"), OP_REPEAT, good.clone());
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
    let mut cancelled = request(&o, "cancelled", OP_REPEAT, good.clone());
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        dispatch(&mut o, &cancelled)["error"]["code"],
        "cancelled_before_dispatch"
    );
    let mut missing = request(&o, "missing", OP_REPEAT, good);
    missing.as_object_mut().unwrap().remove("expectedRevision");
    assert_eq!(
        dispatch(&mut o, &missing)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(state(&o), before);
}

#[test]
fn prepared_candidates_are_sealed_to_their_owner_and_snapshot() {
    let mut source = owner(doc(SMALL));
    let candidate = source.prepare_frame_repeat(&first_edit()).unwrap();
    let mut other = owner(doc(SMALL));
    let before = state(&other);
    assert_eq!(
        other.commit_frame_repeat(candidate).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    assert_eq!(state(&other), before);
    let wrong_instance = source.prepare_frame_repeat(&first_edit()).unwrap();
    let mut foreign = NativeObjectHistory::new("foreign", doc(SMALL)).unwrap();
    assert_eq!(
        foreign.commit_frame_repeat(wrong_instance).unwrap_err(),
        DispatchErrorCode::WrongInstance
    );
    assert_eq!(foreign.content_revision(), 0);
    let first = source.prepare_frame_repeat(&first_edit()).unwrap();
    let second = source.prepare_frame_repeat(&first_edit()).unwrap();
    assert_eq!(source.commit_frame_repeat(first), Ok(1));
    let after = state(&source);
    assert_eq!(
        source.commit_frame_repeat(second).unwrap_err(),
        DispatchErrorCode::StaleRevision
    );
    assert_eq!(state(&source), after);
    assert_eq!(source.history_depths(), (1, 0));
}

#[test]
fn the_typed_cycle_and_the_dispatched_cycle_publish_the_same_bytes() {
    let mut dispatched = owner(doc(SMALL));
    let mut typed = owner(doc(SMALL));
    assert_eq!(
        run_request(&mut dispatched, &cycle(0, 1, 0, 3, 1), "big")["ok"],
        true
    );
    assert_eq!(typed.repeat_frames(&cycle(0, 1, 0, 3, 1)), Ok(1));
    assert_eq!(head(&dispatched).document(), head(&typed).document());
    assert_eq!(typed.history_depths(), (1, 0));
}

#[test]
fn a_chain_of_cycles_grows_the_timeline_and_reopens_at_every_step() {
    let mut o = owner(doc(SMALL));
    let mut saved = vec![bytes(&o)];
    // revision 1: L0 frames 0..=1 once, no growth
    assert_eq!(run_request(&mut o, &first_edit(), "s1")["ok"], true);
    assert_eq!(layer_text(&o, 0), "K=a.1.10;.;K=a.1.10;.");
    saved.push(bytes(&o));
    // revision 2: both layers, all four frames once: needed = 3 + 1 + 4 = 8
    assert_eq!(run_request(&mut o, &cycle(0, 1, 0, 3, 1), "s2")["ok"], true);
    assert_eq!(head(&o).document().total_frames(), 8);
    assert_eq!(
        layer_text(&o, 0),
        "K=a.1.10;.;K=a.1.10;.;K=a.1.10;.;K=a.1.10;."
    );
    assert_eq!(layer_text(&o, 1), "K;.;.;K;K;.;.;K");
    assert_eq!(exposure_string(head(&o).document(), 0), "*0*2*4*6");
    assert_eq!(exposure_string(head(&o).document(), 1), "*00**44*");
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
