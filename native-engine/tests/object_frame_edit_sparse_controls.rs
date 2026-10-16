//! Negative controls for the sparse frame-edit kernel: each deliberately wrong edit is run through
//! the SAME oracle comparison as the real kernel and must fail on exactly the rows where the
//! mistake is observable. A control that passed everything, or failed for another reason, would
//! mean the oracle comparison cannot see that class of bug.
use super::super::{LayerTiming, LayerTimingError, TimingKey, MAX_ENTRIES};
use super::tests::{cases, check, failures, real_apply, Applier, Case};
use super::{finish, TimingEdit};

type Outcome = Result<TimingEdit, LayerTimingError>;

fn gap_rows() -> Vec<Case> {
    cases()
        .into_iter()
        .filter(|c| c.op == "gap" && c.role == "G")
        .collect()
}
fn cell_at(case: &Case, frame: usize) -> char {
    case.input.as_bytes()[frame] as char
}
fn count(op: &str, wanted: impl Fn(&Case) -> bool) -> usize {
    cases().iter().filter(|c| c.op == op && wanted(c)).count()
}

// ---- insert gap: shift direction and the boundary at cf ---------------------------------------

/// A parameterised gap: entries at `frame > cf` (or `>= cf`) move by `delta`. The correct kernel is
/// `(false, +1)`; every other setting is a mutation.
fn shifted(t: &LayerTiming, cf: u32, total: u32, inclusive: bool, delta: i64) -> Outcome {
    let moved = |frame: u32| {
        let hit = if inclusive { frame >= cf } else { frame > cf };
        if hit {
            (i64::from(frame) + delta) as u32
        } else {
            frame
        }
    };
    let mut next = t.clone();
    next.keys
        .iter_mut()
        .for_each(|key| key.frame = moved(key.frame));
    next.samples
        .iter_mut()
        .for_each(|sample| sample.frame = moved(sample.frame));
    finish(next, total + 1)
}
fn gap_with(inclusive: bool, delta: i64) -> impl Fn(&Case, &LayerTiming) -> Outcome {
    move |case, t| {
        if case.op == "gap" && case.role == "G" {
            shifted(t, case.args[0], case.input.len() as u32, inclusive, delta)
        } else {
            real_apply(case, t)
        }
    }
}

#[test]
fn control_correct_shift_passes_every_gap_row() {
    let apply = gap_with(false, 1);
    assert!(
        failures("gap", &apply).is_empty(),
        "the comparison must be able to pass"
    );
}

#[test]
fn mutated_shift_direction_is_caught_on_every_row_with_a_later_entry() {
    let apply = gap_with(false, -1);
    let expected = gap_rows()
        .iter()
        .filter(|c| c.input[c.args[0] as usize + 1..].chars().any(|x| x != '.'))
        .count();
    assert_eq!(failures("gap", &apply).len(), expected);
    assert_eq!(expected, 505);
}

#[test]
fn mutated_off_by_one_at_cf_is_caught_exactly_when_an_entry_sits_on_cf() {
    let apply = gap_with(true, 1);
    let expected = gap_rows()
        .iter()
        .filter(|c| cell_at(c, c.args[0] as usize) != '.')
        .count();
    assert_eq!(failures("gap", &apply).len(), expected);
    assert_eq!(expected, 556);
}

// ---- promote: sample/key collision, cap overflow, publishing a no-op ---------------------------

/// Promote that forgets to drop the sample under the new key.
fn promote_keeping_samples(t: &LayerTiming, cells: &[u32], total: u32) -> Outcome {
    let mut next = t.clone();
    for &cell in cells {
        if let Err(at) = next.keys.binary_search_by_key(&cell, |key| key.frame) {
            next.keys.insert(at, TimingKey { frame: cell });
        }
    }
    if next == *t {
        return Ok(TimingEdit::NoChange);
    }
    finish(next, total)
}

#[test]
fn mutated_promote_leaving_the_sample_collides_and_is_refused_by_validation() {
    let apply = |case: &Case, t: &LayerTiming| match case.op {
        "promote" => promote_keeping_samples(t, &case.args, case.input.len() as u32),
        _ => real_apply(case, t),
    };
    let bad = failures("promote", &apply);
    let expected = count("promote", |c| {
        c.args
            .iter()
            .any(|a| matches!(cell_at(c, *a as usize), 'I' | 'M' | 'N'))
    });
    assert_eq!(bad.len(), expected);
    assert_eq!(expected, 644);
    // The refusal comes from validate-after-mutation, not from the oracle equality.
    assert!(
        bad.iter()
            .all(|line| line.starts_with("Err(Invalid) but baseline")),
        "{:?}",
        bad.first()
    );
}

#[test]
fn mutated_promote_without_the_entry_cap_overflows_and_is_refused_by_validation() {
    let mut full = LayerTiming::blank_drawing();
    full.keys = (0..MAX_ENTRIES as u32)
        .map(|frame| TimingKey { frame })
        .collect();
    assert_eq!(full.validate(5000), Ok(()));
    let mut naive = full.clone();
    naive.keys.push(TimingKey { frame: 4500 });
    assert_eq!(
        naive.keys.len() + naive.samples.len(),
        MAX_ENTRIES + 1,
        "the mutant really overflows"
    );
    assert_eq!(finish(naive, 5000), Err(LayerTimingError::Invalid));
    assert_eq!(
        full.promote_cell(4500, 5000),
        Err(LayerTimingError::Invalid)
    );
}

