//! Negative and mutation controls of the pure edit rules (L03-L, F17/F18). The production rules and nine
//! single-flaw copies run through the SAME check against the executed-baseline rows; production must pass
//! it and every flawed copy must be rejected, so the rows are proven to discriminate. The controls also pin
//! the authored-state rules (absent stays absent, equal numbers keep their spelling) that no oracle row can
//! express, each against a flawed copy.
use super::object_image_mesh_edit_oracle as oracle;
use super::rules;
use crate::object_document::{ImageMesh, Presence};
use serde_json::Number;

type SetFn = fn(&[[Number; 2]], usize, &Number, &Number) -> Option<Vec<[Number; 2]>>;
type ResetFn = fn(&[[Number; 2]]) -> Vec<[Number; 2]>;

fn num(value: f64) -> Number {
    Number::from_f64(value).expect("finite")
}

fn val(number: &Number) -> f64 {
    number.as_f64().expect("number")
}

fn numbers(points: &[[f64; 2]]) -> Vec<[Number; 2]> {
    points.iter().map(|p| [num(p[0]), num(p[1])]).collect()
}

fn floats(offsets: &[[Number; 2]]) -> Vec<[f64; 2]> {
    offsets.iter().map(|o| [val(&o[0]), val(&o[1])]).collect()
}

/// Runs a `setOffset` rule over every baseline row; `Ok` carries the number of rows it satisfied.
fn check_set(set: SetFn) -> Result<usize, String> {
    let starts = oracle::starts();
    let mut checked = 0;
    for row in oracle::offset_rows() {
        let start = &starts.iter().find(|(n, _)| *n == row.start).unwrap().1;
        let before = numbers(&start.offsets);
        let got = if row.index < 0 {
            None
        } else {
            set(&before, row.index as usize, &num(row.du), &num(row.dv))
        };
        let what = format!("{} {} {} {}", row.start, row.index, row.du, row.dv);
        match (got, row.ret) {
            (None, false) => {}
            (Some(next), true) => {
                if !oracle::same_pts(&floats(&next), &row.offsets) {
                    return Err(format!("{what}: the offsets differ from the baseline"));
                }
            }
            (Some(_), false) => return Err(format!("{what}: accepted what the baseline refused")),
            (None, true) => return Err(format!("{what}: refused what the baseline applied")),
        }
        checked += 1;
    }
    Ok(checked)
}

fn check_reset(reset: ResetFn) -> Result<usize, String> {
    let starts = oracle::starts();
    let mut checked = 0;
    for (name, expected) in oracle::reset_rows() {
        let start = &starts.iter().find(|(n, _)| *n == name).unwrap().1;
        let got = floats(&reset(&numbers(&start.offsets)));
        if !oracle::same_pts(&got, &expected) {
            return Err(format!(
                "{name}: the reset offsets differ from the baseline"
            ));
        }
        checked += 1;
    }
    Ok(checked)
}

fn slot(du: &Number, dv: &Number) -> [Number; 2] {
    [du.clone(), dv.clone()]
}

// ---- single-flaw copies of setOffset ---------------------------------------------------------
fn accumulates(o: &[[Number; 2]], i: usize, du: &Number, dv: &Number) -> Option<Vec<[Number; 2]>> {
    let old = o.get(i)?;
    let mut next = o.to_vec();
    next[i] = [num(val(&old[0]) + val(du)), num(val(&old[1]) + val(dv))];
    Some(next)
}

fn accepts_index_equal_to_length(
    o: &[[Number; 2]],
    i: usize,
    du: &Number,
    dv: &Number,
) -> Option<Vec<[Number; 2]>> {
    if i > o.len() {
        return None;
    }
    let mut next = o.to_vec();
    if i == o.len() {
        next.push(slot(du, dv));
    } else {
        next[i] = slot(du, dv);
    }
    Some(next)
}

fn swaps_the_components(
    o: &[[Number; 2]],
    i: usize,
    du: &Number,
    dv: &Number,
) -> Option<Vec<[Number; 2]>> {
    o.get(i)?;
    let mut next = o.to_vec();
    next[i] = slot(dv, du);
    Some(next)
}

fn clears_every_other_slot(
    o: &[[Number; 2]],
    i: usize,
    du: &Number,
    dv: &Number,
) -> Option<Vec<[Number; 2]>> {
    o.get(i)?;
    let mut next = vec![[num(0.0), num(0.0)]; o.len()];
    next[i] = slot(du, dv);
    Some(next)
}

