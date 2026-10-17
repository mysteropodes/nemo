//! Hand-pinned and independent-property tests for the Delaunator port (L03-F, F17).
//!
//! The literal arrays below were read from the executed baseline (the oracle rows) and are
//! written out here so a reader sees what the diagonal choice of the 2x2 `createMesh` set is.
//! The property tests do not use the oracle: they check structural invariants of any correct
//! triangulation (they were also run against the baseline by `tools/gen.cjs`).

use super::{delaunay_from, triangulate_raw, Builder, Delaunay, DelaunayError};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_delaunay_geom::{
    in_circle, quicksort,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_orient::{
    orient2d, orient2d_staged,
};

fn lattice(nx: usize, ny: usize, step: f64) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for j in 0..ny {
        for i in 0..nx {
            out.push([i as f64 * step, j as f64 * step]);
        }
    }
    out
}

/// xorshift64*, the same generator `tools/gen.cjs` `rawRng` uses.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / 9007199254740992.0
    }
    fn range(&mut self, lo: i64, hi: i64) -> i64 {
        lo + (self.next() % (hi - lo + 1) as u64) as i64
    }
}

fn expect(pts: &[[f64; 2]], tris: &[u32], half: &[i32], hull: &[u32]) {
    let d = delaunay_from(pts).expect("triangulation");
    assert_eq!(d.triangles, tris);
    assert_eq!(d.halfedges, half);
    assert_eq!(d.hull, hull);
    let plain: Vec<u32> = triangulate_raw(pts);
    assert_eq!(plain, tris, "triangulate_raw returns the plain u32 list");
}

#[test]
fn create_mesh_2x2_point_set_pins_the_baseline_diagonals() {
    // createMesh({cols:2,rows:2}): outline of 8 then the interior centre (verts9, triangles8).
    let pts = [
        [0.0, 0.0],
        [0.5, 0.0],
        [1.0, 0.0],
        [1.0, 0.5],
        [1.0, 1.0],
        [0.5, 1.0],
        [0.0, 1.0],
        [0.0, 0.5],
        [0.5, 0.5],
    ];
    expect(
        &pts,
        &[
            8, 1, 0, 0, 7, 8, 8, 2, 1, 8, 3, 2, 7, 5, 8, 8, 5, 3, 7, 6, 5, 5, 4, 3,
        ],
        &[
            8, -1, 5, -1, 14, 2, 11, -1, 0, 17, -1, 6, 20, 15, 4, 13, 23, 9, -1, -1, 12, -1, -1, 16,
        ],
        &[5, 4, 3, 2, 1, 0, 7, 6],
    );
}

#[test]
fn integer_3x3_lattice_pins_the_baseline_diagonals() {
    expect(
        &lattice(3, 3, 1.0),
        &[
            4, 1, 0, 0, 3, 4, 4, 2, 1, 4, 5, 2, 3, 6, 4, 4, 7, 5, 6, 7, 4, 7, 8, 5,
        ],
        &[
            8, -1, 5, -1, 14, 2, 11, -1, 0, 17, -1, 6, -1, 20, 4, 19, 23, 9, -1, 15, 13, -1, -1, 16,
        ],
        &[7, 8, 5, 2, 1, 0, 3, 6],
    );
}

#[test]
fn lattice_5x3_pins_triangles_and_hull_and_5x5_pins_counts() {
    let pts = lattice(5, 3, 1.0);
    let d = delaunay_from(&pts).expect("5x3");
    assert_eq!(
        d.triangles,
        [
            7, 2, 1, 1, 6, 7, 1, 0, 6, 7, 3, 2, 0, 5, 6, 7, 8, 3, 5, 11, 6, 6, 11, 7, 7, 12, 8, 11,
            12, 7, 5, 10, 11, 12, 13, 8, 8, 4, 3, 9, 4, 8, 13, 9, 8, 13, 14, 9
        ]
    );
    assert_eq!(d.hull, [13, 14, 9, 4, 3, 2, 1, 0, 5, 10, 11, 12]);
    let d5 = delaunay_from(&lattice(5, 5, 1.0)).expect("5x5");
    assert_eq!(d5.triangles.len() / 3, 32);
    assert_eq!(
        d5.hull,
        [23, 24, 19, 14, 9, 4, 3, 2, 1, 0, 5, 10, 15, 20, 21, 22]
    );
    assert_eq!(&d5.triangles[..9], &[12, 7, 6, 6, 11, 12, 7, 2, 6]);
}

