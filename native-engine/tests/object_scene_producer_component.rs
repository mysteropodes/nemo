//! Oracle and pinned tests for the component evaluated-shape kernel (L07-H / I16a, D32).
//!
//! Every expected value comes from `fixtures/object-scene-producer-component/oracle.txt`
//! (see `object_scene_producer_component_rows.rs`) or is a hand-computed literal.

use super::rows::{fixture_playback, oracle, same_json, Expect, Oracle};
use super::{
    duplicator_copy, evaluate_component_shapes, Affine, EvaluateError, ResolvedComponentPlacement,
    ShapeData, ShapeError, ShapeOrigin,
};
use crate::evaluation::object_component_kernels::object_component_playback::{
    evaluate_component_playback, PlaybackOutcome, ResolvedFrame,
};
use serde_json::{json, Value};

/// The fixture component totals 12 internal frames (`sym_comp.totalFrames`).
const TOTAL_FRAMES: f64 = 12.0;

/// Parse the stored strokes the oracle recorded for one internal frame.
fn sources(o: &Oracle, inner: u32) -> Vec<ShapeData> {
    o.inputs[&inner]
        .iter()
        .map(|v| ShapeData::from_value(v).expect("fixture stroke parses"))
        .collect()
}

/// `ResolvedFrame` for the fixture layer at an outer frame, via the L04-A kernel.
fn resolved(uid: &str, frame: i64) -> Option<ResolvedFrame> {
    match evaluate_component_playback(&fixture_playback(uid), TOTAL_FRAMES, frame, None) {
        PlaybackOutcome::Blank => None,
        PlaybackOutcome::Frame(f) => Some(f),
    }
}

fn expected_json(e: &Expect) -> &Value {
    match e {
        Expect::Json(v) => v,
        other => panic!("row expects a refusal: {other:?}"),
    }
}

fn bit_equal(a: &[f64; 6], b: &[f64; 6]) -> bool {
    a.iter()
        .zip(b)
        .all(|(x, y)| x.to_bits() == y.to_bits() || (*x == 0.0 && *y == 0.0))
}

#[test]
fn oracle_row_counts_are_pinned() {
    let o = oracle();
    assert_eq!(o.apply.len(), 454);
    assert_eq!(o.dup.len(), 36);
    assert_eq!(o.inputs.len(), 12);
    assert_eq!(o.layers.len(), 168);
    assert_eq!(o.scenarios.len(), 6);
    assert_eq!(o.outs.len(), 72);
    let throws = o
        .apply
        .iter()
        .filter(|r| matches!(r.out, Expect::Throws))
        .count();
    let nonfinite = o
        .apply
        .iter()
        .filter(|r| matches!(r.out, Expect::NonFinite))
        .count();
    assert_eq!((throws, nonfinite), (18, 12));
}

#[test]
fn apply_rows_match_the_verbatim_baseline() {
    let o = oracle();
    let mut compared = 0;
    for row in o.apply.iter().filter(|r| matches!(r.out, Expect::Json(_))) {
        let shape = ShapeData::from_value(&row.input).expect(&row.id);
        let moved = shape.apply_affine(&Affine(row.matrix)).expect(&row.id);
        assert!(
            same_json(&moved.to_value(), expected_json(&row.out)),
            "row {}: {} != {}",
            row.id,
            moved.to_value(),
            expected_json(&row.out)
        );
        compared += 1;
    }
    assert_eq!(compared, 454 - 18 - 12);
}

#[test]
fn throwing_and_overflowing_rows_are_typed_refusals() {
    let o = oracle();
    for row in &o.apply {
        let result =
            ShapeData::from_value(&row.input).and_then(|s| s.apply_affine(&Affine(row.matrix)));
        match row.out {
            Expect::Throws => assert!(
                matches!(
                    result,
                    Err(ShapeError::SegmentsNotArray | ShapeError::MalformedSegment { .. })
                ),
                "row {}: {result:?}",
                row.id
            ),
            Expect::NonFinite => {
                assert_eq!(result, Err(ShapeError::NonFiniteResult), "row {}", row.id)
            }
            Expect::Json(_) => assert!(result.is_ok(), "row {}", row.id),
        }
    }
}

