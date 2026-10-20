//! Original layerMotionAt FollowPath arithmetic through retained native inputs.
use crate::evaluation::motion_parent_chain::MotionMat;
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::object_duplicator_evaluate::snapshot::SnapshotMotion;
use crate::evaluation::object_component_kernels::object_effector_layers::MotionPort;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_frame_packet::duplicators::tests::{fixture, selector, snapshot};
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameError};
use serde_json::{json, Value};

fn document() -> Value {
    let mut doc = fixture(0);
    for record in doc["objects"].as_array_mut().unwrap() {
        record["geometry"]["segments"] = json!([[-1, -1], [1, -1], [1, 1], [-1, 1]]
            .map(|[x, y]| json!({"point":{"x":x,"y":y},
                "handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    }
    for (index, uid, visible) in [(1, "E0", true), (2, "P0", false)] {
        doc["layers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":uid,"visible":visible,
            "timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
                "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}));
        doc["compositionOrder"]["contexts"][0]["layers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":uid,"naturalPosition":index,"order":{}}));
    }
    doc["layers"][1]["followPath"] = json!({"targetLayerUid":"P0"});
    let mut guide = doc["objects"][0].clone();
    guide["family"] = json!("editable-vector-path");
    guide["target"]["layerUid"] = json!("P0");
    guide["target"]["strokeId"] = json!("guide");
    guide["geometry"]["closed"] = json!(false);
    guide["geometry"]["segments"] = json!([[0, 0], [100, 0]].map(|[x, y]| json!({
        "point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    guide["fill"] = Value::Null;
    guide["vector"] = json!({"stroke":{"kind":"solid","r":0,"g":0,"b":0,"a":1},
        "hasRealStroke":true,"strokeWidth":1,"strokeCap":"round","strokeJoin":"round","opacity":1});
    doc["objects"].as_array_mut().unwrap().push(guide);
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"P0","strokeId":"guide","naturalPosition":0,"order":{}}));
    doc["componentTables"]["duplicators"]["effectorLayers"] = json!([{
        "contextId":"scene-root","layerUid":"E0","effector":{
            "targetLayerUid":"L0","pos":[0,0],"falloff":"radial"}}]);
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"E0"},
        "properties":[{"property":"pathPercent","static":[100]},
            {"property":"effRadius","static":[100]},
            {"property":"effOffPosition","static":[0,40]}]}]);
    doc
}

fn delta(snap: &crate::revision::ObjectSnapshot) -> [f64; 2] {
    let value = SnapshotMotion::new(snap, "scene-root", 2.)
        .motion_delta("E0")
        .unwrap()
        .unwrap();
    [value.dx.operand(), value.dy.operand()]
}

#[test]
fn original_follow_path_endpoint_moves_the_real_retained_effector_and_frame_copies() {
    let admitted = snapshot(document());
    let before = encode_storage_project(admitted.document()).unwrap();
    let snap = snapshot(serde_json::from_slice(&before).unwrap());
    // Original Paper selector oracle row136: this open [0,0] -> [100,0]
    // guide at100% returns [100,0,0]. An empty native effector group has
    // centre[0,0], so original layerMotionAt adds delta[100,0].
    let oracle = include_str!("fixtures/property-follow-path-sample/oracle.txt");
    assert!(oracle.lines().any(|line| line == "sel t 0,0,0,0,0,0;4059000000000000,0,0,0,0,0@0 4059000000000000 4059000000000000 4059000000000000 4059000000000000,0,0"));
    assert_eq!(delta(&snap), [100., 0.]);
    let frame = prepare_object_exposure_frame(&snap, &selector(&snap, 2)).unwrap();
    assert_eq!(frame.records().len(), 1);
    assert_eq!(frame.duplicator_outputs().len(), 1);
    let output = &frame.duplicator_outputs()[0];
    assert_eq!(output.source_indices, [1, 1, 1]);
    assert_eq!(output.output.copies.len(), 3);
    // Original radial weight1-distance/100 at clone centres0,10,20 from
    // effector origin100 is0,.1,.2. Offset[0,40] adds0,4,8 to first y=-1.
    // Literal geometry expectations are derived from pinned original source,
    // not from SnapshotMotion, the native sampler or its placement result.
    for (copy, expected) in output
        .output
        .copies
        .iter()
        .zip([[-1., -1.], [9., 3.], [19., 7.]])
    {
        let p = &copy.record.geometry.segments[0].point;
        for (got, want) in [p.x.as_f64().unwrap(), p.y.as_f64().unwrap()]
            .into_iter()
            .zip(expected)
        {
            assert!((got - want).abs() < 1e-12, "{got} vs original {want}");
        }
    }
    assert_eq!(snap.document().objects().len(), 4);
    assert_eq!(encode_storage_project(snap.document()).unwrap(), before);
    assert_eq!(snap.content_revision(), 0);
    let mut without_path = document();
    without_path["layers"][1]["followPath"] = Value::Null;
    let control = snapshot(without_path);
    let control_frame = prepare_object_exposure_frame(&control, &selector(&control, 2)).unwrap();
    let first = &control_frame.duplicator_outputs()[0].output.copies[0]
        .record
        .geometry
        .segments[0]
        .point;
    assert_eq!(first.y.as_f64().unwrap(), 39.); // Origin0 gives full +40, versus path origin100.
}

#[test]
fn the_shared_post_local_hook_adds_follow_path_without_replacing_evaluated_position_or_anchor() {
    let mut doc = document();
    doc["propertyHolders"][0]["properties"].as_array_mut().unwrap().push(json!({
        "property":"position","static":[999,999],"expression":{"enabled":true,"code":"[12.5,-3.25]"}}));
    let snap = snapshot(doc);
    let before = encode_storage_project(snap.document()).unwrap();
    let motion = SnapshotMotion::new(&snap, "scene-root", 2.);
    assert!(motion.motion_delta("E0").is_err()); // Raw tracks never replace an expression.
                                                 // The application expression adapter owns producing this pose. This test
                                                 // exercises only the shared native composition seam, not the interpreter.
    let local = MotionMat {
        dx: 12.5,
        dy: -3.25,
        rot: 0.,
        sx: 1.,
        sy: 1.,
        op: 1.,
        ax: 9.,
        ay: 17.,
    };
    let got = motion
        .motion_delta_from_local("E0", Some(local))
        .unwrap()
        .unwrap();
    // Original additive formula: local + sampled point - own bounds - anchor.
    assert_eq!(
        [got.dx, got.dy],
        [JsNumber::Value(103.5), JsNumber::Value(-20.25)]
    );
    assert_eq!(encode_storage_project(snap.document()).unwrap(), before);
}

#[test]
fn consumed_follow_path_and_guide_motion_expressions_keep_the_strict_raw_refusal() {
    for key in ["pathPercent", "pathInfluence"] {
        let mut doc = document();
        doc["layers"][1]["followPath"]["align"] = json!(true);
        doc["propertyHolders"][0]["properties"]
            .as_array_mut()
            .unwrap()
            .push(json!({
            "property":key,"static":[50],"expression":{"enabled":true,"code":"75"}}));
        // Replace rather than duplicate the existing pathPercent row.
        if key == "pathPercent" {
            doc["propertyHolders"][0]["properties"]
                .as_array_mut()
                .unwrap()
                .remove(0);
        }
        let snap = snapshot(doc);
        assert!(SnapshotMotion::new(&snap, "scene-root", 2.)
            .motion_delta("E0")
            .is_err());
    }
    let mut doc = document();
    doc["propertyHolders"].as_array_mut().unwrap().push(json!({
        "target":{"contextId":"scene-root","layerUid":"P0"},"properties":[{
            "property":"position","static":[0,0],"expression":{"enabled":true,"code":"[10,20]"}}]}));
    let snap = snapshot(doc);
    assert!(SnapshotMotion::new(&snap, "scene-root", 2.)
        .motion_delta("E0")
        .is_err());
}

#[test]
fn unconsumed_ordinary_follow_path_and_guide_parent_transforms_are_explicitly_unavailable() {
    let mut doc = document();
    doc["layers"][0]["followPath"] = json!({"targetLayerUid":"P0","align":false});
    let snap = snapshot(doc);
    assert!(matches!(
        prepare_object_exposure_frame(&snap, &selector(&snap, 2)),
        Err(ObjectFrameError::Unavailable)
    ));
    let mut doc = document();
    doc["layers"][2]["parentLayerUid"] = json!("L0");
    let snap = snapshot(doc);
    assert!(SnapshotMotion::new(&snap, "scene-root", 2.)
        .motion_delta("E0")
        .is_err());
}

#[test]
fn original_follow_path_wire_presence_defaults_and_unrelated_strict_fields_are_preserved() {
    for stored in [
        None,
        Some(Value::Null),
        Some(json!({"targetLayerUid":"P0"})),
        Some(json!({"targetLayerUid":"P0","align":false})),
        Some(json!({"targetLayerUid":"P0","align":true})),
    ] {
        let mut doc = document();
        doc["layers"][1]
            .as_object_mut()
            .unwrap()
            .remove("followPath");
        if let Some(value) = stored.clone() {
            doc["layers"][1]["followPath"] = value;
        }
        let decoded = decode_storage_project(&serde_json::to_vec(&doc).unwrap()).unwrap();
        let output: Value =
            serde_json::from_slice(&encode_storage_project(&decoded).unwrap()).unwrap();
        assert_eq!(output["layers"][1].get("followPath"), stored.as_ref());
        let snap = snapshot(output);
        if stored.as_ref().is_some_and(|v| !v.is_null()) {
            assert_eq!(delta(&snap), [100., 0.]);
        } else {
            assert_eq!(
                SnapshotMotion::new(&snap, "scene-root", 2.).motion_delta("E0"),
                Ok(None)
            );
        }
    }
    for bad in [
        json!([]),
        json!({"targetLayerUid":7}),
        json!({"targetLayerUid":"P0","align":null}),
        json!({"targetLayerUid":"P0","align":1}),
        json!({"targetLayerUid":"P0","unknown":1}),
        json!({"targetLayerUid":"x".repeat(256)}),
    ] {
        let mut doc = document();
        doc["layers"][1]["followPath"] = bad;
        assert!(decode_storage_project(&serde_json::to_vec(&doc).unwrap()).is_err());
    }
    let mut doc = document();
    doc["layers"][1]["unrelatedUnknown"] = json!(true);
    assert!(decode_storage_project(&serde_json::to_vec(&doc).unwrap()).is_err());
}

#[test]
fn guide_plain_motion_is_composed_but_missing_guides_and_unconsumed_align_inputs_stay_distinct() {
    let mut doc = document();
    doc["propertyHolders"].as_array_mut().unwrap().push(json!({
        "target":{"contextId":"scene-root","layerUid":"P0"},"properties":[
            {"property":"position","static":[10,-5]}]}));
    let snap = snapshot(doc);
    let before = encode_storage_project(snap.document()).unwrap();
    assert_eq!(delta(&snap), [110., -5.]); // Original target plain translation on [100,0].
    assert_eq!(encode_storage_project(snap.document()).unwrap(), before);
    let mut doc = document();
    doc["layers"][1]["followPath"]["targetLayerUid"] = json!("missing");
    let snap = snapshot(doc);
    assert_eq!(
        SnapshotMotion::new(&snap, "scene-root", 2.).motion_delta("E0"),
        Ok(None)
    );
    let mut doc = document();
    doc["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({
        "property":"pathInfluence","static":[50],"expression":{"enabled":true,"code":"75"}}));
    let snap = snapshot(doc);
    assert_eq!(delta(&snap), [100., 0.]); // Absent align=false never consumes pathInfluence.
}

#[test]
fn a_role_tag_cannot_admit_unconsumed_follow_path_on_a_retained_content_layer() {
    let mut doc = document();
    let mut content = doc["objects"][1].clone();
    content["target"]["layerUid"] = json!("E0");
    content["target"]["strokeId"] = json!("mixed");
    doc["objects"].as_array_mut().unwrap().push(content);
    doc["compositionOrder"]["contexts"][0]["scopes"][1]["elements"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"E0","strokeId":"mixed","naturalPosition":0,"order":{}}));
    let snap = snapshot(doc);
    let before = encode_storage_project(snap.document()).unwrap();
    assert!(matches!(
        prepare_object_exposure_frame(&snap, &selector(&snap, 2)),
        Err(ObjectFrameError::Unavailable)
    ));
    assert_eq!(encode_storage_project(snap.document()).unwrap(), before);
    assert_eq!(snap.content_revision(), 0);
}