#[test]
fn mutated_promote_that_publishes_a_no_change_is_caught_on_every_unchanged_row() {
    let apply = |case: &Case, t: &LayerTiming| match case.op {
        "promote" => real_apply(case, t).map(|edit| match edit {
            TimingEdit::NoChange => TimingEdit::Changed(t.clone()),
            changed => changed,
        }),
        _ => real_apply(case, t),
    };
    let expected = count("promote", |c| !c.chg);
    assert_eq!(failures("promote", &apply).len(), expected);
    assert_eq!(expected, 294);
}

// ---- remove span: shift distance, entries inside the span -------------------------------------

/// Remove with a configurable later-entry shift and optional retention of entries inside.
fn remove_with(
    t: &LayerTiming,
    min: u32,
    max: u32,
    total: u32,
    less: u32,
    keep_inside: bool,
) -> Outcome {
    let span = max - min + 1;
    if span >= total {
        return Ok(TimingEdit::NoChange);
    }
    let mut next = t.clone();
    let after = |frame: u32| frame > max;
    if !keep_inside {
        next.keys.retain(|key| key.frame < min || after(key.frame));
        next.samples
            .retain(|sample| sample.frame < min || after(sample.frame));
    }
    next.keys
        .iter_mut()
        .filter(|key| after(key.frame))
        .for_each(|key| key.frame -= span - less);
    next.samples
        .iter_mut()
        .filter(|s| after(s.frame))
        .for_each(|s| s.frame -= span - less);
    finish(next, total - span)
}
fn rm_with(less: u32, keep_inside: bool) -> impl Fn(&Case, &LayerTiming) -> Outcome {
    move |case, t| match case.op {
        "rm" => remove_with(
            t,
            case.args[0],
            case.args[1],
            case.input.len() as u32,
            less,
            keep_inside,
        ),
        _ => real_apply(case, t),
    }
}

#[test]
fn control_correct_removal_passes_every_row() {
    assert!(failures("rm", &rm_with(0, false)).is_empty());
}

#[test]
fn mutated_removal_shift_is_caught_on_every_changed_row_with_a_later_entry() {
    let apply = rm_with(1, false);
    let expected = count("rm", |c| {
        c.chg && c.input[c.args[1] as usize + 1..].chars().any(|x| x != '.')
    });
    assert_eq!(failures("rm", &apply).len(), expected);
    assert_eq!(expected, 773);
}

#[test]
fn mutated_removal_keeping_entries_inside_the_span_is_caught() {
    let apply = rm_with(0, true);
    let expected = count("rm", |c| {
        c.chg
            && c.input[c.args[0] as usize..=c.args[1] as usize]
                .chars()
                .any(|x| x != '.')
    });
    assert_eq!(failures("rm", &apply).len(), expected);
    assert_eq!(expected, 1031);
}

// ---- clear and demote -------------------------------------------------------------------------

#[test]
fn mutated_clear_that_spares_samples_is_caught_on_every_row_selecting_a_sample() {
    let apply = |case: &Case, t: &LayerTiming| match case.op {
        "clear" => {
            let mut next = t.clone();
            next.keys.retain(|key| !case.args.contains(&key.frame));
            if next == *t {
                Ok(TimingEdit::NoChange)
            } else {
                finish(next, case.input.len() as u32)
            }
        }
        _ => real_apply(case, t),
    };
    let expected = count("clear", |c| {
        c.args
            .iter()
            .any(|a| matches!(cell_at(c, *a as usize), 'I' | 'M' | 'N'))
    });
    assert_eq!(failures("clear", &apply).len(), expected);
    assert_eq!(expected, 530);
}

#[test]
fn mutated_demote_that_also_removes_the_end_key_is_caught_when_the_end_is_an_entry() {
    let apply = |case: &Case, t: &LayerTiming| match case.op {
        "demote" => {
            let (from, to) = (case.args[0], case.args[1]);
            let mut next = t.clone();
            next.keys
                .retain(|key| !(key.frame > from && key.frame <= to));
            next.samples.retain(|s| !(s.frame > from && s.frame <= to));
            if next == *t {
                Ok(TimingEdit::NoChange)
            } else {
                finish(next, case.input.len() as u32)
            }
        }
        _ => real_apply(case, t),
    };
    let expected = count("demote", |c| cell_at(c, c.args[1] as usize) != '.');
    assert_eq!(failures("demote", &apply).len(), expected);
    assert_eq!(expected, 651);
}

#[test]
fn the_oracle_comparison_rejects_a_wrong_exposure_even_when_the_sparse_state_matches() {
    // Same keys and samples as the expected output but a corrupted exposure literal.
    let case = Case {
        op: "clearkey",
        role: "",
        args: vec![1],
        chg: true,
        undo: true,
        input: "KK.",
        output: "K..",
        exposure: "*01",
    };
    let apply: &Applier = &real_apply;
    let error = check(&case, apply).unwrap_err();
    assert!(error.starts_with("exposure differs"), "{error}");
}
