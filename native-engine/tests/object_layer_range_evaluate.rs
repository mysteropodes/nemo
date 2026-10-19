//! Literal baseline edge oracles through real document admission, never a parallel evaluator.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
use build::{Build, Cell, Stroke};

pub(super) fn drawing(count: usize, total: usize, v2: bool) -> Value {
    let cells: Vec<_> = (0..count)
        .map(|layer| {
            (0..total)
                .map(|frame| Cell {
                    flag: if frame == 0 { 'K' } else { '.' },
                    strokes: if frame == 0 {
                        vec![Stroke {
                            id: format!("s{layer}"),
                            z: 1,
                            g: 1,
                        }]
                    } else {
                        Vec::new()
                    },
                })
                .collect()
        })
        .collect();
    build::document_json(&cells, if v2 { Build::V2 } else { Build::V1 })
}
pub(super) fn admit(value: &Value) -> ObjectDocument {
    decode_storage_project(&serde_json::to_vec(value).unwrap()).expect("real admitted fixture")
}
fn points(value: &Value, uid: &str, clamped: bool) -> (f64, f64) {
    let doc = admit(value);
    let range = RangeEvaluator::for_document(&doc, "scene-root", 0)
        .unwrap()
        .resolve(uid, clamped)
        .unwrap();
    (range.in_point, range.out_point)
}
fn offsets(value: &mut Value, incoming: f64, outgoing: f64) {
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"timeLinkInOffset","static":[incoming]},
            {"property":"timeLinkOutOffset","static":[outgoing]}]}]);
}

#[test]
fn missing_null_markers_and_explicit_zero_are_distinct_without_mutating_storage() {
    for (range, gates) in [
        (json!({}), false),
        (
            json!({"inPoint":null,"outPoint":null,"timeLink":null}),
            false,
        ),
        (json!({"markers":[{"frame":2}]}), false),
        (json!({"inPoint":0}), true),
    ] {
        let mut value = drawing(1, 12, false);
        value["layers"][0]["range"] = range;
        let document = admit(&value);
        let bytes = encode_storage_project(&document).unwrap();
        let resolver = RangeEvaluator::for_document(&document, "scene-root", 0).unwrap();
        assert_eq!(resolver.gates("L0"), Ok(gates));
        assert_eq!(
            resolver.resolve("L0", true).unwrap(),
            ResolvedRange {
                in_point: 0.,
                out_point: 11.
            }
        );
        assert_eq!(encode_storage_project(&document).unwrap(), bytes);
    }
}

#[test]
fn clamping_is_nondestructive_and_inverted_edges_stay_empty() {
    let mut value = drawing(1, 12, false);
    value["layers"][0]["range"] = json!({"inPoint":-9,"outPoint":90});
    assert_eq!(points(&value, "L0", false), (-9., 90.));
    assert_eq!(points(&value, "L0", true), (0., 11.));
    value["layers"][0]["range"] = json!({"inPoint":8,"outPoint":3});
    let doc = admit(&value);
    let window = RangeEvaluator::for_document(&doc, "scene-root", 0)
        .unwrap()
        .resolve("L0", true)
        .unwrap();
    assert!((0..12).all(|frame| !window.contains(frame)));
}

#[test]
fn only_authored_keys_change_leading_and_trailing_blank_defaults() {
    let mut value = drawing(1, 8, false);
    let layer = &mut value["layers"][0];
    layer["timing"]["keys"] = json!([{"frame":0},{"frame":2},{"frame":5}]);
    layer["timing"]["samples"] = json!([{"frame":6,"manualEdit":true}]);
    value["objects"][0]["target"]["frameScope"]["frame"] = json!(2);
    let mut sample = value["objects"][0].clone();
    sample["target"]["frameScope"]["frame"] = json!(6);
    sample["target"]["strokeId"] = json!("manual");
    value["objects"].as_array_mut().unwrap().push(sample);
    assert_eq!(points(&value, "L0", false), (2., 4.));
    let doc = admit(&value);
    assert_eq!(
        RangeEvaluator::for_document(&doc, "scene-root", 0)
            .unwrap()
            .gates("L0"),
        Ok(false)
    );
    value["layers"][0]["range"] = json!({"inPoint":0});
    assert_eq!(
        points(&value, "L0", false),
        (0., 4.),
        "explicit zero overrides auto-in only"
    );
}

#[test]
fn link_modes_cross_anchors_source_clamping_and_static_offsets_use_one_document() {
    let mut value = drawing(2, 12, true);
    value["layers"][1]["range"] = json!({"inPoint":-5,"outPoint":30});
    value["layers"][0]["range"] = json!({"inPoint":2,"outPoint":9,
        "timeLink":{"uid":"L1","mode":"in","srcAnchor":"out"}});
    offsets(&mut value, -1.5, 7.);
    assert_eq!(points(&value, "L0", false), (9.5, 9.));
    value["layers"][0]["range"]["timeLink"] = json!({"uid":"L1","mode":"out","srcAnchor":"in"});
    assert_eq!(points(&value, "L0", true), (2., 7.));
    value["layers"][0]["range"]["timeLink"] = json!({"uid":"L1"});
    assert_eq!(points(&value, "L0", true), (0., 11.));
}

