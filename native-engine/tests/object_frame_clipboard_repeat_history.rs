//! Repeat Selection through the real history owner and the real dispatcher: every oracle row (oracle.txt; the verbatim
//! baseline editor at fbe4b8a3) is rebuilt in four native document forms, dispatched as `object.frames.repeat`, compared
//! back to the baseline (cells, strokes, `heldKeyframeAt` exposure, shown strokes, ordered elements, everything that
//! must not change), undone to the original bytes, redone and reopened from storage. Hand-computed cases, replay,
//! no-op/redo, preflight, sealing and a chain of cycles are in object_frame_clipboard_repeat_edges.rs; negative controls in
//! object_frame_clipboard_repeat_controls.rs.
use super::*;
use crate::commands::{clamp_times, grown_total, repeat_payload};
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
pub(crate) mod build;
#[path = "object_frame_clipboard_repeat_controls.rs"]
mod controls;
#[path = "object_frame_clipboard_repeat_edges.rs"]
mod edges;
#[cfg(feature = "evaluation")]
#[path = "object_frame_clipboard_repeat_exposure.rs"]
mod exposure;
#[cfg(feature = "project_media_codec")]
#[path = "object_frame_clipboard_repeat_project.rs"]
mod project;
#[path = "object_frame_clipboard_repeat_rows.rs"]
pub(crate) mod rows;

use build::Build;
use rows::{compare, rows, Row, FORMS, OP_REPEAT};

