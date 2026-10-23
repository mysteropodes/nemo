//! Oracle and pinned tests for the bitmap-brush stamp plan (L07-E / G06, G17).
//!
//! The baseline rows live in `bitmap_brush_resources_stamp_rows.rs` (parser and
//! comparison engine); the hand-derived literals below are independent of the
//! oracle: LCG states computed with integer arithmetic, texture geometry worked out
//! from the baseline formulas, and the pressure lookup on a three-point profile.

use super::rows::{
    bounds_mismatches, cases, plan_mismatches_all, quadratic_mismatches, rng_mismatches, rows_in,
    ORACLE,
};
use super::{plan_stamps, to_uint32, StampPath, StampPlanError, StampRules, StampSpec, WorldRect};
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_stamp_bounds::{
    path_bounds, solve_quadratic,
};
use crate::evaluation::property_follow_path_sample::SegmentIn;

pub(crate) fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Vec<SegmentIn> {
    [[x0, y0], [x1, y1]]
        .iter()
        .map(|point| SegmentIn {
            point: *point,
            handle_in: None,
            handle_out: None,
        })
        .collect()
}

pub(crate) fn spec(size: f64, spacing: f64, scatter: f64, seed: f64) -> StampSpec {
    StampSpec {
        size,
        spacing,
        scatter,
        seed,
        opacity: 1.0,
        pressure: false,
    }
}

pub(crate) fn open_path(segments: &[SegmentIn]) -> StampPath<'_> {
    StampPath {
        segments,
        closed: false,
        pressure_profile: None,
    }
}

const RNG_SEED_1: [u64; 6] = [
    1015568748, 1586005467, 2165703038, 3027450565, 217083232, 1587069247,
];

#[test]
fn oracle_row_counts_are_pinned() {
    let count = |tag: char| rows_in(ORACLE, tag).len();
    assert_eq!(count('R'), 22, "seededRng rows");
    assert_eq!(count('Q'), 278, "solveQuadratic rows");
    assert_eq!(count('C'), 242, "bake cases");
    assert_eq!(count('G'), 764, "segment rows");
    assert_eq!(count('F'), 73, "pressure sample rows");
    assert_eq!(count('B'), 242, "bounds rows");
    assert_eq!(count('T'), 242, "geometry rows");
    assert_eq!(count('K'), 2136, "stamp rows");
    assert_eq!(count('N'), 242, "count rows");
    assert_eq!(cases().len(), 242);
}

#[test]
fn seeded_rng_rows_match_the_verbatim_baseline() {
    let bad = rng_mismatches(&StampRules::BASELINE, ORACLE);
    assert!(
        bad.is_empty(),
        "{} mismatches, first {:?}",
        bad.len(),
        bad.first()
    );
}

#[test]
fn seeded_rng_follows_the_integer_recurrence_from_seed_one() {
    let mut rng = StampRules::BASELINE.rng(1.0);
    for state in RNG_SEED_1 {
        assert_eq!(
            rng.next().to_bits(),
            (state as f64 / 4294967296.0).to_bits()
        );
    }
    let mut zero = StampRules::BASELINE.rng(0.0);
    assert_eq!(zero.next(), 1013904223.0 / 4294967296.0);
    assert_eq!(zero.next(), 1196435762.0 / 4294967296.0);
    assert_eq!(zero.next(), 3519870697.0 / 4294967296.0);
}

#[test]
fn seed_conversion_is_ecmascript_touint32() {
    let table: [(f64, u32); 12] = [
        (0.0, 0),
        (-0.0, 0),
        (1.9, 1),
        (-1.0, 4294967295),
        (-1.9, 4294967295),
        (4294967295.0, 4294967295),
        (4294967296.0, 0),
        (4294967297.0, 1),
        (9007199254740992.0, 0),
        (1e20, 1661992960),
        (-1e20, 2632974336),
        (3735928559.0, 3735928559),
    ];
    for (value, want) in table {
        assert_eq!(to_uint32(value), want, "{value}");
    }
    for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(to_uint32(value), 0, "{value}");
    }
}

