//! Oracle tests for the application of one placement to a record (L04-D, I08).
//!
//! `fixtures/duplicator-placement-apply/oracle.txt` was produced by
//! `tools/gen_oracle.cjs` of the L04-D leaf. Its `cid` rows evaluate the
//! verbatim `'dup' + (dup.seed || 0) + '_' + k` expression of
//! `applyLayerDuplicator`; its scenarios call the verbatim
//! `applyLayerDuplicator` (baseline `fbe4b8a3`, over Paper.js 0.12.18) with an
//! injected or real `_duplicatorClonePlacement` and record the descriptors it
//! received and the strokes it returned. The scenarios are replayed through the
//! real kernel by `temporal::test_codec`, which compares identity, geometry,
//! fill, `dup3D` and the reported unavailable fields.

use super::{apply_placement, dup_clone_id, ApplyError, Dup3d, PlacementApply, UnavailableField};
use crate::evaluation::object_component_kernels::duplicator_placement_color::FillHueError;
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::test_codec::{
    byte_of, hex, num, parse_record, rows, utf8,
};
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::{
    Descriptor, MatrixError, Pivot,
};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::duplicator_placement_temporal::test_codec::{
    mismatch, run, scenarios,
};
use crate::evaluation::object_component_kernels::duplicator_placement_temporal::BASELINE_RULES;

pub(crate) const ORACLE: &str = include_str!("fixtures/duplicator-placement-apply/oracle.txt");

pub(crate) fn seed_of(tok: &str) -> JsNumber {
    match tok {
        "u" => JsNumber::Undefined,
        "n" => JsNumber::Null,
        h => JsNumber::Value(hex(h)),
    }
}

/// A placement with only a translation.
pub(crate) fn shift(dx: f64, dy: f64) -> PlacementApply {
    PlacementApply {
        matrix: Descriptor {
            rot: 0.0,
            sx: 1.0,
            sy: 1.0,
            dx,
            dy,
        },
        opacity_factor: 1.0,
        hue_deg: 0.0,
        dup_3d: None,
    }
}

const SQUARE: &str = "src1/#336699/1/4024000000000000:4034000000000000:3ff0000000000000:0000000000000000:0000000000000000:4000000000000000;4044000000000000:4034000000000000:0000000000000000:0000000000000000:0000000000000000:0000000000000000";
const PIVOT: Pivot = Pivot { x: 100.0, y: 50.0 };

#[test]
fn clone_id_rows_match_the_baseline_expression() {
    let all = rows(ORACLE, "cid");
    assert_eq!(all.len(), 110);
    for t in all {
        let k: u32 = t[1].parse().expect("k");
        assert_eq!(dup_clone_id(seed_of(t[0]), k), utf8(t[3]), "{t:?}");
    }
}

#[test]
fn pinned_clone_ids_are_the_documented_strings() {
    let pins: [(JsNumber, u32, &str); 8] = [
        (JsNumber::Value(42.0), 0, "dup42_0"),
        (JsNumber::Value(42.0), 899, "dup42_899"),
        (JsNumber::Undefined, 1, "dup0_1"),
        (JsNumber::Null, 7, "dup0_7"),
        (JsNumber::Value(-0.0), 1, "dup0_1"),
        (JsNumber::Value(0.5), 0, "dup0.5_0"),
        (JsNumber::Value(1e21), 0, "dup1e+21_0"),
        (JsNumber::Value(-5.0), 1, "dup-5_1"),
    ];
    let oracle = rows(ORACLE, "cid");
    for (seed, k, want) in pins {
        assert_eq!(dup_clone_id(seed, k), want);
        let tok = match seed {
            JsNumber::Undefined => "u".to_string(),
            JsNumber::Null => "n".to_string(),
            JsNumber::Value(v) => format!("{:016x}", v.to_bits()),
        };
        let row = oracle
            .iter()
            .find(|t| t[0] == tok && t[1] == k.to_string())
            .expect("an oracle row for the pin");
        assert_eq!(utf8(row[3]), want, "oracle row of {want}");
    }
}

#[test]
fn every_apply_scenario_matches_the_baseline() {
    let all = scenarios(ORACLE);
    assert_eq!(all.len(), 16);
    for sc in &all {
        let ran = run(sc, BASELINE_RULES);
        assert_eq!(mismatch(sc, &ran), None, "{}", sc.name);
    }
}

#[test]
fn a_translation_moves_the_anchor_and_leaves_the_handles_alone() {
    let source = parse_record(SQUARE);
    let copy = apply_placement(&source, 2, JsNumber::Value(7.0), &shift(5.0, -3.0), PIVOT)
        .expect("finite");
    let seg = &copy.record.geometry.segments[0];
    assert_eq!(
        (seg.point.x.clone(), seg.point.y.clone()),
        (num(15.0), num(17.0))
    );
    assert_eq!(seg.handle_in.x, num(1.0));
    assert_eq!(seg.handle_in.y, num(0.0));
    assert_eq!(seg.handle_out.y, num(2.0));
    let second = &copy.record.geometry.segments[1];
    assert_eq!(
        (second.point.x.clone(), second.point.y.clone()),
        (num(45.0), num(17.0))
    );
}