fn writes_the_next_slot(
    o: &[[Number; 2]],
    i: usize,
    du: &Number,
    dv: &Number,
) -> Option<Vec<[Number; 2]>> {
    o.get(i + 1)?;
    let mut next = o.to_vec();
    next[i + 1] = slot(du, dv);
    Some(next)
}

fn ignores_dv(o: &[[Number; 2]], i: usize, du: &Number, _dv: &Number) -> Option<Vec<[Number; 2]>> {
    let old = o.get(i)?;
    let mut next = o.to_vec();
    next[i] = [du.clone(), old[1].clone()];
    Some(next)
}

// ---- single-flaw copies of resetOffsets -------------------------------------------------------
fn resets_even_slots_only(o: &[[Number; 2]]) -> Vec<[Number; 2]> {
    o.iter()
        .enumerate()
        .map(|(i, s)| {
            if i % 2 == 0 {
                [num(0.0), num(0.0)]
            } else {
                s.clone()
            }
        })
        .collect()
}

fn resets_nothing(o: &[[Number; 2]]) -> Vec<[Number; 2]> {
    o.to_vec()
}

fn keeps_the_last_slot(o: &[[Number; 2]]) -> Vec<[Number; 2]> {
    let last = o.len().saturating_sub(1);
    o.iter()
        .enumerate()
        .map(|(i, s)| {
            if i == last {
                s.clone()
            } else {
                [num(0.0), num(0.0)]
            }
        })
        .collect()
}

#[test]
fn production_passes_the_check_the_flawed_copies_are_run_through() {
    assert_eq!(check_set(rules::set_offset), Ok(161));
    assert_eq!(check_reset(rules::reset_offsets), Ok(6));
}

#[test]
fn every_single_flaw_of_set_offset_is_rejected_by_the_baseline_rows() {
    let flawed: [(&str, SetFn); 6] = [
        ("accumulates instead of writing", accumulates),
        ("accepts index == length", accepts_index_equal_to_length),
        ("swaps du and dv", swaps_the_components),
        ("clears every other slot", clears_every_other_slot),
        ("writes the next slot", writes_the_next_slot),
        ("ignores dv", ignores_dv),
    ];
    for (name, flaw) in flawed {
        check_set(flaw).expect_err(name);
    }
}

#[test]
fn every_single_flaw_of_reset_offsets_is_rejected_by_the_baseline_rows() {
    let flawed: [(&str, ResetFn); 3] = [
        ("resets even slots only", resets_even_slots_only),
        ("resets nothing", resets_nothing),
        ("keeps the last slot", keeps_the_last_slot),
    ];
    for (name, flaw) in flawed {
        check_reset(flaw).expect_err(name);
    }
}

#[test]
fn equal_numbers_keep_their_authored_spelling_and_a_replacing_copy_is_detected() {
    let stored = vec![[num(0.0), num(0.5)], [Number::from(1), num(-0.0)]];
    let kept = rules::set_offset(&stored, 0, &Number::from(0), &num(0.5)).unwrap();
    assert_eq!(kept, stored, "0 is the stored 0.0 and 0.5 the stored 0.5");
    let kept = rules::set_offset(&stored, 1, &num(1.0), &Number::from(0)).unwrap();
    assert_eq!(kept, stored, "1.0 is the stored 1 and 0 the stored -0.0");
    fn replaces_always(
        o: &[[Number; 2]],
        i: usize,
        du: &Number,
        dv: &Number,
    ) -> Option<Vec<[Number; 2]>> {
        o.get(i)?;
        let mut next = o.to_vec();
        next[i] = [du.clone(), dv.clone()];
        Some(next)
    }
    let flawed = replaces_always(&stored, 0, &Number::from(0), &num(0.5)).unwrap();
    assert_ne!(
        flawed, stored,
        "the flawed copy rewrites the spelling of an unchanged value"
    );
    // A reset keeps already-zero slots as they are and writes JSON zeros elsewhere.
    let reset = rules::reset_offsets(&[[num(0.0), num(-0.0)], [num(0.25), Number::from(0)]]);
    assert_eq!(
        reset,
        vec![[num(0.0), num(-0.0)], [Number::from(0), Number::from(0)]]
    );
}

fn mesh_of(name: &str) -> ImageMesh {
    serde_json::from_value(oracle::start(name).entry()).expect("admitted record")
}

