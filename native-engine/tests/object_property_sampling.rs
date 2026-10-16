//! Independent literal baseline oracles through actual admitted snapshots.
//! Intended as a cfg(test) crate child with history+evaluation enabled.
//! Reads the same immutable snapshot facet consumed by native edit preparation.
//! Literal output numbers are frozen from shipped JS,
//! not generated from the new Rust sampler or an independent reimplementation.
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{FrameScopeKind, PropertyHolderTarget};
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameError, ObjectFrameSelector};
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn sample_property_raw(
    snapshot: &ObjectSnapshot,
    target: &PropertyHolderTarget,
    property: &str,
    frame: f64,
) -> Result<Vec<f64>, DispatchErrorCode> {
    snapshot.sample_property_raw(target, property, frame)
}

fn target(element: bool) -> PropertyHolderTarget {
    serde_json::from_value(if element {
        json!({"contextId":"scene-root","layerUid":"L0","strokeId":"ink"})
    } else {
        json!({"contextId":"scene-root","layerUid":"L0"})
    })
    .unwrap()
}
fn source(properties: Value, element: bool) -> Value {
    // Full authored object JSON, not a synthetic PropertyRecord or mock snapshot.
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":16,
      "layers":[{"layerUid":"L0"}],
      "objects":[{"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":{"contextId":"scene-root","layerUid":"L0","strokeId":"ink",
          "frameScope":{"kind":"authored","frame":0}},
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative",
          "segments":[
            {"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
            {"point":{"x":10,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}},
            {"point":{"x":0,"y":10},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}}]},
        "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":0.75}}],
      "propertyHolders":[{"target":target(element),"properties":properties}]})
}
fn owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "d11-independent-oracles",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn close(actual: Vec<f64>, expected: &[f64]) {
    assert_eq!(actual.len(), expected.len());
    for (a, b) in actual.iter().zip(expected) {
        assert!(
            a.is_finite() && (a - b).abs() < 1e-9,
            "{actual:?} != {expected:?}"
        );
    }
}
fn sampled(snapshot: &ObjectSnapshot, element: bool, property: &str, frame: f64, expected: &[f64]) {
    close(
        sample_property_raw(snapshot, &target(element), property, frame).unwrap(),
        expected,
    );
}

#[test]
fn static_override_and_neutral_defaults_are_detached_arrays() {
    for element in [false, true] {
        let o = owner(&source(
            json!([{"property":"opacity","static":[37.5]}]),
            element,
        ));
        let s = o.acquire_snapshot(0).unwrap();
        sampled(&s, element, "opacity", -25.5, &[37.5]);
        sampled(&s, element, "position", 0., &[0., 0.]);
        sampled(&s, element, "scale", 2., &[100., 100.]);
        sampled(&s, element, "rotation", 9., &[0.]);
        sampled(&s, element, "volume", 9., &[1.]);
        let mut returned = sample_property_raw(&s, &target(element), "opacity", 0.).unwrap();
        returned[0] = 999.;
        sampled(&s, element, "opacity", 0., &[37.5]);
    }
}

#[test]
fn empty_track_uses_static_override_or_real_builtin_default() {
    for (record, expected) in [
        (
            json!({"property":"opacity","static":[33],"track":{"keys":[]}}),
            33.,
        ),
        (json!({"property":"opacity","track":{"keys":[]}}), 100.),
    ] {
        let o = owner(&source(json!([record]), false));
        let s = o.acquire_snapshot(0).unwrap();
        for f in [-1.5, 0., 99.] {
            sampled(&s, false, "opacity", f, &[expected]);
        }
    }
}

#[test]
fn valid_control_defaults_come_from_the_admitted_holder() {
    let mut value = source(json!([]), false);
    value["propertyHolders"][0]["controls"] = json!([
      {"key":"xc_point","name":"Point","type":"point","default":[12,34]},
      {"key":"xc_color","name":"Color","type":"color"}]);
    let o = owner(&value);
    let s = o.acquire_snapshot(0).unwrap();
    sampled(&s, false, "xc_point", 0.25, &[12., 34.]);
    sampled(&s, false, "xc_color", 0.25, &[255., 255., 255., 255.]);
}

#[test]
fn linear_negative_fractional_authored_times_clamp_to_authored_endpoints() {
    let o = owner(&source(
        json!([{"property":"opacity","static":[99],"track":{"keys":[
      {"frame":-2,"v":[10],"curvePoints":[]},{"frame":2,"v":[90]}]}}]),
        false,
    ));
    let s = o.acquire_snapshot(0).unwrap();
    for (f, v) in [
        (-20., 10.),
        (-2., 10.),
        (-1.5, 20.),
        (0., 50.),
        (1.5, 80.),
        (2., 90.),
        (200., 90.),
    ] {
        sampled(&s, false, "opacity", f, &[v]);
    }
}

#[test]
fn exact_middle_key_belongs_to_that_key_not_the_previous_left_hold() {
    let o = owner(&source(
        json!([{"property":"opacity","track":{"keys":[
      {"frame":-2,"v":[10],"hold":true},
      {"frame":2,"v":[70],"curvePoints":[]},{"frame":5,"v":[100]}]}}]),
        false,
    ));
    let s = o.acquire_snapshot(0).unwrap();
    for (f, v) in [
        (-2., 10.),
        (-0.5, 10.),
        (1.999, 10.),
        (2., 70.),
        (3.5, 85.),
        (5., 100.),
    ] {
        sampled(&s, false, "opacity", f, &[v]);
    }
}

