//! Real retained owner -> original guest -> production frame/scene. ALL UNRUN.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{FrameScopeKind, ProjectEnvelope, ProjectMetadata};
use crate::object_render_scene::{
    ordered_entries, prepare_object_paint_plan, prepare_object_render_scene, PaintEntry,
};
use serde_json::{json, Value};
use vello::kurbo::PathEl;

fn fixture(code: &str) -> Value {
    let mut doc = crate::object_frame_packet::duplicator_sources::tests::duplicated_document(1);
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"dupOffsetPos","static":[0,0],
            "expression":{"enabled":true,"code":code}}]}]);
    doc
}
fn owner(doc: Value) -> NativeObjectHistory {
    let doc = decode_storage_project(&serde_json::to_vec(&doc).unwrap()).unwrap();
    let project = ProjectEnvelope::no_media(ProjectMetadata::new(100, 100, 24.).unwrap(), doc);
    NativeObjectHistory::from_admitted_project("evaluated-motion-frame", project).unwrap()
}
fn pins(s: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
fn add_effector(doc: &mut Value, uid: &str, target: &str, properties: Value) {
    let n = doc["layers"].as_array().unwrap().len();
    doc["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":uid,"timing":{
        "format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
        "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}));
    doc["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":uid,"naturalPosition":n,"order":{}}));
    doc["componentTables"]["duplicators"]["effectorLayers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"contextId":"scene-root","layerUid":uid,"effector":{
            "targetLayerUid":target,"falloff":"step"}}));
    doc["propertyHolders"]
        .as_array_mut()
        .unwrap()
        .push(json!({"target":{"contextId":"scene-root","layerUid":uid},"properties":properties}));
}
fn x_values(packet: &ObjectFramePacket) -> Vec<f64> {
    packet.duplicator_outputs()[0]
        .output
        .copies
        .iter()
        .map(|c| c.record.geometry.segments[0].point.x.as_f64().unwrap())
        .collect()
}

#[test]
fn real_enabled_track_self_and_project_time_values_reach_the_actual_frame_and_scene() {
    let mut doc = fixture("[self.at(0)[0] + time * comp.fps, 0]");
    doc["propertyHolders"][0]["properties"][0]["track"] = json!({"keys":[
        {"frame":0,"v":[0,0],"curvePoints":[]},{"frame":4,"v":[40,0]}]});
    let o = owner(doc);
    let s = o.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(s.document()).unwrap();
    // The raw sample at2 is20. The actual enabled expression returns2, so a
    // raw-track substitute would visibly move different copies.
    assert_eq!(
        SnapshotMotion::new(&s, "scene-root", 2.).value("L0", "dupOffsetPos"),
        Err(Unavailable::Property("dupOffsetPos"))
    );
    assert!(crate::object_frame_packet::prepare_object_exposure_frame(&s, &pins(&s, 2)).is_err());
    let mut effects = None;
    let packet =
        prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut effects).unwrap();
    assert_eq!(x_values(&packet), [20., 22., 34.]); // Original temporal/grid arithmetic.
    assert!(effects.is_none());
    let scene = prepare_object_render_scene(&packet, &pins(&s, 2)).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pins(&s, 2)).unwrap();
    let painted = ordered_entries(&plan)
        .map(|entry| match entry {
            PaintEntry::Duplicator(c) => match c.path.elements()[0] {
                PathEl::MoveTo(p) => p.x,
                _ => panic!("expected native curve"),
            },
            _ => panic!("unexpected extra source paint"),
        })
        .collect::<Vec<_>>();
    assert_eq!(painted, [20., 22., 34.]);
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
}

