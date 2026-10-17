//! Oracle tests for the duplicator placement matrix (L04-D, I08).
//!
//! `fixtures/duplicator-placement-matrix/oracle.txt` was produced by
//! `tools/gen_oracle.cjs` of the L04-D leaf, which cuts `dupMatrixFromDescriptor`,
//! `applyMatrixToStrokeData` and `_boundsCenterOfStrokes` out of `src/js/app.js`
//! at baseline `fbe4b8a3` and runs them verbatim over the shipped Paper.js
//! 0.12.18 `Matrix` and `Point` in a Node vm. Numbers are IEEE 754 bit patterns.
//! Rows: `mx` (descriptor -> matrix), `tr` (`Matrix.transform`), `sd` (segments
//! of a stroke), `rr` (the raster branch) and `bc` (bounds center); the header
//! of the fixture spells out each column.
//!
//! The matrix rows with a nonzero rotation depend on `sin`/`cos`, which the
//! platform `libm` and V8 compute independently, so they are compared within
//! `TRIG` of relative error; every other row, and every row whose rotation is
//! falsy, is compared bit for bit.

use super::test_codec::{
    close, hex, num, parse_list, parse_record, parse_segs, rows, same, segments_of,
};
use super::{
    bounds_center, matrix_from_descriptor, transform_geometry, transform_raster_rect,
    transform_segment, Descriptor, Matrix, MatrixError, Pivot, RasterRect,
};
use crate::evaluation::object_component_kernels::duplicator_placement_jsnum::truthy;

pub(crate) const ORACLE: &str = include_str!("fixtures/duplicator-placement-matrix/oracle.txt");
pub(crate) const TRIG: f64 = 1e-9;

pub(crate) fn oracle_rows(kind: &str) -> Vec<Vec<&'static str>> {
    rows(ORACLE, kind)
}

pub(crate) fn matrix_of(t: &[&str]) -> Matrix {
    let v: Vec<f64> = t[..6].iter().map(|h| hex(h)).collect();
    Matrix {
        a: v[0],
        b: v[1],
        c: v[2],
        d: v[3],
        tx: v[4],
        ty: v[5],
    }
}

/// One `mx` row: the descriptor, the pivot and the matrix the baseline built.
pub(crate) struct MxRow {
    pub desc: Descriptor,
    pub pivot: Pivot,
    pub want: Matrix,
}

pub(crate) fn mx_rows() -> Vec<MxRow> {
    oracle_rows("mx")
        .iter()
        .map(|t| MxRow {
            desc: Descriptor {
                rot: hex(t[0]),
                sx: hex(t[1]),
                sy: hex(t[2]),
                dx: hex(t[3]),
                dy: hex(t[4]),
            },
            pivot: Pivot {
                x: hex(t[5]),
                y: hex(t[6]),
            },
            want: matrix_of(&t[8..]),
        })
        .collect()
}

/// Whether two matrices agree: exactly when the rotation is falsy, else within `TRIG`.
pub(crate) fn matrix_agrees(row: &MxRow, got: &Matrix) -> bool {
    let pairs = [
        (got.a, row.want.a),
        (got.b, row.want.b),
        (got.c, row.want.c),
        (got.d, row.want.d),
        (got.tx, row.want.tx),
        (got.ty, row.want.ty),
    ];
    if truthy(row.desc.rot) {
        pairs.iter().all(|(g, w)| close(*g, *w, TRIG))
    } else {
        pairs.iter().all(|(g, w)| same(*g, *w))
    }
}

#[test]
fn descriptor_rows_match_dup_matrix_from_descriptor() {
    let all = mx_rows();
    assert_eq!(all.len(), 436);
    let exact = all.iter().filter(|r| !truthy(r.desc.rot)).count();
    assert_eq!(exact, 117, "rows with a falsy rotation are bit-exact");
    for row in &all {
        let got = matrix_from_descriptor(&row.desc, row.pivot);
        assert!(matrix_agrees(row, &got), "{:?} -> {got:?}", row.desc);
    }
}

#[test]
fn paper_transform_rows_match_the_closed_form() {
    let all = oracle_rows("tr");
    assert_eq!(all.len(), 600);
    for t in all {
        let m = matrix_of(t.as_slice());
        let (x, y) = m.transform(hex(t[6]), hex(t[7]));
        assert!(same(x, hex(t[9])) && same(y, hex(t[10])), "{t:?}");
    }
}

fn seg_rows_equal(got: &[[f64; 6]], want: &[[f64; 6]]) -> bool {
    got.len() == want.len()
        && got
            .iter()
            .zip(want)
            .all(|(g, w)| g.iter().zip(w).all(|(a, b)| same(*a, *b)))
}

#[test]
fn segment_rows_match_apply_matrix_to_stroke_data() {
    let all = oracle_rows("sd");
    assert_eq!(all.len(), 90);
    for t in all {
        let m = matrix_of(&t);
        let rec = super::test_codec::record("s", "#102030", false, &parse_segs(t[6]));
        let got = transform_geometry(&m, &rec.geometry).expect("finite rows");
        assert!(
            seg_rows_equal(&geometry_segs(&got), &parse_segs(t[8])),
            "{t:?}"
        );
    }
}

fn geometry_segs(g: &crate::object_document::ObjectGeometry) -> Vec<[f64; 6]> {
    let rec = crate::object_document::ObjectRecord {
        geometry: g.clone(),
        ..parse_record("x/#000000/0/-")
    };
    segments_of(&rec)
}

