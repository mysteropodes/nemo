//! Mutation and negative controls for the range kernel.
//!
//! `apply` below is the kernel written a second time with one switch per plausible mistake. With
//! every switch off it must reproduce the baseline on all oracle rows (the pass control); with one
//! switch on it must disagree on exactly the pinned number of rows, so a comparison that cannot
//! fail is caught here. The counts were produced independently in Node by tools/kernel_port.js.
use super::tests::{
    all_rows, expected, kernel, layer_text, mismatches, parse_layer, split_mismatches, split_rows,
    Applied, Row, SplitRow,
};
use super::{
    split_cut, FrameBound, FrameEditKind, LayerRanges, LayerRole, RangeEdit, RangeEditError,
    SplitCutOutcome, TimeLinkSlot,
};

#[derive(Clone, Copy)]
struct Mu {
    ge: bool,
    flip: bool,
    none_zero: bool,
    padded_shift: bool,
    span_shifts: bool,
    extend_shifts: bool,
    wa_decrement: bool,
    comp_frozen: bool,
    clear_link: bool,
}

const OFF: Mu = Mu {
    ge: false,
    flip: false,
    none_zero: false,
    padded_shift: false,
    span_shifts: false,
    extend_shifts: false,
    wa_decrement: false,
    comp_frozen: false,
    clear_link: false,
};

fn shift(bound: FrameBound, cursor: i64, delta: i64, mu: &Mu) -> FrameBound {
    match bound {
        FrameBound::At(frame) => {
            let hit = if mu.ge {
                frame >= cursor
            } else {
                frame > cursor
            };
            if hit {
                FrameBound::At(frame + delta)
            } else {
                bound
            }
        }
        _ if mu.none_zero => FrameBound::At(0),
        _ => bound,
    }
}

fn shift_list(
    list: &Option<Vec<FrameBound>>,
    cursor: i64,
    delta: i64,
    mu: &Mu,
) -> Option<Vec<FrameBound>> {
    list.as_ref().map(|items| {
        items
            .iter()
            .map(|item| shift(*item, cursor, delta, mu))
            .collect()
    })
}

fn apply(row: &Row, mu: &Mu) -> Applied {
    let edit = RangeEdit {
        kind: row.kind(),
        cursor: row.cursor,
        total: row.total,
    };
    let new_total = edit.new_total()?;
    let shifts = match row.kind() {
        FrameEditKind::InsertFrame | FrameEditKind::RemoveFrame => true,
        FrameEditKind::RemoveFrameSpan { .. } => mu.span_shifts,
        FrameEditKind::ExtendExposure { .. } => mu.extend_shifts,
        FrameEditKind::DuplicateKeyframe => false,
    };
    let sign = if mu.flip { -1 } else { 1 };
    let delta = match row.kind() {
        FrameEditKind::InsertFrame => sign,
        FrameEditKind::RemoveFrame => -sign,
        FrameEditKind::RemoveFrameSpan { .. } => -1,
        FrameEditKind::ExtendExposure { count } => i64::from(count),
        FrameEditKind::DuplicateKeyframe => 0,
    };
    let cursor = i64::from(row.cursor);
    let last = i64::from(new_total) - 1;
    let wa = match row.kind() {
        FrameEditKind::InsertFrame | FrameEditKind::DuplicateKeyframe => {
            if row.wa < last {
                row.wa + 1
            } else {
                row.wa
            }
        }
        FrameEditKind::RemoveFrame | FrameEditKind::RemoveFrameSpan { .. } => {
            if mu.wa_decrement {
                row.wa - 1
            } else if row.wa >= i64::from(new_total) {
                last
            } else {
                row.wa
            }
        }
        FrameEditKind::ExtendExposure { .. } => {
            if row.wa < last {
                last
            } else {
                row.wa
            }
        }
    };
    let comp = if shifts && !mu.comp_frozen {
        shift_list(&row.comp, cursor, delta, mu)
    } else {
        row.comp.clone()
    };
    let mut out = vec![
        new_total.to_string(),
        wa.to_string(),
        super::tests::markers_text(&comp),
    ];
    for (index, layer) in row.layers.iter().enumerate() {
        let do_shift = shifts
            && !(row.kind() == FrameEditKind::InsertFrame
                && row.role(index) == LayerRole::Padded
                && !mu.padded_shift);
        let mut next = layer.clone();
        if do_shift {
            next = LayerRanges {
                in_point: shift(layer.in_point, cursor, delta, mu),
                out_point: shift(layer.out_point, cursor, delta, mu),
                markers: shift_list(&layer.markers, cursor, delta, mu),
                time_link: layer.time_link,
            };
        }
        if mu.clear_link && shifts && layer.time_link == TimeLinkSlot::Linked {
            next.time_link = TimeLinkSlot::Absent;
        }
        out.push(layer_text(&next));
    }
    Ok(out)
}

fn count_with(mu: Mu) -> usize {
    mismatches(&|row: &Row| apply(row, &mu))
}

#[test]
fn switches_off_reproduce_the_baseline_and_the_kernel() {
    assert_eq!(count_with(OFF), 0);
    for row in all_rows() {
        assert_eq!(apply(&row, &OFF), kernel(&row));
    }
}

#[test]
fn comparing_against_a_tampered_row_fails() {
    // The comparison reads the output: change one expected token of a row that shifts and it must fail.
    let rows = all_rows();
    let row = rows
        .iter()
        .find(|r| r.op == "insert" && r.expect[3] != layer_text(&r.layers[0]))
        .unwrap();
    let mut tampered: Vec<String> = row.expect.iter().map(|t| t.to_string()).collect();
    tampered[3] = layer_text(&row.layers[0]);
    assert_ne!(kernel(row).unwrap(), tampered);
    assert_eq!(kernel(row), expected(row));
}

