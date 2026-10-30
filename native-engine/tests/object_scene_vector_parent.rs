//! Actual retained ordinary path parent chains; original rest fixtures and literal baseline arithmetic.
use super::{drawing, owner, packet, pins, plan, starts, svg};
use crate::history::NativeObjectHistory;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{Presence, PropertyHolderTarget};
use crate::object_expression_runtime::prepare_object_motion_exposure_frame_for_owner;
use crate::object_frame_packet::prepare_object_exposure_frame;
use crate::object_render_scene::prepare_object_render_scene;
use serde_json::{json, Value};

fn near(actual: f64, expected: f64) {
    assert!((actual - expected).abs() <= 1e-10, "{actual} != {expected}");
}
fn add_parent(source: &mut Value, uid: &str, parent: Option<&str>, offset: f64, properties: Value) {
    let mut layer = source["layers"][0].clone();
    layer["layerUid"] = json!(uid);
    layer["visible"] = json!(false);
    layer.as_object_mut().unwrap().remove("parentLayerUid");
    if let Some(parent) = parent {
        layer["parentLayerUid"] = json!(parent)
    }
    let natural = source["layers"].as_array().unwrap().len();
    source["layers"].as_array_mut().unwrap().push(layer);
    source["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":uid,"naturalPosition":natural,"order":{}}));
    for index in 0..2 {
        let mut record = source["objects"][index].clone();
        record["target"]["layerUid"] = json!(uid);
        for s in record["geometry"]["segments"].as_array_mut().unwrap() {
            s["point"]["x"] = json!(s["point"]["x"].as_f64().unwrap() + offset);
        }
        let stroke = record["target"]["strokeId"].clone();
        source["objects"].as_array_mut().unwrap().push(record);
        source["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
            .as_array_mut()
            .unwrap()
            .push(json!({"layerUid":uid,"strokeId":stroke,"naturalPosition":index,"order":{}}));
    }
    if !properties.is_null() {
        source["propertyHolders"]
            .as_array_mut()
            .unwrap()
            .push(json!({
            "target":{"contextId":"scene-root","layerUid":uid},"properties":properties}));
    }
}
fn parented(legacy: bool) -> Value {
    let mut source = drawing(legacy);
    add_parent(
        &mut source,
        "P",
        Some("G"),
        400.,
        json!([
        {"property":"position","static":[-999,-999],"expression":{"enabled":true,"code":"[frame*2,10]"}},
        {"property":"anchor","static":[20,-10]},{"property":"rotation","static":[90]},
        {"property":"opacity","static":[50]}]),
    );
    add_parent(
        &mut source,
        "G",
        None,
        800.,
        json!([
        {"property":"position","static":[-3,4]},{"property":"anchor","static":[-50,0]},
        {"property":"scale","static":[200,100]},{"property":"opacity","static":[50]}]),
    );
    source["layers"][0]["parentLayerUid"] = json!("P");
    source
}

#[test]
fn actual_hidden_ancestors_use_their_own_union_pivots_and_original_transform_opacity_order() {
    for legacy in [false, true] {
        let o = owner(&parented(legacy));
        let s = o.acquire_snapshot(0).unwrap();
        let before = encode_storage_project(s.document()).unwrap();
        assert!(
            prepare_object_exposure_frame(&s, &pins(&s, 0)).is_err(),
            "raw route keeps enabled parent expressions strict"
        );
        for (frame, x, y) in [(0, 263., -691.), (2, 271., -689.), (0, 263., -691.)] {
            let p = packet(&o, &s, frame);
            let plan = plan(&p);
            assert_eq!(p.records().len(), 2);
            assert_eq!(plan.commands().len(), 2);
            let values = starts(&plan);
            for (index, (_, point)) in values.iter().enumerate() {
                near(point[0], x);
                near(point[1], y + index as f64 * 400.);
            }
            // Own pivot[160,30]; P pivot[570,20], then G pivot[900,30].
            // Own opacity .5, immediate P .5, outer G .5; no repeated own factor.
            for c in plan.commands() {
                assert_eq!(
                    c.paint()[3].as_f64(),
                    Some(if legacy { 0.09375 } else { 0.125 })
                );
                assert_eq!(
                    c.source().record().geometry.segments[0].point.y.as_f64(),
                    Some(0.)
                );
            }
            let target = PropertyHolderTarget {
                context_id: "scene-root".into(),
                layer_uid: "P".into(),
                stroke_id: Presence::Missing,
            };
            assert_eq!(
                p.sampled_motion_values(&target).unwrap().position,
                [f64::from(frame) * 2., 10.]
            );
        }
        assert_eq!(encode_storage_project(s.document()).unwrap(), before);
        assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    }
}

