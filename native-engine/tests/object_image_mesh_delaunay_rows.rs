//! Oracle row reader and runner for the Delaunator port (L03-F, F17).
//!
//! `fixtures/object-image-mesh-delaunay/oracle.txt` (`ORACLE`) is produced by `tools/gen.cjs`
//! (kept outside the repo; sha256 in the leaf HANDOFF) from the baseline at fbe4b8a3: the
//! VENDORED `src/js/delaunator.vendor.js` executed in a Node `vm` (one appended probe line
//! exposes its module-private helpers), and `src/js/image-mesh.js` `createMesh`/`rebuild` for the
//! production point sets. No suite existed before; this oracle is the executed baseline.
//!
//! Rows (space separated; every f64 is 16 hex digits of its bits; `_` is an empty list):
//! `ori ax ay bx by cx cy OUT STAGE EXACT` (STAGE 0..4 of the adaptive predicate, EXACT `+`/`-`/`0`
//! from big-integer arithmetic), `psa dx dy OUT`, `inc ax ay bx by cx cy px py 0|1`,
//! `cir ax ay bx by cx cy OUT`, `cen ax ay bx by cx cy OUTX OUTY`, `qs DISTS IDS` (comma lists),
//! `tri NAME PTS TRIS HALFEDGES HULL` (PTS `x,y;x,y`), `mut NAME FIXTURE TRIS HALFEDGES HULL` (the
//! output of a textually mutated baseline), `muto NAME ax ay bx by cx cy MUTOUT`.

