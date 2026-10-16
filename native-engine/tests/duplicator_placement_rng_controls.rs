//! Controls for the duplicator random stream: every deviation fed through the
//! real kernel must be caught by the oracle rows.
//!
//! A control changes ONE thing in how the real kernel is driven (the stream
//! stride, the draw order, the number of draws consumed, the seed reduction)
//! and requires that at least one oracle row then mismatches. The unchanged
//! drive is the baseline test of `duplicator_placement_rng.rs`.

use super::tests::{clone_rows, same_draws, CloneRow};
use super::{draws_for_seed, stream_seed, to_uint32, JsNumber, SeededRng, DRAWS_PER_CLONE};

/// Rows whose draws differ when `driver` supplies the ten draws of a clone.
fn mismatches(driver: &dyn Fn(&CloneRow) -> [f64; DRAWS_PER_CLONE]) -> usize {
    clone_rows()
        .iter()
        .filter(|row| !same_draws(&driver(*row), &row.draws))
        .count()
}

fn stream(seed: u32, n: usize) -> Vec<f64> {
    let mut rng = SeededRng::new(seed);
    (0..n).map(|_| rng.next_f64()).collect()
}

fn array(v: &[f64]) -> [f64; DRAWS_PER_CLONE] {
    let mut out = [0.0; DRAWS_PER_CLONE];
    out.copy_from_slice(&v[..DRAWS_PER_CLONE]);
    out
}

#[test]
fn the_undeviated_drive_matches_every_row() {
    let exact = |row: &CloneRow| draws_for_seed(stream_seed(row.seed, row.k, 7919.0)).as_array();
    assert_eq!(mismatches(&exact), 0);
}

#[test]
fn a_wrong_stride_is_detected() {
    for stride in [7918.0, 7920.0, 7907.0, 104_729.0, 1.0, 0.0] {
        let wrong =
            move |row: &CloneRow| draws_for_seed(stream_seed(row.seed, row.k, stride)).as_array();
        // 21 seeds x 11 non-zero clones = 231, minus the 11 clones of the Infinity seed,
        // which reduces to 0 for every stride (JS `Infinity >>> 0`).
        assert_eq!(mismatches(&wrong), 220, "stride {stride}");
    }
}

#[test]
fn a_skipped_draw_is_detected() {
    let skipped = |row: &CloneRow| array(&stream(row.seed_u32, DRAWS_PER_CLONE + 1)[1..]);
    assert_eq!(mismatches(&skipped), clone_rows().len());
}

/// The ten draws with two channels exchanged.
fn swapped(row: &CloneRow, i: usize, j: usize) -> [f64; DRAWS_PER_CLONE] {
    let mut d = draws_for_seed(row.seed_u32).as_array();
    d.swap(i, j);
    d
}

#[test]
fn reordered_draws_are_detected() {
    for (i, j) in [(0, 1), (1, 2), (5, 6), (8, 9), (0, 9), (3, 4)] {
        assert_eq!(
            mismatches(&|row| swapped(row, i, j)),
            clone_rows().len(),
            "swap {i} {j}"
        );
    }
    // the hue draw was appended last: moving it forward shifts every other channel
    let hue_first = |row: &CloneRow| {
        let d = draws_for_seed(row.seed_u32).as_array();
        let mut out = [d[9]; DRAWS_PER_CLONE];
        out[1..].copy_from_slice(&d[..9]);
        out
    };
    assert_eq!(mismatches(&hue_first), clone_rows().len());
    // nine draws (hue read as a repeat of the previous channel)
    let nine = |row: &CloneRow| {
        let mut d = draws_for_seed(row.seed_u32).as_array();
        d[9] = d[8];
        d
    };
    assert_eq!(mismatches(&nine), clone_rows().len());
}

#[test]
fn a_seed_reduction_that_is_not_the_unsigned_shift_is_detected() {
    // Rust's saturating `as u32` instead of `>>> 0`.
    let saturating = |row: &CloneRow| {
        let sum = match row.seed {
            JsNumber::Value(v) if v != 0.0 && !v.is_nan() => v,
            _ => 0.0,
        } + row.k as f64 * 7919.0;
        draws_for_seed(sum as u32).as_array()
    };
    assert!(
        mismatches(&saturating) >= 1,
        "wrap-around and negative seeds"
    );
    // no reduction of a negative seed at all
    assert_ne!(to_uint32(-1.0), 0);
    assert_ne!((-1.0_f64) as u32, to_uint32(-1.0));
    assert_ne!(4_294_967_297.0_f64 as u32, to_uint32(4_294_967_297.0));
}

#[test]
fn clone_streams_are_fresh_and_independent() {
    // The same seed restarts the same stream; the next call differs.
    let mut a = SeededRng::new(42);
    let mut b = SeededRng::new(42);
    assert_eq!(a.next_u32(), b.next_u32());
    assert_ne!(a.next_u32(), SeededRng::new(42).next_u32());
    // each clone owns a fresh stream: clone 1 does not continue clone 0's
    let c0 = draws_for_seed(42);
    let c1 = draws_for_seed(42 + 7919);
    assert_ne!(c0, c1);
    assert_ne!(c1.rx, stream(42, DRAWS_PER_CLONE + 1)[DRAWS_PER_CLONE]);
}
