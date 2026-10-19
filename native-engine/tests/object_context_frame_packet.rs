//! Actual snapshot -> frame -> render -> paint component-context controls.
//! Expected identities/order are literal baseline component records, not root indices.
use crate::evaluation::object_component_kernels::object_context_resolver::world;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{
    prepare_object_exposure_frame, prepare_object_frame, ObjectFrameError, ObjectFrameSelector,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn source() -> Value {
    let mut value: Value = serde_json::from_str(world::json("D")).unwrap();
    for component in value["componentTables"]["components"]["components"]
        .as_array_mut()
        .unwrap()
    {
        for (index, layer) in component["layers"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .enumerate()
        {
            let frames: &[u32] = if index == 0 {
                &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11]
            } else {
                &[0]
            };
            layer["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,
                "family":"drawing","contextId":"scene-root",
                "keys":frames.iter().map(|frame|json!({"frame":frame})).collect::<Vec<_>>(),"samples":[]});
        }
    }
    value
}
fn admitted(value: &Value) -> ObjectSnapshot {
    ObjectSnapshot::from_admitted(
        "component-frame-owner".into(),
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn selector<'a>(
    snapshot: &'a ObjectSnapshot,
    context: &'a str,
    frame: u32,
) -> ObjectFrameSelector<'a> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: context,
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
#[test]
fn declared_components_use_their_own_layers_records_held_sources_and_same_snapshot() {
    let snapshot = admitted(&source());
    let before = encode_storage_project(snapshot.document()).unwrap();
    let mut targets = Vec::new();
    for context in ["sym_comp", "sym_comp_b"] {
        let packet = prepare_object_frame(&snapshot, &selector(&snapshot, context, 7)).unwrap();
        assert_eq!(
            packet
                .source_layers()
                .iter()
                .map(|layer| layer.layer_uid())
                .collect::<Vec<_>>(),
            ["ly_comp_ia", "ly_comp_ib"]
        );
        assert_eq!(
            (
                packet.source_frame_for_layer(0),
                packet.source_frame_for_layer(1)
            ),
            (Some(7), Some(0))
        );
        let records = packet
            .records()
            .map(|record| record.record().target().clone())
            .collect::<Vec<_>>();
        assert_eq!(
            records
                .iter()
                .map(|target| target.stroke_id())
                .collect::<Vec<_>>(),
            ["s_comp_a_7", "s_comp_b_0"]
        );
        assert!(records.iter().all(|target| target.context_id() == context));
        assert_eq!(packet.context_id(), context);
        assert_eq!(packet.frame(), 7);
        assert_eq!(packet.document_snapshot_id(), snapshot.snapshot_id());
        targets.push(records);
    }
    assert_ne!(targets[0][0], targets[1][0]);
    assert_eq!(targets[0][0].stroke_id(), targets[1][0].stroke_id());
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
}
#[test]
fn entered_layer_range_is_inclusive_and_never_readdresses_the_root_layer_array() {
    let mut value = source();
    value["componentTables"]["components"]["components"][0]["layers"][0]["range"] =
        json!({"inPoint":3,"outPoint":7});
    let snapshot = admitted(&value);
    let before = encode_storage_project(snapshot.document()).unwrap();
    for (frame, source) in [(2, None), (3, Some(3)), (7, Some(7)), (8, None)] {
        let packet =
            prepare_object_frame(&snapshot, &selector(&snapshot, "sym_comp", frame)).unwrap();
        assert_eq!(packet.source_frame_for_layer(0), source);
        assert_eq!(packet.source_frame_for_layer(1), Some(0));
        assert_eq!(packet.records().len(), if source.is_some() { 2 } else { 1 });
        assert!(packet
            .records()
            .all(|record| record.record().target().context_id() == "sym_comp"));
    }
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
}
#[test]
fn all_four_pins_unknown_context_reference_scope_and_local_frame_bounds_stay_closed() {
    let snapshot = admitted(&source());
    for (field, expected) in [
        (0, ObjectFrameError::WrongInstance),
        (1, ObjectFrameError::WrongDocument),
        (2, ObjectFrameError::WrongRevision),
        (3, ObjectFrameError::WrongSnapshot),
        (4, ObjectFrameError::InvalidContext),
        (5, ObjectFrameError::Unavailable),
        (6, ObjectFrameError::FrameOutOfRange),
    ] {
        let mut query = selector(&snapshot, "sym_comp", 0);
        match field {
            0 => query.instance_id = "other",
            1 => query.document_id = "other",
            2 => query.content_revision += 1,
            3 => query.document_snapshot_id = "other",
            4 => query.context_id = "unknown",
            5 => query.scope_kind = FrameScopeKind::Reference,
            _ => query.frame = 12,
        };
        assert_eq!(
            prepare_object_frame(&snapshot, &query).unwrap_err(),
            expected
        );
    }
    let mut value = source();
    value["componentTables"]["components"]["components"][0]["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("timing");
    let missing = admitted(&value);
    assert_eq!(
        prepare_object_frame(&missing, &selector(&missing, "sym_comp", 0)).unwrap_err(),
        ObjectFrameError::Unavailable
    );
}
#[cfg(feature = "compositor")]
#[test]
fn both_frame_entries_reach_the_actual_shared_scene_paint_and_composition_route() {
    use crate::object_render_scene::{
        prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
        ObjectSceneError,
    };
    let snapshot = admitted(&source());
    let before = encode_storage_project(snapshot.document()).unwrap();
    for context in ["sym_comp", "sym_comp_b"] {
        let query = selector(&snapshot, context, 7);
        let plain = prepare_object_frame(&snapshot, &query).unwrap();
        let exposure = prepare_object_exposure_frame(&snapshot, &query).unwrap();
        let mut sequences = Vec::new();
        for packet in [&plain, &exposure] {
            let scene = prepare_object_render_scene(packet, &query).unwrap();
            let plan = prepare_object_paint_plan(&scene, &query).unwrap();
            let witness = prepare_object_composition(&plan, &query).unwrap();
            assert_eq!(witness.context_id(), context);
            assert_eq!(plan.commands().len(), 2);
            sequences.push(
                plan.commands()
                    .map(|command| {
                        (
                            command.source().record().target().clone(),
                            command.paint().map(|number| (*number).clone()),
                        )
                    })
                    .collect::<Vec<_>>(),
            );
            let other = selector(
                &snapshot,
                if context == "sym_comp" {
                    "sym_comp_b"
                } else {
                    "sym_comp"
                },
                7,
            );
            assert_eq!(
                prepare_object_render_scene(packet, &other).unwrap_err(),
                ObjectSceneError::InvalidContext
            );
        }
        assert_eq!(sequences[0], sequences[1]);
    }
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
}

fn explicit_component_plan(value: &mut Value) {
    let component = &value["componentTables"]["components"]["components"][0];
    let mut scopes = Vec::new();
    for frame in 0..12 {
        let entries = component["objects"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|record| record["target"]["frameScope"]["frame"] == frame)
            .map(|record| {
                json!({"layerUid":record["target"]["layerUid"],
                "strokeId":record["target"]["strokeId"],"naturalPosition":0,"order":{}})
            })
            .collect::<Vec<_>>();
        scopes.push(json!({"frameScope":{"kind":"authored","frame":frame},"elements":entries}));
    }
    value["compositionOrder"]["contexts"]
        .as_array_mut()
        .unwrap()
        .push(json!({
        "contextId":"sym_comp","layers":[
            {"layerUid":"ly_comp_ib","naturalPosition":0,"order":{}},
            {"layerUid":"ly_comp_ia","naturalPosition":1,"order":{}}],"scopes":scopes}));
}
#[test]
fn explicit_component_order_uses_its_own_complete_plan_and_is_not_replaced_by_natural_order() {
    let mut value = source();
    explicit_component_plan(&mut value);
    let snapshot = admitted(&value);
    let before = encode_storage_project(snapshot.document()).unwrap();
    let packet = prepare_object_frame(&snapshot, &selector(&snapshot, "sym_comp", 7)).unwrap();
    let order = packet
        .numeric_order()
        .unwrap()
        .layers()
        .iter()
        .map(|layer| packet.source_layers()[layer.source_layer_index()].layer_uid())
        .collect::<Vec<_>>();
    assert_eq!(order, ["ly_comp_ib", "ly_comp_ia"]);
    // Explicit plan rank is back-to-front; the opposite natural stack proves
    // this result came from this context's authored order, not its root plan.
    let natural = admitted(&source());
    let natural_packet =
        prepare_object_frame(&natural, &selector(&natural, "sym_comp", 7)).unwrap();
    let natural_order = natural_packet
        .numeric_order()
        .unwrap()
        .layers()
        .iter()
        .map(|layer| natural_packet.source_layers()[layer.source_layer_index()].layer_uid())
        .collect::<Vec<_>>();
    assert_ne!(order, natural_order);
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
}
#[test]
fn malformed_explicit_component_plans_are_refused_instead_of_falling_back_to_natural_order() {
    for defect in 0..5 {
        let mut value = source();
        explicit_component_plan(&mut value);
        let plan = &mut value["compositionOrder"]["contexts"][1];
        match defect {
            0 => {
                plan["layers"][0]["layerUid"] = json!("ly_plain");
            }
            1 => {
                plan["scopes"][7]["elements"]
                    .as_array_mut()
                    .unwrap()
                    .clear();
            }
            2 => {
                plan["scopes"][7]["elements"][0]["strokeId"] = json!("missing");
            }
            3 => {
                plan["scopes"][7]["frameScope"]["frame"] = json!(12);
            }
            _ => {
                plan["scopes"][7]["elements"][0]["layerUid"] = json!("ly_plain");
            }
        }
        assert!(
            decode_storage_project(value.to_string().as_bytes()).is_err(),
            "defect {defect}"
        );
    }
}
#[test]
fn v1_entered_component_natural_order_does_not_change_root_migration_policy() {
    let mut value = source();
    value["formatVersion"] = json!(1);
    value.as_object_mut().unwrap().remove("compositionOrder");
    let snapshot = admitted(&value);
    let packet = prepare_object_frame(&snapshot, &selector(&snapshot, "sym_comp", 7)).unwrap();
    assert!(packet.numeric_order().is_some());
    assert!(packet
        .records()
        .all(|record| record.record().target().context_id() == "sym_comp"));
    // Root v1 still has no synthesized OrderPlan. This is the established
    // entered-component native encounter order only.
    assert!(
        prepare_object_frame(&snapshot, &selector(&snapshot, "scene-root", 0))
            .unwrap()
            .numeric_order()
            .is_none()
    );
}

#[cfg(feature = "compositor")]
#[test]
fn editable_component_vectors_reach_actual_contours_without_root_index_aliasing() {
    use crate::evaluation::object_vector_kernels::render_vector_eval::{
        evaluate_record, VectorEvalError,
    };
    use crate::object_render_scene::{prepare_object_paint_plan, prepare_object_render_scene};
    let mut value = source();
    for (component_index, component) in value["componentTables"]["components"]["components"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .enumerate()
    {
        for record in component["objects"].as_array_mut().unwrap() {
            record["family"] = json!("editable-vector-path");
            record["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":3,
                "strokeCap":"round","strokeJoin":"round","opacity":1});
            let x = 40 + (component_index as i64) * 200;
            record["geometry"]["segments"][0]["point"]["x"] = json!(x);
        }
    }
    let snapshot = admitted(&value);
    let foreign = admitted(&value);
    let before = encode_storage_project(snapshot.document()).unwrap();
    for (context, first_x) in [("sym_comp", 40.0), ("sym_comp_b", 240.0)] {
        let query = selector(&snapshot, context, 7);
        let packet = prepare_object_frame(&snapshot, &query).unwrap();
        let scoped = packet.records().next().unwrap();
        let evaluated = evaluate_record(&packet, scoped).unwrap();
        assert_eq!(evaluated.segments[0].point[0], first_x);
        assert_eq!(evaluated.source_object_index, scoped.source_object_index());
        let other = prepare_object_frame(&foreign, &selector(&foreign, context, 7)).unwrap();
        assert_eq!(
            evaluate_record(&packet, other.records().next().unwrap()).unwrap_err(),
            VectorEvalError::MissingRecord
        );
        let scene = prepare_object_render_scene(&packet, &query).unwrap();
        let paint = prepare_object_paint_plan(&scene, &query).unwrap();
        let command = paint.commands().next().unwrap();
        assert_eq!(command.source().record().target().context_id(), context);
        assert_eq!(command.contour().segments()[0].point[0], first_x);
    }
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
}