#[test]
fn fewer_than_three_points_and_collinear_sets_give_empty_lists() {
    assert!(triangulate_raw(&[]).is_empty());
    assert!(triangulate_raw(&[[0.25, 0.75]]).is_empty());
    assert!(triangulate_raw(&[[0.0, 0.0], [1.0, 1.0]]).is_empty());
    let one = delaunay_from(&[[0.25, 0.75]]).expect("one point");
    assert_eq!((one.triangles.len(), one.hull), (0, vec![0]));
    let two = delaunay_from(&[[0.0, 0.0], [1.0, 1.0]]).expect("two points");
    assert_eq!(two.hull, [0, 1]);
    // a diagonal of five points: no triangle, the hull lists them by increasing x
    let diag = lattice(5, 5, 1.0)
        .into_iter()
        .enumerate()
        .filter(|(i, _)| i % 6 == 0)
        .map(|(_, p)| p)
        .collect::<Vec<_>>();
    let d = delaunay_from(&diag).expect("diagonal");
    assert!(d.triangles.is_empty() && d.halfedges.is_empty());
    assert_eq!(d.hull, [0, 1, 2, 3, 4]);
    assert!(triangulate_raw(&diag).is_empty());
    // shuffled input order: sorted by x, then listed by original index
    let shuffled = [[3.0, 3.0], [0.0, 0.0], [2.0, 2.0], [1.0, 1.0], [4.0, 4.0]];
    assert_eq!(
        delaunay_from(&shuffled).expect("shuffled").hull,
        [1, 3, 2, 0, 4]
    );
    // every x equal: the order falls back to y (increasing from the first point's y)
    let vertical = [
        [0.25, 2.5],
        [0.25, 2.0],
        [0.25, 1.5],
        [0.25, 1.0],
        [0.25, 0.5],
        [0.25, 0.0],
    ];
    assert_eq!(
        delaunay_from(&vertical).expect("vertical").hull,
        [5, 4, 3, 2, 1, 0]
    );
    // five identical points collapse to one hull entry
    let same = [[0.5, 0.5]; 5];
    let d = delaunay_from(&same).expect("identical");
    assert_eq!((d.triangles.len(), d.hull), (0, vec![0]));
}

#[test]
fn a_repeated_lattice_point_is_dropped_and_changes_nothing() {
    let base = delaunay_from(&lattice(3, 3, 1.0)).expect("base");
    let mut dup = lattice(3, 3, 1.0);
    dup.push([1.0, 1.0]);
    assert_eq!(delaunay_from(&dup).expect("dup"), base);
    let mut doubled = lattice(3, 3, 1.0);
    doubled.extend(lattice(3, 3, 1.0));
    let d = delaunay_from(&doubled).expect("doubled");
    assert_eq!(
        d.triangles, base.triangles,
        "the second copies are never inserted"
    );
    assert_eq!(d.hull, base.hull);
}

#[test]
fn unit_square_in_circle_is_a_strict_tie_so_no_flip_happens() {
    let sq = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    for k in 0..4 {
        let q: Vec<[f64; 2]> = (0..4).map(|i| sq[(i + k) % 4]).collect();
        assert!(
            !in_circle(q[0][0], q[0][1], q[1][0], q[1][1], q[2][0], q[2][1], q[3][0], q[3][1]),
            "rotation {k}: co-circular points are not strictly inside"
        );
    }
    // baseline convention (`det < 0`): for the counter-clockwise triple a point strictly inside
    // gives false and one strictly outside gives true; for the clockwise triple it is reversed
    assert!(!in_circle(0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 0.5, 0.5));
    assert!(in_circle(0.0, 0.0, 1.0, 0.0, 1.0, 1.0, 3.0, 3.0));
    assert!(in_circle(0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 0.5, 0.5));
    assert!(!in_circle(0.0, 0.0, 1.0, 1.0, 1.0, 0.0, 3.0, 3.0));
}

