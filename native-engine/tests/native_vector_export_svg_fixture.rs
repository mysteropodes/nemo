//! Real-owner builders: every plan below comes from the actual codec, history
//! owner, frame packet, render scene and paint plan the PNG export uses.
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    paint_test_document, prepare_object_paint_plan, prepare_object_render_scene, ObjectPaintPlan,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

pub(super) fn segment(point: [f64; 2], handle_in: [f64; 2], handle_out: [f64; 2]) -> Value {
    json!({"point":{"x":point[0],"y":point[1]},
        "handleIn":{"x":handle_in[0],"y":handle_in[1]},
        "handleOut":{"x":handle_out[0],"y":handle_out[1]}})
}

pub(super) fn owner(instance: &str, value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        instance,
        decode_storage_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap()
}

pub(super) fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}

pub(super) fn plan(snapshot: &ObjectSnapshot, frame: u32) -> ObjectPaintPlan {
    let pins = selector(snapshot, frame);
    let packet = prepare_object_frame(snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    prepare_object_paint_plan(&scene, &pins).unwrap()
}

/// The accepted five-record frame-7 document, untouched (fractional fills such
/// as 0.20000000000000004 and three-point triangles).
pub(super) fn default_document() -> Value {
    paint_test_document(&[7])
}

fn retain_only(value: &mut Value, keep: impl Fn(&str, &str) -> bool) {
    value["objects"].as_array_mut().unwrap().retain(|r| {
        keep(
            r["target"]["layerUid"].as_str().unwrap(),
            r["target"]["strokeId"].as_str().unwrap(),
        )
    });
    for scope in value["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .unwrap()
    {
        scope["elements"].as_array_mut().unwrap().retain(|e| {
            keep(
                e["layerUid"].as_str().unwrap(),
                e["strokeId"].as_str().unwrap(),
            )
        });
    }
}

/// One record (L0:a), a 3-segment curve with fractional anchors and handles,
/// and an exactly representable fill, so the whole document can be a literal.
pub(super) fn single_curve_document() -> Value {
    let mut value = default_document();
    retain_only(&mut value, |layer, id| layer == "L0" && id == "a");
    let record = &mut value["objects"][0];
    record["geometry"]["segments"] = json!([
        segment([1.0, 2.0], [0.0, 0.0], [0.5, 0.25]),
        segment([11.5, 2.0], [-0.5, 0.0], [0.0, 1.25]),
        segment([6.25, 12.75], [1.0, 0.5], [-1.0, -0.5]),
    ]);
    record["fill"] = json!({"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.75});
    value
}

/// Five records with distinct red channels 0.1..0.5 in natural paint order
/// L0:a, L0:b, L0:c, L1:a, L2:a (storage order is the reverse). When
/// `displaced`, layer L2 asks for numeric order 2 (the accepted
/// native-object-order-v2 "displacement" case: paint L0, L2, L1).
pub(super) fn ordered_document(displaced: bool) -> Value {
    let mut value = default_document();
    for record in value["objects"].as_array_mut().unwrap() {
        let key = format!(
            "{}:{}",
            record["target"]["layerUid"].as_str().unwrap(),
            record["target"]["strokeId"].as_str().unwrap()
        );
        let red = match key.as_str() {
            "L0:a" => 0.1,
            "L0:b" => 0.2,
            "L0:c" => 0.3,
            "L1:a" => 0.4,
            _ => 0.5,
        };
        record["fill"] = json!({"kind":"solid","r":red,"g":0.5,"b":0.25,"a":0.9});
    }
    if displaced {
        value["compositionOrder"]["contexts"][0]["layers"][2]["order"] = json!({"static":[2]});
    }
    value
}

/// Valid document with zero records (the accepted "empty plan" shape).
pub(super) fn empty_document() -> Value {
    let mut value = default_document();
    value["objects"] = json!([]);
    value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = json!([]);
    value
}

/// Red channels of a plan's commands in paint order, read from the plan.
pub(super) fn plan_reds(plan: &ObjectPaintPlan) -> Vec<f64> {
    plan.commands()
        .map(|c| c.paint()[0].as_f64().unwrap())
        .collect()
}
