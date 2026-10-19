//! Independent F03 Position regression afterimage; AUTHORED, UNRUN.
//! Original browser oracle: opacity-consumers.spec.cjs:134-190. No candidate-generated expectations.
//! CPU path/paint coverage is not GPU pixels, installed acceptance or F03 client acceptance.
use super::super::{evaluate_vector, VectorEvalError};
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameError};
use crate::object_render_scene::prepare_object_composition;
use serde_json::{json, Value};
#[path = "render_vector_eval_motion_fixture.rs"]
mod fixture;
use fixture::{assert_paint, assert_vector, close, original_initial, pins, plan, snap, terminal};

#[test]
fn original_position_and_opacity_reach_real_geometry_scene_paint_without_source_mutation() {
    let s = snap(&terminal());
    let before = encode_storage_project(s.document()).unwrap();
    for (frame, dx, left, alpha) in [
        (0, 0., 20., 0.2),
        (10, 64., 84., 0.5),
        (20, 128., 148., 0.8),
        (0, 0., 20., 0.2),
    ] {
        assert_vector(&s, frame, dx, 0.);
        assert_paint(&s, frame, left, alpha);
        assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    }
    let mut detached = evaluate_vector(&s, 0, 10).unwrap();
    detached.segments[0].point = [-999., -999.];
    assert_vector(&s, 10, 64., 0.);
    assert_vector(&s, 0, 0., 0.);
    let stored: Value = serde_json::from_slice(&before).unwrap();
    assert_eq!(
        stored["propertyHolders"][0]["properties"][1]["static"],
        json!([25])
    );
    assert_eq!(
        stored["propertyHolders"][0]["properties"][1]["track"]["keys"][0]["v"],
        json!([20])
    );
    assert_eq!(
        stored["propertyHolders"][0]["properties"][1]["track"]["keys"][1]["v"],
        json!([80])
    );
}

