//! Controls for the placement application: every deviation fed through the
//! real kernel (or compared against its rows) must be caught by the oracle.
//!
//! A control changes ONE thing and requires that rows or scenarios then
//! mismatch; the unchanged drive is the baseline test of
//! `duplicator_placement_apply.rs`.

use super::tests::{seed_of, shift, ORACLE};
use super::{apply_placement, dup_clone_id, PlacementApply};
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::test_codec::{
    parse_record, rows, utf8,
};
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::{
    matrix_from_descriptor, transform_geometry, Pivot,
};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::duplicator_placement_temporal::test_codec::{
    mismatch, run_tweaked, scenarios, Scenario,
};
use crate::evaluation::object_component_kernels::duplicator_placement_temporal::BASELINE_RULES;
use crate::object_document::ObjectRecord;

/// Scenarios (by name) that the tweaked descriptors no longer reproduce.
fn failing(tweak: &dyn Fn(PlacementApply) -> PlacementApply) -> Vec<String> {
    scenarios(ORACLE)
        .iter()
        .filter(|sc| mismatch(sc, &run_tweaked(sc, BASELINE_RULES, tweak)).is_some())
        .map(|sc: &Scenario| sc.name.clone())
        .collect()
}

fn id_mismatches(driver: &dyn Fn(JsNumber, u32) -> String) -> usize {
    rows(ORACLE, "cid")
        .iter()
        .filter(|t| driver(seed_of(t[0]), t[1].parse().expect("k")) != utf8(t[3]))
        .count()
}

#[test]
fn the_undeviated_drive_matches_every_row_and_scenario() {
    assert_eq!(id_mismatches(&dup_clone_id), 0);
    assert!(failing(&|p| p).is_empty());
}

#[test]
fn a_clone_id_without_the_zero_default_is_detected() {
    // `dup.seed || 0` is what turns an absent, null, zero and NaN seed into "0"
    let undefined_text = |seed: JsNumber, k: u32| match seed {
        JsNumber::Undefined => format!("dupundefined_{k}"),
        JsNumber::Null => format!("dupnull_{k}"),
        other => dup_clone_id(other, k),
    };
    assert_eq!(id_mismatches(&undefined_text), 10);
}

#[test]
fn a_clone_id_that_is_off_by_one_or_misspelled_is_detected() {
    let plus_one = |seed: JsNumber, k: u32| dup_clone_id(seed, k + 1);
    assert_eq!(id_mismatches(&plus_one), rows(ORACLE, "cid").len());
    let dash = |seed: JsNumber, k: u32| dup_clone_id(seed, k).replace('_', "-");
    assert_eq!(id_mismatches(&dash), rows(ORACLE, "cid").len());
    let upper = |seed: JsNumber, k: u32| dup_clone_id(seed, k).replace("dup", "DUP");
    assert_eq!(id_mismatches(&upper), rows(ORACLE, "cid").len());
}

#[test]
fn a_clone_id_that_prints_the_seed_like_rust_is_detected() {
    // Rust's `{}` never uses an exponent, JavaScript does from 1e21 and below 1e-6
    let rust_display = |seed: JsNumber, k: u32| {
        let text = match seed {
            JsNumber::Value(v) if v != 0.0 && !v.is_nan() => format!("{v}"),
            _ => "0".to_string(),
        };
        format!("dup{text}_{k}")
    };
    // 1e21, 1.5e21, 1e-7, 5e-324 and both infinities differ, for five clone indices each
    assert_eq!(id_mismatches(&rust_display), 30);
}

#[test]
fn a_hue_that_is_not_applied_is_detected() {
    let names = failing(&|p| PlacementApply { hue_deg: 0.0, ..p });
    for expected in ["opacity_and_hue", "hue_only_full_wheel"] {
        assert!(
            names.iter().any(|n| n == expected),
            "{expected} in {names:?}"
        );
    }
    assert_eq!(names.len(), 5, "{names:?}");
}

