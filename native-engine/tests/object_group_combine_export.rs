//! Evidence defect 1 follow-up (#2108): the vector exporters write a combined group's result island, never nothing.
//! The islands are paint-plan `Dab` entries; the Rive exporter used to skip every `Dab`, so a combined group vanished from
//! the `.riv`. A result the exporters cannot write (a stroke or gradient style) is refused with a combine-specific reason.
use crate::export_job::native_vector_export::native_vector_export_lottie::{
    build_lottie, LottieExportError, LottieSpec,
};
use crate::export_job::native_vector_export::native_vector_export_rive::{
    plan_rive, RiveExportError, RiveSpec,
};
use crate::export_job::native_vector_export::native_vector_export_svg::{
    serialize_svg_frame, SvgExportError, SvgFrameSpec,
};
use serde_json::{json, Value};

#[path = "render_vector_support.rs"]
mod support;
use super::object_group_combine_render::grouped_of;
use support::*;

fn square(x: f64, y: f64, size: f64, rgba: [f64; 4], vector: Value) -> Value {
    let [r, g, b, a] = rgba;
    parts(
        true,
        &[[x, y], [x + size, y], [x + size, y + size], [x, y + size]],
        solid(r, g, b, a),
        vector,
    )
}

/// Big red square under a small blue one; the blue one (the style source) optionally carries a real stroke.
fn document(mode: &str, stroked: bool) -> Value {
    let blue_body = if stroked {
        body(json!({"stroke": solid(0., 0., 0., 1.), "hasRealStroke": true}))
    } else {
        body(json!({}))
    };
    grouped_of(
        vec![
            square(8., 8., 48., [1., 0., 0., 1.], body(json!({}))),
            square(24., 24., 16., [0., 0., 1., 1.], blue_body),
        ],
        Some(mode),
    )
}

fn rive_spec() -> RiveSpec {
    RiveSpec {
        start: 0,
        end: 0,
        fps: 24.into(),
        width: 64,
        height: 64,
        background: "#ffffff".into(),
        artboard_name: "A".into(),
    }
}

fn lottie_spec() -> LottieSpec {
    LottieSpec {
        start: 0,
        end: 0,
        fps: 24.into(),
        width: 64,
        height: 64,
        background: "#ffffff".into(),
        include_shadow_guides: false,
    }
}

#[test]
fn rive_writes_the_combined_result_and_the_loose_shape() {
    for mode in ["subtract", "unite", "intersect", "exclude"] {
        let snapshot = snapshot(&document(mode, false));
        let plan = plan_rive(&[plan_of(&snapshot)], &rive_spec()).expect("rive plan");
        // The group's result island and the ungrouped shape above it; the two members are not written.
        assert_eq!(plan.shapes.len(), 2, "{mode}: result island + loose shape");
    }
}

#[test]
fn lottie_writes_the_combined_result_and_the_loose_shape() {
    let snapshot = snapshot(&document("unite", false));
    let built = build_lottie(&[plan_of(&snapshot)], &lottie_spec()).expect("lottie");
    let layers = built["layers"].as_array().expect("layers");
    let shapes: usize = layers
        .iter()
        .map(|l| l["shapes"].as_array().map_or(0, Vec::len))
        .sum();
    assert!(shapes >= 2, "island + loose shape, got {shapes}");
}

#[test]
fn a_stroked_combine_result_is_refused_with_a_combine_specific_reason() {
    let snapshot = snapshot(&document("unite", true));
    let rive = plan_rive(&[plan_of(&snapshot)], &rive_spec());
    assert!(
        matches!(rive, Err(RiveExportError::Unsupported { reason, .. }) if reason.contains("combined group")),
        "rive: {:?}",
        rive.map(|_| ())
    );
    let lottie = build_lottie(&[plan_of(&snapshot)], &lottie_spec());
    assert!(
        matches!(lottie, Err(LottieExportError::Unsupported { reason, .. }) if reason.contains("combined group")),
        "lottie: {lottie:?}"
    );
    let svg = serialize_svg_frame(&plan_of(&snapshot), SvgFrameSpec::new(64, 64, 1 << 20).unwrap());
    assert!(
        matches!(svg, Err(SvgExportError::Unsupported { reason, .. }) if reason.contains("combined group")),
        "svg: {:?}",
        svg.map(|_| ())
    );
}
