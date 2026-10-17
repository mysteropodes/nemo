//! L01-G: the Paper.js move/scale arithmetic against the oracle (`tr` and `sc` rows of
//! fixtures/object-parametric-edit/oracle.txt, produced by the shipped Paper.js at fbe4b8a3), bit for bit, plus
//! hand-written literals. Nothing here goes through cos/sin, so every comparison is exact.
use super::{scale_segments, translate_segments};
use crate::evaluation::object_vector_kernels::object_parametric_geometry::{
    ParamGeomError, ParamSegment,
};

#[path = "object_parametric_transform_controls.rs"]
mod controls;

pub(super) const ORACLE: &str = include_str!("fixtures/object-parametric-edit/oracle.txt");

pub(super) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split('\t').collect::<Vec<&'static str>>())
        .filter(|cols| cols[0] == kind)
        .collect()
}

pub(super) fn hexf(cell: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(cell, 16).expect("hex f64 cell"))
}

pub(super) fn pair(cell: &str) -> [f64; 2] {
    let (first, second) = cell.split_once(',').expect("pair cell");
    [hexf(first), hexf(second)]
}

pub(super) fn segs(cell: &str) -> Vec<ParamSegment> {
    cell.split(';')
        .map(|anchor| {
            let v: Vec<f64> = anchor.split(',').map(hexf).collect();
            assert_eq!(v.len(), 6, "{anchor}");
            ParamSegment {
                point: [v[0], v[1]],
                handle_in: [v[2], v[3]],
                handle_out: [v[4], v[5]],
            }
        })
        .collect()
}

pub(super) fn bits(segments: &[ParamSegment]) -> Vec<[u64; 6]> {
    segments
        .iter()
        .map(|s| {
            [
                s.point[0].to_bits(),
                s.point[1].to_bits(),
                s.handle_in[0].to_bits(),
                s.handle_in[1].to_bits(),
                s.handle_out[0].to_bits(),
                s.handle_out[1].to_bits(),
            ]
        })
        .collect()
}

fn anchor(point: [f64; 2], handle_in: [f64; 2], handle_out: [f64; 2]) -> ParamSegment {
    ParamSegment {
        point,
        handle_in,
        handle_out,
    }
}

#[test]
fn translate_matches_paper_bit_for_bit_on_every_row() {
    let all = rows("tr");
    assert_eq!(all.len(), 74);
    for row in &all {
        let [dx, dy] = pair(row[4]);
        let got = translate_segments(&segs(row[5]), dx, dy).expect("translated");
        assert_eq!(bits(&got), bits(&segs(row[6])), "{} {}", row[1], row[4]);
    }
}

#[test]
fn scale_matches_paper_bit_for_bit_on_every_row() {
    let all = rows("sc");
    assert_eq!(all.len(), 102);
    for row in &all {
        let (factors, pivot) = (pair(row[5]), pair(row[6]));
        let got = scale_segments(&segs(row[7]), factors[0], factors[1], pivot).expect("scaled");
        assert_eq!(bits(&got), bits(&segs(row[8])), "{} {}", row[1], row[5]);
    }
}

#[test]
fn an_identity_matrix_returns_the_input_untouched() {
    let curved = vec![
        anchor([1.1, 2.2], [0.3, 0.7], [-0.3, 0.1]),
        anchor([3.3, 4.4], [-0.9, 0.2], [0.5, 0.6]),
    ];
    for result in [
        translate_segments(&curved, 0.0, 0.0),
        translate_segments(&curved, -0.0, 0.0),
        scale_segments(&curved, 1.0, 1.0, [12.5, -3.25]),
        scale_segments(&curved, 1.0, 1.0, [0.0, 0.0]),
    ] {
        assert_eq!(bits(&result.expect("identity")), bits(&curved));
    }
}

#[test]
fn a_handle_within_the_paper_epsilon_is_not_transformed() {
    let tiny = vec![anchor([0.0, 0.0], [1e-13, 0.0], [0.0, 1e-13])];
    let moved = translate_segments(&tiny, 5.0, 7.0).expect("moved");
    assert_eq!(moved[0].point, [5.0, 7.0]);
    assert_eq!(moved[0].handle_in, [1e-13, 0.0]);
    assert_eq!(moved[0].handle_out, [0.0, 1e-13]);
    let scaled = scale_segments(&tiny, 3.0, 3.0, [1.0, 1.0]).expect("scaled");
    assert_eq!(scaled[0].handle_in, [1e-13, 0.0]);
    assert_eq!(scaled[0].handle_out, [0.0, 1e-13]);
}

#[test]
fn literal_translate_and_scale_of_a_straight_segment_list() {
    let line = vec![
        anchor([10.0, 20.0], [0.0, 0.0], [0.0, 0.0]),
        anchor([30.0, 40.0], [0.0, 0.0], [0.0, 0.0]),
    ];
    let moved = translate_segments(&line, 2.5, -4.0).expect("moved");
    assert_eq!(moved[0].point, [12.5, 16.0]);
    assert_eq!(moved[1].point, [32.5, 36.0]);
    // x' = 10 + (x - 10) * 2, y' = 20 + (y - 20) * 3 about (10, 20).
    let scaled = scale_segments(&line, 2.0, 3.0, [10.0, 20.0]).expect("scaled");
    assert_eq!(scaled[0].point, [10.0, 20.0]);
    assert_eq!(scaled[1].point, [50.0, 80.0]);
    // A negative factor mirrors the points and keeps the anchor order.
    let mirrored = scale_segments(&line, -1.0, 1.0, [0.0, 0.0]).expect("mirrored");
    assert_eq!(mirrored[0].point, [-10.0, 20.0]);
    assert_eq!(mirrored[1].point, [-30.0, 40.0]);
    assert!(translate_segments(&[], 1.0, 1.0).expect("empty").is_empty());
}

#[test]
fn a_non_finite_input_or_an_overflowing_result_is_a_typed_refusal() {
    let line = vec![anchor([1.0, 1.0], [0.0, 0.0], [0.0, 0.0])];
    assert_eq!(
        translate_segments(&line, f64::NAN, 0.0),
        Err(ParamGeomError::NonFinite)
    );
    assert_eq!(
        scale_segments(&line, 2.0, f64::INFINITY, [0.0, 0.0]),
        Err(ParamGeomError::NonFinite)
    );
    assert_eq!(
        scale_segments(&line, 1.0, 1.0, [f64::NAN, 0.0]),
        Err(ParamGeomError::NonFinite)
    );
    let broken = vec![anchor([f64::NAN, 1.0], [0.0, 0.0], [0.0, 0.0])];
    assert_eq!(
        translate_segments(&broken, 1.0, 1.0),
        Err(ParamGeomError::NonFinite)
    );
    let big = vec![anchor([1e308, 1.0], [0.0, 0.0], [0.0, 0.0])];
    assert_eq!(
        scale_segments(&big, 10.0, 1.0, [0.0, 0.0]),
        Err(ParamGeomError::NonFinite)
    );
}
