//! Controls for the placement matrix: every deviation fed through the real
//! kernel (or compared against its rows) must be caught by the oracle.
//!
//! A control changes ONE thing and requires that rows then mismatch; the
//! unchanged drive is the baseline test of `duplicator_placement_matrix.rs`.

use super::test_codec::{close, hex, parse_list, parse_segs, record, same, segments_of};
use super::tests::{matrix_agrees, matrix_of, mx_rows, oracle_rows, MxRow, TRIG};
use super::{
    bounds_center, matrix_from_descriptor, matrix_from_descriptor_with, transform_geometry,
    transform_raster_rect, Descriptor, Matrix, Pivot, RasterRect,
};
use std::f64::consts::PI;

/// `mx` rows the driver does not reproduce.
fn mx_mismatches(driver: &dyn Fn(&Descriptor, Pivot) -> Matrix) -> usize {
    mx_rows()
        .iter()
        .filter(|row| !matrix_agrees(row, &driver(&row.desc, row.pivot)))
        .count()
}

#[test]
fn the_undeviated_drive_matches_every_row() {
    assert_eq!(mx_mismatches(&matrix_from_descriptor), 0);
}

#[test]
fn a_pivot_that_is_not_rebased_is_detected() {
    // Without the pivot terms the clone rotates and scales around the origin.
    let unrebased = |d: &Descriptor, p: Pivot| matrix_from_descriptor_with(d, p, false);
    let bad = mx_mismatches(&unrebased);
    let off_origin = mx_rows()
        .iter()
        .filter(|r| r.pivot.x != 0.0 || r.pivot.y != 0.0)
        .count();
    // 109 rows pivot about the origin and are unaffected; 7 of the 327 others have no rotation or scale
    assert_eq!((bad, off_origin), (320, 327));
    // every row about the origin pivot (0, 0) is still right: the rebase vanishes there
    let at_origin: Vec<MxRow> = mx_rows()
        .into_iter()
        .filter(|r| r.pivot.x == 0.0 && r.pivot.y == 0.0)
        .collect();
    assert!(!at_origin.is_empty());
    assert!(at_origin
        .iter()
        .all(|r| matrix_agrees(r, &unrebased(&r.desc, r.pivot))));
}

#[test]
fn degrees_read_as_radians_are_detected() {
    let radians = |d: &Descriptor, p: Pivot| {
        let mut m = matrix_from_descriptor(d, p);
        let rad = if d.rot == 0.0 || d.rot.is_nan() {
            0.0
        } else {
            d.rot
        };
        m.a = rad.cos() * d.sx;
        m.b = rad.sin() * d.sx;
        m
    };
    assert!(mx_mismatches(&radians) > 0);
}

#[test]
fn swapped_sine_signs_are_detected() {
    let flipped = |d: &Descriptor, p: Pivot| {
        let mut m = matrix_from_descriptor(d, p);
        m.c = -m.c;
        m
    };
    assert!(mx_mismatches(&flipped) > 0);
}

#[test]
fn exchanged_diagonal_terms_are_detected() {
    // a and d exchanged: sx and sy end up on the wrong axes.
    let crossed = |d: &Descriptor, p: Pivot| {
        let mut m = matrix_from_descriptor(d, p);
        std::mem::swap(&mut m.a, &mut m.d);
        m
    };
    assert!(mx_mismatches(&crossed) > 0);
}

#[test]
fn a_transposed_point_transform_is_detected() {
    // x*a + y*b + tx, the transpose of Paper's x*a + y*c + tx.
    let bad = oracle_rows("tr")
        .iter()
        .filter(|t| {
            let m = matrix_of(t);
            let (x, y) = (hex(t[6]), hex(t[7]));
            let tx = x * m.a + y * m.b + m.tx;
            let ty = x * m.c + y * m.d + m.ty;
            !(same(tx, hex(t[9])) && same(ty, hex(t[10])))
        })
        .count();
    assert!(bad > 0);
}

fn sd_mismatches(driver: &dyn Fn(&Matrix, &[[f64; 6]]) -> Vec<[f64; 6]>) -> usize {
    oracle_rows("sd")
        .iter()
        .filter(|t| {
            let got = driver(&matrix_of(t), &parse_segs(t[6]));
            let want = parse_segs(t[8]);
            !(got.len() == want.len()
                && got
                    .iter()
                    .zip(&want)
                    .all(|(g, w)| g.iter().zip(w).all(|(a, b)| same(*a, *b))))
        })
        .count()
}

#[test]
fn the_real_geometry_transform_matches_every_segment_row() {
    let real = |m: &Matrix, segs: &[[f64; 6]]| {
        let rec = record("s", "#102030", false, segs);
        segments_of(&crate::object_document::ObjectRecord {
            geometry: transform_geometry(m, &rec.geometry).expect("finite"),
            ..rec
        })
    };
    assert_eq!(sd_mismatches(&real), 0);
}