fn check_invariants(name: &str, pts: &[[f64; 2]], d: &Delaunay) {
    let (t, h) = (&d.triangles, &d.halfedges);
    assert_eq!(t.len() % 3, 0, "{name}");
    assert_eq!(t.len(), h.len(), "{name}");
    let next = |x: usize| x - x % 3 + (x + 1) % 3;
    for e in 0..h.len() {
        if h[e] == -1 {
            continue;
        }
        let he = h[e] as usize;
        assert_eq!(h[he], e as i32, "{name}: halfedge symmetry");
        assert_eq!(t[e], t[next(he)], "{name}: shared edge");
        assert_eq!(t[next(e)], t[he], "{name}: shared edge");
        let (a0, b0) = (e - e % 3, he - he % 3);
        let p = |i: usize| pts[t[i] as usize];
        let (p0, pr) = (p(a0 + (e + 2) % 3), p(e));
        let (pl, p1) = (p(a0 + (e + 1) % 3), p(b0 + (he + 2) % 3));
        assert!(
            !in_circle(p0[0], p0[1], pr[0], pr[1], pl[0], pl[1], p1[0], p1[1]),
            "{name}: edge {e} is locally non-Delaunay"
        );
    }
    let mut sign = 0.0f64;
    let mut area = 0.0f64;
    for k in (0..t.len()).step_by(3) {
        let (a, b, c) = (
            pts[t[k] as usize],
            pts[t[k + 1] as usize],
            pts[t[k + 2] as usize],
        );
        let o = orient2d(a[0], a[1], b[0], b[1], c[0], c[1]);
        assert!(o != 0.0, "{name}: zero-area triangle");
        if sign == 0.0 {
            sign = o.signum();
        }
        assert_eq!(sign, o.signum(), "{name}: mixed orientation");
        area += ((b[0] - a[0]) * (c[1] - a[1]) - (c[0] - a[0]) * (b[1] - a[1])).abs() / 2.0;
    }
    let mut hull_area = 0.0f64;
    for i in 0..d.hull.len() {
        let a = pts[d.hull[i] as usize];
        let b = pts[d.hull[(i + 1) % d.hull.len()] as usize];
        hull_area += a[0] * b[1] - b[0] * a[1];
    }
    hull_area = hull_area.abs() / 2.0;
    assert!(
        (area - hull_area).abs() <= 1e-9 * hull_area.max(1.0),
        "{name}: area"
    );
    assert_eq!(
        t.len() / 3,
        2 * pts.len() - 2 - d.hull.len(),
        "{name}: triangle count"
    );
}

#[test]
fn random_clouds_and_lattices_satisfy_the_triangulation_invariants() {
    for n in [10usize, 40, 120, 300] {
        let mut rng = Rng(0x1234 + n as u64);
        let pts: Vec<[f64; 2]> = (0..n).map(|_| [rng.unit(), rng.unit()]).collect();
        let d = delaunay_from(&pts).expect("cloud");
        check_invariants(&format!("rand{n}"), &pts, &d);
    }
    for (name, size) in [("lat3x3", 3usize), ("lat5x5", 5), ("lat8x8", 8)] {
        let pts = lattice(size, size, 1.0);
        let d = delaunay_from(&pts).expect("lattice");
        check_invariants(name, &pts, &d);
    }
}

fn sign(v: f64) -> i32 {
    if v > 0.0 {
        1
    } else if v < 0.0 {
        -1
    } else {
        0
    }
}

