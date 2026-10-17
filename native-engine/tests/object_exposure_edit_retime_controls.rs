//! Negative controls for the key-retime oracle. Each control runs the REAL dispatch path over every unlocked oracle row
//! (locked rows need the F06 admission and are covered by their own test) with ONE deliberate mistake (an altered request, or
//! a published document corrupted afterwards), through the SAME comparison the positive test uses, and requires it to fail on
//! exactly the rows where the mistake is observable. The failing set is derived from the row data in Rust, and its size is
//! pinned as a literal that tools/reach_counts.js (the same predicates written a second time) prints.
use super::build::{document, uid, Build};
use super::rows::{compare, input_document, rows, Row};
use super::{dispatch, head, owner, request};
use crate::object_document::{FrameScopeKind, LayerTiming, ObjectDocument, ObjectRecord, Presence};
use serde_json::{json, Value};

type Mutate<'a> = &'a dyn Fn(&Row, Build, &mut ObjectDocument);

fn unlocked() -> Vec<Row> {
    rows().into_iter().filter(|row| !row.any_lock()).collect()
}
fn replay(row: &Row, build: Build, payload: Value, mutate: Mutate<'_>) -> Result<(), String> {
    let input = input_document(row, build);
    let mut o = owner(input.clone());
    let submitted = request(&o, "control", payload);
    let reply = dispatch(&mut o, &submitted);
    if let Some(code) = row.refusal() {
        // a row the baseline refused must be refused again with the typed code
        return if reply["error"]["code"] == code {
            Ok(())
        } else {
            Err(format!("expected {code}: {reply}"))
        };
    }
    if reply["ok"] != true {
        return Err(format!("refused: {reply}"));
    }
    if (reply["result"]["applied"] == true) != row.chg {
        return Err("the applied flag differs from the baseline".into());
    }
    let mut out = head(&o).document().clone();
    mutate(row, build, &mut out);
    compare(row, build, &input, &out)
}
fn failing(alter: &dyn Fn(&Row) -> Value, mutate: Mutate<'_>) -> Vec<String> {
    let mut failed = Vec::new();
    for row in &unlocked() {
        let payload = alter(row);
        if Build::ALL
            .iter()
            .any(|build| replay(row, *build, payload.clone(), mutate).is_err())
        {
            failed.push(row.label());
        }
    }
    failed
}
fn labels(select: &dyn Fn(&Row) -> bool) -> Vec<String> {
    unlocked()
        .iter()
        .filter(|row| select(row))
        .map(|row| row.label())
        .collect()
}
fn untouched_payload(row: &Row) -> Value {
    row.payload()
}
fn no_mutation(_: &Row, _: Build, _: &mut ObjectDocument) {}

fn changed(row: &Row) -> bool {
    row.outcome == "changed"
}
fn is_sample(flag: char) -> bool {
    matches!(flag, 'I' | 'M' | 'N')
}
/// The frame the dragged key lands on when asked for `to`, by the ripple rule alone: the last key stays inside the
/// timeline, and a zero move lands nowhere (`None`).
fn landing(row: &Row, to: i64) -> Option<i64> {
    let later_last = row
        .subject()
        .0
        .iter()
        .enumerate()
        .filter(|(frame, cell)| cell.flag == 'K' && (*frame as i64) > row.from)
        .map(|(frame, _)| frame as i64)
        .last();
    let mut delta = to - row.from;
    if delta > 0 {
        if let Some(last) = later_last {
            delta = delta.min((i64::from(row.total) - 1 - last).max(0));
        }
    }
    (delta != 0).then_some(row.from + delta)
}
/// A changed row whose request, shifted by `step`, no longer lands where the baseline landed (or is refused).
fn shifted_matters(row: &Row, step: i64) -> bool {
    changed(row)
        && (row.to + step >= i64::from(row.total)
            || landing(row, row.to + step) != row.applied.map(i64::from))
}
fn shifted(row: &Row, step: i64) -> Value {
    let mut payload = row.payload();
    if changed(row) {
        payload["to"] = json!(row.to + step);
    }
    payload
}
fn left_behind(row: &Row) -> Vec<usize> {
    let (before, after) = row.subject();
    (0..before.len())
        .filter(|frame| {
            before[*frame].flag == 'K'
                && !before[*frame].strokes.is_empty()
                && after[*frame].flag == '.'
        })
        .collect()
}
fn sample_cells(cells: &[super::build::Cell]) -> Vec<(usize, super::build::Cell)> {
    cells
        .iter()
        .enumerate()
        .filter(|(_, cell)| is_sample(cell.flag))
        .map(|(frame, cell)| (frame, cell.clone()))
        .collect()
}

