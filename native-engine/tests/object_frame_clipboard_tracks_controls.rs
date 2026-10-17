//! Negative controls for the Motion-track repeat. Each runs ONE plausible mistake over the oracle rows through the same
//! comparison the positive test uses, and must fail on exactly the rows where the baseline itself differs. Where that
//! set comes from: the generator ran the verbatim baseline with the same mistake (oracle.txt, column `mut`), so the
//! expected failing rows are carried by the fixture, not derived here.
//!
//! The five faulty ARGUMENTS (times-1, times+1, bounds one frame short at the end / at the start, one layer short) run the
//! REAL kernel. The four structural mistakes (no sort after the merge, a range one frame too wide, a range excluding its first
//! frame, a destination one frame too far) have no argument form, so they run through `ported`, a deliberately wrong JSON port
//! of the loop: they prove that the oracle comparison can see each of them, not that the kernel has them. Rows where the
//! baseline keeps two keys on one frame (`dup`) are not used: the kernel refuses them by design.
use super::super::{repeat_motion_tracks, MAX_ROUNDS};
use super::rows::{
    decodes, document, document_json, matches_expected, rows, Form, Row, F_MINUS, F_ZERO, L_MINUS,
    NARROW, NO_SORT, SHIFT, T_MINUS, T_PLUS, WIDE,
};
use serde_json::{json, Value};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fault {
    None,
    TimesMinus,
    TimesPlus,
    ShortEnd,
    ShortStart,
    ShortLayer,
}
fn clamp(raw: i64) -> u32 {
    raw.clamp(1, i64::from(MAX_ROUNDS)) as u32
}
/// The arguments of one run: covered layers, first and last frame, rounds.
fn arguments(row: &Row, fault: Fault) -> (Vec<String>, u32, u32, u32) {
    let mut layers = row.layers();
    let (mut first, mut last, mut times) = (row.first, row.last, row.eff);
    match fault {
        Fault::None => {}
        Fault::TimesMinus => times = clamp(row.times - 1),
        Fault::TimesPlus => times = clamp(row.times + 1),
        Fault::ShortEnd if last > first => last -= 1,
        Fault::ShortStart if last > first => first += 1,
        Fault::ShortLayer if layers.len() > 1 => {
            layers.pop();
        }
        _ => {}
    }
    (layers, first, last, times)
}
/// The real kernel over a row's input with a possibly faulty choice, compared with the baseline's output.
fn check(row: &Row, form: Form, fault: Fault) -> Result<(), String> {
    let mut doc = document(row.total, &row.input, form);
    let (layers, first, last, times) = arguments(row, fault);
    repeat_motion_tracks(&mut doc, &layers, first, last, times)
        .map_err(|code| format!("{} {form:?}: {code:?}", row.label()))?;
    matches_expected(row, form, &doc)
}
/// Rows (ids) on which the faulty run differs from the baseline; each row must agree in all its forms.
fn failing(fault: Fault) -> Vec<usize> {
    let mut out = Vec::new();
    for row in rows().iter().filter(|row| !row.dup) {
        let outcomes: Vec<bool> = row
            .forms()
            .into_iter()
            .map(|form| check(row, form, fault).is_err())
            .collect();
        assert!(
            outcomes.iter().all(|o| *o == outcomes[0]),
            "{}: forms disagree",
            row.label()
        );
        if outcomes[0] {
            out.push(row.id);
        }
    }
    out
}
fn expected(bit: usize) -> Vec<usize> {
    rows()
        .iter()
        .filter(|row| !row.dup && row.mutants[bit])
        .map(|row| row.id)
        .collect()
}

#[test]
fn the_unmodified_request_passes_every_row_in_every_form() {
    assert!(failing(Fault::None).is_empty());
    assert_eq!(rows().iter().filter(|row| !row.dup).count(), 101);
}

#[test]
fn one_round_too_few_is_caught_exactly_where_the_baseline_differs() {
    let caught = failing(Fault::TimesMinus);
    assert_eq!(caught, expected(T_MINUS));
    assert_eq!(caught.len(), 48);
}

#[test]
fn one_round_too_many_is_caught_exactly_where_the_baseline_differs() {
    let caught = failing(Fault::TimesPlus);
    assert_eq!(caught, expected(T_PLUS));
    assert_eq!(caught.len(), 73);
}

#[test]
fn a_range_one_frame_short_at_the_end_is_caught_exactly_where_the_baseline_differs() {
    let caught = failing(Fault::ShortEnd);
    assert_eq!(caught, expected(F_MINUS));
    assert_eq!(caught.len(), 49);
}