#[test]
fn shifting_at_the_cursor_instead_of_after_it_is_caught() {
    assert_eq!(count_with(Mu { ge: true, ..OFF }), 195);
}

#[test]
fn flipped_shift_direction_is_caught() {
    assert_eq!(count_with(Mu { flip: true, ..OFF }), 378);
}

#[test]
fn coercing_absent_and_null_to_zero_is_caught() {
    assert_eq!(
        count_with(Mu {
            none_zero: true,
            ..OFF
        }),
        515
    );
}

#[test]
fn shifting_padded_layers_is_caught() {
    assert_eq!(
        count_with(Mu {
            padded_shift: true,
            ..OFF
        }),
        170
    );
}

#[test]
fn shifting_ranges_on_span_removal_is_caught() {
    assert_eq!(
        count_with(Mu {
            span_shifts: true,
            ..OFF
        }),
        100
    );
}

#[test]
fn shifting_ranges_on_extend_exposure_is_caught() {
    assert_eq!(
        count_with(Mu {
            extend_shifts: true,
            ..OFF
        }),
        110
    );
}

#[test]
fn decrementing_the_work_area_unconditionally_is_caught() {
    assert_eq!(
        count_with(Mu {
            wa_decrement: true,
            ..OFF
        }),
        104
    );
}

#[test]
fn freezing_the_composition_markers_is_caught() {
    assert_eq!(
        count_with(Mu {
            comp_frozen: true,
            ..OFF
        }),
        151
    );
}

#[test]
fn dropping_the_link_on_insert_or_remove_is_caught() {
    assert_eq!(
        count_with(Mu {
            clear_link: true,
            ..OFF
        }),
        409
    );
}

fn split_with(
    row: &SplitRow,
    lt: bool,
    out_at_cut: bool,
    keep_link: bool,
    null_drops: bool,
    drop_markers: bool,
    second_keeps_link: bool,
) -> Vec<String> {
    let original = &row.original;
    let outside = if lt {
        row.cut < row.eff_in || row.cut > row.eff_out
    } else {
        row.cut <= row.eff_in || row.cut > row.eff_out
    };
    if outside {
        return vec!["refused".to_string(), "-".to_string(), "-".to_string()];
    }
    let dropped = original.time_link == TimeLinkSlot::Linked && !keep_link;
    let first_link = if dropped || (null_drops && original.time_link == TimeLinkSlot::Null) {
        TimeLinkSlot::Absent
    } else {
        original.time_link
    };
    let first = LayerRanges {
        in_point: original.in_point,
        out_point: FrameBound::At(if out_at_cut { row.cut } else { row.cut - 1 }),
        markers: original.markers.clone(),
        time_link: first_link,
    };
    let second = LayerRanges {
        in_point: FrameBound::At(row.cut),
        out_point: FrameBound::At(row.eff_out),
        markers: if drop_markers {
            None
        } else {
            original.markers.clone()
        },
        time_link: if second_keeps_link {
            original.time_link
        } else {
            TimeLinkSlot::Absent
        },
    };
    vec!["cut".to_string(), layer_text(&first), layer_text(&second)]
}

#[test]
fn split_switches_off_reproduce_the_baseline() {
    let plain = |r: &SplitRow| split_with(r, false, false, false, false, false, false);
    assert_eq!(split_mismatches(&plain), 0);
}

#[test]
fn split_mutants_are_caught() {
    let lt = |r: &SplitRow| split_with(r, true, false, false, false, false, false);
    assert_eq!(split_mismatches(&lt), 144);
    let at_cut = |r: &SplitRow| split_with(r, false, true, false, false, false, false);
    assert_eq!(split_mismatches(&at_cut), 216);
    let keep = |r: &SplitRow| split_with(r, false, false, true, false, false, false);
    assert_eq!(split_mismatches(&keep), 72);
    let null = |r: &SplitRow| split_with(r, false, false, false, true, false, false);
    assert_eq!(split_mismatches(&null), 72);
    let markers = |r: &SplitRow| split_with(r, false, false, false, false, true, false);
    assert_eq!(split_mismatches(&markers), 162);
    let second = |r: &SplitRow| split_with(r, false, false, false, false, false, true);
    assert_eq!(split_mismatches(&second), 144);
}

#[test]
fn split_controls_read_the_kernel_not_a_copy() {
    // The mutant harness above is a second implementation; pin that the real kernel agrees with it
    // on every row where the two paths are supposed to coincide.
    for row in split_rows() {
        let real = match split_cut(row.cut, row.eff_in, row.eff_out, &row.original) {
            SplitCutOutcome::OutsideLayer => {
                vec!["refused".to_string(), "-".to_string(), "-".to_string()]
            }
            SplitCutOutcome::Cut(c) => vec![
                "cut".to_string(),
                layer_text(&c.original),
                layer_text(&c.second),
            ],
        };
        assert_eq!(
            real,
            split_with(&row, false, false, false, false, false, false)
        );
    }
    assert_eq!(parse_layer("1/n/e/L").time_link, TimeLinkSlot::Linked);
}

#[test]
fn invalid_edits_are_typed_refusals_not_row_mismatches() {
    let row = all_rows()
        .into_iter()
        .find(|r| r.op == "remove" && r.refused())
        .unwrap();
    assert_eq!(kernel(&row), Err(RangeEditError::WouldEmptyTimeline));
    assert_eq!(mismatches(&|r: &Row| kernel(r)), 0);
}
