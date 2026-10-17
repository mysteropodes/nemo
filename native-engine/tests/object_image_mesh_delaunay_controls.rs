//! Negative and mutation controls for the Delaunator port (L03-F, F17).
//!
//! `mut` rows of the oracle are the outputs of the VENDORED baseline after one textual
//! mutation (swapped tie-break comparison, wrong orient2d sign, skipped duplicate handling,
//! ...), produced by `tools/gen.cjs`. A control holds when (a) production equals the pinned
//! baseline on the named fixture and (b) the mutated baseline differs from it there, i.e. the
//! fixture would fail any implementation carrying that defect.

use super::delaunay_from;
use super::oracle_rows::{bits, hex, i32s, rows, tri_row, u32s};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_delaunay_geom::in_circle;
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_orient::orient2d;

type Out = (Vec<u32>, Vec<i32>, Vec<u32>);

fn production(pts: &[[f64; 2]]) -> Out {
    let d = delaunay_from(pts).expect("triangulation");
    (d.triangles, d.halfedges, d.hull)
}

const MUTANTS: [&str; 13] = [
    "incircle-tie-flips",
    "insertion-sort-tie-swapped",
    "median-tie-swapped",
    "seed-centre-last-tie",
    "seed-radius-last-tie",
    "seed-orient-sign",
    "orient-fast-sign",
    "orient-adapt-sign",
    "orient-no-adapt",
    "dup-skip-removed",
    "dup-epsilon-tiny",
    "sweep-forward-nonstrict",
    "collinear-hull-keeps-dups",
];

#[test]
fn every_mutant_of_the_catalogue_is_pinned_by_at_least_one_row() {
    let mut names: Vec<&str> = rows("mut")
        .iter()
        .chain(rows("muto").iter())
        .map(|r| r[0])
        .collect();
    names.sort_unstable();
    names.dedup();
    let mut expected = MUTANTS.to_vec();
    expected.sort_unstable();
    assert_eq!(names, expected);
}

#[test]
fn production_equals_the_baseline_and_differs_from_every_mutated_baseline() {
    for r in rows("mut") {
        let base = tri_row(r[1]);
        let baseline: Out = (base.tris.clone(), base.halfedges.clone(), base.hull.clone());
        let mutated: Out = (u32s(r[2]), i32s(r[3]), u32s(r[4]));
        assert_ne!(baseline, mutated, "{} must change {}", r[0], r[1]);
        assert_eq!(production(&base.pts), baseline, "production on {}", r[1]);
        assert_ne!(production(&base.pts), mutated, "{} on {}", r[0], r[1]);
    }
}

#[test]
fn orient2d_mutants_are_distinguished_by_the_pinned_predicate_rows() {
    let baseline_rows = rows("ori");
    let mut seen = 0;
    for r in rows("muto") {
        let key = &r[1..7];
        let row = baseline_rows
            .iter()
            .find(|o| o[..6] == *key)
            .unwrap_or_else(|| panic!("no ori row for {} {key:?}", r[0]));
        let v: Vec<f64> = key.iter().map(|h| hex(h)).collect();
        let got = orient2d(v[0], v[1], v[2], v[3], v[4], v[5]);
        assert_eq!(bits(got), row[6], "production equals the baseline row");
        assert_ne!(r[7], row[6], "{} must change the predicate value", r[0]);
        assert_ne!(bits(got), r[7], "production differs from {}", r[0]);
        seen += 1;
    }
    assert_eq!(seen, 9);
}

#[test]
fn the_controls_named_by_the_contract_each_fail_a_named_fixture() {
    let pairs = [
        // swapped tie-break comparisons
        ("incircle-tie-flips", "square4"),
        ("insertion-sort-tie-swapped", "col-vert-dup"),
        ("median-tie-swapped", "cm4x4"),
        ("seed-centre-last-tie", "tri3"),
        ("seed-radius-last-tie", "square4"),
        // wrong orient2d sign
        ("orient-fast-sign", "tri3"),
        ("seed-orient-sign", "tri3"),
        // skipped duplicate handling
        ("dup-skip-removed", "search-dup-skip-removed-0"),
        ("dup-epsilon-tiny", "search-dup-skip-removed-1"),
        ("collinear-hull-keeps-dups", "same5"),
        // the lattice diagonal rule
        ("sweep-forward-nonstrict", "lat3x3"),
    ];
    let mutant_rows = rows("mut");
    for (mutant, fixture) in pairs {
        assert!(
            mutant_rows
                .iter()
                .any(|r| r[0] == mutant && r[1] == fixture),
            "{mutant} must be pinned on {fixture}"
        );
    }
}

