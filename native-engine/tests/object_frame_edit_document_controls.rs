//! Negative controls for the document-level frame edits. Each control takes the REAL kernel output of every
//! oracle row, makes one specific mistake a wrong implementation could make, and runs the SAME oracle
//! comparison the positive test uses. The set of rows that then fail must be exactly the rows where the
//! mistake is observable (derived below from the row data, written a second time in tools/reach_counts.js)
//! and its size is pinned, so a comparison that cannot see the mistake fails the control.
use super::build::{document, Build, Cell};
use super::tests::{compare, rows, run, Row};
use super::DocumentEdit;
use crate::object_document::{FrameScopeKind, ObjectDocument, ObjectRecord, Presence};
use serde_json::Number;

type Tamper = fn(&Row, &ObjectDocument, ObjectDocument) -> ObjectDocument;

/// Ids of the rows whose tampered kernel output no longer matches the baseline.
fn failing(build: Build, tamper: Tamper) -> Vec<usize> {
    rows()
        .iter()
        .filter(|row| {
            let input = document(&row.input, build);
            let out = match run(row, &input).unwrap() {
                DocumentEdit::Changed(next) => next,
                DocumentEdit::NoChange => input.clone(),
            };
            compare(row, build, &input, &tamper(row, &input, out)).is_err()
        })
        .map(|row| row.id)
        .collect()
}
fn expected(predicate: fn(&Row) -> bool) -> Vec<usize> {
    rows()
        .iter()
        .filter(|row| predicate(row))
        .map(|row| row.id)
        .collect()
}
fn cells(row: &Row) -> Vec<usize> {
    row.args.split(',').map(|n| n.parse().unwrap()).collect()
}
fn scene_authored(record: &ObjectRecord) -> bool {
    record.target.context_id == "scene-root"
        && record.target.frame_scope.kind == FrameScopeKind::Authored
}
/// Promoted cells that were holds: where the F6 content copy lands.
fn held_targets(row: &Row) -> Vec<u32> {
    let held = |c: &usize| row.input[0][*c].flag == '.';
    cells(row)
        .into_iter()
        .filter(held)
        .map(|c| c as u32)
        .collect()
}
fn copied_content(row: &Row) -> bool {
    row.op == "promote"
        && cells(row)
            .iter()
            .any(|&c| row.input[0][c].flag == '.' && !row.output[0][c].strokes.is_empty())
}

fn rename_clones(row: &Row, _: &ObjectDocument, mut out: ObjectDocument) -> ObjectDocument {
    if row.op == "promote" {
        let held = held_targets(row);
        for record in out.objects.iter_mut().filter(|r| scene_authored(r)) {
            if record.target.layer_uid == "L0" && held.contains(&record.target.frame_scope.frame) {
                record.target.stroke_id.push('x');
            }
        }
    }
    out
}
fn bump_clone_order(row: &Row, _: &ObjectDocument, mut out: ObjectDocument) -> ObjectDocument {
    let held = if row.op == "promote" {
        held_targets(row)
    } else {
        Vec::new()
    };
    if let Presence::Present(plan) = &mut out.composition_order {
        for context in plan
            .contexts
            .iter_mut()
            .filter(|c| c.context_id == "scene-root")
        {
            for scope in context
                .scopes
                .iter_mut()
                .filter(|s| held.contains(&s.frame_scope.frame))
            {
                for element in scope.elements.iter_mut().filter(|e| e.layer_uid == "L0") {
                    let z = element.order.static_value.value().unwrap()[0]
                        .as_i64()
                        .unwrap();
                    element.order.static_value = Presence::Present(vec![Number::from(z + 1)]);
                }
            }
        }
    }
    out
}
fn unshifted_records(row: &Row, _: &ObjectDocument, mut out: ObjectDocument) -> ObjectDocument {
    if row.op == "gap" {
        let cf: u32 = row.args.parse().unwrap();
        for record in out.objects.iter_mut().filter(|r| scene_authored(r)) {
            let layer: usize = record.target.layer_uid[1..].parse().unwrap();
            if row.roles.as_bytes()[layer] == b'G' && record.target.frame_scope.frame > cf + 1 {
                record.target.frame_scope.frame -= 1;
            }
        }
    }
    out
}
fn gap_moved_content(row: &Row) -> bool {
    if row.op != "gap" {
        return false;
    }
    let cf: usize = row.args.parse().unwrap();
    row.roles.chars().enumerate().any(|(layer, role)| {
        role == 'G'
            && (cf + 1..row.input[layer].len()).any(|f| !row.input[layer][f].strokes.is_empty())
    })
}
fn clear_leaves_records(
    row: &Row,
    input: &ObjectDocument,
    mut out: ObjectDocument,
) -> ObjectDocument {
    if row.op == "clearkey" && row.chg {
        let cell: u32 = row.args.parse().unwrap();
        let kept = |r: &&ObjectRecord| {
            scene_authored(r) && r.target.layer_uid == "L0" && r.target.frame_scope.frame == cell
        };
        out.objects
            .extend(input.objects.iter().filter(kept).cloned());
    }
    out
}
fn shifted_reference(row: &Row, _: &ObjectDocument, mut out: ObjectDocument) -> ObjectDocument {
    // the removal map applied to the Reference records: frame 0 falls inside the span exactly when min is 0
    if row.op == "rm" && row.chg && row.args.starts_with("0:") {
        out.objects
            .retain(|r| r.target.frame_scope.kind != FrameScopeKind::Reference);
    }
    out
}
fn stale_timing(row: &Row, input: &ObjectDocument, mut out: ObjectDocument) -> ObjectDocument {
    if row.op != "gap" && row.op != "rm" {
        out.layers[0].timing = input.layers[0].timing.clone();
    }
    out
}
fn stale_plan(_: &Row, input: &ObjectDocument, mut out: ObjectDocument) -> ObjectDocument {
    out.composition_order = input.composition_order.clone();
    out
}
fn rewritten_holders(_: &Row, _: &ObjectDocument, mut out: ObjectDocument) -> ObjectDocument {
    // the baseline never rewrites Motion tracks on insert or remove: any change to the table is a mistake
    out.property_holders = Presence::Present(Vec::new());
    out
}
fn at(layers: &[Vec<Cell>], layer: usize, frame: usize) -> Vec<(String, i64)> {
    let cell = layers[layer].get(frame);
    cell.map_or(Vec::new(), |c| {
        c.strokes.iter().map(|s| (s.id.clone(), s.z)).collect()
    })
}
fn plan_observable(row: &Row) -> bool {
    let total = row.out_total as usize;
    (0..row.output.len()).any(|l| {
        // a scope of the old plan beyond the new frame count fails storage admission even when nothing else differs
        let stranded = (total..row.input[l].len()).any(|f| !row.input[l][f].strokes.is_empty());
        stranded || (0..total).any(|f| at(&row.output, l, f) != at(&row.input, l, f))
    })
}

