//! Literal baseline oracles for property curve edits.
use super::*;
use serde_json::{json, Value};
fn track() -> OrderTrack {
    serde_json::from_value(json!({"keys":[
  {"frame":0,"v":[10],"hold":true,"hIn":[0,0]},
  {"frame":4,"v":[20],"hold":true,"hOut":[0,0]},
  {"frame":8,"v":[30],"hold":true}]}))
    .unwrap()
}
fn image(key: &OrderKey) -> Value {
    serde_json::to_value(key).unwrap()
}
fn curve(key: &OrderKey) -> Value {
    image(key)["curvePoints"].clone()
}
fn n(n: &Number) -> f64 {
    n.as_f64().unwrap()
}
fn near(actual: f64, want: f64) {
    assert!((actual - want).abs() < 1e-12, "{actual} != {want}");
}

#[test]
fn literal_presets_touch_the_baseline_previous_and_selected_keys_only() {
    let original = track();
    for (preset, expected, previous, selected) in [
        (
            CurvePreset::Linear,
            json!([{"x":0.0,"y":0.0},{"x":1.0,"y":1.0}]),
            true,
            true,
        ),
        (
            CurvePreset::Ease,
            json!([{"x":0.0,"y":0.0},{"x":0.25,"y":0.09},{"x":0.5,"y":0.5},{"x":0.75,"y":0.91},{"x":1.0,"y":1.0}]),
            true,
            true,
        ),
        (
            CurvePreset::EaseIn,
            json!([{"x":0.0,"y":0.0},{"x":0.25,"y":0.25},{"x":0.5,"y":0.5},{"x":0.75,"y":0.91},{"x":1.0,"y":1.0}]),
            true,
            false,
        ),
        (
            CurvePreset::EaseOut,
            json!([{"x":0.0,"y":0.0},{"x":0.25,"y":0.09},{"x":0.5,"y":0.5},{"x":0.75,"y":0.75},{"x":1.0,"y":1.0}]),
            false,
            true,
        ),
    ] {
        let (out, count) = apply_curve_preset(&original, &[1], preset).unwrap();
        assert_eq!(count, 1);
        assert_eq!(out.keys[0].hold, Presence::Present(true));
        assert_eq!(out.keys[1].hold, Presence::Present(false));
        assert_eq!(out.keys[2], original.keys[2]);
        assert_eq!(out.keys[0].h_in, original.keys[0].h_in);
        assert_eq!(out.keys[1].h_out, original.keys[1].h_out);
        if previous {
            assert_eq!(curve(&out.keys[0]), expected);
        } else {
            assert!(out.keys[0].curve_points.missing());
        }
        if selected {
            assert_eq!(curve(&out.keys[1]), expected);
        } else {
            assert!(out.keys[1].curve_points.missing());
        }
    }
    assert!(original.keys[0].curve_points.missing());
}
#[test]
fn hold_toggles_and_ease_in_first_changes_hold_even_when_reported_count_zero() {
    let original = track();
    let (first, count) = apply_curve_preset(&original, &[1], CurvePreset::Hold).unwrap();
    assert_eq!(count, 1);
    assert_eq!(first.keys[1].hold, Presence::Present(false));
    assert!(first.keys[1].curve_points.missing());
    let (twice, _) = apply_curve_preset(&first, &[1], CurvePreset::Hold).unwrap();
    assert_eq!(twice, original);
    let (incoming, count) = apply_curve_preset(&original, &[0], CurvePreset::EaseIn).unwrap();
    assert_eq!(count, 0);
    assert_ne!(incoming, original);
    assert_eq!(incoming.keys[0].hold, Presence::Present(false));
    let mut absent = original.clone();
    absent.keys[0].hold = Presence::Missing;
    let (promoted, _) = apply_curve_preset(&absent, &[0], CurvePreset::EaseIn).unwrap();
    assert_eq!(
        promoted.keys[0].hold,
        Presence::Present(false),
        "Missing differs from authored false"
    );
}
#[test]
fn interpolation_kind_treats_zero_manual_tangents_as_authored_smooth() {
    for (json, expected) in [
        (json!({"frame":0,"v":[0]}), InterpolationKind::Smooth),
        (
            json!({"frame":0,"v":[0],"curvePoints":null}),
            InterpolationKind::Smooth,
        ),
        (
            json!({"frame":0,"v":[0],"curvePoints":[]}),
            InterpolationKind::Smooth,
        ),
        (
            json!({"frame":0,"v":[0],"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}]}),
            InterpolationKind::Linear,
        ),
        (
            json!({"frame":0,"v":[0],"curvePoints":[{"x":0,"y":0,"tx":0,"ty":0},{"x":1,"y":1}]}),
            InterpolationKind::Smooth,
        ),
        (
            json!({"frame":0,"v":[0],"hold":true}),
            InterpolationKind::Hold,
        ),
    ] {
        assert_eq!(
            key_interpolation_kind(&serde_json::from_value(json).unwrap()).unwrap(),
            expected
        );
    }
}
#[test]
fn influence_reads_have_literal_boundary_and_previous_segment_semantics() {
    let mut t = track();
    t.keys[0].curve_points = Presence::Present(Some(
        serde_json::from_value(json!([
    {"x":0,"y":0},{"x":1,"y":1}]))
        .unwrap(),
    ));
    assert_eq!(key_ease_influence(&t, 0, InfluenceSide::In).unwrap(), None);
    assert_eq!(
        key_ease_influence(&t, 0, InfluenceSide::Out).unwrap(),
        Some(100)
    );
    assert_eq!(
        key_ease_influence(&t, 1, InfluenceSide::In).unwrap(),
        Some(100)
    );
    assert_eq!(key_ease_influence(&t, 1, InfluenceSide::Out).unwrap(), None);
    assert_eq!(key_ease_influence(&t, 2, InfluenceSide::Out).unwrap(), None);
}
#[test]
fn literal_influence_reach_y_clamps_and_promotes_short_curves() {
    for source in [
        json!({"frame":0,"v":[1]}),
        json!({"frame":0,"v":[1],"curvePoints":null}),
        json!({"frame":0,"v":[1],"curvePoints":[]}),
        json!({"frame":0,"v":[1],"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}]}),
        json!({"frame":0,"v":[1],"curvePoints":[{"x":0,"y":0},{"x":0.5,"y":0.8},{"x":1,"y":1}]}),
    ] {
        let key: OrderKey = serde_json::from_value(source).unwrap();
        for (side, percent, x, y) in [
            (InfluenceSide::Out, -25., 0.02, 0.02),
            (InfluenceSide::Out, 50., 0.25, 0.125),
            // Baseline setSegmentInfluence (motion.js @1065e9d8): reach = 0.02 + amount*0.46,
            // out y = x*(1 - amount), so exactly 100% lands on the far margin with y = 0.
            (InfluenceSide::Out, 100., 0.48000000000000004, 0.),
            (InfluenceSide::Out, 150., 0.48000000000000004, 0.),
            (InfluenceSide::In, 0., 0.98, 0.98),
            (InfluenceSide::In, 50., 0.75, 0.875),
            (InfluenceSide::In, 100., 0.52, 1.),
        ] {
            let out = with_segment_influence(&key, side, percent).unwrap();
            let pts = authored_points(&out).unwrap();
            assert_eq!(pts.len(), 5);
            let p = &pts[if side == InfluenceSide::Out { 1 } else { 3 }];
            near(n(&p.x), x);
            near(n(&p.y), y);
            assert_eq!(out.hold, Presence::Present(false));
            assert!(p.tx.missing() && p.ty.missing());
            let untouched = &pts[if side == InfluenceSide::Out { 3 } else { 1 }];
            near(
                n(&untouched.x),
                if side == InfluenceSide::Out {
                    0.75
                } else {
                    0.25
                },
            );
            near(
                n(&untouched.y),
                if side == InfluenceSide::Out {
                    0.75
                } else {
                    0.25
                },
            );
        }
    }
}
#[test]
fn release_only_changed_waypoint_tangents_preserve_authored_zero_on_other_points() {
    let key:OrderKey=serde_json::from_value(json!({"frame":0,"v":[1],"hold":true,"hOut":[0,0],
    "curvePoints":[{"x":0,"y":0,"tx":0,"ty":0},{"x":0.25,"y":0.2,"tx":0,"ty":0},
      {"x":0.5,"y":0.5,"tx":0,"ty":0},{"x":0.75,"y":0.8,"tx":0,"ty":0},{"x":1,"y":1,"tx":0,"ty":0}]})).unwrap();
    let before = authored_points(&key).unwrap();
    for (side, changed) in [(InfluenceSide::Out, 1), (InfluenceSide::In, 3)] {
        let out = with_segment_influence(&key, side, 50.).unwrap();
        let pts = authored_points(&out).unwrap();
        for i in 0..5 {
            if i != changed {
                assert_eq!(pts[i], before[i], "untouched waypoint {i}");
            }
        }
        assert!(pts[changed].tx.missing() && pts[changed].ty.missing());
        assert_eq!(out.h_out, key.h_out);
    }
}
#[test]
fn selected_influence_edits_previous_for_in_and_skips_absent_segments() {
    let original = track();
    let (out, count) = apply_selected_influence(&original, &[1], InfluenceSide::In, 50.).unwrap();
    assert_eq!(count, 1);
    assert_eq!(out.keys[1], original.keys[1]);
    assert_eq!(out.keys[2], original.keys[2]);
    assert_eq!(out.keys[0].hold, Presence::Present(false));
    near(n(&authored_points(&out.keys[0]).unwrap()[3].y), 0.875);
    assert_eq!(
        apply_selected_influence(&original, &[0], InfluenceSide::In, 50.).unwrap(),
        (original.clone(), 0)
    );
    assert_eq!(
        apply_selected_influence(&original, &[2], InfluenceSide::Out, 50.).unwrap(),
        (original.clone(), 0)
    );
}
#[test]
fn strict_finite_inputs_bad_selection_and_obstructed_reach_fail_without_source_mutation() {
    let original = track();
    for percent in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert_eq!(
            with_segment_influence(&original.keys[0], InfluenceSide::Out, percent).unwrap_err(),
            DispatchErrorCode::InvalidRequest
        );
    }
    assert!(apply_curve_preset(&original, &[3], CurvePreset::Ease).is_err());
    assert!(apply_curve_preset(&original, &[1, 1], CurvePreset::Hold).is_err());
    let blocked: OrderKey = serde_json::from_value(json!({"frame":0,"v":[1],"curvePoints":[
    {"x":0,"y":0},{"x":0.01,"y":0.1},{"x":0.02,"y":0.9},{"x":1,"y":1}]}))
    .unwrap();
    assert_eq!(
        with_segment_influence(&blocked, InfluenceSide::Out, 100.).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(original, track());
}
#[test]
fn subdivision_plan_rounds_ties_toward_positive_infinity_and_skips_collisions() {
    for (frames, selected, want) in [
        (vec![0., 5.], vec![0, 1], vec![3.]),
        (vec![-4., -1.], vec![0, 1], vec![-2.]),
        (vec![-1., 0.], vec![0, 1], vec![]),
        (vec![0., 1.], vec![0, 1], vec![]),
        (vec![0., 2., 4.], vec![0, 2], vec![]),
        (vec![0., 4., 8.], vec![2, 0], vec![]),
        (vec![0., 1., 5.], vec![0, 2], vec![3.]),
    ] {
        let t: OrderTrack = serde_json::from_value(
            json!({"keys":frames.iter().map(|f|json!({"frame":f,"v":[1]})).collect::<Vec<_>>()}),
        )
        .unwrap();
        let plan = subdivision_plan(&t, &selected).unwrap();
        assert_eq!(plan.iter().map(|p| n(&p.frame)).collect::<Vec<_>>(), want);
    }
    let mut t = track();
    let defaults = subdivision_plan(&t, &[0, 1]).unwrap();
    assert_eq!(defaults[0].curve_points.len(), 5);
    near(n(&defaults[0].curve_points[1].y), 0.156);
    t.keys[0].curve_points = Presence::Present(Some(vec![]));
    assert!(subdivision_plan(&t, &[0, 1]).unwrap()[0]
        .curve_points
        .is_empty());
    t.keys[0].curve_points = Presence::Present(Some(
        serde_json::from_value(json!([
    {"x":0,"y":0,"tx":0,"ty":0},{"x":1,"y":1}]))
        .unwrap(),
    ));
    let plan = subdivision_plan(&t, &[1, 0]).unwrap();
    assert_eq!(plan.len(), 1);
    near(n(&plan[0].frame), 2.);
    assert_eq!(plan[0].left_index, 0);
    assert_eq!(plan[0].curve_points, authored_points(&t.keys[0]).unwrap());
    // Integration contract: Root samples this frame from the OLD snapshot, then
    // inserts copied curve + zero spatial handles. No UI effective-value input,
    // new owner or geometric subdivision is provided by this helper.
}

#[test]
fn same_preset_keeps_authored_number_storage_for_owner_no_op_detection() {
    let original: OrderTrack =
        serde_json::from_value(json!({"keys":[{"frame":0,"v":[1],"hold":false,
    "curvePoints":[{"x":0,"y":0},{"x":1,"y":1}]}]}))
        .unwrap();
    let (out, count) = apply_curve_preset(&original, &[0], CurvePreset::Linear).unwrap();
    assert_eq!(count, 1);
    assert_eq!(
        out, original,
        "reported action count is not a real-change receipt"
    );
}