/// Some pair of points whose coordinates differ by at most `f64::EPSILON` each and, when
/// `beyond` is given, by more than `beyond` in at least one coordinate.
fn has_close_pair(pts: &[[f64; 2]], beyond: Option<f64>) -> bool {
    (0..pts.len()).any(|i| {
        (0..i).any(|j| {
            let (dx, dy) = ((pts[i][0] - pts[j][0]).abs(), (pts[i][1] - pts[j][1]).abs());
            dx <= f64::EPSILON && dy <= f64::EPSILON && beyond.map_or(true, |b| dx > b || dy > b)
        })
    })
}

#[test]
fn the_duplicate_fixtures_really_contain_points_within_epsilon() {
    let (mut skip, mut tiny) = (0, 0);
    for r in rows("mut") {
        if !r[0].starts_with("dup-") || !r[1].starts_with("search-dup") {
            continue;
        }
        let pts = tri_row(r[1]).pts;
        assert!(
            has_close_pair(&pts, None),
            "{} holds no duplicate pair",
            r[1]
        );
        if r[0] == "dup-epsilon-tiny" {
            // the mutated tolerance 2^-60 only differs when the pair is not an exact repeat
            assert!(has_close_pair(&pts, Some(2f64.powi(-60))), "{}", r[1]);
            tiny += 1;
        } else {
            skip += 1;
        }
    }
    assert_eq!((skip, tiny), (2, 2));
}

#[test]
fn a_corrupted_result_does_not_pass_the_comparison_used_by_the_rows() {
    let row = tri_row("lat3x3");
    let (mut tris, mut half, mut hull) = production(&row.pts);
    assert_eq!(
        (tris.clone(), half.clone(), hull.clone()),
        (row.tris.clone(), row.halfedges.clone(), row.hull.clone())
    );
    tris.swap(0, 3);
    assert_ne!(tris, row.tris, "a swapped triangle is detected");
    half[1] = 0;
    assert_ne!(half, row.halfedges, "a changed halfedge is detected");
    hull.reverse();
    assert_ne!(hull, row.hull, "a reversed hull is detected");
}

/// The determinant of `inCircle`, written out so a tie is visible as an exact zero.
fn in_circle_determinant(q: &[[f64; 2]; 4]) -> f64 {
    let (dx, dy) = (q[0][0] - q[3][0], q[0][1] - q[3][1]);
    let (ex, ey) = (q[1][0] - q[3][0], q[1][1] - q[3][1]);
    let (fx, fy) = (q[2][0] - q[3][0], q[2][1] - q[3][1]);
    let (ap, bp, cp) = (dx * dx + dy * dy, ex * ex + ey * ey, fx * fx + fy * fy);
    dx * (ey * cp - bp * fy) - dy * (ex * cp - bp * fx) + ap * (ex * fy - ey * fx)
}

#[test]
fn a_less_or_equal_in_circle_would_flip_every_exact_tie_that_the_baseline_keeps() {
    let sq = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    for k in 0..4 {
        let q = [sq[k], sq[(k + 1) % 4], sq[(k + 2) % 4], sq[(k + 3) % 4]];
        let det = in_circle_determinant(&q);
        assert_eq!(det, 0.0, "rotation {k} is an exact tie");
        let production_says = in_circle(
            q[0][0], q[0][1], q[1][0], q[1][1], q[2][0], q[2][1], q[3][0], q[3][1],
        );
        assert!(!production_says, "strict comparison keeps the diagonal");
    }
}