#[test]
fn an_opacity_factor_that_is_dropped_is_detected() {
    let names = failing(&|p| PlacementApply {
        opacity_factor: 1.0,
        ..p
    });
    assert!(names.iter().any(|n| n == "opacity_and_hue"), "{names:?}");
    assert_eq!(names.len(), 3, "{names:?}");
}

#[test]
fn a_dropped_dup_3d_is_detected() {
    let names = failing(&|p| PlacementApply { dup_3d: None, ..p });
    assert_eq!(names, vec!["dup3d_on_3d_layer".to_string()]);
}

#[test]
fn swapped_axes_scales_and_offsets_are_detected() {
    let swap_scale = failing(&|mut p| {
        std::mem::swap(&mut p.matrix.sx, &mut p.matrix.sy);
        p
    });
    assert!(
        swap_scale.iter().any(|n| n == "rotate_scale"),
        "{swap_scale:?}"
    );
    assert_eq!(swap_scale.len(), 4, "{swap_scale:?}");
    let swap_offset = failing(&|mut p| {
        std::mem::swap(&mut p.matrix.dx, &mut p.matrix.dy);
        p
    });
    assert!(
        swap_offset.iter().any(|n| n == "translate_only"),
        "{swap_offset:?}"
    );
    assert_eq!(swap_offset.len(), 9, "{swap_offset:?}");
    let reversed = failing(&|mut p| {
        p.matrix.rot = -p.matrix.rot;
        p
    });
    assert!(reversed.iter().any(|n| n == "rotate_scale"), "{reversed:?}");
    assert_eq!(reversed.len(), 5, "{reversed:?}");
}

/// The deviation the contract names: transform the source IN PLACE.
fn apply_in_place(source: &mut ObjectRecord, place: &PlacementApply, pivot: Pivot) {
    let matrix = matrix_from_descriptor(&place.matrix, pivot);
    source.geometry = transform_geometry(&matrix, &source.geometry).expect("finite");
}

const SQUARE: &str = "src1/#336699/1/4024000000000000:4034000000000000:3ff0000000000000:0000000000000000:0000000000000000:4000000000000000";

#[test]
fn a_source_mutated_in_place_is_detected_and_the_real_apply_is_not() {
    let pivot = Pivot { x: 5.0, y: 5.0 };
    let place = shift(7.0, 0.0);
    // the real kernel leaves the serialized source untouched
    let source = parse_record(SQUARE);
    let before = serde_json::to_vec(&source).expect("serialize");
    apply_placement(&source, 0, JsNumber::Undefined, &place, pivot).expect("copy");
    assert_eq!(serde_json::to_vec(&source).expect("serialize"), before);
    // the in-place deviation changes the serialized bytes, so the same check fails
    let mut mutated = parse_record(SQUARE);
    apply_in_place(&mut mutated, &place, pivot);
    assert_ne!(serde_json::to_vec(&mutated).expect("serialize"), before);
    // and a second clone built from the mutated source would drift
    let drift = apply_placement(&mutated, 0, JsNumber::Undefined, &place, pivot).expect("copy");
    let honest = apply_placement(&source, 0, JsNumber::Undefined, &place, pivot).expect("copy");
    assert_ne!(drift.record.geometry, honest.record.geometry);
}

#[test]
fn a_copy_identity_that_is_the_source_stroke_id_is_detected() {
    let source = parse_record(SQUARE);
    let a = apply_placement(
        &source,
        0,
        JsNumber::Value(1.0),
        &shift(0.0, 0.0),
        Pivot { x: 0.0, y: 0.0 },
    )
    .expect("copy");
    let b = apply_placement(
        &source,
        1,
        JsNumber::Value(1.0),
        &shift(0.0, 0.0),
        Pivot { x: 0.0, y: 0.0 },
    )
    .expect("copy");
    // identity by stroke id would make both copies indistinguishable
    assert_eq!(a.record.target.stroke_id(), b.record.target.stroke_id());
    assert_ne!(a.clone_id, b.clone_id);
    assert_ne!(a.clone_id, a.record.target.stroke_id());
}