#[test]
fn duplicator_rows_match_the_inline_baseline_lines() {
    let o = oracle();
    for row in &o.dup {
        let shape = ShapeData::from_value(&row.input).expect(&row.id);
        let copy = duplicator_copy(&shape, &Affine(row.matrix), row.factor, row.k).expect(&row.id);
        assert!(
            same_json(&copy.to_value(), expected_json(&row.out)),
            "row {}: {} != {}",
            row.id,
            copy.to_value(),
            expected_json(&row.out)
        );
        assert_eq!(
            copy.origin(),
            ShapeOrigin::DuplicatorCopy { dup_index: row.k }
        );
        assert!(!copy.is_persistable(), "row {}", row.id);
    }
}

#[test]
fn fixture_layers_resolve_to_the_expected_internal_frames() {
    let o = oracle();
    let mut blank = 0;
    for row in &o.layers {
        match (resolved(&row.uid, row.frame), row.inner) {
            (Some(f), Some(ii)) => assert_eq!(f.raw(), ii, "{} @{}", row.uid, row.frame),
            (None, None) => blank += 1,
            (got, want) => panic!("{} @{}: {got:?} vs {want:?}", row.uid, row.frame),
        }
    }
    assert_eq!(blank, 12);
}

/// Placement for a scenario. `roll` goes through the oracle matrix literally (its
/// matrix depends on `cos`/`sin`, whose last ulp is libm dependent); every other
/// scenario computes the camera with the existing `camera_matrix_at_frame`.
fn placement(o: &Oracle, name: &str, frame: ResolvedFrame) -> ResolvedComponentPlacement {
    let sc = &o.scenarios[name];
    if name == "roll" {
        let row = o
            .outs
            .iter()
            .find(|r| r.name == name && f64::from(r.inner) == frame.raw());
        return ResolvedComponentPlacement {
            inner_frame: frame,
            sym_matrix: sc.sym,
            camera_matrix: row.and_then(|r| r.cam),
        };
    }
    ResolvedComponentPlacement::from_camera_keys(frame, sc.sym, sc.cam.as_deref(), sc.w, sc.h)
}

#[test]
fn composition_scenarios_match_the_baseline_for_every_fixture_frame() {
    let o = oracle();
    let mut checked = 0;
    for row in o.layers.iter().filter(|r| r.inner.is_some()) {
        let frame = resolved(&row.uid, row.frame).expect("not blank");
        let ii = frame.raw() as u32;
        for name in o.scenarios.keys() {
            let want = o
                .outs
                .iter()
                .find(|r| &r.name == name && r.inner == ii)
                .expect("O row");
            let got = evaluate_component_shapes(&sources(&o, ii), &placement(&o, name, frame))
                .expect("evaluates");
            assert_eq!(got.inner_frame.raw(), frame.raw());
            let values = got.to_values();
            assert_eq!(values.len(), want.out.len());
            for (g, w) in values.iter().zip(&want.out) {
                assert!(same_json(g, w), "{name} ii {ii}: {g} != {w}");
            }
            checked += 1;
        }
    }
    assert_eq!(checked, 156 * 6);
}