#[test]
fn actual_legacy_svg_reads_parented_plan_paths_and_paint_from_the_same_packet() {
    let o = owner(&parented(true));
    let s = o.acquire_snapshot(0).unwrap();
    let p = packet(&o, &s, 2);
    let plan = plan(&p);
    let text = svg(&plan).unwrap();
    let points = text
        .split("<path d=\"")
        .skip(1)
        .map(|s| {
            s.strip_prefix('M')
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .split(',')
                .map(|n| n.parse::<f64>().unwrap())
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_eq!(points.len(), 2);
    for (point, y) in points.iter().zip([-689., -289.]) {
        near(point[0], 271.);
        near(point[1], y);
    }
    assert_eq!(text.matches("fill-opacity=\"0.09375\"").count(), 2);
    assert!(!text.contains("M0,0"));
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn real_parent_pivot_uses_original_cubic_extremum_not_control_hull() {
    let mut source = drawing(false);
    source["propertyHolders"] = json!([]);
    add_parent(
        &mut source,
        "P",
        None,
        0.,
        json!([
        {"property":"anchor","static":[10,5]},{"property":"rotation","static":[180]}]),
    );
    source["layers"][0]["parentLayerUid"] = json!("P");
    source["objects"].as_array_mut().unwrap().pop();
    source["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .pop();
    // Reuse the exact numerical cubic from the existing projection control:
    // y=300t(1-t), maximum75 despite absolute handles at100.
    source["objects"][2]["geometry"]["segments"] = json!([
        {"point":{"x":0,"y":0},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":100}},
        {"point":{"x":100,"y":0},"handleIn":{"x":0,"y":100},"handleOut":{"x":0,"y":0}}]);
    let o = owner(&source);
    let s = o.acquire_snapshot(0).unwrap();
    let p = packet(&o, &s, 0);
    let plan = plan(&p);
    let points = starts(&plan);
    near(points[0].1[0], 120.);
    near(points[0].1[1], 85.);
    near(points[1].1[0], -80.);
    near(points[1].1[1], 85.);
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn missing_cycle_foreign_owner_stale_frame_and_overdepth_never_publish_parented_paint() {
    for fault in ["missing", "cycle"] {
        let mut source = parented(true);
        if fault == "missing" {
            source["layers"][0]["parentLayerUid"] = json!("absent")
        } else {
            source["layers"][2]["parentLayerUid"] = json!("L1")
        }
        let bytes = serde_json::to_vec(&source).unwrap();
        assert!(decode_storage_project(&bytes).is_err(), "{fault}");
        assert_eq!(serde_json::to_vec(&source).unwrap(), bytes);
    }
    let o = owner(&parented(true));
    let s = o.acquire_snapshot(0).unwrap();
    let p = packet(&o, &s, 0);
    let before = encode_storage_project(s.document()).unwrap();
    assert!(prepare_object_render_scene(&p, &pins(&s, 2)).is_err());
    let mut foreign_source = parented(true);
    foreign_source["propertyHolders"][0]["properties"][0]["static"] = json!([1, 2]);
    let foreign = owner(&foreign_source);
    let other = foreign.acquire_snapshot(0).unwrap();
    let mut effects = None;
    assert!(prepare_object_motion_exposure_frame_for_owner(
        &o,
        &other,
        &pins(&other, 0),
        &mut effects
    )
    .is_err());
    assert!(effects.is_none());
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    assert_eq!(
        (o.history_depths(), foreign.history_depths()),
        ((0, 0), (0, 0))
    );
    let mut deep = drawing(true);
    for index in 0..65 {
        add_parent(
            &mut deep,
            &format!("D{index}"),
            (index < 64).then(|| format!("D{}", index + 1)).as_deref(),
            0.,
            Value::Null,
        );
    }
    deep["layers"][0]["parentLayerUid"] = json!("D0");
    let o = owner(&deep);
    let s = o.acquire_snapshot(0).unwrap();
    let mut effects = None;
    let before = encode_storage_project(s.document()).unwrap();
    let refusal =
        prepare_object_motion_exposure_frame_for_owner(&o, &s, &pins(&s, 0), &mut effects)
            .unwrap_err();
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    assert_eq!(o.history_depths(), (0, 0));
    // C02 retains a verified owner's admission failure, even before any guest effects.
    assert_eq!(
        refusal,
        "evaluated Motion frame refused: Pin(Unavailable); scene-root/0; effects=[]"
    );
    assert_eq!(
        effects
            .as_ref()
            .expect("retain the depth admission refusal")
            .to_string(),
        refusal
    );
    // Exact documented D30 boundary:64 ancestors remain eligible,65 cannot be admitted.
    deep["layers"].as_array_mut().unwrap().pop();
    deep["layers"][64]
        .as_object_mut()
        .unwrap()
        .remove("parentLayerUid");
    for _ in 0..2 {
        deep["objects"].as_array_mut().unwrap().pop();
        deep["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
            .as_array_mut()
            .unwrap()
            .pop();
    }
    deep["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .pop();
    let owner64 = owner(&deep);
    let snapshot64 = owner64.acquire_snapshot(0).unwrap();
    let target = PropertyHolderTarget {
        context_id: "scene-root".into(),
        layer_uid: "L1".into(),
        stroke_id: Presence::Missing,
    };
    assert!(
        crate::evaluation::object_vector_kernels::render_vector_eval::common_layer_motion_target(
            snapshot64.document(),
            &target
        )
    );
}

#[test]
fn missing_held_parent_geometry_stays_explicit() {
    let mut source = parented(true);
    source["layers"][1]["timing"]["keys"] = json!([]);
    let o = owner(&source);
    let s = o.acquire_snapshot(0).unwrap();
    // An enabled expression is still captured, but it cannot fabricate a pivot for an unselected source.
    let p = packet(&o, &s, 0);
    let error = format!(
        "{:?}",
        prepare_object_render_scene(&p, &pins(&s, 0)).unwrap_err()
    );
    assert!(
        error.contains("ordinary parent held source is unavailable"),
        "{error}"
    );
}

fn null_bytes(snapshot: &crate::revision::ObjectSnapshot) -> Vec<u8> {
    encode_storage_project(snapshot.document()).unwrap()
}

fn null_parent(legacy: bool, pos: Option<Value>) -> Value {
    let mut source = drawing(legacy);
    source["layers"][0]["parentLayerUid"] = json!("N");
    let mut layer = json!({"layerUid":"N","name":"Null","visible":false,"isNullLayer":true});
    if let Some(pos) = pos {
        layer["nullPos"] = pos;
    }
    source["layers"].as_array_mut().unwrap().push(layer);
    source["compositionOrder"]["contexts"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(json!({"layerUid":"N","naturalPosition":1,"order":{}}));
    source["propertyHolders"]
        .as_array_mut()
        .unwrap()
        .push(json!({
        "target":{"contextId":"scene-root","layerUid":"N"},"properties":[
            {"property":"position","static":[3,4]},{"property":"anchor","static":[10,-5]},
            {"property":"rotation","static":[180]},{"property":"opacity","static":[50]}]}));
    source
}

#[test]
fn real_null_ancestor_pivots_and_svg_use_authored_position_or_actual_project_center() {
    // Original own-layer points(-155,7)/(245,7), parent180deg around(base+[10,-5]), then+[3,4].
    for legacy in [false, true] {
        for (pos, x, y) in [
            (Some(json!([40, 20])), 258.0, 27.0),
            (Some(json!([0, 0])), 178.0, -13.0),
            (Some(Value::Null), 578.0, 187.0),
            (None, 578.0, 187.0),
        ] {
            let source = null_parent(legacy, pos);
            let o = owner(&source);
            let snapshot = o.acquire_snapshot(0).unwrap();
            let before = null_bytes(&snapshot);
            let roundtrip = decode_storage_project(&before).unwrap();
            assert_eq!(encode_storage_project(&roundtrip).unwrap(), before);
            for (frame, shift) in [(0, 0.0), (2, -2.0), (0, 0.0)] {
                let p = packet(&o, &snapshot, frame);
                assert_eq!(p.records().len(), 2, "Null has no stand-in content");
                let paint = plan(&p);
                for (index, (_, point)) in starts(&paint).iter().enumerate() {
                    near(point[0], x + shift - index as f64 * 400.0);
                    near(point[1], y);
                }
                assert!(paint
                    .commands()
                    .all(|c| c.paint()[3].as_f64() == Some(if legacy { 0.1875 } else { 0.25 })));
                if legacy {
                    let text = svg(&paint).unwrap();
                    assert_eq!(text.matches("fill-opacity=\"0.1875\"").count(), 2);
                    let coordinates = text
                        .split("<path d=\"")
                        .skip(1)
                        .map(|path| {
                            path.strip_prefix('M')
                                .unwrap()
                                .split_whitespace()
                                .next()
                                .unwrap()
                                .split(',')
                                .map(|n| n.parse::<f64>().unwrap())
                                .collect::<Vec<_>>()
                        })
                        .collect::<Vec<_>>();
                    for (index, point) in coordinates.iter().enumerate() {
                        near(point[0], x + shift - index as f64 * 400.0);
                        near(point[1], y);
                    }
                }
            }
            assert_eq!(null_bytes(&snapshot), before);
            assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
        }
    }
}

#[test]
fn drawing_only_null_requires_actual_position_and_keeps_no_canvas_unavailable() {
    for (pos, available) in [
        (None, false),
        (Some(Value::Null), false),
        (Some(json!([40, 20])), true),
    ] {
        let mut source = null_parent(true, pos);
        source["propertyHolders"][0]["properties"][0]
            .as_object_mut()
            .unwrap()
            .remove("expression");
        source["propertyHolders"][0]["properties"][0]["static"] = json!([5, 7]);
        let document = decode_storage_project(&serde_json::to_vec(&source).unwrap()).unwrap();
        let o = NativeObjectHistory::new("bare-null", document).unwrap();
        let snapshot = o.acquire_snapshot(0).unwrap();
        let before = null_bytes(&snapshot);
        let p = prepare_object_exposure_frame(&snapshot, &pins(&snapshot, 0)).unwrap();
        let scene = prepare_object_render_scene(&p, &pins(&snapshot, 0));
        assert_eq!(scene.is_ok(), available);
        if let Err(error) = scene {
            assert!(format!("{error:?}")
                .contains("Null parent needs nullPos or retained project canvas"));
        }
        assert_eq!(null_bytes(&snapshot), before);
        assert_eq!((o.content_revision(), o.history_depths()), (0, (0, 0)));
    }
}

#[test]
fn null_record_presence_roundtrips_and_malformed_or_content_bearing_nulls_refuse() {
    let decode = |d: &Value| decode_storage_project(&serde_json::to_vec(d).unwrap());
    for flag in [Value::Null, json!(false), json!(true)] {
        let mut source = null_parent(true, Some(Value::Null));
        source["layers"][1]["isNullLayer"] = flag;
        let decoded = decode(&source).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), source);
    }
    for bad in [json!([1]), json!([1, 2, 3]), json!(["1", 2]), json!(true)] {
        let mut source = null_parent(true, Some(bad));
        assert!(decode(&source).is_err());
        source["layers"][1]["nullPos"] = Value::Null;
        source["layers"][1]["isNullLayer"] = json!(1);
        assert!(decode(&source).is_err());
    }
    let mut source = null_parent(true, Some(json!([0, 0])));
    source["layers"][0]["isNullLayer"] = json!(true);
    let error = decode(&source).unwrap_err();
    assert!(error.message().contains("Null layer"));
    source["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("isNullLayer");
    source["items"] = json!({"raster":[{"target":{"contextId":"scene-root","layerUid":"N",
        "frameScope":{"kind":"authored","frame":0},"strokeId":"forbidden"},
        "item":{"assetId":"image","x":0,"y":0,"width":2,"height":2,"opacity":1}}]});
    let error = decode(&source).unwrap_err();
    assert!(error.message().contains("Null layer"));
}

#[test]
fn actual_null_duplicate_undo_redo_and_reopen_preserve_role_and_pivot_presence() {
    for pos in [None, Some(Value::Null), Some(json!([40, 20]))] {
        let source = null_parent(true, pos);
        let mut o = owner(&source);
        let original = o.acquire_snapshot(0).unwrap();
        let bytes = null_bytes(&original);
        let candidate = o
            .prepare_layer_duplicate_json(br#"{"command":"object.layer.duplicate","layerUid":"N"}"#)
            .unwrap();
        o.commit_layer_lifecycle(candidate).unwrap();
        let after = o.acquire_snapshot(1).unwrap();
        let value = serde_json::to_value(after.document()).unwrap();
        assert_eq!(value["layers"][2]["isNullLayer"], json!(true));
        assert_eq!(
            value["layers"][2].get("nullPos"),
            source["layers"][1].get("nullPos")
        );
        let changed = null_bytes(&after);
        let decoded = decode_storage_project(&changed).unwrap();
        assert_eq!(encode_storage_project(&decoded).unwrap(), changed);
        #[cfg(feature = "project_media_codec")]
        {
            let file = o.serialize_project_file(1, after.snapshot_id()).unwrap();
            let reopened = NativeObjectHistory::from_project_file("reopened-null", &file).unwrap();
            let loaded = reopened.acquire_snapshot(0).unwrap();
            assert_ne!(loaded.document_id(), after.document_id());
            assert_eq!(null_bytes(&loaded), changed);
        }
        assert_eq!(o.undo(1).unwrap(), 2);
        assert_eq!(null_bytes(&o.acquire_snapshot(2).unwrap()), bytes);
        assert_eq!(o.redo(2).unwrap(), 3);
        assert_eq!(null_bytes(&o.acquire_snapshot(3).unwrap()), changed);
        assert_eq!(null_bytes(&original), bytes);
        assert_eq!(o.history_depths(), (1, 0));
    }
}
