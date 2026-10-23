//! Tests of the image-mesh topology kernel (L03-G, F17): the executed baseline oracle
//! (`createMesh`/`setOutline`/`rebuild` at fbe4b8a3, bit for bit) plus literal values
//! pinned by hand for creation, density clamps, sculpt carrying and set-outline rules.
//! The documented v1 limits, typed refusals and joins are in `_limits`.

use super::oracle_rows::{
    check, opts, rows, same_pts, square_mesh, Mismatches, L_SHAPE, ORACLE, PRODUCTION, TINY_TRI,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::{
    densify, point_in_poly,
};
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_topology::{
    create_mesh, rebuild, set_outline, signed_area2, DensityOpts, MeshTopology, TopologyError,
};

#[test]
fn oracle_row_counts_are_pinned() {
    let r = rows();
    assert_eq!(ORACLE.lines().count(), 214);
    assert_eq!(
        (
            r.sa.len(),
            r.tri.len(),
            r.crt.len(),
            r.reb.len(),
            r.so.len(),
            r.lost.len()
        ),
        (16, 25, 18, 49, 105, 1)
    );
    assert_eq!(r.so.iter().filter(|s| s.out.is_none()).count(), 8);
}

#[test]
fn production_matches_every_baseline_oracle_row() {
    assert_eq!(check(&PRODUCTION, rows()), Mismatches::default());
}

#[test]
fn create_mesh_2x2_is_the_fixture_topology() {
    let m = square_mesh(2, 2);
    let want_outline = [
        [0.0, 0.0],
        [0.5, 0.0],
        [1.0, 0.0],
        [1.0, 0.5],
        [1.0, 1.0],
        [0.5, 1.0],
        [0.0, 1.0],
        [0.0, 0.5],
    ];
    assert!(same_pts(&m.outline, &want_outline), "{:?}", m.outline);
    assert_eq!(m.outline.len(), 8);
    assert_eq!(m.verts.len(), 9);
    assert!(same_pts(&m.verts[..8], &want_outline));
    assert_eq!(m.verts[8], [0.5, 0.5]);
    assert_eq!(
        m.tris,
        vec![8, 1, 0, 0, 7, 8, 8, 2, 1, 8, 3, 2, 7, 5, 8, 8, 5, 3, 7, 6, 5, 5, 4, 3]
    );
    assert_eq!(m.tris.len() / 3, 8);
    assert_eq!(m.offsets, vec![[0.0, 0.0]; 9]);
    assert_eq!((m.cols, m.rows), (2.0, 2.0));
}

#[test]
fn outline_is_the_stable_prefix_and_the_interior_grid_is_exactly_the_inside_points() {
    let r = rows();
    let mut results: Vec<MeshTopology> = Vec::new();
    for (o, _) in &r.crt {
        results.push(create_mesh(*o).expect("create"));
    }
    for (input, _) in &r.reb {
        results.push(rebuild(input).expect("rebuild"));
    }
    for s in &r.so {
        if let Some(m) = set_outline(&s.input, &s.poly, s.opts).expect("set outline") {
            results.push(m);
        }
    }
    assert_eq!(results.len(), 18 + 49 + 97);
    for m in &results {
        let n = m.outline.len();
        assert!(same_pts(&m.verts[..n], &m.outline));
        assert_eq!(m.offsets.len(), m.verts.len());
        assert_eq!(m.tris.len() % 3, 0);
        assert!(m.tris.iter().all(|i| (*i as usize) < m.verts.len()));
        for v in &m.verts[n..] {
            assert!(
                point_in_poly(&m.outline, v[0], v[1]),
                "interior vertex outside"
            );
        }
        let (c, rw) = (m.cols, m.rows);
        if c.fract() == 0.0
            && rw.fract() == 0.0
            && (0.0..=64.0).contains(&c)
            && (0.0..=64.0).contains(&rw)
        {
            let mut want = Vec::new();
            for gy in 1..(rw as i32) {
                for gx in 1..(c as i32) {
                    let (u, v) = (f64::from(gx) / c, f64::from(gy) / rw);
                    if point_in_poly(&m.outline, u, v) {
                        want.push([u, v]);
                    }
                }
            }
            assert!(same_pts(&m.verts[n..], &want), "interior grid differs");
        }
    }
}

#[test]
fn create_clamps_falsy_defaults_and_keeps_fractions() {
    let cases: [(Option<f64>, Option<f64>, f64, f64); 9] = [
        (None, None, 4.0, 4.0),
        (Some(0.0), Some(-0.0), 4.0, 4.0),
        (Some(f64::NAN), Some(f64::NAN), 4.0, 4.0),
        (Some(-3.0), Some(-7.0), 1.0, 1.0),
        (Some(100.0), Some(1.0), 64.0, 1.0),
        (Some(f64::INFINITY), Some(f64::NEG_INFINITY), 64.0, 1.0),
        (Some(3.5), Some(2.5), 3.5, 2.5),
        (Some(1.2), None, 1.2, 4.0),
        (None, Some(0.4), 4.0, 1.0),
    ];
    for (c, r, wc, wr) in cases {
        let m = create_mesh(opts(c, r)).expect("create");
        assert_eq!((m.cols, m.rows), (wc, wr), "opts {c:?} {r:?}");
    }
    let ragged = create_mesh(opts(Some(3.5), Some(2.5))).expect("create");
    assert_eq!(
        ragged.record_density(),
        Err(TopologyError::DensityNotRecordable)
    );
    assert_eq!(square_mesh(4, 4).record_density(), Ok((4, 4)));
    assert_eq!(square_mesh(64, 1).record_density(), Ok((64, 1)));
}

#[test]
fn rebuild_reads_the_density_through_to_int32_and_keeps_the_stored_numbers() {
    let verts = |cols: f64, rows: f64| {
        let mut m = square_mesh(2, 2);
        m.cols = cols;
        m.rows = rows;
        rebuild(&m).expect("rebuild")
    };
    assert_eq!(verts(3.0, 3.0).verts.len(), 12);
    assert_eq!(verts(4_294_967_299.0, 3.0).verts, verts(3.0, 3.0).verts);
    assert_eq!(verts(3.7, 3.2).verts, verts(3.0, 3.0).verts);
    assert_eq!(
        verts(2_147_483_648.0, 3.0).verts.len(),
        8,
        "2^31 wraps to a negative"
    );
    assert_eq!(verts(f64::INFINITY, 3.0).verts.len(), 8);
    assert_eq!(verts(65.9, 3.0).verts, verts(64.0, 3.0).verts);
    assert_eq!(verts(-2.0, 5.0).verts.len(), 8);
    let kept = verts(3.7, 3.2);
    assert_eq!((kept.cols, kept.rows), (3.7, 3.2));
}

#[test]
fn rebuild_keeps_the_sculpt_when_the_density_changes() {
    let mut m = square_mesh(2, 2);
    m.offsets[8] = [0.1, 0.0];
    m.cols = 4.0;
    m.rows = 4.0;
    let out = rebuild(&m).expect("rebuild");
    assert_eq!(out.verts.len(), 17);
    let want_x = [
        0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.05, 0.05, 0.05, 0.05, 0.1, 0.05, 0.0, 0.05, 0.0,
    ];
    let got_x: Vec<f64> = out.offsets.iter().map(|o| o[0]).collect();
    assert_eq!(got_x, want_x);
    assert!(out.offsets.iter().all(|o| o[1] == 0.0));
    assert_eq!(out.verts[12], [0.5, 0.5]);
    assert_eq!(
        out.offsets[12],
        [0.1, 0.0],
        "the sculpted vertex keeps its full offset"
    );
    assert_eq!(m.offsets[8], [0.1, 0.0], "the input mesh is untouched");
    assert_eq!(m.verts.len(), 9);
}

#[test]
fn rebuild_zeroes_offsets_when_none_is_truthy() {
    let mut m = square_mesh(2, 2);
    m.offsets = vec![[-0.0, -0.0]; 9];
    m.cols = 4.0;
    m.rows = 4.0;
    let out = rebuild(&m).expect("rebuild");
    assert_eq!(out.offsets.len(), 17);
    assert!(out
        .offsets
        .iter()
        .all(|o| o[0].to_bits() == 0 && o[1].to_bits() == 0));
    m.offsets = vec![[f64::NAN, 0.0]; 9];
    let nan = rebuild(&m).expect("rebuild");
    assert!(
        nan.offsets.iter().all(|o| o[0].to_bits() == 0),
        "NaN is falsy like 0"
    );
}

#[test]
fn rebuild_without_an_outline_or_old_points_is_empty_or_zero() {
    let empty = MeshTopology {
        outline: Vec::new(),
        cols: 4.0,
        rows: 4.0,
        verts: Vec::new(),
        tris: Vec::new(),
        offsets: Vec::new(),
    };
    let out = rebuild(&empty).expect("rebuild");
    assert!(out.verts.is_empty() && out.tris.is_empty() && out.offsets.is_empty());
    let corners = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut sq = empty.clone();
    sq.outline = corners.clone();
    sq.offsets = vec![[0.3, 0.1]];
    let no_points = rebuild(&sq).expect("rebuild");
    assert_eq!(no_points.verts.len(), 4 + 9);
    assert!(
        no_points
            .offsets
            .iter()
            .all(|o| o.iter().all(|x| x.to_bits() == 0)),
        "no old points: zeros even though an old offset is truthy"
    );
    sq.verts = corners;
    sq.offsets = vec![[0.1, 0.0], [0.2, 0.0], [0.3, 0.0], [0.4, 0.0]];
    let nearest = rebuild(&sq).expect("rebuild");
    assert!(nearest.tris.len() % 3 == 0 && sq.tris.is_empty());
    let first_x: Vec<f64> = [0, 1, 2, 3, 4, 6, 10, 12]
        .iter()
        .map(|i| nearest.offsets[*i][0])
        .collect();
    assert_eq!(
        first_x,
        vec![0.1, 0.2, 0.3, 0.4, 0.1, 0.2, 0.4, 0.3],
        "no old triangles: nearest old vertex"
    );
}

#[test]
fn set_outline_refuses_fewer_than_three_points_and_changes_nothing() {
    let m = square_mesh(2, 2);
    let before = m.clone();
    for n in 0..3 {
        let poly = &L_SHAPE[..n];
        assert_eq!(set_outline(&m, poly, DensityOpts::default()), Ok(None));
    }
    assert_eq!(m, before);
}

#[test]
fn set_outline_reverses_a_negative_area_polygon_and_densifies_after_the_density() {
    let m = square_mesh(2, 2);
    let cw = [[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    let ccw = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    assert!(signed_area2(&cw) < 0.0 && signed_area2(&ccw) > 0.0);
    let a = set_outline(&m, &cw, DensityOpts::default())
        .unwrap()
        .unwrap();
    let b = set_outline(&m, &ccw, DensityOpts::default())
        .unwrap()
        .unwrap();
    assert_eq!(a, b);
    assert_eq!(a.outline[0], [0.0, 0.0]);
    assert_eq!(a.outline[3], [1.0, 0.0]);
    let eight = set_outline(&m, &ccw, opts(Some(8.0), None))
        .unwrap()
        .unwrap();
    assert_eq!(eight.outline, densify(&ccw, 0.125).unwrap());
    assert_ne!(
        eight.outline.len(),
        a.outline.len(),
        "the new density sizes the step"
    );
    assert_eq!((eight.cols, eight.rows), (8.0, 2.0));
}

#[test]
fn set_outline_stores_the_density_unclamped_and_ignores_falsy_values() {
    let m = square_mesh(2, 2);
    let big = set_outline(&m, &TINY_TRI, opts(Some(100.0), Some(1.0)))
        .unwrap()
        .unwrap();
    assert_eq!((big.cols, big.rows), (100.0, 1.0));
    assert_eq!(
        big.record_density(),
        Err(TopologyError::DensityNotRecordable)
    );
    for falsy in [0.0, -0.0, f64::NAN] {
        let kept = set_outline(&m, &L_SHAPE, opts(Some(falsy), Some(falsy)))
            .unwrap()
            .unwrap();
        assert_eq!((kept.cols, kept.rows), (2.0, 2.0));
    }
    let negative = set_outline(&m, &L_SHAPE, opts(Some(-5.0), None))
        .unwrap()
        .unwrap();
    assert_eq!((negative.cols, negative.rows), (-5.0, 2.0));
}