/// A scene-root authored record of `layer` at one of `frames`.
fn at_frames(record: &ObjectRecord, layer: &str, frames: &[usize]) -> bool {
    record.target.context_id == "scene-root"
        && record.target.frame_scope.kind == FrameScopeKind::Authored
        && record.target.layer_uid == layer
        && frames.contains(&(record.target.frame_scope.frame as usize))
}
fn records_at(document: &ObjectDocument, layer: &str, frames: &[usize]) -> Vec<ObjectRecord> {
    document
        .objects
        .iter()
        .filter(|record| at_frames(record, layer, frames))
        .cloned()
        .collect()
}

#[test]
fn unmodified_transactions_pass_every_row_in_every_form() {
    assert_eq!(
        failing(&untouched_payload, &no_mutation),
        Vec::<String>::new()
    );
}

#[test]
fn a_drag_that_lands_one_frame_late_is_caught_where_the_ripple_clamp_does_not_absorb_it() {
    let failed = failing(&|row| shifted(row, 1), &no_mutation);
    assert_eq!(failed, labels(&|row| shifted_matters(row, 1)));
    assert_eq!(failed.len(), 263);
}

#[test]
fn a_drag_that_lands_one_frame_early_is_caught_where_the_ripple_clamp_does_not_absorb_it() {
    let failed = failing(&|row| shifted(row, -1), &no_mutation);
    assert_eq!(failed, labels(&|row| shifted_matters(row, -1)));
    assert_eq!(failed.len(), 257);
}

#[test]
fn a_request_for_the_other_layer_is_caught_on_every_changed_row() {
    let alter = |row: &Row| {
        let mut payload = row.payload();
        if changed(row) {
            payload["layerUid"] = json!(uid(1 - row.layer));
        }
        payload
    };
    let failed = failing(&alter, &no_mutation);
    assert_eq!(failed, labels(&changed));
    assert_eq!(failed.len(), 317);
}

#[test]
fn a_published_document_that_forgot_its_new_timing_is_caught_on_every_changed_row() {
    fn stale(row: &Row, build: Build, out: &mut ObjectDocument) {
        let input = document(&row.input, build);
        for (layer, source) in out.layers.iter_mut().zip(input.layers.iter()) {
            layer.timing = source.timing.clone();
        }
    }
    let failed = failing(&untouched_payload, &stale);
    assert_eq!(failed, labels(&changed));
    assert_eq!(failed.len(), 317);
}

#[test]
fn records_left_behind_at_a_vacated_key_are_caught_where_that_key_carried_strokes() {
    fn left(row: &Row, build: Build, out: &mut ObjectDocument) {
        let input = document(&row.input, build);
        let layer = uid(row.layer);
        out.objects
            .extend(records_at(&input, &layer, &left_behind(row)));
    }
    let failed = failing(&untouched_payload, &left);
    assert_eq!(
        failed,
        labels(&|row| changed(row) && !left_behind(row).is_empty())
    );
    assert_eq!(failed.len(), 270);
}

#[test]
fn in_betweens_that_were_not_retimed_are_caught_where_the_sample_cells_changed() {
    fn unretimed(row: &Row, build: Build, out: &mut ObjectDocument) {
        let input = document(&row.input, build);
        let layer = uid(row.layer);
        let mut timing =
            serde_json::to_value(out.layers[row.layer].timing.value().unwrap()).unwrap();
        let before = serde_json::to_value(input.layers[row.layer].timing.value().unwrap()).unwrap();
        timing["samples"] = before["samples"].clone();
        let restored: LayerTiming = serde_json::from_value(timing).unwrap();
        out.layers[row.layer].timing = Presence::Present(restored);
        let (old, new) = row.subject();
        let in_output: Vec<usize> = sample_cells(new).iter().map(|(frame, _)| *frame).collect();
        let in_input: Vec<usize> = sample_cells(old).iter().map(|(frame, _)| *frame).collect();
        out.objects
            .retain(|record| !at_frames(record, &layer, &in_output));
        out.objects.extend(records_at(&input, &layer, &in_input));
    }
    let failed = failing(&untouched_payload, &unretimed);
    assert_eq!(
        failed,
        labels(
            &|row| changed(row) && sample_cells(row.subject().0) != sample_cells(row.subject().1)
        )
    );
    assert_eq!(failed.len(), 229);
}