#[test]
fn raster_rows_match_the_raster_branch() {
    let all = oracle_rows("rr");
    assert_eq!(all.len(), 40);
    for t in all {
        let m = matrix_of(&t);
        let opacity = |tok: &str| if tok == "u" { None } else { Some(hex(tok)) };
        let rect = RasterRect {
            x: hex(t[6]),
            y: hex(t[7]),
            width: hex(t[8]),
            height: hex(t[9]),
            opacity: opacity(t[10]),
        };
        let got = transform_raster_rect(&m, &rect);
        let (wx, wy, ww, wh) = (hex(t[12]), hex(t[13]), hex(t[14]), hex(t[15]));
        assert!(same(got.x, wx) && same(got.y, wy), "{t:?}");
        assert!(same(got.width, ww) && same(got.height, wh), "{t:?}");
        assert_eq!(
            got.opacity.map(f64::to_bits),
            opacity(t[16]).map(f64::to_bits)
        );
    }
}

#[test]
fn bounds_center_rows_match_bounds_center_of_strokes() {
    let all = oracle_rows("bc");
    assert_eq!(all.len(), 41);
    for t in all {
        let list = parse_list(t[2]);
        let got = bounds_center(&list, hex(t[0]), hex(t[1])).expect("finite");
        assert!(same(got.x, hex(t[4])) && same(got.y, hex(t[5])), "{t:?}");
    }
}

#[test]
fn a_set_without_anchors_takes_the_canvas_center() {
    let none: Vec<_> = Vec::new();
    let got = bounds_center(&none, 1920.0, 1080.0).expect("empty set");
    assert_eq!((got.x, got.y), (960.0, 540.0));
    let bare = [parse_record("e/#000000/0/-")];
    let got = bounds_center(&bare, 101.0, 33.0).expect("no anchor");
    assert_eq!((got.x, got.y), (50.5, 16.5));
}

#[test]
fn pinned_matrix_and_points_follow_the_documented_formula() {
    // rot 90 about (100, 50), scale 2 x 3, offset (10, -5): a = cos*2, b = sin*2,
    // c = -sin*3, d = cos*3; the pivot maps to itself plus the offset.
    let desc = Descriptor {
        rot: 90.0,
        sx: 2.0,
        sy: 3.0,
        dx: 10.0,
        dy: -5.0,
    };
    let pivot = Pivot { x: 100.0, y: 50.0 };
    let m = matrix_from_descriptor(&desc, pivot);
    assert!(close(m.a, 0.0, 1e-15) && close(m.b, 2.0, 1e-15));
    assert!(close(m.c, -3.0, 1e-15) && close(m.d, 0.0, 1e-15));
    let (x, y) = m.transform(100.0, 50.0);
    assert!(close(x, 110.0, 1e-12) && close(y, 45.0, 1e-12), "{x} {y}");
    // a handle is a vector: it takes the linear part only
    let (hx, hy) = m.transform_vector(1.0, 0.0);
    assert!(close(hx, m.a, 1e-12) && close(hy, m.b, 1e-12));
    // the identity descriptor is exactly the identity matrix
    let id = matrix_from_descriptor(
        &Descriptor {
            rot: 0.0,
            sx: 1.0,
            sy: 1.0,
            dx: 0.0,
            dy: 0.0,
        },
        pivot,
    );
    assert_eq!(
        (id.a, id.b, id.c, id.d, id.tx, id.ty),
        (1.0, 0.0, 0.0, 1.0, 0.0, 0.0)
    );
}

#[test]
fn a_falsy_rotation_is_exactly_the_unrotated_matrix() {
    for rot in [0.0, -0.0, f64::NAN] {
        let m = matrix_from_descriptor(
            &Descriptor {
                rot,
                sx: 1.5,
                sy: 2.5,
                dx: 4.0,
                dy: 6.0,
            },
            Pivot { x: 10.0, y: 20.0 },
        );
        assert_eq!((m.a, m.b, m.c, m.d), (1.5, 0.0, -0.0, 2.5));
        assert!(m.b.is_sign_positive() && m.c.is_sign_negative());
    }
}

#[test]
fn geometry_keeps_every_non_coordinate_field_and_the_source_is_untouched() {
    let source = parse_record("keep/#336699/1/4024000000000000:4034000000000000:3ff0000000000000:0000000000000000:0000000000000000:4000000000000000");
    let before = serde_json::to_vec(&source).expect("serialize");
    let m = Matrix {
        a: 2.0,
        b: 0.0,
        c: 0.0,
        d: 2.0,
        tx: 1.0,
        ty: 1.0,
    };
    let moved = transform_geometry(&m, &source.geometry).expect("finite");
    assert_eq!(serde_json::to_vec(&source).expect("serialize"), before);
    assert_eq!(moved.kind, source.geometry.kind);
    assert_eq!(moved.closed, source.geometry.closed);
    assert_eq!(moved.coordinate_space, source.geometry.coordinate_space);
    assert_eq!(moved.handle_space, source.geometry.handle_space);
    assert_eq!(moved.segments.len(), 1);
    let seg = transform_segment(&m, &source.geometry.segments[0]).expect("finite");
    assert_eq!(seg, moved.segments[0]);
    assert_eq!(seg.point.x, num(21.0));
    assert_eq!(seg.handle_in.x, num(2.0));
    assert_eq!(seg.handle_out.y, num(4.0));
}

#[test]
fn a_result_json_cannot_hold_is_a_typed_error() {
    let source = parse_record("big/#336699/0/7fefffffffffffff:0000000000000000:0000000000000000:0000000000000000:0000000000000000:0000000000000000");
    let m = Matrix {
        a: 4.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };
    assert_eq!(
        transform_geometry(&m, &source.geometry),
        Err(MatrixError::ResultNotFinite)
    );
    let nan = Matrix { a: f64::NAN, ..m };
    assert_eq!(
        transform_geometry(&nan, &source.geometry),
        Err(MatrixError::ResultNotFinite)
    );
}