#[test]
fn a_copy_keeps_the_source_identity_and_adds_its_own() {
    let source = parse_record(SQUARE);
    let a = apply_placement(&source, 0, JsNumber::Value(7.0), &shift(0.0, 0.0), PIVOT).expect("a");
    let b = apply_placement(&source, 1, JsNumber::Value(7.0), &shift(9.0, 0.0), PIVOT).expect("b");
    // both inherit the seed shape's stable target, exactly as the baseline's strokeId
    assert_eq!(a.record.target, source.target);
    assert_eq!(b.record.target, source.target);
    // the identity that tells them apart is the clone id, never the stroke id
    assert_eq!((a.clone_id.as_str(), a.dup_index), ("dup7_0", 0));
    assert_eq!((b.clone_id.as_str(), b.dup_index), ("dup7_1", 1));
    assert_ne!(a.clone_id, source.target.stroke_id());
    assert_ne!(a.clone_id, b.clone_id);
    // the other record fields are the source's
    assert_eq!(a.record.schema_version, source.schema_version);
    assert_eq!(a.record.family, source.family);
    assert_eq!(a.record.geometry.kind, source.geometry.kind);
    assert_eq!(a.record.geometry.closed, source.geometry.closed);
    assert_eq!(
        a.record.geometry.coordinate_space,
        source.geometry.coordinate_space
    );
    assert_eq!(a.record.geometry.handle_space, source.geometry.handle_space);
}

#[test]
fn applying_never_mutates_the_source_record() {
    let source = parse_record(SQUARE);
    let before = serde_json::to_vec(&source).expect("serialize");
    let place = PlacementApply {
        matrix: Descriptor {
            rot: 33.0,
            sx: 2.0,
            sy: 0.5,
            dx: 11.0,
            dy: -7.0,
        },
        opacity_factor: 0.5,
        hue_deg: 80.0,
        dup_3d: Some(Dup3d {
            dz: 1.0,
            drx: 2.0,
            dry: 3.0,
        }),
    };
    for k in 0..4 {
        let _ = apply_placement(&source, k, JsNumber::Value(3.0), &place, PIVOT).expect("copy");
        assert_eq!(
            serde_json::to_vec(&source).expect("serialize"),
            before,
            "k={k}"
        );
    }
}

#[test]
fn opacity_and_stroke_paint_are_reported_never_defaulted() {
    let source = parse_record(SQUARE);
    let with = |opacity_factor: f64, hue_deg: f64| {
        let place = PlacementApply {
            opacity_factor,
            hue_deg,
            ..shift(0.0, 0.0)
        };
        apply_placement(&source, 0, JsNumber::Undefined, &place, PIVOT)
            .expect("copy")
            .unavailable
    };
    assert_eq!(with(1.0, 0.0), vec![]);
    assert_eq!(with(0.75, 0.0), vec![UnavailableField::Opacity]);
    assert_eq!(with(0.0, 0.0), vec![UnavailableField::Opacity]);
    assert_eq!(with(1.0, 40.0), vec![UnavailableField::StrokePaint]);
    assert_eq!(
        with(0.5, -120.0),
        vec![UnavailableField::Opacity, UnavailableField::StrokePaint]
    );
    assert_eq!(with(f64::NAN, 0.0), vec![UnavailableField::Opacity]);
    assert_eq!(with(1.0, f64::NAN), vec![]);
}

#[test]
fn the_hue_rotates_the_fill_and_keeps_its_alpha() {
    let red = parse_record("r/#ff000080/0/-");
    let place = PlacementApply {
        hue_deg: 120.0,
        ..shift(0.0, 0.0)
    };
    let copy = apply_placement(&red, 0, JsNumber::Undefined, &place, PIVOT).expect("copy");
    assert_eq!(
        (
            byte_of(&copy.record.fill.r),
            byte_of(&copy.record.fill.g),
            byte_of(&copy.record.fill.b),
            byte_of(&copy.record.fill.a)
        ),
        (0, 255, 0, 128)
    );
    assert_eq!(copy.record.fill.a, red.fill.a);
}

#[test]
fn dup_3d_is_carried_as_is() {
    let source = parse_record(SQUARE);
    let d3 = Dup3d {
        dz: 12.5,
        drx: 0.0,
        dry: -3.0,
    };
    let place = PlacementApply {
        dup_3d: Some(d3),
        ..shift(1.0, 1.0)
    };
    let copy = apply_placement(&source, 0, JsNumber::Undefined, &place, PIVOT).expect("copy");
    assert_eq!(copy.dup_3d, Some(d3));
    let none =
        apply_placement(&source, 0, JsNumber::Undefined, &shift(1.0, 1.0), PIVOT).expect("copy");
    assert_eq!(none.dup_3d, None);
}

#[test]
fn unrepresentable_results_and_fills_are_typed_errors() {
    let source = parse_record(SQUARE);
    let huge = PlacementApply {
        matrix: Descriptor {
            sx: f64::MAX,
            ..shift(0.0, 0.0).matrix
        },
        ..shift(0.0, 0.0)
    };
    assert_eq!(
        apply_placement(&source, 0, JsNumber::Undefined, &huge, PIVOT).err(),
        Some(ApplyError::Geometry(MatrixError::ResultNotFinite))
    );
    let mut bad_fill = parse_record(SQUARE);
    bad_fill.fill.r = num(2.0);
    let hue = PlacementApply {
        hue_deg: 30.0,
        ..shift(0.0, 0.0)
    };
    assert_eq!(
        apply_placement(&bad_fill, 0, JsNumber::Undefined, &hue, PIVOT).err(),
        Some(ApplyError::Fill(FillHueError::ChannelOutOfRange))
    );
    // without a hue the fill is never inspected
    assert!(apply_placement(&bad_fill, 0, JsNumber::Undefined, &shift(0.0, 0.0), PIVOT).is_ok());
}
