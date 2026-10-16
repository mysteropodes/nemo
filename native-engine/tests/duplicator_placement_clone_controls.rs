//! Controls for the per-clone placement kernel: every deviation fed through
//! the real kernel must be caught by the oracle rows.
//!
//! A control changes ONE thing in what the real kernel is given (an input, the
//! draws, the owner, the 3D flag) and requires at least a stated number of the
//! 765 `pl` rows to mismatch. The counts in the comments were measured with an
//! independent port of the kernel over the generated oracle; the asserted
//! bounds are lower. The unchanged drive is the baseline test of
//! `duplicator_placement_clone.rs`.

use super::test_codec::{
    d3_of, effective, kernel_input, pl_rows, place, values_of, Owner, PlInput, PlRow,
};
use super::{clone_placement_with_draws, ClonePlacement};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::duplicator_placement_rng::{
    clone_draws, clone_seed, CloneDraws, SeededRng,
};
use crate::evaluation::object_component_kernels::object_effector_falloff::test_codec::{
    close, has_linear,
};
use crate::evaluation::object_component_kernels::object_effector_falloff::Falloff;

/// The row's answer differs from the oracle's.
fn differs(row: &PlRow, p: &ClonePlacement) -> bool {
    let tol = has_linear(&effective(&row.input));
    let values = values_of(p)
        .iter()
        .zip(&row.out.values)
        .any(|(g, w)| !close(*g, *w, tol));
    values
        || match (d3_of(p), row.out.d3) {
            (None, None) => false,
            (Some(g), Some(w)) => (0..3).any(|i| !close(g[i], w[i], tol)),
            _ => true,
        }
}

/// Rows that mismatch when the real kernel is given a changed copy of the input.
fn with_input(change: impl Fn(&mut PlInput)) -> usize {
    pl_rows()
        .iter()
        .filter(|row| {
            let mut input = row.input.clone();
            change(&mut input);
            differs(row, &place(&input))
        })
        .count()
}

/// Rows that mismatch when the real kernel is given other draws.
fn with_draws(make: impl Fn(&PlInput) -> CloneDraws) -> usize {
    pl_rows()
        .iter()
        .filter(|row| {
            let effs = effective(&row.input);
            let input = kernel_input(&row.input, &effs);
            differs(row, &clone_placement_with_draws(&input, &make(&row.input)))
        })
        .count()
}

fn from_array(a: [f64; 10]) -> CloneDraws {
    CloneDraws {
        rx: a[0],
        ry: a[1],
        rrr: a[2],
        rsx: a[3],
        rsy: a[4],
        rop: a[5],
        rz: a[6],
        rrx: a[7],
        rry: a[8],
        rhue: a[9],
    }
}

fn own(input: &PlInput) -> CloneDraws {
    clone_draws(input.seed, input.k)
}

/// The draws of the clone's own stream with a change applied to the array.
fn changed(input: &PlInput, edit: impl Fn(&mut [f64; 10])) -> CloneDraws {
    let mut a = own(input).as_array();
    edit(&mut a);
    from_array(a)
}

#[test]
fn the_undeviated_drive_matches_every_row() {
    assert_eq!(with_input(|_| {}), 0);
    assert_eq!(with_draws(own), 0);
}

#[test]
fn swapped_x_and_y_are_detected() {
    // 620 rows: the position and scale deltas with their axes exchanged.
    let n = with_input(|i| {
        i.deltas.pos.swap(0, 1);
        i.deltas.scale.swap(0, 1);
    });
    assert!(n >= 450, "{n}");
}

#[test]
fn a_flipped_random_flag_is_detected() {
    // 644, 650, 642, 425 and 541 rows: position, rotation, scale, opacity, hue.
    let flips: [(&str, usize, &dyn Fn(&mut PlInput)); 5] = [
        ("position", 300, &|i| i.rand.position = !i.rand.position),
        ("rotation", 300, &|i| i.rand.rotation = !i.rand.rotation),
        ("scale", 300, &|i| i.rand.scale = !i.rand.scale),
        ("opacity", 250, &|i| i.rand.opacity = !i.rand.opacity),
        ("hue", 300, &|i| i.rand.hue = !i.rand.hue),
    ];
    for (name, bound, flip) in flips {
        let n = with_input(flip);
        assert!(n >= bound, "{name}: {n}");
    }
}