#[test]
fn never_authored_offsets_stay_absent_only_while_the_result_is_all_zero() {
    let mut absent = mesh_of("g4");
    absent.offsets = Presence::Missing;
    let zeros = rules::effective_offsets(&absent);
    assert_eq!(
        zeros.len(),
        absent.verts.len(),
        "one zero pair per vertex, like load()"
    );
    assert!(rules::with_offsets(&absent, zeros.clone())
        .offsets
        .missing());
    let mut moved = zeros.clone();
    moved[3] = [num(0.2), Number::from(0)];
    assert_eq!(
        rules::with_offsets(&absent, moved.clone()).offsets,
        Presence::Present(moved)
    );
    // Offsets that were authored stay authored, even when they are zeros again.
    let authored = mesh_of("g4");
    assert!(!authored.offsets.missing());
    assert!(!rules::with_offsets(&authored, zeros).offsets.missing());
    // The flawed copy that always authors the field is not equal to production on the absent case.
    fn always_authors(old: &ImageMesh, offsets: Vec<[Number; 2]>) -> ImageMesh {
        let mut next = old.clone();
        next.offsets = Presence::Present(offsets);
        next
    }
    let zeros = rules::effective_offsets(&absent);
    assert_ne!(
        always_authors(&absent, zeros.clone()),
        rules::with_offsets(&absent, zeros)
    );
}

#[test]
fn an_identity_retopology_changes_nothing_not_even_absent_members() {
    for stripped in [false, true] {
        let mut mesh = mesh_of("hexs");
        if stripped {
            mesh.offsets = Presence::Missing;
            mesh.cols = Presence::Missing;
            mesh.rows = Presence::Missing;
        }
        let state = rules::state_of(&mesh);
        assert_eq!(
            rules::merged(&mesh, &state),
            Some(mesh.clone()),
            "stripped={stripped}"
        );
    }
    // A moved vertex is the only difference: one new number, every other number keeps its spelling.
    let mesh = mesh_of("g4s");
    let mut state = rules::state_of(&mesh);
    let interior = state.verts.len() - 1;
    state.verts[interior][0] += 0.01;
    let merged = rules::merged(&mesh, &state).unwrap();
    assert_ne!(merged, mesh);
    assert_eq!(merged.outline, mesh.outline);
    assert_eq!(merged.offsets, mesh.offsets);
    // A computed number that is not finite is refused, never stored.
    let mut state = rules::state_of(&mesh);
    state.offsets[0][1] = f64::NAN;
    assert_eq!(rules::merged(&mesh, &state), None);
}

#[test]
fn density_members_are_authored_only_when_they_differ_from_what_absence_means() {
    let mut mesh = mesh_of("g4");
    mesh.cols = Presence::Missing;
    mesh.rows = Presence::Missing;
    let mut state = rules::state_of(&mesh);
    assert_eq!(
        (state.cols, state.rows),
        (4, 4),
        "absence evaluates as the load() default 4"
    );
    assert!(rules::merged(&mesh, &state).unwrap().cols.missing());
    state.cols = 6;
    let merged = rules::merged(&mesh, &state).unwrap();
    assert_eq!(
        (merged.cols.clone(), merged.rows.clone()),
        (Presence::Present(6), Presence::Missing)
    );
}

#[test]
fn the_oracle_is_not_vacuous() {
    let rows = oracle::offset_rows();
    let starts = oracle::starts();
    let start = |name: &str| starts.iter().find(|(n, _)| *n == name).unwrap().1.clone();
    assert_eq!(rows.len(), 161);
    assert!(
        rows.iter().filter(|r| !r.ret).count() >= 50,
        "refusals: past the end, negative, far away"
    );
    assert!(rows.iter().filter(|r| r.du != 0.0 || r.dv != 0.0).count() >= 100);
    for (name, _) in &starts {
        assert!(
            rows.iter().filter(|r| r.start == *name).count() >= 18,
            "{name}"
        );
    }
    let outline_slot = rows
        .iter()
        .filter(|r| r.ret && (r.index as usize) < start(r.start).outline.len())
        .count();
    let interior_slot = rows
        .iter()
        .filter(|r| r.ret && (r.index as usize) >= start(r.start).outline.len())
        .count();
    assert!(
        outline_slot >= 20 && interior_slot >= 20,
        "{outline_slot} {interior_slot}"
    );
    let over_sculpt = rows
        .iter()
        .filter(|r| {
            r.ret
                && start(r.start)
                    .offsets
                    .iter()
                    .enumerate()
                    .any(|(j, o)| j != r.index as usize && (o[0] != 0.0 || o[1] != 0.0))
        })
        .count();
    assert!(
        over_sculpt >= 40,
        "{over_sculpt}: slots that other slots' sculpt must survive"
    );
    let at_length = rows
        .iter()
        .filter(|r| r.index as usize == start(r.start).verts.len())
        .count();
    assert!(
        at_length >= 18,
        "{at_length}: index == length for every start and value"
    );
}