#[test]
fn solve_quadratic_rows_match_paper_numerical() {
    let bad = quadratic_mismatches(ORACLE);
    assert!(
        bad.is_empty(),
        "{} mismatches, first {:?}",
        bad.len(),
        bad.first()
    );
}

#[test]
fn solve_quadratic_literals() {
    assert_eq!(solve_quadratic(0.0, 0.0, 0.0).0, -1);
    assert_eq!(solve_quadratic(0.0, 0.0, 5.0).0, 0);
    assert_eq!(solve_quadratic(1.0, 0.0, 1.0).0, 0);
    assert_eq!(solve_quadratic(1.0, 0.0, -1.0), (2, [1.0, -1.0]));
    assert_eq!(solve_quadratic(1.0, -2.0, 1.0), (1, [1.0, 0.0]));
    assert_eq!(solve_quadratic(0.0, -3.0, 1.0), (1, [1.0 / 3.0, 0.0]));
}

#[test]
fn path_bounds_rows_match_paper() {
    let bad = bounds_mismatches(ORACLE);
    assert!(
        bad.is_empty(),
        "{} mismatches, first {:?}",
        bad.len(),
        bad.first()
    );
}

#[test]
fn path_bounds_literals() {
    let segments = line(3.0, -4.0, -7.0, 9.0);
    let b = path_bounds(&segments, false);
    assert_eq!((b.x, b.y, b.width, b.height), (-7.0, -4.0, 10.0, 13.0));
    let empty = path_bounds(&[], true);
    assert_eq!(
        (empty.x, empty.y, empty.width, empty.height),
        (0.0, 0.0, 0.0, 0.0)
    );
    let single = path_bounds(&segments[..1], false);
    assert_eq!(
        (single.x, single.y, single.width, single.height),
        (3.0, -4.0, 0.0, 0.0)
    );
    // a curve bulging above its end points: the derivative root widens the box
    let bulge = [
        SegmentIn {
            point: [0.0, 0.0],
            handle_in: None,
            handle_out: Some([0.0, 30.0]),
        },
        SegmentIn {
            point: [10.0, 0.0],
            handle_in: Some([0.0, 30.0]),
            handle_out: None,
        },
    ];
    let b = path_bounds(&bulge, false);
    assert_eq!((b.x, b.width, b.y), (0.0, 10.0, 0.0));
    assert_eq!(
        b.height, 22.5,
        "peak of 3(1-t)^2 t 30 + 3(1-t) t^2 30 at t = 1/2"
    );
}

#[test]
fn every_oracle_case_plans_identically_to_the_baseline() {
    let bad = plan_mismatches_all(&StampRules::BASELINE, ORACLE);
    assert!(
        bad.is_empty(),
        "{} mismatches, first {:?}",
        bad.len(),
        bad.first()
    );
}

#[test]
fn straight_line_plan_matches_the_hand_derived_baseline_values() {
    let segments = line(0.0, 0.0, 100.0, 0.0);
    let plan = plan_stamps(&spec(10.0, 25.0, 0.0, 1.0), &open_path(&segments)).unwrap();
    // pad = 10 * 1.25 / 2 + 0 + 4 = 10.25; w = 100 + 20.5, h = 20.5; SCALE = min(2, 4096/w, 4096/h) = 2
    let t = plan.texture;
    assert_eq!(
        t.rect,
        WorldRect {
            min_x: -10.25,
            min_y: -10.25,
            w: 120.5,
            h: 20.5
        }
    );
    assert_eq!(t.scale, 2.0);
    assert_eq!((t.pixel_width, t.pixel_height), (241, 41));
    assert_eq!(plan.alpha, 1.0);
    // spacing = max(1, 10 * 25 / 100) = 2.5: the first point, then offsets 2.5 ..= 97.5
    assert_eq!(plan.stamps.len(), 40);
    assert_eq!(plan.byte_cost(), Some(241 * 41 * 4));
    // three draws per stamp (jitter x, jitter y, size unit): the units are LCG states 3 and 6
    let unit = |state: u64| state as f64 / 4294967296.0;
    assert_eq!(
        plan.stamps[0].size,
        10.0 * (0.75 + unit(RNG_SEED_1[2]) * 0.5)
    );
    assert_eq!(
        plan.stamps[1].size,
        10.0 * (0.75 + unit(RNG_SEED_1[5]) * 0.5)
    );
    assert_eq!(plan.stamps[0].x, 0.0);
    assert_eq!(plan.stamps[1].x, 2.5);
    assert_eq!(plan.stamps[2].x, 5.0);
    assert_eq!(plan.stamps[1].ps, plan.stamps[1].size * 2.0);
}