pub(crate) fn owner(document: ObjectDocument) -> NativeObjectHistory {
    NativeObjectHistory::new("frame-repeat", document).unwrap()
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
/// One cycle as the dispatcher runs it; returns the reply.
pub(crate) fn run_request(o: &mut NativeObjectHistory, cycle: &RepeatRequest, id: &str) -> Value {
    let submitted = request(o, id, OP_REPEAT, repeat_payload(cycle));
    dispatch(o, &submitted)
}
/// One cycle through the typed API (the controls feed their faulty requests through here).
pub(crate) fn apply(o: &mut NativeObjectHistory, cycle: &RepeatRequest) -> Result<u64, String> {
    let candidate = o
        .prepare_frame_repeat(cycle)
        .map_err(|code| format!("{code:?}"))?;
    o.commit_frame_repeat(candidate)
        .map_err(|code| format!("{code:?}"))
}

/// One row through the history owner: dispatched, compared with the baseline, undone to the original bytes, redone and
/// reopened.
fn run_row(row: &Row, form: Build) {
    let who = format!("{} {form:?}", row.label());
    let input = row.document(form);
    let mut o = owner(input.clone());
    let before = bytes(&o);
    let reply = run_request(&mut o, &row.request(), "row");
    assert_eq!(reply["ok"], true, "{who}: {reply}");
    assert_eq!(
        reply["result"],
        json!({"applied":row.chg,"historyEntriesAdded":u8::from(row.chg),
            "cycledLayers":row.bounds().1 - row.bounds().0 + 1,"skippedComponentLayers":0}),
        "{who}"
    );
    let out = head(&o).document().clone();
    if let Err(message) = compare(row, &input, &out, form) {
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
fn every_oracle_row_matches_through_the_dispatcher_undo_redo_and_reopen() {
    let all = rows();
    assert_eq!(all.len(), 270);
    for row in &all {
        for form in FORMS {
            run_row(row, form);
        }
    }
}

/// The growth is the baseline's `max(total, maxF + 1 + span * clamp(times))`, computed natively by `grown_total`; the
/// work-area out point the baseline also moves is `needed - 1` exactly when the timeline grew, which is the value the
/// project revision also publishes (these rows deliberately retain bare drawing inputs).
#[test]
fn native_growth_and_the_project_work_area_value_follow_the_baseline() {
    let mut grew = 0;
    for row in &rows() {
        let (_, _, first, last) = row.bounds();
        let grown = grown_total(row.total_in(), first, last, clamp_times(row.times)).expect("fits");
        assert_eq!(grown, row.total, "{}", row.label());
        let expected_wa = if grown > row.total_in() {
            grown - 1
        } else {
            row.total_in() - 1
        };
        assert_eq!(row.wa_out, expected_wa, "{}", row.label());
        grew += usize::from(grown > row.total_in());
    }
    assert_eq!(grew, 186);
}

/// Pins how much of the baseline the 270 rows reach (same predicates written in `tools/model_check.js`), so a regenerated
/// oracle that loses a branch fails here instead of passing vacuously.
#[test]
fn oracle_reaches_every_branch() {
    let all = rows();
    let count = |predicate: &dyn Fn(&Row) -> bool| all.iter().filter(|row| predicate(row)).count();
    // the cells of the bounds rectangle in the row's input
    let cells = |row: &Row| -> Vec<(usize, u32, char, bool)> {
        let (min_l, max_l, min_f, max_f) = row.bounds();
        let mut out = Vec::new();
        for layer in min_l..=max_l {
            for frame in min_f..=max_f {
                let cell = &row.input[layer][frame as usize];
                out.push((layer, frame, cell.flag, !cell.strokes.is_empty()));
            }
        }
        out
    };
    let has = |row: &Row, flag: &str, strokes: Option<bool>| {
        cells(row)
            .iter()
            .any(|c| flag.contains(c.2) && strokes.is_none_or(|want| want == c.3))
    };
    assert_eq!(count(&|r| r.chg), 265);
    assert_eq!(count(&|r| !r.chg), 5);
    assert_eq!(count(&|r| r.total > r.total_in()), 186);
    assert_eq!(count(&|r| r.chg && r.total == r.total_in()), 79);
    assert_eq!(count(&|r| !r.chg && r.total == r.total_in()), 5);
    assert_eq!(count(&|r| r.any_locked()), 80);
    assert_eq!(
        count(&|r| r.locked[r.bounds().0..=r.bounds().1].contains(&true)),
        63
    );
    assert_eq!(count(&|r| r.times < 1), 25);
    assert_eq!(count(&|r| r.times > 50), 18);
    assert_eq!(count(&|r| (1..=50).contains(&r.times)), 227);
    assert_eq!(count(&|r| has(r, "IMN", None)), 188);
    assert_eq!(count(&|r| has(r, "M", None)), 79);
    assert_eq!(count(&|r| has(r, "N", None)), 75);
    assert_eq!(count(&|r| has(r, "K", Some(false))), 67);
    assert_eq!(count(&|r| has(r, "K", Some(true))), 145);
    assert_eq!(count(&|r| has(r, ".", None)), 174);
    assert_eq!(count(&|r| r.bounds().1 > r.bounds().0), 86);
    assert_eq!(
        count(&|r| r.input.len() > r.bounds().1 - r.bounds().0 + 1),
        119
    );
    assert_eq!(count(&|r| r.sel.len() != cells(r).len()), 64);
    assert_eq!(count(&|r| r.bounds().2 == r.bounds().3), 94);
    // a destination inside the old timeline that held a key or an in-between before it was replaced
    let overwrote = |row: &Row| {
        let (min_l, max_l, first, last) = row.bounds();
        let reach = last + (last - first + 1) * clamp_times(row.times);
        (min_l..=max_l).any(|layer| {
            (last + 1..=reach).any(|frame| {
                row.input[layer]
                    .get(frame as usize)
                    .is_some_and(|cell| cell.flag != '.')
            })
        })
    };
    assert_eq!(count(&overwrote), 134);
}

/// A layer in the bounds without a timing record is never given a made-up one (a layer with no records, so that the
/// document is admitted without it).
#[test]
fn a_layer_in_the_bounds_without_timing_refuses_typed_and_publishes_nothing() {
    for form in [Build::V1, Build::V2] {
        let mut value = build::document_json(&rows::parse_layers("K;."), form);
        value["layers"][0].as_object_mut().unwrap().remove("timing");
        let mut o = owner(decode_storage_project(value.to_string().as_bytes()).unwrap());
        let before = state(&o);
        let cycle = RepeatRequest {
            from_layer: build::uid(0),
            to_layer: build::uid(0),
            from_frame: 0,
            to_frame: 0,
            times: 1,
        };
        let reply = run_request(&mut o, &cycle, "bare");
        assert_eq!(reply["error"]["code"], "unavailable", "{form:?}: {reply}");
        assert_eq!(state(&o), before, "{form:?}");
    }
}

/// Genuine admitted component table, matching Lead's retained Repeat kernel controls.
/// L1 has no drawing timing; it is a native scene-root component instance, not a flag pretending to be one.
fn component_outcome_document() -> ObjectDocument {
    let mut value = build::document_json(&rows::parse_layers("K=a.1.10;.;.|.;.;."), Build::V2);
    value["layers"][1].as_object_mut().unwrap().remove("timing");
    // Component frameKeys require outer timing keys; this genuine no-timing instance has none.
    value["componentTables"] = json!({"components": {
        "format":"nemo.native-component-set", "formatVersion":1,
        "components":[{"symbolId":"symbol", "totalFrames":3,
            "layers":[{"layerUid":"inside"}], "objects":[]}],
        "instances":[{"contextId":"scene-root", "layerUid":"L1", "symbolId":"symbol",
            "symSpeed":2}]
    }});
    decode_storage_project(value.to_string().as_bytes()).unwrap()
}

fn outcome_cycle(from: usize, to: usize, first: u32, last: u32, times: i64) -> RepeatRequest {
    RepeatRequest {
        from_layer: build::uid(from),
        to_layer: build::uid(to),
        from_frame: first,
        to_frame: last,
        times,
    }
}

#[test]
fn real_repeat_dispatch_retains_mixed_and_component_only_counts_through_replay_undo_and_redo() {
    for (from, to, times, changed, cycled, skipped) in [
        (0, 1, 3, true, 1, 1),
        (1, 1, 3, true, 0, 1),
        (1, 1, 1, false, 0, 1),
    ] {
        let input = component_outcome_document();
        let mut o = owner(input.clone());
        let before = state(&o);
        let cycle = outcome_cycle(from, to, 0, 0, times);
        let submitted = request(&o, "component-counts", OP_REPEAT, repeat_payload(&cycle));
        let reply = dispatch(&mut o, &submitted);
        assert_eq!(reply["ok"], true, "{reply}");
        assert_eq!(
            reply["result"],
            json!({"applied":changed,"historyEntriesAdded":u8::from(changed),
            "cycledLayers":cycled,"skippedComponentLayers":skipped})
        );
        assert_eq!(o.history_depths(), (usize::from(changed), 0));
        let output = head(&o).document().clone();
        assert_eq!(output.layers[1], input.layers[1]);
        assert_eq!(output.component_tables, input.component_tables);
        if cycled == 0 {
            assert_eq!(
                output.objects, input.objects,
                "global growth does not cycle any component drawing"
            );
            assert_eq!(output.total_frames, if changed { 4 } else { 3 });
        }
        let after = state(&o);
        assert_eq!(dispatch(&mut o, &submitted), reply);
        assert_eq!(
            state(&o),
            after,
            "retained count reply cannot execute twice"
        );
        if changed {
            let undo = request(
                &o,
                "undo",
                "command.document.object.undo",
                json!({"command":"object.undo"}),
            );
            let undone = dispatch(&mut o, &undo);
            assert_eq!(
                undone["result"],
                json!({"applied":true,"historyEntriesAdded":0}),
                "other command results keep exactly two fields"
            );
            assert_eq!(bytes(&o), before.3);
            assert_eq!(o.history_depths(), (0, 1));
            let after_undo = state(&o);
            assert_eq!(
                dispatch(&mut o, &submitted),
                reply,
                "replay retains the original four fields after Undo"
            );
            assert_eq!(state(&o), after_undo);
            let redo = request(
                &o,
                "redo",
                "command.document.object.redo",
                json!({"command":"object.redo"}),
            );
            let redone = dispatch(&mut o, &redo);
            assert_eq!(
                redone["result"],
                json!({"applied":true,"historyEntriesAdded":0})
            );
            assert_eq!(*head(&o).document(), output);
        } else {
            assert_eq!(
                state(&o),
                before,
                "all-component no-op retains its skip count without publication"
            );
        }
    }
}

#[test]
fn real_identical_layer_noop_reports_visited_cycle_and_preserves_redo_and_exact_replay() {
    let mut o = owner(build::document(
        &rows::parse_layers("K=a.1.10;.;K=b.2.20;.|K;.;.;K"),
        Build::V1,
    ));
    let changed = run_request(&mut o, &outcome_cycle(0, 0, 0, 1, 1), "seed-redo");
    assert_eq!(
        changed["result"],
        json!({"applied":true,"historyEntriesAdded":1,"cycledLayers":1,"skippedComponentLayers":0})
    );
    o.undo(1).unwrap();
    let before = state(&o);
    let submitted = request(
        &o,
        "noop-counts",
        OP_REPEAT,
        repeat_payload(&outcome_cycle(1, 1, 1, 1, 1)),
    );
    let reply = dispatch(&mut o, &submitted);
    assert_eq!(
        reply["result"],
        json!({"applied":false,"historyEntriesAdded":0,"cycledLayers":1,"skippedComponentLayers":0})
    );
    assert_eq!(state(&o), before);
    assert_eq!(o.history_depths(), (0, 1));
    assert_eq!(dispatch(&mut o, &submitted), reply);
    assert_eq!(state(&o), before);
    let mut altered = submitted.clone();
    altered["payload"]["times"] = json!(2);
    let refused = dispatch(&mut o, &altered);
    assert_eq!(refused["ok"], false);
    assert_eq!(refused["error"]["code"], "invalid_request");
    assert_eq!(refused["result"], Value::Null);
    assert_eq!(state(&o), before);
    o.redo(o.content_revision()).unwrap();
    assert_eq!(o.history_depths(), (1, 0));
}

#[test]
fn refused_repeat_never_publishes_prepared_counts_or_changes_history() {
    let mut o = owner(component_outcome_document());
    let before = state(&o);
    let invalid = outcome_cycle(0, 1, 0, 3, 1);
    let submitted = request(&o, "invalid-counts", OP_REPEAT, repeat_payload(&invalid));
    let reply = dispatch(&mut o, &submitted);
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["result"], Value::Null);
    assert_eq!(state(&o), before);
    assert_eq!(dispatch(&mut o, &submitted), reply);
    assert_eq!(
        state(&o),
        before,
        "a retained failure cannot publish a partial outcome"
    );
}