#[test]
fn deleted_self_and_cycle_sources_follow_baseline_own_edge_fallback() {
    let mut value = drawing(2, 12, true);
    for uid in ["gone", "L0"] {
        value["layers"][0]["range"] = json!({"inPoint":2,"outPoint":9,"timeLink":{"uid":uid}});
        offsets(&mut value, 1., 1.);
        assert_eq!(
            points(&value, "L0", true),
            (2., 9.),
            "unresolved links do not apply offsets"
        );
    }
    value["layers"][0]["range"] = json!({"inPoint":2,"outPoint":9,"timeLink":{"uid":"L1"}});
    value["layers"][1]["range"] = json!({"inPoint":5,"outPoint":8,"timeLink":{"uid":"L0"}});
    value["propertyHolders"]
        .as_array_mut()
        .unwrap()
        .push(json!({
        "target":{"contextId":"scene-root","layerUid":"L1"},
        "properties":[{"property":"timeLinkInOffset","static":[2]}]}));
    assert_eq!(
        points(&value, "L0", true),
        (5., 10.),
        "cycle own2 + B2 + A1; out9 + A1"
    );
}

#[test]
fn link_depth_falls_back_at_the_baseline_seventeenth_hop() {
    let mut value = drawing(21, 64, false);
    for index in 0..21 {
        value["layers"][index]["range"] = json!({"inPoint":index,"outPoint":63});
        if index < 20 {
            value["layers"][index]["range"]["timeLink"] = json!({"uid":format!("L{}",index+1)});
        }
    }
    assert_eq!(points(&value, "L0", true), (17., 63.));
    assert_eq!(points(&value, "L1", true), (18., 63.));
}

#[test]
fn actual_folder_parent_intersects_raw_and_clamped_without_changing_child() {
    let mut value = drawing(2, 12, true);
    value["layers"][0]["parentLayerUid"] = json!("L1");
    value["layers"][0]["range"] = json!({"inPoint":-5,"outPoint":30});
    value["layers"][1]["isFolderLayer"] = json!(true);
    value["layers"][1]["range"] = json!({"inPoint":-2,"outPoint":20});
    value["objects"]
        .as_array_mut()
        .unwrap()
        .retain(|record| record["target"]["layerUid"] != "L1");
    // The actual order plan must not retain a removed content target.
    value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
        .as_array_mut()
        .unwrap()
        .retain(|record| record["layerUid"] != "L1");
    assert_eq!(points(&value, "L0", false), (-2., 20.));
    assert_eq!(points(&value, "L0", true), (0., 11.));
    value["layers"][0].as_object_mut().unwrap().remove("range");
    let doc = admit(&value);
    assert_eq!(
        RangeEvaluator::for_document(&doc, "scene-root", 0)
            .unwrap()
            .gates("L0"),
        Ok(true)
    );
}

#[test]
fn active_offset_expression_and_any_track_are_not_raw_fallbacks() {
    let mut value = drawing(2, 12, true);
    value["layers"][0]["range"] = json!({"timeLink":{"uid":"L1"}});
    offsets(&mut value, 2., 0.);
    value["propertyHolders"][0]["properties"][0]["expression"] =
        json!({"enabled":true,"code":"value+frame"});
    let doc = admit(&value);
    assert_eq!(
        RangeEvaluator::for_document(&doc, "scene-root", 3)
            .unwrap()
            .resolve("L0", true),
        Err(RangeError::OffsetExpressionUnavailable)
    );
    value["propertyHolders"][0]["properties"][0]
        .as_object_mut()
        .unwrap()
        .remove("expression");
    value["propertyHolders"][0]["properties"][0]["track"] = json!({"keys":[]});
    assert!(
        decode_storage_project(&serde_json::to_vec(&value).unwrap()).is_err(),
        "nonanimatable offset rejects even an empty track at actual admission"
    );
}

#[test]
fn borrowed_working_document_observes_each_sealed_edit_without_mutating_old_snapshot() {
    let value = drawing(2, 12, true);
    let mut doc = admit(&value);
    let old = crate::revision::ObjectSnapshot::new("range-old", doc.clone()).unwrap();
    doc.layers[0].range = serde_json::from_value(json!({"outPoint":4})).unwrap();
    assert_eq!(
        RangeEvaluator::for_document(&doc, "scene-root", 0)
            .unwrap()
            .resolve("L0", true)
            .unwrap()
            .out_point,
        4.
    );
    doc.layers[1].range = serde_json::from_value(json!({"timeLink":{"uid":"L0"}})).unwrap();
    assert_eq!(
        RangeEvaluator::for_document(&doc, "scene-root", 0)
            .unwrap()
            .resolve("L1", true)
            .unwrap()
            .out_point,
        4.
    );
    doc.layers[0].range = serde_json::from_value(json!({"outPoint":7})).unwrap();
    assert_eq!(
        RangeEvaluator::for_document(&doc, "scene-root", 0)
            .unwrap()
            .resolve("L1", true)
            .unwrap()
            .out_point,
        7.
    );
    assert!(old.document().layers()[0].range.value().is_none());
}

#[test]
fn wrong_context_missing_layer_and_out_of_timeline_sample_refuse_typed() {
    let doc = admit(&drawing(1, 12, false));
    assert!(matches!(
        RangeEvaluator::for_document(&doc, "gone", 0),
        Err(RangeError::ContextUnavailable)
    ));
    assert!(matches!(
        RangeEvaluator::for_document(&doc, "scene-root", 12),
        Err(RangeError::FrameOutOfRange)
    ));
    assert_eq!(
        RangeEvaluator::for_document(&doc, "scene-root", 0)
            .unwrap()
            .resolve("missing", true),
        Err(RangeError::LayerMissing)
    );
}
