//! The paint-plan controls and fixtures, moved verbatim out of `object_paint_plan.rs` (a child module by path).
use crate::object_frame_packet::ObjectFrameSelector;
use crate::object_render_scene::paint::{
    join, prepare_object_paint_plan, LayerSequence, ObjectPaintPlan, ObjectPaintPlanError,
};
use crate::object_render_scene::ObjectRenderScene;
use serde_json::{json, Value};

pub(crate) fn document(frames: &[u32]) -> Value {
    let template: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut objects = Vec::new();
    let mut scopes = Vec::new();
    for frame in frames {
        let mut elements = Vec::new();
        for (layer, ids) in [
            ("L0", vec!["a", "b", "c"]),
            ("L1", vec!["a"]),
            ("L2", vec!["a"]),
        ] {
            for (position, id) in ids.into_iter().enumerate() {
                let mut record = template["records"][0].clone();
                record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":id});
                record["geometry"]["segments"] = json!([[0,0],[10,0],[0,10]].map(|[x,y]| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
                record["fill"] = json!({"kind":"solid","r":0.20000000000000004,"g":0.4000000000000001,"b":0.6000000000000001,"a":0.7500000000000001});
                objects.push(record);
                elements.push(json!({"layerUid":layer,"strokeId":id,"naturalPosition":position,"order":{}}));
            }
        }
        elements.reverse();
        scopes
            .push(json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements}));
    }
    objects.reverse();
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":21,
        "layers":[{"layerUid":"L2"},{"layerUid":"L0"},{"layerUid":"L1"}],"objects":objects,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}},
        {"layerUid":"L1","naturalPosition":1,"order":{}},{"layerUid":"L2","naturalPosition":2,"order":{}}],"scopes":scopes}],
        "migration":"explicit-plan-required","referencePolicy":"unresolved"}})
}
fn selector(packet: &crate::object_frame_packet::ObjectFramePacket) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: packet.instance_id(),
        document_id: packet.document_id(),
        content_revision: packet.content_revision(),
        document_snapshot_id: packet.document_snapshot_id(),
        context_id: packet.context_id(),
        scope_kind: packet.scope_kind().clone(),
        frame: packet.frame(),
    }
}
type MakeScene = fn(&Value, u32) -> ObjectRenderScene;
fn plan(value: &Value, frame: u32, make: MakeScene) -> ObjectPaintPlan {
    let scene = make(value, frame);
    prepare_object_paint_plan(&scene, &selector(scene.packet())).unwrap()
}
fn order(holder: &Value) -> Value {
    let mut out = json!({});
    if let Some(v) = holder.pointer("/motionStatic/order") {
        out["static"] = v.clone();
    }
    if let Some(v) = holder.pointer("/motion/order") {
        out["track"] = v.clone();
    }
    if let Some(v) = holder.pointer("/expressions/order") {
        out["expression"] = json!({"enabled":v["enabled"],"code":v["code"]});
    }
    out
}
fn labels(plan: &ObjectPaintPlan) -> Vec<String> {
    plan.commands()
        .map(|c| {
            format!(
                "{}:{}",
                c.source().record().target().layer_uid(),
                c.source().record().target().stroke_id()
            )
        })
        .collect()
}

pub(crate) fn order_cases(make: MakeScene) {
    let fixed: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-order-v2/baseline.json"
    ))
    .unwrap();
    for case in fixed["layerCases"].as_array().unwrap() {
        let mut value = document(&[7]);
        for (index, holder) in case["holders"].as_array().unwrap().iter().enumerate() {
            value["compositionOrder"]["contexts"][0]["layers"][index]["order"] = order(holder);
        }
        let expected: Vec<String> = case["expected"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|layer| {
                let layer = layer.as_str().unwrap();
                (if layer == "L0" {
                    vec!["a", "b", "c"]
                } else {
                    vec!["a"]
                })
                .into_iter()
                .map(move |id| format!("{layer}:{id}"))
            })
            .collect();
        assert_eq!(labels(&plan(&value, 7, make)), expected, "{}", case["id"]);
    }
    for case in fixed["elementCases"].as_array().unwrap() {
        let mut value = document(&[7]);
        for element in value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
            .as_array_mut()
            .unwrap()
        {
            if element["layerUid"] == "L0" {
                element["order"] =
                    order(&case["holders"][element["strokeId"].as_str().unwrap()]);
            }
        }
        let result = plan(&value, 7, make);
        let ids: Vec<_> = result
            .commands()
            .filter(|c| c.source().record().target().layer_uid() == "L0")
            .map(|c| c.source().record().target().stroke_id().to_owned())
            .collect();
        assert_eq!(json!(ids), case["expected"], "{}", case["id"]);
        for c in result.commands() {
            assert_eq!(c.source().record().target().frame(), 7);
            assert_eq!(
                c.source().record().target().scope_kind(),
                result.packet().scope_kind()
            );
        }
    }
}