#[test]
fn independent_original_initial_stage_has_unkeyed_opacity_and_still_consumes_position() {
    let v = original_initial();
    let s = snap(&v);
    let before = encode_storage_project(s.document()).unwrap();
    for (frame, dx) in [(0, 0.), (10, 64.), (20, 128.)] {
        assert_vector(&s, frame, dx, 0.);
        let p = plan(&s, frame);
        close(
            p.commands().next().unwrap().paint()[3].as_f64().unwrap(),
            1.,
        );
    }
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    let mut static_stage = original_initial();
    static_stage["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"opacity","static":[25]}));
    let s = snap(&static_stage);
    for frame in [0, 10, 20] {
        close(
            plan(&s, frame).commands().next().unwrap().paint()[3]
                .as_f64()
                .unwrap(),
            0.25,
        );
    }
    assert_eq!(
        terminal()["propertyHolders"][0]["properties"][1]["track"]["keys"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}
#[test]
fn static_position_and_disabled_expression_are_not_an_animated_or_expression_fallback() {
    let mut v = terminal();
    v["propertyHolders"][0]["properties"][0] = json!({"property":"position","static":[12,-3]});
    let s = snap(&v);
    for frame in [0, 10, 20] {
        assert_vector(&s, frame, 12., -3.);
        let _ = plan(&s, frame);
    }
    v["propertyHolders"][0]["properties"][0]["expression"] =
        json!({"enabled":false,"code":"throw 1"});
    assert_vector(&snap(&v), 10, 12., -3.);
}
#[test]
fn enabled_position_expression_other_motion_holders_and_foreign_pins_fail_closed() {
    let mut expression = terminal();
    expression["propertyHolders"][0]["properties"][0]["expression"] =
        json!({"enabled":true,"code":"[100,0]"});
    let mut rotation = terminal();
    rotation["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({"property":"rotation","static":[15]}));
    for value in [expression, rotation] {
        let s = snap(&value);
        let before = encode_storage_project(s.document()).unwrap();
        assert_eq!(
            prepare_object_exposure_frame(&s, &pins(&s, 10)).unwrap_err(),
            ObjectFrameError::Unavailable
        );
        assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    }
    let s = snap(&terminal());
    for which in 0..5 {
        let mut p = pins(&s, 10);
        match which {
            0 => p.context_id = "foreign",
            1 => p.instance_id = "foreign",
            2 => p.document_id = "foreign",
            3 => p.document_snapshot_id = "foreign",
            _ => p.frame = 21,
        };
        assert!(prepare_object_exposure_frame(&s, &p).is_err());
    }
    let p = plan(&s, 10);
    let mut wrong = pins(&s, 10);
    wrong.context_id = "foreign";
    assert!(prepare_object_composition(&p, &wrong).is_err());
}
#[test]
fn position_does_not_admit_a_nonvector_family_or_element_transform_holder() {
    let mut family = terminal();
    family["objects"][0]["family"] = json!("closed-solid-fill-cubic-path");
    family["objects"][0]
        .as_object_mut()
        .unwrap()
        .remove("vector");
    let s = snap(&family);
    assert_eq!(
        evaluate_vector(&s, 0, 10),
        Err(VectorEvalError::NotVectorFamily)
    );
    assert_eq!(
        prepare_object_exposure_frame(&s, &pins(&s, 10)).unwrap_err(),
        ObjectFrameError::Unavailable
    );
    let mut element = terminal();
    element["propertyHolders"][0]["target"]["strokeId"] = json!("r08_curve_rect");
    // Layer opacity is not converted into element opacity by this change.
    let s = snap(&element);
    assert_eq!(
        prepare_object_exposure_frame(&s, &pins(&s, 10)).unwrap_err(),
        ObjectFrameError::Unavailable
    );
}

#[cfg(feature = "history")]
#[test]
fn original_static_edit_history_then_keyed_stage_preserves_correct_set_value_semantics() {
    use crate::history::NativeObjectHistory;
    fn edit(o: &mut NativeObjectHistory, kind: &str, frame: u32, value: f64) -> u64 {
        let payload=json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"r08_curve_layer"},
            "property":"opacity","action":{"kind":kind,"frame":frame,"values":[value]}}).to_string();
        let c = o.prepare_property_edit_json(payload.as_bytes()).unwrap();
        o.commit_property_edit(c).unwrap()
    }
    fn stored(o: &NativeObjectHistory, revision: u64) -> Value {
        serde_json::from_slice(
            &encode_storage_project(o.acquire_snapshot(revision).unwrap().document()).unwrap(),
        )
        .unwrap()
    }
    let initial = original_initial();
    let mut o = NativeObjectHistory::new(
        "f03-static-stage",
        decode_storage_project(initial.to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    assert_eq!(edit(&mut o, "setValue", 0, 25.), 1);
    let saved25 = encode_storage_project(o.acquire_snapshot(1).unwrap().document()).unwrap();
    assert_eq!(edit(&mut o, "setValue", 0, 40.), 2);
    assert_eq!(edit(&mut o, "setValue", 0, 60.), 3);
    assert_eq!(o.undo(3).unwrap(), 4);
    let after = stored(&o, 4);
    assert_eq!(
        after["propertyHolders"][0]["properties"][1]["static"],
        json!([40.])
    );
    assert_eq!(
        after["propertyHolders"][0]["properties"][0],
        initial["propertyHolders"][0]["properties"][0]
    );
    assert_vector(&o.acquire_snapshot(4).unwrap(), 10, 64., 0.);
    assert_eq!(o.redo(4).unwrap(), 5);
    assert_eq!(
        stored(&o, 5)["propertyHolders"][0]["properties"][1]["static"],
        json!([60.])
    );
    // Browser re-imports staticSaved before keys; this fresh native owner is that storage-stage equivalent.
    let mut reopened =
        NativeObjectHistory::new("f03-keyed-stage", decode_storage_project(&saved25).unwrap())
            .unwrap();
    assert_eq!(edit(&mut reopened, "setKey", 0, 20.), 1);
    assert_eq!(edit(&mut reopened, "setKey", 20, 80.), 2);
    let keyed = stored(&reopened, 2);
    let opacity = &keyed["propertyHolders"][0]["properties"][1];
    assert_eq!(opacity["static"], json!([25.]));
    assert_eq!(opacity["track"]["keys"][0]["v"], json!([20.]));
    assert_eq!(opacity["track"]["keys"][1]["v"], json!([80.]));
    for (point, want) in opacity["track"]["keys"][0]["curvePoints"]
        .as_array()
        .unwrap()
        .iter()
        .zip([[0., 0.], [0.25, 0.156], [0.5, 0.5], [0.75, 0.844], [1., 1.]])
    {
        close(point["x"].as_f64().unwrap(), want[0]);
        close(point["y"].as_f64().unwrap(), want[1]);
    }
    for (f, left, a) in [(0, 20., 0.2), (10, 84., 0.5), (20, 148., 0.8)] {
        assert_paint(&reopened.acquire_snapshot(2).unwrap(), f, left, a);
    }
    // Independent parity guard: on an animated track SetValue edits the current key, NOT hidden static base25.
    assert_eq!(edit(&mut reopened, "setValue", 0, 40.), 3);
    let edited = stored(&reopened, 3);
    let opacity = &edited["propertyHolders"][0]["properties"][1];
    assert_eq!(opacity["static"], json!([25.]));
    assert_eq!(opacity["track"]["keys"][0]["v"], json!([40.]));
    assert_eq!(opacity["track"]["keys"][1]["v"], json!([80.]));
}

#[test]
fn parent_pivot_input_keeps_true_evaluated_geometry_before_layer_position() {
    let snapshot = snap(&terminal());
    for frame in [0, 10, 20, 0] {
        let rest = super::super::evaluate_vector_rest(&snapshot, 0, frame).unwrap();
        let expected = [[20., 80.], [20., 60.], [40., 60.], [40., 80.]];
        assert_eq!(rest.segments.len(), expected.len());
        for (segment, point) in rest.segments.iter().zip(expected) {
            assert_eq!(segment.point, point);
            assert_eq!(segment.handle_in, [0., 0.]);
            assert_eq!(segment.handle_out, [0., 0.]);
        }
    }
}