#[test]
fn all_five_local_fields_and_original_effector_dimensions_are_evaluated_in_production() {
    let mut doc = fixture("[frame,0]");
    add_effector(
        &mut doc,
        "E",
        "L0",
        json!([
            {"property":"position","static":[-999,-999],"expression":{"enabled":true,"code":"[frame+10.5,-3.25]"}},
            {"property":"anchor","static":[20,30],"expression":{"enabled":true,"code":"[3]"}},
            {"property":"rotation","static":[87],"expression":{"enabled":true,"code":"90"}},
            {"property":"scale","static":[200,150],"expression":{"enabled":true,"code":"100"}},
            {"property":"opacity","static":[17],"expression":{"enabled":true,"code":"[100]"}},
            {"property":"effOffPosition","static":[13,17],"expression":{"enabled":true,"code":"0"}},
            {"property":"effOffScale","static":[13,17],"expression":{"enabled":true,"code":"[0]"}},
            {"property":"effOffRotation","static":[12],"expression":{"enabled":true,"code":"[0]"}},
            {"property":"effOffPositionZ","static":[12],"expression":{"enabled":true,"code":"0"}},
            {"property":"effOffRotationX","static":[12],"expression":{"enabled":true,"code":"[0]"}},
            {"property":"effOffRotationY","static":[12],"expression":{"enabled":true,"code":"0"}},
            {"property":"effOffOpacity","static":[100],"expression":{"enabled":true,"code":"-5"}},
            {"property":"effRadius","static":[1],"expression":{"enabled":true,"code":"comp.width*1000"}},
            {"property":"effStrength","static":[0],"expression":{"enabled":true,"code":"100"}},
            {"property":"effAngle","static":[180],"expression":{"enabled":true,"code":"frame-frame"}}
        ]),
    );
    let o = owner(doc);
    let s = o.acquire_snapshot(0).unwrap();
    let mut effects = None;
    let p =
        prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut effects).unwrap();
    assert_eq!(x_values(&p), [20., 22., 34.]);
    // Original app.js:1545-1574: Step uses k/(count-1), so offset -5
    // and strength100 give factors [1, 0.975, 0.95] for clones [0, 1, 2].
    let placements = &p.duplicator_outputs()[0].output.placements;
    assert_eq!(placements.len(), 3);
    assert_eq!(
        placements.iter().map(|trace| trace.k).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    for (trace, expected) in placements.iter().zip([1., 0.975, 0.95]) {
        assert!((trace.placement.opacity_factor - expected).abs() < 1e-12);
    }
    let scene = prepare_object_render_scene(&p, &pins(&s, 2)).unwrap();
    assert_eq!(
        prepare_object_paint_plan(&scene, &pins(&s, 2))
            .unwrap()
            .packet()
            .duplicator_outputs()
            .len(),
        1
    );
    assert!(effects.is_none());
}

#[test]
fn recursive_reads_use_actual_same_snapshot_property_values_and_expressions() {
    let mut doc = fixture("[frame,0]");
    add_effector(
        &mut doc,
        "E",
        "L0",
        json!([
        {"property":"effOffOpacity","expression":{"enabled":true,"code":"-layer('C').opacity/8"}},
        {"property":"effRadius","static":[100000]}]),
    );
    // C is another original effector role, with no painter to steal its pose.
    add_effector(
        &mut doc,
        "C",
        "missing",
        json!([
        {"property":"opacity","static":[20],"expression":{"enabled":true,"code":"value*2"}}]),
    );
    let o = owner(doc);
    let s = o.acquire_snapshot(0).unwrap();
    let mut effects = None;
    let p =
        prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut effects).unwrap();
    // C's live opacity is20*2=40, so -layer('C').opacity/8 is -5.
    // Original Step weights [0, 0.5, 1] yield [1, 0.975, 0.95].
    let placements = &p.duplicator_outputs()[0].output.placements;
    assert_eq!(placements.len(), 3);
    assert_eq!(
        placements.iter().map(|trace| trace.k).collect::<Vec<_>>(),
        [0, 1, 2]
    );
    for (trace, expected) in placements.iter().zip([1., 0.975, 0.95]) {
        assert!((trace.placement.opacity_factor - expected).abs() < 1e-12);
    }
    assert!(effects.is_none());
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn unrelated_host_geometry_expressions_do_not_gain_coverage_from_the_motion_port() {
    let mut doc = fixture("[frame,0]");
    doc["propertyHolders"][0]["properties"]
        .as_array_mut()
        .unwrap()
        .push(json!({
        "property":"position","expression":{"enabled":true,"code":"[frame,0]"}}));
    let o = owner(doc);
    let s = o.acquire_snapshot(0).unwrap();
    let mut effects = None;
    assert!(
        prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut effects).is_err()
    );
    assert!(
        effects.unwrap().effects.is_empty(),
        "unjoined geometry must refuse before executing a guest"
    );
}

