//! The range join and the record mapping against the verbatim dense baseline editor (fbe4b8a3).
//!
//! Every shift row is one run of the baseline function over three layers with full range records. The
//! real `join_structure_ranges` rewrites a document built from the row (through the real codec) and the
//! resulting `range` members, as wire JSON, must equal what the baseline left behind: shifted frames,
//! untouched marker names / colors / link objects, absent members still absent. Targeting for F5 is
//! taken from the baseline's own observation (`G` = the layer spliced a slot). Split rows run the record
//! level cut. Wrong-edit controls: object_frame_edit_range_records_controls.rs.
use super::{
    join_structure_ranges, range_edit_for, shift_marker_records, RangeJoinError, StructureEdit,
};
use crate::object_document::{
    split_layer_range, LayerRange, MarkerRecord, ObjectDocument, RangeEdit, RangeEditError,
    RangeSplit,
};

#[path = "object_frame_edit_range_records_rows.rs"]
#[allow(dead_code)]
pub(crate) mod rows;
use rows::{range_doc, range_json, shift_rows, Outcome, Row, ShiftRow};

/// The layers the baseline really spliced (F5 `G` roles), as the transaction would derive them.
pub(crate) fn targeted_by_roles(row: &ShiftRow) -> Vec<&str> {
    row.layers
        .iter()
        .zip(row.roles.chars())
        .filter(|(_, role)| *role == 'G')
        .map(|(layer, _)| layer.uid.as_str())
        .collect()
}