#[test]
fn orient2d_sign_equals_the_exact_integer_determinant_on_near_degenerate_triples() {
    // Coordinates are n * 2^-40 with |n| < 2^46, so every value is exact in f64 while the
    // products reach 2^90 and are not. The determinant is computed exactly in i128.
    let scaled = |n: i64| n as f64 / 1_099_511_627_776.0;
    let mut rng = Rng(0xD1CE);
    let (mut adaptive, mut zero) = (0usize, 0usize);
    let trials = 900usize;
    for k in 0..trials {
        let (ax, ay) = (
            rng.range(-(1 << 45), 1 << 45),
            rng.range(-(1 << 45), 1 << 45),
        );
        let (sx, sy) = (rng.range(-1000, 1000), rng.range(-1000, 1000));
        let (p, q) = (
            rng.range(-(1 << 20), 1 << 20),
            rng.range(-(1 << 20), 1 << 20),
        );
        let e = (k % 3) as i64 - 1;
        let (bx, by) = (ax + p * sx, ay + p * sy);
        let (cx, cy) = (ax + q * sx, ay + q * sy + e);
        let det = (ay - cy) as i128 * (bx - cx) as i128 - (ax - cx) as i128 * (by - cy) as i128;
        let (v, stage) = orient2d_staged(
            scaled(ax),
            scaled(ay),
            scaled(bx),
            scaled(by),
            scaled(cx),
            scaled(cy),
        );
        assert_eq!(sign(v), det.signum() as i32, "trial {k}");
        adaptive += usize::from(stage > 0);
        zero += usize::from(det == 0);
    }
    assert!(
        adaptive >= trials / 4,
        "adaptive stages answered {adaptive} of {trials}"
    );
    assert!(zero >= trials / 4, "exactly collinear triples {zero}");
}

#[test]
fn a_non_finite_coordinate_is_a_typed_error_and_maps_to_an_empty_list() {
    let nan = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [f64::NAN, 0.0]];
    assert_eq!(
        delaunay_from(&nan),
        Err(DelaunayError::NonFiniteCoordinate { point: 3 })
    );
    assert!(triangulate_raw(&nan).is_empty());
    let inf = [[0.0, 0.0], [f64::INFINITY, 0.0], [0.0, 1.0], [1.0, 1.0]];
    assert_eq!(
        delaunay_from(&inf),
        Err(DelaunayError::NonFiniteCoordinate { point: 1 })
    );
    assert!(triangulate_raw(&inf).is_empty());
}

#[test]
fn a_point_on_the_circumcentre_has_no_hash_key_and_is_refused() {
    let mut b = Builder::new(vec![0.0, 0.0, 2.0, 0.0, 0.0, 2.0]);
    b.cx = 1.0;
    b.cy = 1.0;
    assert_eq!(b.hash_size, 2);
    assert_eq!(b.hash_key(1.0, 1.0), Err(DelaunayError::DegenerateGeometry));
    assert_eq!(b.hash_key(2.0, 0.0), Ok(0));
    assert_eq!(b.hash_key(0.0, 2.0), Ok(1));
}

#[test]
fn quicksort_orders_by_distance_keeps_every_id_and_is_deterministic() {
    let mut rng = Rng(77);
    let dists: Vec<f64> = (0..500).map(|_| (rng.next() % 17) as f64).collect();
    let mut a: Vec<u32> = (0..500).collect();
    let mut b = a.clone();
    quicksort(&mut a, &dists, 0, 499);
    quicksort(&mut b, &dists, 0, 499);
    assert_eq!(a, b);
    assert!(a
        .windows(2)
        .all(|w| dists[w[0] as usize] <= dists[w[1] as usize]));
    let mut seen = a.clone();
    seen.sort_unstable();
    assert_eq!(seen, (0..500).collect::<Vec<u32>>());
    // it is not a stable sort: equal distances do not keep their input order
    let tied = [1.0f64; 40];
    let mut ids: Vec<u32> = (0..40).collect();
    quicksort(&mut ids, &tied, 0, 39);
    assert!(ids.windows(2).any(|w| w[0] > w[1]));
}