#[test]
fn ordinary_no_holder_and_raw_or_disabled_properties_keep_their_original_positive_paths() {
    for mode in ["no-holder", "raw", "disabled", "no-duplicator"] {
        let mut doc = fixture("[999,999]");
        if mode == "no-holder" || mode == "no-duplicator" {
            doc.as_object_mut().unwrap().remove("propertyHolders");
        } else {
            doc["propertyHolders"][0]["properties"][0]["static"] = json!([2, 0]);
            if mode == "raw" {
                doc["propertyHolders"][0]["properties"][0]
                    .as_object_mut()
                    .unwrap()
                    .remove("expression");
            } else {
                doc["propertyHolders"][0]["properties"][0]["expression"]["enabled"] = json!(false);
            }
        }
        if mode == "no-duplicator" {
            doc.as_object_mut().unwrap().remove("componentTables");
        }
        let o = owner(doc);
        let s = o.acquire_snapshot(0).unwrap();
        let mut effects = None;
        let before = encode_storage_project(s.document()).unwrap();
        let raw =
            crate::object_frame_packet::prepare_object_exposure_frame(&s, &pins(&s, 2)).unwrap();
        let evaluated =
            prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut effects)
                .unwrap();
        assert_eq!(evaluated.records().len(), raw.records().len(), "{mode}");
        if mode == "no-duplicator" {
            assert!(evaluated.duplicator_outputs().is_empty());
        } else {
            assert_eq!(x_values(&evaluated), x_values(&raw), "{mode}");
        }
        let scene = prepare_object_render_scene(&evaluated, &pins(&s, 2)).unwrap();
        prepare_object_paint_plan(&scene, &pins(&s, 2)).unwrap();
        assert!(effects.is_none());
        assert_eq!(encode_storage_project(s.document()).unwrap(), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
}

#[path = "object_expression_motion_effects.rs"]
mod effect_tests;

#[path = "object_expression_motion_vector.rs"]
mod vector_tests;

#[test]
fn actual_enabled_effector_local_motion_and_follow_path_compose_before_retained_scene_paint() {
    let mut doc = fixture("[frame,0]");
    add_effector(
        &mut doc,
        "E",
        "L0",
        json!([
            {"property":"position","static":[-999,-999],
                "expression":{"enabled":true,"code":"[frame-2,0]"}},
            {"property":"pathPercent","static":[100]},
            {"property":"effRadius","static":[1]},
            {"property":"effOffPosition","static":[0,40]}
        ]),
    );
    doc["layers"][1]["followPath"] = json!({"targetLayerUid":"G"});
    doc["componentTables"]["duplicators"]["effectorLayers"][0]["effector"]["pos"] = json!([0, 0]);
    doc["componentTables"]["duplicators"]["effectorLayers"][0]["effector"]["falloff"] =
        json!("linear");
    doc["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"G","visible":false,
        "timing":{"format":"nemo.native-layer-timing","formatVersion":1,
            "family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}));
    doc["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"G","naturalPosition":2,"order":{}}));
    let mut guide = doc["objects"][0].clone();
    guide["family"] = json!("editable-vector-path");
    guide["target"]["layerUid"] = json!("G");
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
        .push(json!({"layerUid":"G","strokeId":"guide","naturalPosition":0,"order":{}}));
    let admitted = owner(doc.clone());
    let original = admitted.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(original.document()).unwrap();
    let o = owner(serde_json::from_slice(&before).unwrap());
    let s = o.acquire_snapshot(0).unwrap();
    assert!(crate::object_frame_packet::prepare_object_exposure_frame(&s, &pins(&s, 2)).is_err());
    let mut effects = None;
    let packet =
        prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 2), &mut effects).unwrap();
    // Pinned motion.js:3527-3548 adds the original D30 endpoint [100,0]
    // (property-follow-path-sample/oracle.txt:136) to evaluated local [0,0].
    // Original linear falloff at angle0/radius1 clamps1-(clone.x-100) to1;
    // each positive clone centre receives +40y. Omitting FollowPath or
    // substituting raw local [-999,-999] would leave y20.
    let placed = packet.duplicator_outputs()[0]
        .output
        .copies
        .iter()
        .map(|copy| {
            let p = &copy.record.geometry.segments[0].point;
            [p.x.as_f64().unwrap(), p.y.as_f64().unwrap()]
        })
        .collect::<Vec<_>>();
    assert_eq!(placed, [[20., 60.], [22., 60.], [34., 60.]]);
    let scene = prepare_object_render_scene(&packet, &pins(&s, 2)).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pins(&s, 2)).unwrap();
    let painted = ordered_entries(&plan)
        .map(|entry| match entry {
            PaintEntry::Duplicator(copy) => match copy.path.elements()[0] {
                PathEl::MoveTo(p) => [p.x, p.y],
                _ => panic!("expected native curve"),
            },
            _ => panic!("unexpected source or guide paint"),
        })
        .collect::<Vec<_>>();
    assert_eq!(painted, [[20., 60.], [22., 60.], [34., 60.]]);
    assert!(effects.is_none());
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    assert_eq!(
        (
            s.document().objects().len(),
            o.content_revision(),
            o.history_depths()
        ),
        (4, 0, (0, 0))
    );
    doc["layers"][1]["followPath"] = Value::Null;
    let control = owner(doc);
    let cs = control.acquire_snapshot(0).unwrap();
    let mut ce = None;
    let cp = prepare_object_motion_exposure_frame_for_owner(&control, &cs, &pins(&cs, 2), &mut ce)
        .unwrap();
    for copy in &cp.duplicator_outputs()[0].output.copies {
        assert_eq!(
            copy.record.geometry.segments[0].point.y.as_f64().unwrap(),
            20.
        );
    }
    assert!(ce.is_none());
}

#[path = "object_expression_motion_commit.rs"]
mod commit_tests;