pub(crate) fn frame_cases(make: MakeScene) {
    for (hold, points, expected) in [
        (
            false,
            json!([{"x":0,"y":0},{"x":1,"y":1}]),
            [
                vec!["L0:a", "L1:a"],
                vec!["L1:a", "L0:a"],
                vec!["L1:a", "L0:a"],
            ],
        ),
        (
            true,
            json!([{"x":0,"y":0},{"x":1,"y":1}]),
            [
                vec!["L0:a", "L1:a"],
                vec!["L0:a", "L1:a"],
                vec!["L1:a", "L0:a"],
            ],
        ),
        (
            false,
            json!([{"x":0,"y":0},{"x":0.5,"y":0.25},{"x":1,"y":1}]),
            [
                vec!["L0:a", "L1:a"],
                vec!["L0:a", "L1:a"],
                vec!["L1:a", "L0:a"],
            ],
        ),
    ] {
        let mut value = document(&[0, 5, 10]);
        value["layers"]
            .as_array_mut()
            .unwrap()
            .retain(|l| l["layerUid"] != "L2");
        value["objects"]
            .as_array_mut()
            .unwrap()
            .retain(|r| r["target"]["layerUid"] != "L2" && r["target"]["strokeId"] == "a");
        let context = &mut value["compositionOrder"]["contexts"][0];
        context["layers"]
            .as_array_mut()
            .unwrap()
            .retain(|l| l["layerUid"] != "L2");
        context["layers"][1]["order"] = json!({"track":{"keys":[{"frame":0,"v":[1],"hold":hold,"curvePoints":points},{"frame":10,"v":[2]}]}});
        for scope in context["scopes"].as_array_mut().unwrap() {
            scope["elements"]
                .as_array_mut()
                .unwrap()
                .retain(|e| e["layerUid"] != "L2" && e["strokeId"] == "a");
        }
        for (index, frame) in [0, 5, 10].into_iter().enumerate() {
            let result = plan(&value, frame, make);
            assert_eq!(labels(&result), expected[index]);
            assert_eq!(result.packet().frame(), frame);
            assert!(result
                .commands()
                .all(|c| c.source().record().target().frame() == frame));
        }
    }
}

pub(crate) fn join_cases(make: MakeScene, reference: &ObjectRenderScene) {
    let value = document(&[7]);
    let scene = make(&value, 7);
    pin_cases(&scene, reference);
    for (index, entry) in scene.entries().enumerate() {
        let record = scene.packet().record_at(index).unwrap();
        let indexed = scene.entry(index).unwrap();
        assert!(std::ptr::eq(record.record(), entry.source().record()));
        assert_eq!(
            record.source_layer_index(),
            entry.source().source_layer_index()
        );
        assert_eq!(
            record.source_object_index(),
            entry.source().source_object_index()
        );
        assert!(std::ptr::eq(
            indexed.source().record(),
            entry.source().record()
        ));
        assert!(std::ptr::eq(indexed.contour(), entry.contour()));
    }
    assert!(scene.entry(scene.entries().len()).is_none());
    assert!(scene.packet().record_at(usize::MAX).is_none());
    assert!(scene.entry(usize::MAX).is_none());
    let good: LayerSequence = scene
        .packet()
        .numeric_order()
        .unwrap()
        .layers()
        .iter()
        .map(|l| {
            (
                l.source_layer_index(),
                l.elements()
                    .iter()
                    .map(|e| e.source_object_index())
                    .collect(),
            )
        })
        .collect();
    let before = labels(&plan(&value, 7, make));
    for mode in 0..7 {
        let mut bad = good.clone();
        match mode {
            0 => {
                let duplicate = bad[0].1[0];
                bad[0].1.push(duplicate);
            }
            1 => {
                bad[0].1.pop();
            }
            2 => {
                bad[0].1[0] = usize::MAX;
            }
            3 => {
                bad[0].1[0] = bad[1].1[0];
            }
            4 => {
                bad.push(bad[0].clone());
            }
            5 => {
                bad.pop();
            }
            _ => bad[1].0 = usize::MAX,
        }
        assert_eq!(
            join(&scene, &bad).unwrap_err(),
            ObjectPaintPlanError::InvalidOrder
        );
        assert_eq!(labels(&plan(&value, 7, make)), before);
    }
    let mut empty = value.clone();
    empty["objects"] = json!([]);
    empty["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = json!([]);
    assert_eq!(plan(&empty, 7, make).commands().len(), 0);
    let empty_scene = make(&empty, 7);
    pin_cases(&empty_scene, reference);
    assert_eq!(
        join(&empty_scene, &vec![]).unwrap_err(),
        ObjectPaintPlanError::InvalidOrder
    );
}

fn pin_cases(scene: &ObjectRenderScene, reference: &ObjectRenderScene) {
    for mode in 0..7 {
        let mut expected = selector(scene.packet());
        match mode {
            0 => expected.instance_id = "foreign",
            1 => expected.document_id = "foreign",
            2 => expected.content_revision += 1,
            3 => expected.document_snapshot_id = "foreign",
            4 => expected.context_id = "component-1",
            5 => expected.scope_kind = reference.packet().scope_kind().clone(),
            _ => expected.frame += 1,
        }
        assert!(matches!(
            prepare_object_paint_plan(scene, &expected),
            Err(ObjectPaintPlanError::Pins(_))
        ));
    }
    assert_eq!(
        prepare_object_paint_plan(reference, &selector(reference.packet())).unwrap_err(),
        ObjectPaintPlanError::Unavailable
    );
}

#[cfg(feature = "history")]
pub(crate) fn paint_contents(plan: &ObjectPaintPlan) -> Value {
    json!({"document":plan.packet().document_id(),"revision":plan.packet().content_revision(),
        "snapshot":plan.packet().document_snapshot_id(),"frame":plan.packet().frame(),
        "commands":plan.commands().map(|c| json!({"layer":c.source().source_layer_index(),
            "index":c.source().source_object_index(),"record":c.source().record(),"paint":c.paint()})).collect::<Vec<_>>()})
}