#[test]
fn a_range_one_frame_short_at_the_start_is_caught_exactly_where_the_baseline_differs() {
    let caught = failing(Fault::ShortStart);
    assert_eq!(caught, expected(F_ZERO));
    assert_eq!(caught.len(), 49);
}

#[test]
fn one_layer_short_is_caught_exactly_where_the_baseline_differs() {
    let caught = failing(Fault::ShortLayer);
    assert_eq!(caught, expected(L_MINUS));
    assert_eq!(caught.len(), 26);
}

// ---- structural mistakes, through a deliberately wrong port

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Right,
    NoSort,
    Wide,
    Narrow,
    Shift,
}
/// The loop of repeatSelection over the row's input tables, with one mistake. Frames are printed as the baseline prints
/// them: a whole result is an integer.
fn ported(row: &Row, mode: Mode) -> Vec<Value> {
    let span = i64::from(row.last - row.first) + 1;
    let (low, high) = (f64::from(row.first), f64::from(row.last));
    row.input
        .iter()
        .enumerate()
        .map(|(layer, table)| {
            if table.is_null() || layer < row.min_layer || layer > row.max_layer {
                return table.clone();
            }
            let mut next = serde_json::Map::new();
            for (name, track) in table.as_object().unwrap() {
                let keys = track["keys"].as_array().unwrap();
                let mut merged: Vec<Value> = keys.clone();
                for key in keys {
                    let at = key["frame"].as_f64().unwrap();
                    let inside = match mode {
                        Mode::Wide => at >= low && at <= high + 1.0,
                        Mode::Narrow => at > low && at <= high,
                        _ => at >= low && at <= high,
                    };
                    if !inside {
                        continue;
                    }
                    for round in 1..=i64::from(row.eff) {
                        let shifted = at
                            + (round * span) as f64
                            + if mode == Mode::Shift { 1.0 } else { 0.0 };
                        let mut clone = key.clone();
                        clone["frame"] = if shifted.fract() == 0.0 {
                            json!(shifted as i64)
                        } else {
                            json!(shifted)
                        };
                        merged.push(clone);
                    }
                }
                if mode != Mode::NoSort {
                    merged.sort_by(|a, b| {
                        a["frame"]
                            .as_f64()
                            .unwrap()
                            .partial_cmp(&b["frame"].as_f64().unwrap())
                            .unwrap()
                    });
                }
                next.insert(name.clone(), json!({"keys":merged}));
            }
            Value::Object(next)
        })
        .collect()
}
fn port_failing(mode: Mode) -> Vec<usize> {
    rows()
        .iter()
        .filter(|row| !row.dup && ported(row, mode) != row.output)
        .map(|row| row.id)
        .collect()
}

#[test]
fn the_correct_port_reproduces_every_row_so_the_mistaken_ports_below_are_meaningful() {
    assert!(port_failing(Mode::Right).is_empty());
}

#[test]
fn clones_left_unsorted_are_caught_exactly_where_the_baseline_differs() {
    let caught = port_failing(Mode::NoSort);
    assert_eq!(caught, expected(NO_SORT));
    assert_eq!(caught.len(), 37);
}

#[test]
fn a_range_one_frame_too_wide_is_caught_exactly_where_the_baseline_differs() {
    let caught = port_failing(Mode::Wide);
    assert_eq!(caught, expected(WIDE));
    assert_eq!(caught.len(), 18);
}

#[test]
fn a_range_that_excludes_its_first_frame_is_caught_exactly_where_the_baseline_differs() {
    let caught = port_failing(Mode::Narrow);
    assert_eq!(caught, expected(NARROW));
    assert_eq!(caught.len(), 65);
}

#[test]
fn a_destination_one_frame_too_far_is_caught_exactly_where_the_baseline_differs() {
    let caught = port_failing(Mode::Shift);
    assert_eq!(caught, expected(SHIFT));
    assert_eq!(caught.len(), 79);
}

#[test]
fn the_native_admission_would_refuse_every_unsorted_merge() {
    // The kernel sorts, so it never writes these; the point is that a native track CANNOT hold what the unsorted loop
    // produces, so a forgotten sort could not pass storage admission either.
    let mut refused = 0;
    for row in rows().iter().filter(|row| !row.dup && row.mutants[NO_SORT]) {
        let tables = ported(row, Mode::NoSort);
        for form in row.forms() {
            let text = document_json(row.total, &tables, form);
            assert!(!decodes(&text), "{} {form:?}", row.label());
        }
        refused += 1;
    }
    assert_eq!(refused, 37);
}