#[test]
fn absent_and_null_curves_use_frozen_default_but_empty_curve_is_linear() {
    // Characterized by actual motion.js rawValueAtFrame + animation/curve.js.
    for curve in [None, Some(Value::Null), Some(json!([]))] {
        let mut first = json!({"frame":0,"v":[0]});
        if let Some(points) = curve.clone() {
            first["curvePoints"] = points;
        }
        let o = owner(&source(
            json!([{"property":"opacity","track":{"keys":[first,
          {"frame":8,"v":[100]}]}}]),
            false,
        ));
        let s = o.acquire_snapshot(0).unwrap();
        if curve == Some(json!([])) {
            for (f, v) in [(1., 12.5), (2., 25.), (3., 37.5), (6., 75.)] {
                sampled(&s, false, "opacity", f, &[v]);
            }
        } else {
            for (f, v) in [
                (1., 6.511604608298387),
                (2., 15.6),
                (3., 31.625000000000004),
                (6., 84.39999999999999),
            ] {
                sampled(&s, false, "opacity", f, &[v]);
            }
        }
    }
}

#[test]
fn only_position_uses_spatial_handles_after_temporal_easing() {
    let o = owner(&source(
        json!([
          {"property":"position","track":{"keys":[
            {"frame":0,"v":[0,0],"curvePoints":[],"hOut":[0,8]},
            {"frame":8,"v":[8,0],"hIn":[0,8]}]}},
          {"property":"scale","track":{"keys":[
            {"frame":0,"v":[0,0],"curvePoints":[],"hOut":[0,800]},
            {"frame":8,"v":[8,0],"hIn":[0,800]}]}},
          {"property":"opacity","track":{"keys":[
            {"frame":0,"v":[0],"curvePoints":[],"hOut":[0,800]},
            {"frame":8,"v":[100],"hIn":[0,800]}]}}
        ]),
        true,
    ));
    let s = o.acquire_snapshot(0).unwrap();
    for (f, v) in [(2., [1.25, 4.5]), (4., [4., 6.]), (6., [6.75, 4.5])] {
        sampled(&s, true, "position", f, &v);
    }
    sampled(&s, true, "scale", 2., &[2., 0.]);
    sampled(&s, true, "opacity", 2., &[25.]);
    // Same path, but omitted curve: easing must precede spatial interpolation.
    let eased = owner(&source(
        json!([{"property":"position","track":{"keys":[
      {"frame":0,"v":[0,0],"hOut":[0,8]},
      {"frame":8,"v":[8,0],"hIn":[0,8]}]}}]),
        true,
    ));
    sampled(
        &eased.acquire_snapshot(0).unwrap(),
        true,
        "position",
        2.,
        &[0.523321344, 3.1599359999999996],
    );
}

#[test]
fn active_expression_is_deliberately_ignored_by_raw_sampling_and_final_frame_stays_closed() {
    let o = owner(&source(
        json!([{"property":"opacity","static":[33],
      "expression":{"enabled":true,"code":"throw new Error('RAW MUST NOT EXECUTE')"}}]),
        false,
    ));
    let s = o.acquire_snapshot(0).unwrap();
    sampled(&s, false, "opacity", 4.5, &[33.]);
    let selector = ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    assert_eq!(
        prepare_object_frame(&s, &selector).unwrap_err(),
        ObjectFrameError::Unavailable
    );
}

#[test]
fn inherited_paint_without_a_bound_raw_value_is_unavailable_not_invented_zero() {
    let o = owner(&source(json!([]), false));
    let s = o.acquire_snapshot(0).unwrap();
    // No implicit source-paint resolver is admitted by this raw sampler API.
    for property in ["fillColor", "strokeColor", "strokeWidth", "brushSize"] {
        assert_eq!(
            sample_property_raw(&s, &target(false), property, 0.).unwrap_err(),
            DispatchErrorCode::Unavailable
        );
    }
    let absent: PropertyHolderTarget = serde_json::from_value(json!({"contextId":"scene-root",
      "layerUid":"L0","strokeId":"not-authored"}))
    .unwrap();
    assert!(
        sample_property_raw(&s, &absent, "opacity", 0.).is_err(),
        "unbound target cannot synthesize a neutral value"
    );
}

#[test]
fn sampling_cannot_mutate_snapshot_history_revision_or_serialized_authored_storage() {
    let original = source(
        json!([{"property":"opacity","static":[12],"track":{"keys":[
      {"frame":0,"v":[20],"curvePoints":[]},{"frame":8,"v":[80]}]}}]),
        false,
    );
    let o = owner(&original);
    let s = o.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(s.document()).unwrap();
    let identity = (s.document_id().to_owned(), s.snapshot_id().to_owned());
    for f in [-10., 0., 0.25, 2., 8., 100.] {
        let _ = sample_property_raw(&s, &target(false), "opacity", f).unwrap();
    }
    assert_eq!(o.content_revision(), 0);
    assert_eq!(o.history_depths(), (0, 0));
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    let again = o.acquire_snapshot(0).unwrap();
    assert_eq!(
        (again.document_id(), again.snapshot_id()),
        (identity.0.as_str(), identity.1.as_str())
    );
    assert_eq!(encode_storage_project(again.document()).unwrap(), before);
}