#[test]
fn pressure_scales_the_stamp_by_the_nearest_stored_sample() {
    let segments = line(0.0, 0.0, 10.0, 0.0);
    let profile = [[0.0, 0.0, 0.25], [10.0, 0.0, 0.75], [50.0, 50.0, 9.0]];
    let mut with = spec(8.0, 100.0, 0.0, 1.0);
    with.pressure = true;
    let path = StampPath {
        segments: &segments,
        closed: false,
        pressure_profile: Some(&profile[..]),
    };
    let plan = plan_stamps(&with, &path).unwrap();
    // spacing 8: the walk point 0 and the walk point 8; each reads the nearest sample
    assert_eq!(plan.stamps.len(), 2);
    let unit = |state: u64| state as f64 / 4294967296.0;
    assert_eq!(
        plan.stamps[0].size,
        8.0 * (0.75 + unit(RNG_SEED_1[2]) * 0.5) * 0.25
    );
    assert_eq!(
        plan.stamps[1].size,
        8.0 * (0.75 + unit(RNG_SEED_1[5]) * 0.5) * 0.75
    );
    // with pressure off the same profile is ignored
    let off = plan_stamps(&spec(8.0, 100.0, 0.0, 1.0), &path).unwrap();
    assert_eq!(off.stamps[0].size, 8.0 * (0.75 + unit(RNG_SEED_1[2]) * 0.5));
    assert_eq!(off.stamps[1].size, 8.0 * (0.75 + unit(RNG_SEED_1[5]) * 0.5));
}

#[test]
fn a_closed_path_walks_its_closing_curve() {
    let triangle = [[0.0, 0.0], [60.0, 0.0], [30.0, 52.0]]
        .iter()
        .map(|p| SegmentIn {
            point: *p,
            handle_in: None,
            handle_out: None,
        })
        .collect::<Vec<_>>();
    let brush = spec(8.0, 50.0, 0.0, 5.0);
    let open = plan_stamps(&brush, &open_path(&triangle)).unwrap();
    let closed = plan_stamps(
        &brush,
        &StampPath {
            segments: &triangle,
            closed: true,
            pressure_profile: None,
        },
    )
    .unwrap();
    // the closing curve (30, 52) -> (0, 0) is 60 long: well over a spacing of 4
    assert!(closed.stamps.len() > open.stamps.len() + 10);
    assert_eq!(closed.stamps[..open.stamps.len()], open.stamps[..]);
}

#[test]
fn the_plan_is_a_pure_function_of_the_stored_seed() {
    let segments = line(0.0, 0.0, 100.0, 40.0);
    let brush = spec(12.0, 40.0, 30.0, 777.0);
    let a = plan_stamps(&brush, &open_path(&segments)).unwrap();
    let b = plan_stamps(&brush, &open_path(&segments)).unwrap();
    assert_eq!(a, b, "a re-stamp with the stored seed never boils");
    let c = plan_stamps(&spec(12.0, 40.0, 30.0, 778.0), &open_path(&segments)).unwrap();
    assert_ne!(a.stamps, c.stamps);
    assert_eq!(
        a.stamps.len(),
        c.stamps.len(),
        "the seed moves the jitter, not the walk"
    );
}

#[test]
fn a_walk_longer_than_the_sample_guard_is_a_typed_error_not_a_truncation() {
    let segments = line(0.0, 0.0, 100.0, 0.0);
    let mut small = StampRules::BASELINE;
    small.max_samples = 10;
    assert_eq!(
        small.plan(&spec(10.0, 25.0, 0.0, 1.0), &open_path(&segments)),
        Err(StampPlanError::TooManySamples { limit: 10 })
    );
}