#[test]
fn camera_matrix_is_the_existing_kernel_and_matches_the_baseline_matrix() {
    let o = oracle();
    for ii in 0..12u32 {
        for name in ["cam", "camsym"] {
            let sc = &o.scenarios[name];
            let rf = resolved_inner(ii);
            let p = ResolvedComponentPlacement::from_camera_keys(
                rf,
                None,
                sc.cam.as_deref(),
                sc.w,
                sc.h,
            );
            let want = o
                .outs
                .iter()
                .find(|r| r.name == name && r.inner == ii)
                .unwrap();
            assert!(
                bit_equal(
                    &p.camera_matrix.expect("camera"),
                    &want.cam.expect("oracle")
                ),
                "{name} {ii}"
            );
        }
        for name in ["none", "sym"] {
            assert_eq!(placement(&o, name, resolved_inner(ii)).camera_matrix, None);
        }
        // empty key list: `sym.cameraKeys.length` is 0, so no camera is applied
        assert_eq!(
            placement(&o, "emptycam", resolved_inner(ii)).camera_matrix,
            None
        );
    }
}

/// A `ResolvedFrame` for an internal frame number, through the `single` mode.
fn resolved_inner(ii: u32) -> ResolvedFrame {
    use crate::evaluation::object_component_kernels::object_component_playback::{
        resolve_component_frame, PlayMode, PlaybackInput,
    };
    let input = PlaybackInput {
        play_mode: PlayMode::Single,
        single_frame: Some(f64::from(ii)),
        ..PlaybackInput::default()
    };
    resolve_component_frame(&input, TOTAL_FRAMES, 0, None)
}

#[test]
fn rolled_camera_matrix_differs_from_the_baseline_by_at_most_trig_roundoff() {
    let o = oracle();
    let sc = &o.scenarios["roll"];
    for ii in 0..12u32 {
        let p = ResolvedComponentPlacement::from_camera_keys(
            resolved_inner(ii),
            None,
            sc.cam.as_deref(),
            sc.w,
            sc.h,
        );
        let got = p.camera_matrix.expect("camera");
        let want = o
            .outs
            .iter()
            .find(|r| r.name == "roll" && r.inner == ii)
            .unwrap()
            .cam
            .unwrap();
        let mag = want.iter().fold(0.0_f64, |m, x| m.max(x.abs()));
        for (g, w) in got.iter().zip(&want) {
            assert!(
                (g - w).abs() <= 8.0 * mag * f64::EPSILON,
                "ii {ii}: {g} vs {w}"
            );
        }
    }
}

#[test]
fn fixture_stroke_ids_survive_in_source_order() {
    let o = oracle();
    for ii in 0..12u32 {
        let p = placement(&o, "camsym", resolved_inner(ii));
        let got = evaluate_component_shapes(&sources(&o, ii), &p).expect("evaluates");
        let ids: Vec<String> = got
            .to_values()
            .iter()
            .map(|v| v["strokeId"].as_str().expect("strokeId").to_string())
            .collect();
        assert_eq!(
            ids,
            vec![format!("s_comp_a_{ii}"), "s_comp_b_0".to_string()]
        );
    }
}

#[test]
fn rest_geometry_is_bit_identical_and_the_output_is_detached() {
    let o = oracle();
    let mut src = sources(&o, 5);
    let before = src.clone();
    let p = placement(&o, "camsym", resolved_inner(5));
    let got = evaluate_component_shapes(&src, &p).expect("evaluates");
    let snapshot = got.to_values();
    assert_eq!(src, before, "source geometry changed");
    // moved for real, so the equality above is not vacuous
    assert!(got.shapes[0].data() != &src[0]);
    // rewriting the source afterwards cannot reach the evaluated copy
    if let ShapeData::Path { segments, .. } = &mut src[0] {
        segments[0].point = [-1.0, -1.0];
    } else {
        panic!("fixture stroke 0 is a path");
    }
    assert!(src != before);
    assert_eq!(got.to_values(), snapshot);
    let again = evaluate_component_shapes(&before, &p).expect("evaluates");
    assert_eq!(again.to_values(), snapshot);
}