#[test]
fn hue_draw_moved_before_the_nine_originals_is_detected() {
    // 639 rows: the hue draw first, the other nine shifted by one.
    let n = with_draws(|i| {
        changed(i, |a| {
            let hue = a[9];
            a.copy_within(0..9, 1);
            a[0] = hue;
        })
    });
    assert!(n >= 450, "{n}");
}

#[test]
fn reordered_or_foreign_draws_are_detected() {
    // 353 rx<->ry, 200 rz<->rrx, 441 rrr<->rop, 287 hue repeating rry rows.
    let swap = |x: usize, y: usize, bound: usize| {
        let n = with_draws(|i| changed(i, |a| a.swap(x, y)));
        assert!(n >= bound, "swap {x}<->{y}: {n}");
    };
    swap(0, 1, 250);
    swap(6, 7, 120);
    swap(2, 5, 300);
    let repeat = with_draws(|i| changed(i, |a| a[9] = a[8]));
    assert!(repeat >= 200, "{repeat}");
    // 633 rows: another clone's stream; 395 rows: the seed ignored.
    let next = with_draws(|i| clone_draws(i.seed, i.k + 1));
    let no_seed = with_draws(|i| clone_draws(JsNumber::Undefined, i.k));
    assert!(next >= 450, "{next}");
    assert!(no_seed >= 250, "{no_seed}");
}

#[test]
fn a_skipped_draw_is_detected() {
    // 642 rows: the stream consumed from its second number.
    let n = with_draws(|i| {
        let mut rng = SeededRng::new(clone_seed(i.seed, i.k));
        rng.next_f64();
        let mut a = [0.0; 10];
        a.iter_mut().for_each(|slot| *slot = rng.next_f64());
        from_array(a)
    });
    assert!(n >= 450, "{n}");
}

#[test]
fn a_wrong_third_axis_flag_is_detected() {
    // 185 rows read a flat layer as 3D; 267 rows read a 3D layer as flat.
    let as_3d = with_input(|i| i.is_3d = true);
    let as_flat = with_input(|i| i.is_3d = false);
    assert!(as_3d >= 120, "{as_3d}");
    assert!(as_flat >= 180, "{as_flat}");
}

#[test]
fn effector_mistakes_are_detected() {
    // 36 rows: a step effector read as radial (clone distance, not clone index).
    let step = with_input(|i| {
        i.effs
            .iter_mut()
            .filter(|e| e.falloff == Falloff::Step)
            .for_each(|e| e.falloff = Falloff::Radial);
    });
    // 14 rows: a missing strength read as 0 instead of 100.
    let strength = with_input(|i| {
        for e in i
            .effs
            .iter_mut()
            .filter(|e| !matches!(e.strength, JsNumber::Value(_)))
        {
            e.strength = JsNumber::Value(0.0);
        }
    });
    // 135 rows: the pivot left out of the clone position the falloff measures.
    let pivot = with_input(|i| i.pivot = [0.0, 0.0]);
    // 26 rows: the clone count one too large; 27 rows: always 1.
    let count_up = with_input(|i| i.count += 1.0);
    let count_one = with_input(|i| i.count = 1.0);
    assert!(step >= 20, "step {step}");
    assert!(strength >= 8, "strength {strength}");
    assert!(pivot >= 90, "pivot {pivot}");
    assert!(count_up >= 15, "count+1 {count_up}");
    assert!(count_one >= 15, "count=1 {count_one}");
}

#[test]
fn owner_mistakes_are_detected() {
    // 19 rows: the layers ignored although the call has an owner.
    let ignored = with_input(|i| i.owner = Owner::Absent);
    // 7 rows: the layers read although the call has none (nativeVideo shape).
    let read = with_input(|i| {
        if i.owner == Owner::Absent {
            i.owner = Owner::Layer(Some("dupA"));
        }
    });
    assert!(ignored >= 10, "ignored {ignored}");
    assert!(read >= 5, "read {read}");
}

#[test]
fn a_dropped_hue_delta_is_detected() {
    // 383 rows: `dHue || 0` read as 0 whatever the delta.
    let n = with_input(|i| i.deltas.hue = JsNumber::Undefined);
    assert!(n >= 250, "{n}");
}