use crate::evaluation::object_image_mesh_kernels::object_image_mesh_delaunay::{
    delaunay_from, triangulate_raw,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_delaunay_geom::{
    circumcenter, circumradius, in_circle, pseudo_angle, quicksort,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_orient::{
    orient2d, orient2d_staged,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-image-mesh-delaunay/oracle.txt");

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

pub(crate) fn bits(x: f64) -> String {
    format!("{:016x}", x.to_bits())
}

pub(crate) fn nums(t: &str) -> Vec<f64> {
    t.split(',').map(hex).collect()
}

pub(crate) fn pts(t: &str) -> Vec<[f64; 2]> {
    if t == "_" {
        return Vec::new();
    }
    t.split(';')
        .map(|p| {
            let (x, y) = p.split_once(',').expect("point");
            [hex(x), hex(y)]
        })
        .collect()
}

pub(crate) fn u32s(t: &str) -> Vec<u32> {
    if t == "_" {
        Vec::new()
    } else {
        t.split(',').map(|v| v.parse().expect("u32")).collect()
    }
}

pub(crate) fn i32s(t: &str) -> Vec<i32> {
    if t == "_" {
        Vec::new()
    } else {
        t.split(',').map(|v| v.parse().expect("i32")).collect()
    }
}

/// The token lists (after the kind) of every oracle row of `kind`.
pub(crate) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter_map(|l| {
            let mut it = l.split(' ');
            (it.next() == Some(kind)).then(|| it.collect())
        })
        .collect()
}

pub(crate) struct TriRow {
    pub(crate) name: &'static str,
    pub(crate) pts: Vec<[f64; 2]>,
    pub(crate) tris: Vec<u32>,
    pub(crate) halfedges: Vec<i32>,
    pub(crate) hull: Vec<u32>,
}

pub(crate) fn tri_rows() -> Vec<TriRow> {
    rows("tri")
        .into_iter()
        .map(|r| TriRow {
            name: r[0],
            pts: pts(r[1]),
            tris: u32s(r[2]),
            halfedges: i32s(r[3]),
            hull: u32s(r[4]),
        })
        .collect()
}

pub(crate) fn tri_row(name: &str) -> TriRow {
    tri_rows()
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("fixture {name} missing from the oracle"))
}

#[test]
fn oracle_row_counts_are_complete() {
    let count = |k: &str| rows(k).len();
    assert_eq!(count("ori"), 151);
    assert_eq!(count("psa"), 68);
    assert_eq!(count("inc"), 124);
    assert_eq!(count("cir"), 60);
    assert_eq!(count("cen"), 60);
    assert_eq!(count("qs"), 55);
    assert_eq!(count("tri"), 85);
    assert_eq!(count("mut"), 34);
    assert_eq!(count("muto"), 9);
    assert_eq!(ORACLE.lines().count(), 646);
}

#[test]
fn oracle_orient2d_rows_match_baseline_bits_stage_and_exact_sign() {
    let mut per_stage = [0usize; 5];
    for r in rows("ori") {
        let v: Vec<f64> = r[..6].iter().map(|h| hex(h)).collect();
        let (got, stage) = orient2d_staged(v[0], v[1], v[2], v[3], v[4], v[5]);
        assert_eq!(bits(got), r[6], "orient2d value for {r:?}");
        assert_eq!(stage.to_string(), r[7], "orient2d stage for {r:?}");
        assert_eq!(
            orient2d(v[0], v[1], v[2], v[3], v[4], v[5]).to_bits(),
            got.to_bits()
        );
        let sign = if got > 0.0 {
            "+"
        } else if got < 0.0 {
            "-"
        } else {
            "0"
        };
        assert_eq!(sign, r[8], "sign must equal the exact big-integer sign");
        per_stage[stage as usize] += 1;
    }
    // every stage of the adaptive predicate is exercised by at least 6 rows
    assert!(per_stage.iter().all(|&c| c >= 6), "stages {per_stage:?}");
    assert_eq!(per_stage.iter().sum::<usize>(), 151);
}

#[test]
fn oracle_pseudo_angle_and_in_circle_rows_match_baseline() {
    for r in rows("psa") {
        let got = pseudo_angle(hex(r[0]), hex(r[1]));
        assert_eq!(bits(got), r[2], "pseudoAngle {r:?}");
    }
    let mut inside = 0;
    for r in rows("inc") {
        let v: Vec<f64> = r[..8].iter().map(|h| hex(h)).collect();
        let got = in_circle(v[0], v[1], v[2], v[3], v[4], v[5], v[6], v[7]);
        assert_eq!(if got { "1" } else { "0" }, r[8], "inCircle {r:?}");
        inside += usize::from(got);
    }
    // the corpus holds both answers (not a constant function), ties included
    assert_eq!(inside, 59);
}

#[test]
fn oracle_circumradius_and_circumcenter_rows_match_baseline() {
    for r in rows("cir") {
        let v: Vec<f64> = r[..6].iter().map(|h| hex(h)).collect();
        let got = circumradius(v[0], v[1], v[2], v[3], v[4], v[5]);
        assert_eq!(bits(got), r[6], "circumradius {r:?}");
    }
    for r in rows("cen") {
        let v: Vec<f64> = r[..6].iter().map(|h| hex(h)).collect();
        let got = circumcenter(v[0], v[1], v[2], v[3], v[4], v[5]);
        assert_eq!(bits(got[0]), r[6], "circumcenter x {r:?}");
        assert_eq!(bits(got[1]), r[7], "circumcenter y {r:?}");
    }
}

#[test]
fn oracle_quicksort_rows_reproduce_the_unstable_tie_order() {
    let mut tied = 0;
    for r in rows("qs") {
        let dists: Vec<f64> = if r[0] == "_" { Vec::new() } else { nums(r[0]) };
        let mut ids: Vec<u32> = (0..dists.len() as u32).collect();
        quicksort(&mut ids, &dists, 0, dists.len() as isize - 1);
        assert_eq!(ids, u32s(r[1]), "quicksort of {} distances", dists.len());
        if ids
            .windows(2)
            .any(|w| dists[w[0] as usize] == dists[w[1] as usize])
        {
            tied += 1;
        }
    }
    assert_eq!(tied, 16, "rows whose result contains equal distances");
}

#[test]
fn oracle_triangulation_rows_match_baseline_index_for_index() {
    for row in tri_rows() {
        let got = delaunay_from(&row.pts).unwrap_or_else(|e| panic!("{}: {e:?}", row.name));
        assert_eq!(got.triangles, row.tris, "{} triangles", row.name);
        assert_eq!(got.halfedges, row.halfedges, "{} halfedges", row.name);
        assert_eq!(got.hull, row.hull, "{} hull", row.name);
        let raw: Vec<u32> = triangulate_raw(&row.pts);
        if row.pts.len() < 3 {
            assert!(raw.is_empty(), "{}: fewer than three points", row.name);
        } else {
            assert_eq!(raw, row.tris, "{} triangulate_raw", row.name);
        }
    }
}

#[test]
fn oracle_small_and_collinear_sets_give_empty_triangle_lists() {
    let mut empty_with_points = 0;
    for row in tri_rows() {
        if row.tris.is_empty() {
            assert!(row.halfedges.is_empty(), "{}", row.name);
            assert!(triangulate_raw(&row.pts).is_empty(), "{}", row.name);
            empty_with_points += usize::from(row.pts.len() >= 3);
        }
    }
    // same5, col-diag5, col-diag-rev, col-horiz6, col-vert6, col-shuffled, col-dup, col-vert-dup, col-30
    assert_eq!(empty_with_points, 9);
}
