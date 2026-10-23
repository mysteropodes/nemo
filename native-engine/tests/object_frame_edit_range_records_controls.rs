//! Negative controls for the range join. Each control takes the REAL join over every non-refused oracle
//! row with ONE wrong edit and requires the failing-row count to equal the number the leaf's Node model
//! (tools/model_check.js, written separately from this code) prints for the same mistake: a join that
//! ignored selection, moved the cursor, or confused the one-frame removal with a span would fail
//! exactly those rows. The unmodified pass shows the harness itself is real, and a tampered expectation
//! must fail exactly the row it touches.
use super::object_frame_edit_range_records_oracle::rows::{range_doc, shift_rows, ShiftRow};
use super::object_frame_edit_range_records_oracle::{
    compare_ranges, expected, replay_real, targeted_by_roles,
};
use super::{join_structure_ranges, join_with_edit, RangeJoinError, StructureEdit};
use crate::object_document::{FrameEditKind, RangeEdit};
use serde_json::json;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mistake {
    Nothing,
    EveryLayerTargeted,
    CursorOneTooFar,
    OneFrameRemovedAsSpan,
}

/// The kernel kind the transaction uses for this row, before any mistake.
fn base_kind(row: &ShiftRow) -> FrameEditKind {
    match row.op.as_str() {
        "insert" => FrameEditKind::InsertFrame,
        "extend" => FrameEditKind::ExtendExposure { count: row.count },
        "dupkey" => FrameEditKind::DuplicateKeyframe,
        "remove" => FrameEditKind::RemoveFrame,
        "span" if row.count == 1 => FrameEditKind::RemoveFrame,
        "span" => FrameEditKind::RemoveFrameSpan { count: row.count },
        other => panic!("unknown oracle op {other}"),
    }
}

/// Failing rows and rows replayed (rows whose wrong edit is itself invalid for the kernel are skipped).
fn replay_with(rows: &[ShiftRow], mistake: Mistake) -> (usize, usize) {
    let (mut failing, mut replayed) = (0, 0);
    for row in rows {
        let Some(want) = expected(row) else { continue };
        let mut kind = base_kind(row);
        if mistake == Mistake::OneFrameRemovedAsSpan && kind == FrameEditKind::RemoveFrame {
            kind = FrameEditKind::RemoveFrameSpan { count: 1 };
        }
        let cursor = row.cursor + u32::from(mistake == Mistake::CursorOneTooFar);
        let edit = RangeEdit {
            kind,
            cursor,
            total: row.total,
        };
        if edit.new_total().is_err() {
            continue;
        }
        let targeted: Vec<&str> = if mistake == Mistake::EveryLayerTargeted {
            row.layers.iter().map(|layer| layer.uid.as_str()).collect()
        } else {
            targeted_by_roles(row)
        };
        let mut after = range_doc(&row.layers, row.total);
        after.total_frames = want.total;
        replayed += 1;
        let joined = join_with_edit(after, &edit, &targeted).unwrap();
        if compare_ranges(row, &joined.document, want).is_err() {
            failing += 1;
        }
    }
    (failing, replayed)
}

#[test]
fn the_unmodified_join_passes_every_replayed_row() {
    assert_eq!(replay_with(&shift_rows(), Mistake::Nothing), (0, 334));
}

#[test]
fn a_join_that_targets_every_layer_is_caught_where_a_padded_layer_has_frames_to_move() {
    assert_eq!(
        replay_with(&shift_rows(), Mistake::EveryLayerTargeted),
        (94, 334)
    );
}

#[test]
fn a_cursor_one_frame_too_far_is_caught_on_rows_with_a_value_just_after_it() {
    assert_eq!(
        replay_with(&shift_rows(), Mistake::CursorOneTooFar),
        (69, 220)
    );
}

#[test]
fn a_one_frame_removal_treated_as_a_span_is_caught_wherever_it_must_shift() {
    assert_eq!(
        replay_with(&shift_rows(), Mistake::OneFrameRemovedAsSpan),
        (100, 334)
    );
}

#[test]
fn a_tampered_expectation_fails_exactly_the_row_it_touches() {
    let mut rows = shift_rows();
    let target = rows
        .iter()
        .position(|row| {
            expected(row).is_some_and(|want| {
                want.ranges
                    .iter()
                    .any(|range| range["markers"][0]["name"].is_string())
            })
        })
        .unwrap();
    let before = replay_real(&rows[target]);
    assert!(before.is_ok());
    let want = if rows[target].op == "span" && rows[target].count == 1 {
        rows[target].as_remove.as_mut().unwrap()
    } else {
        rows[target].out.as_mut().unwrap()
    };
    let index = want
        .ranges
        .iter()
        .position(|range| range["markers"][0]["name"].is_string())
        .unwrap();
    want.ranges[index]["markers"][0]["name"] = json!("tampered");
    let failing = rows.iter().filter(|row| replay_real(row).is_err()).count();
    assert_eq!(failing, 1);
    assert!(replay_real(&rows[target]).is_err());
}

#[test]
fn a_join_for_the_wrong_frame_count_is_refused_and_changes_nothing() {
    let rows = shift_rows();
    let row = rows
        .iter()
        .find(|row| row.op == "insert")
        .expect("an insert row");
    let before = range_doc(&row.layers, row.total);
    let targeted = targeted_by_roles(row);
    let edit = StructureEdit::Insert {
        cell: row.cursor,
        targeted: &targeted,
    };
    // the document still has the frame count of BEFORE the insert
    let refused = join_structure_ranges(before.clone(), row.total, &edit);
    assert_eq!(refused, Err(RangeJoinError::TotalMismatch));
    assert_eq!(before, range_doc(&row.layers, row.total));
}