#[test]
fn handles_that_take_the_translation_are_detected() {
    let translated = |m: &Matrix, segs: &[[f64; 6]]| -> Vec<[f64; 6]> {
        segs.iter()
            .map(|s| {
                let (px, py) = m.transform(s[0], s[1]);
                let (ix, iy) = m.transform(s[2], s[3]);
                let (ox, oy) = m.transform(s[4], s[5]);
                [px, py, ix, iy, ox, oy]
            })
            .collect()
    };
    assert!(sd_mismatches(&translated) > 0);
}

#[test]
fn handles_that_take_nothing_are_detected() {
    let unmoved = |m: &Matrix, segs: &[[f64; 6]]| -> Vec<[f64; 6]> {
        segs.iter()
            .map(|s| {
                let (px, py) = m.transform(s[0], s[1]);
                [px, py, s[2], s[3], s[4], s[5]]
            })
            .collect()
    };
    assert!(sd_mismatches(&unmoved) > 0);
}

#[test]
fn bounds_taken_over_the_handles_or_the_mean_are_detected() {
    let rows = oracle_rows("bc");
    let mean_center = |t: &Vec<&str>| {
        let list = parse_list(t[2]);
        let pts: Vec<(f64, f64)> = list
            .iter()
            .flat_map(|r| segments_of(r).into_iter().map(|s| (s[0], s[1])))
            .collect();
        if pts.is_empty() {
            (hex(t[0]) / 2.0, hex(t[1]) / 2.0)
        } else {
            let n = pts.len() as f64;
            (
                pts.iter().map(|p| p.0).sum::<f64>() / n,
                pts.iter().map(|p| p.1).sum::<f64>() / n,
            )
        }
    };
    let bad_mean = rows
        .iter()
        .filter(|t| {
            let (x, y) = mean_center(t);
            !(same(x, hex(t[4])) && same(y, hex(t[5])))
        })
        .count();
    assert!(bad_mean > 0);
    let handle_bounds = |t: &Vec<&str>| {
        let list = parse_list(t[2]);
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        for r in &list {
            for s in segments_of(r) {
                xs.extend([s[0], s[0] + s[2], s[0] + s[4]]);
                ys.extend([s[1], s[1] + s[3], s[1] + s[5]]);
            }
        }
        if xs.is_empty() {
            return (hex(t[0]) / 2.0, hex(t[1]) / 2.0);
        }
        let (lo, hi) = (
            xs.iter().cloned().fold(f64::INFINITY, f64::min),
            xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        );
        let (lo_y, hi_y) = (
            ys.iter().cloned().fold(f64::INFINITY, f64::min),
            ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        );
        ((lo + hi) / 2.0, (lo_y + hi_y) / 2.0)
    };
    let bad_handles = rows
        .iter()
        .filter(|t| {
            let (x, y) = handle_bounds(t);
            !(same(x, hex(t[4])) && same(y, hex(t[5])))
        })
        .count();
    assert!(bad_handles > 0);
}

#[test]
fn a_missing_canvas_fallback_is_detected() {
    // rows with no anchor must answer the canvas center, not the origin
    let empty: Vec<_> = oracle_rows("bc")
        .into_iter()
        .filter(|t| {
            parse_list(t[2])
                .iter()
                .all(|r| r.geometry.segments.is_empty())
        })
        .collect();
    assert_eq!(empty.len(), 3);
    for t in &empty {
        let got = bounds_center(&parse_list(t[2]), hex(t[0]), hex(t[1])).expect("finite");
        assert!(same(got.x, hex(t[4])) && same(got.y, hex(t[5])));
        assert!(!(same(0.0, hex(t[4])) && same(0.0, hex(t[5]))), "{t:?}");
    }
}

#[test]
fn raster_sizes_scaled_by_one_axis_only_are_detected() {
    let bad = oracle_rows("rr")
        .iter()
        .filter(|t| {
            let m = matrix_of(t);
            let rect = RasterRect {
                x: hex(t[6]),
                y: hex(t[7]),
                width: hex(t[8]),
                height: hex(t[9]),
                opacity: None,
            };
            let real = transform_raster_rect(&m, &rect);
            // the bug: the height is scaled by the first column's magnitude
            let wrong_h = rect.height * (m.a * m.a + m.b * m.b).sqrt();
            same(real.height, hex(t[15])) && !same(wrong_h, hex(t[15]))
        })
        .count();
    assert!(bad > 0);
}

#[test]
fn the_trig_tolerance_is_tight_enough_to_notice_a_wrong_angle() {
    // a rotation one degree off differs from the oracle by far more than TRIG
    let off_by_a_degree = |d: &Descriptor, p: Pivot| {
        let mut shifted = *d;
        shifted.rot = d.rot + 1.0;
        matrix_from_descriptor(&shifted, p)
    };
    let rotated: Vec<MxRow> = mx_rows()
        .into_iter()
        .filter(|r| {
            r.desc.rot.is_finite() && r.desc.rot != 0.0 && r.desc.sx != 0.0 && r.desc.sy != 0.0
        })
        .collect();
    assert!(!rotated.is_empty());
    for row in &rotated {
        assert!(
            !matrix_agrees(row, &off_by_a_degree(&row.desc, row.pivot)),
            "{:?}",
            row.desc
        );
    }
    assert!(TRIG <= 1e-9 && !close(1.0, 1.0 + PI * 1e-6, TRIG));
}