#[test]
fn unmodified_kernel_output_passes_every_row_in_every_form() {
    for build in Build::ALL {
        assert_eq!(
            failing(build, |_, _, out| out),
            Vec::<usize>::new(),
            "{build:?}"
        );
    }
}

#[test]
fn a_copy_that_renames_the_stroke_id_is_caught_on_the_rows_that_copy() {
    let seen = failing(Build::V1, rename_clones);
    assert_eq!(seen, expected(copied_content));
    assert_eq!(seen.len(), 83);
}

#[test]
fn a_copy_that_changes_the_authored_order_is_caught_through_the_order_plan() {
    let seen = failing(Build::V2, bump_clone_order);
    assert_eq!(seen, expected(copied_content));
    assert_eq!(seen.len(), 83);
    // the same tamper on a plan-less document changes nothing: the order plan is what is being compared
    assert_eq!(failing(Build::V1, bump_clone_order), Vec::<usize>::new());
}

#[test]
fn records_left_at_their_old_frames_after_an_insert_are_caught() {
    let seen = failing(Build::V1, unshifted_records);
    assert_eq!(seen, expected(gap_moved_content));
    assert_eq!(seen.len(), 128);
}

#[test]
fn a_clear_that_leaves_the_records_behind_is_caught() {
    let seen = failing(Build::V1, clear_leaves_records);
    assert_eq!(
        seen,
        expected(|r| r.op == "clearkey"
            && r.chg
            && !r.input[0][r.args.parse::<usize>().unwrap()]
                .strokes
                .is_empty())
    );
    assert_eq!(seen.len(), 29);
}

#[test]
fn reference_records_dragged_along_by_a_removal_are_caught() {
    let seen = failing(Build::V1Ref, shifted_reference);
    assert_eq!(
        seen,
        expected(|r| r.op == "rm" && r.chg && r.args.starts_with("0:"))
    );
    assert_eq!(seen.len(), 86);
    // plain documents carry no Reference record, so the same tamper is invisible there
    assert_eq!(failing(Build::V1, shifted_reference), Vec::<usize>::new());
}

#[test]
fn a_stale_timing_is_caught_on_every_edit_that_changes_the_flags() {
    let seen = failing(Build::V1, stale_timing);
    assert_eq!(
        seen,
        expected(|r| !matches!(r.op.as_str(), "gap" | "rm") && r.chg)
    );
    assert_eq!(seen.len(), 398);
}

#[test]
fn a_stale_order_plan_is_caught_wherever_the_strokes_moved() {
    let seen = failing(Build::V2, stale_plan);
    assert_eq!(seen, expected(plan_observable));
    assert_eq!(seen.len(), 531);
    assert_eq!(
        failing(Build::V1, stale_plan),
        Vec::<usize>::new(),
        "a v1 document has no plan to go stale"
    );
}

#[test]
fn a_rewritten_property_holder_table_is_caught_on_every_row() {
    let every: Vec<usize> = rows().iter().map(|row| row.id).collect();
    for build in [Build::V1, Build::V2] {
        assert_eq!(failing(build, rewritten_holders), every, "{build:?}");
    }
    assert_eq!(every.len(), 1099);
}