#[test]
fn no_placement_matrix_leaves_every_field_untouched() {
    let stored = json!([
        {"segments": [{"point": [1, 2], "handleIn": [0, 0], "handleOut": [3, 4]}], "closed": true, "strokeId": "p"},
        {"isRaster": 1, "src": "q", "x": 5, "y": 6, "width": 7, "height": 8, "rotation": 15, "strokeId": "r"},
        {"strokeId": "plain", "text": "t"}
    ]);
    let shapes: Vec<ShapeData> = stored
        .as_array()
        .unwrap()
        .iter()
        .map(|v| ShapeData::from_value(v).unwrap())
        .collect();
    let placement = ResolvedComponentPlacement {
        inner_frame: resolved_inner(0),
        sym_matrix: None,
        camera_matrix: None,
    };
    let got = evaluate_component_shapes(&shapes, &placement).unwrap();
    for (g, w) in got.to_values().iter().zip(stored.as_array().unwrap()) {
        assert!(same_json(g, w), "{g} != {w}");
    }
    assert!(got.shapes.iter().all(|s| s.is_persistable()));
}

#[test]
fn camera_is_applied_before_the_instance_matrix() {
    let shape = ShapeData::from_value(
        &json!({"isRaster": true, "x": 1, "y": 1, "width": 10, "height": 10}),
    )
    .unwrap();
    let p = ResolvedComponentPlacement {
        inner_frame: resolved_inner(0),
        sym_matrix: Some([1.0, 0.0, 0.0, 1.0, 5.0, 0.0]),
        camera_matrix: Some([2.0, 0.0, 0.0, 2.0, 0.0, 0.0]),
    };
    let got = evaluate_component_shapes(&[shape], &p).unwrap().to_values();
    // camera: (1,1) -> (2,2), size 20; then symMatrix: (2,2) -> (7,2)
    assert!(same_json(
        &got[0],
        &json!({"isRaster": true, "x": 7, "y": 2, "width": 20, "height": 20})
    ));
}

#[test]
fn duplicator_copies_are_synthetic_and_never_persistable() {
    let shape = ShapeData::from_value(&json!({"segments": [], "opacity": 0.5})).unwrap();
    let copy = duplicator_copy(&shape, &Affine([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]), 0.5, 3).unwrap();
    assert_eq!(copy.origin(), ShapeOrigin::DuplicatorCopy { dup_index: 3 });
    assert!(!copy.is_persistable());
    assert!(same_json(
        &copy.to_value(),
        &json!({"segments": [], "opacity": 0.25, "isDuplicatorCopy": true, "dupIndex": 3})
    ));
    // a stored dict that already carries the persisted tag (app.js 873) is not savable either
    let tagged =
        ShapeData::from_value(&json!({"segments": [], "isDuplicatorCopy": true, "dupIndex": 1}))
            .unwrap();
    let got = evaluate_component_shapes(
        &[tagged],
        &ResolvedComponentPlacement {
            inner_frame: resolved_inner(0),
            sym_matrix: Some([1.0, 0.0, 0.0, 1.0, 1.0, 1.0]),
            camera_matrix: None,
        },
    )
    .unwrap();
    assert_eq!(got.shapes[0].origin(), ShapeOrigin::Stored);
    assert!(!got.shapes[0].is_persistable());
}

#[test]
fn evaluation_failure_names_the_shape_and_publishes_nothing() {
    let good = ShapeData::from_value(&json!({"segments": []})).unwrap();
    let bad = ShapeData::from_value(
        &json!({"isRaster": true, "x": 1, "y": 1, "width": 1e308, "height": 1}),
    )
    .unwrap();
    let p = ResolvedComponentPlacement {
        inner_frame: resolved_inner(0),
        sym_matrix: Some([10.0, 0.0, 0.0, 10.0, 0.0, 0.0]),
        camera_matrix: None,
    };
    let err = evaluate_component_shapes(&[good.clone(), bad, good], &p).unwrap_err();
    assert_eq!(
        err,
        EvaluateError {
            shape_index: 1,
            cause: ShapeError::NonFiniteResult
        }
    );
}