fn structure_edit<'a>(row: &ShiftRow, targeted: &'a [&'a str]) -> StructureEdit<'a> {
    match row.op.as_str() {
        "insert" => StructureEdit::Insert {
            cell: row.cursor,
            targeted,
        },
        "extend" => StructureEdit::Extend {
            cell: row.cursor,
            count: row.count,
        },
        "dupkey" => StructureEdit::Duplicate { cell: row.cursor },
        "remove" => StructureEdit::Remove {
            from: row.cursor,
            to: row.cursor,
        },
        "span" => StructureEdit::Remove {
            from: row.cursor,
            to: row.cursor + row.count - 1,
        },
        other => panic!("unknown oracle op {other}"),
    }
}

/// The result the native operation must produce: the baseline span of exactly one frame is the native
/// single-frame removal, so its expectation is the baseline `removeFrame` run on the same input.
pub(crate) fn expected(row: &ShiftRow) -> Option<&Outcome> {
    if row.op == "span" && row.count == 1 {
        row.as_remove.as_ref().or(row.out.as_ref())
    } else {
        row.out.as_ref()
    }
}

pub(crate) fn compare_ranges(
    row: &ShiftRow,
    document: &ObjectDocument,
    want: &Outcome,
) -> Result<(), String> {
    if document.total_frames != want.total {
        return Err(format!(
            "{} total {} != {}",
            row.op, document.total_frames, want.total
        ));
    }
    for (index, wanted) in want.ranges.iter().enumerate() {
        let got = range_json(document, index);
        if got != *wanted {
            return Err(format!(
                "{} total {} cursor {} count {} layer {index}: got {got} want {wanted}",
                row.op, row.total, row.cursor, row.count
            ));
        }
    }
    Ok(())
}

/// One row through the real mapping `StructureEdit` -> kernel edit -> join.
pub(crate) fn replay_real(row: &ShiftRow) -> Result<(), String> {
    let targeted = targeted_by_roles(row);
    let edit = structure_edit(row, &targeted);
    let Some(want) = expected(row) else {
        return match range_edit_for(&edit, row.total) {
            Err(RangeJoinError::Edit(RangeEditError::WouldEmptyTimeline)) => Ok(()),
            other => Err(format!("a refused row was accepted: {other:?}")),
        };
    };
    let mut after = range_doc(&row.layers, row.total);
    after.total_frames = want.total;
    let joined = join_structure_ranges(after, row.total, &edit)
        .map_err(|error| format!("{} join refused: {error:?}", row.op))?;
    compare_ranges(row, &joined.document, want)
}

/// The kernel edit the real mapping builds for a non-refused row.
fn real_edit(row: &ShiftRow) -> RangeEdit {
    let targeted = targeted_by_roles(row);
    range_edit_for(&structure_edit(row, &targeted), row.total).unwrap()
}

#[test]
fn every_shift_row_matches_the_real_join() {
    let mut checked = 0;
    let mut refused = 0;
    for row in shift_rows() {
        if expected(&row).is_none() {
            refused += 1;
        } else {
            checked += 1;
        }
        if let Err(message) = replay_real(&row) {
            panic!("{message}");
        }
    }
    assert_eq!(
        (checked, refused),
        (334, 17),
        "351 shift rows in the oracle"
    );
}

#[test]
fn work_area_end_and_composition_markers_follow_the_same_edit() {
    let mut markers_checked = 0;
    for row in shift_rows() {
        let Some(want) = expected(&row) else { continue };
        let edit = real_edit(&row);
        assert_eq!(edit.work_area_out(row.wa), Ok(want.wa), "{} wa", row.op);
        if row.comp.is_null() {
            assert!(
                want.comp.is_null(),
                "absent composition markers stay absent"
            );
            continue;
        }
        let stored: Vec<MarkerRecord> = serde_json::from_value(row.comp.clone()).unwrap();
        let shifted = shift_marker_records(&edit, &stored).unwrap();
        assert_eq!(
            serde_json::to_value(&shifted).unwrap(),
            want.comp,
            "{}",
            row.op
        );
        markers_checked += 1;
    }
    assert_eq!(
        markers_checked, 281,
        "non-refused rows with composition markers"
    );
}

#[test]
fn every_split_row_matches_the_record_cut() {
    let (mut cuts, mut refused, mut dropped) = (0, 0, 0);
    for row in rows::rows() {
        let Row::Split(row) = row else { continue };
        let stored: Option<LayerRange> = if row.layer.is_null() {
            None
        } else {
            Some(serde_json::from_value(row.layer.clone()).unwrap())
        };
        let got = split_layer_range(stored.as_ref(), row.cut, row.eff_in, row.eff_out).unwrap();
        match (&row.out, got) {
            (None, RangeSplit::OutsideLayer) => refused += 1,
            (
                Some((original, second, link_dropped)),
                RangeSplit::Cut {
                    original: got_original,
                    second: got_second,
                    link_dropped: got_dropped,
                },
            ) => {
                assert_eq!(serde_json::to_value(&got_original).unwrap(), *original);
                assert_eq!(serde_json::to_value(&got_second).unwrap(), *second);
                assert_eq!(got_dropped, *link_dropped);
                cuts += 1;
                dropped += usize::from(got_dropped);
            }
            (want, got) => panic!(
                "cut {} of {}: want {want:?}, got {got:?}",
                row.cut, row.layer
            ),
        }
    }
    assert_eq!((cuts, refused, dropped), (36, 24, 20));
}

#[test]
fn a_native_one_frame_remove_shifts_ranges_unlike_the_baseline_span_of_one() {
    // Decision pinned for the lead: `from == to` is the baseline removeFrame; the baseline span of one
    // cell (a selection) shifts nothing, and that single case is not reachable natively.
    let (mut spans, mut differing) = (0, 0);
    for row in shift_rows() {
        if row.op != "span" || row.count != 1 || row.out.is_none() {
            continue;
        }
        spans += 1;
        let span = row.out.as_ref().unwrap();
        let single = row.as_remove.as_ref().unwrap();
        if span.ranges != single.ranges {
            differing += 1;
        }
        assert_eq!(expected(&row).unwrap().ranges, single.ranges);
    }
    assert_eq!((spans, differing), (32, 32));
}

#[test]
fn the_oracle_reaches_every_branch() {
    let mut reach = std::collections::BTreeMap::new();
    let mut bump = |key: &str| *reach.entry(key.to_string()).or_insert(0usize) += 1;
    for row in shift_rows() {
        if row.out.is_none() {
            bump(&format!("{}_refused", row.op));
            continue;
        }
        let want = expected(&row).unwrap();
        for (layer, wanted) in row.layers.iter().zip(&want.ranges) {
            if layer.range != *wanted {
                bump(&format!("{}_layers_changed", row.op));
            }
            if layer.range.is_null() {
                bump(&format!("{}_layer_without_range", row.op));
            }
            if layer.locked {
                bump(&format!("{}_locked_layer", row.op));
            }
            if layer.range["timeLink"].is_null() && layer.range.get("timeLink").is_some() {
                bump(&format!("{}_null_link", row.op));
            }
            if layer.range["timeLink"].is_object() {
                bump(&format!("{}_live_link", row.op));
            }
        }
        if row.op == "insert" {
            for role in row.roles.chars() {
                bump(&format!("insert_role_{role}"));
            }
            if row.roles == "PPP" {
                bump("insert_all_padded");
            }
        }
    }
    let get = |key: &str| reach.get(key).copied().unwrap_or(0);
    assert_eq!(get("insert_layers_changed"), 124);
    assert_eq!(get("remove_layers_changed"), 146);
    assert_eq!(get("span_layers_changed"), 71);
    assert_eq!(
        get("extend_layers_changed") + get("dupkey_layers_changed"),
        0
    );
    assert_eq!((get("insert_role_G"), get("insert_role_P")), (167, 166));
    assert_eq!(get("insert_all_padded"), 6);
    assert_eq!(get("insert_locked_layer"), 97);
    assert_eq!(get("insert_layer_without_range"), 35);
    assert_eq!(get("remove_refused") + get("span_refused"), 17);
    assert_eq!(get("insert_live_link") + get("remove_live_link"), 218);
    assert_eq!(get("insert_null_link") + get("remove_null_link"), 75);
}
