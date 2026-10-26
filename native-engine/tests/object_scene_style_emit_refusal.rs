//! L07-I (F16, G03a): the production compositor preparation (`prepare_command`, the one function every compose
//! entry calls before any target exists) never sees a stroke the style contract refuses, because admission refuses
//! it first with a typed codec error; admitted gradients arrive as gradients, never flat-filled.
use crate::compositor::object_compositor::prepare_command;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    paint_test_document, prepare_object_paint_plan, prepare_object_render_scene, ObjectPaintPlan,
};
use crate::revision::ObjectSnapshot;
use serde_json::{json, Value};

fn vector_body(stroke: Option<Value>, width: f64, dash: Option<Value>) -> Value {
    let mut body = json!({"stroke":stroke,"hasRealStroke":stroke.is_some(),"strokeWidth":width,
        "strokeCap":"butt","strokeJoin":"miter","miterLimit":10,"dashOffset":0,"opacity":1});
    if let Some(dash) = dash {
        body["dashArray"] = dash;
    }
    body
}
/// Layer L0's three records carry a two-stop gradient; its position track stays at `x`.
fn gradient_document(x: f64) -> Value {
    let mut value = paint_test_document(&[7]);
    for record in value["objects"].as_array_mut().unwrap() {
        if record["target"]["layerUid"] != "L0" {
            continue;
        }
        record["family"] = json!("editable-vector-path");
        record["fill"] = Value::Null;
        record["vector"] = vector_body(None, 1.0, None);
        record["vectorExt"] = json!({"fillGradient":{"kind":"linear","from":[0,0],"to":[32,0],
            "stops":[{"offset":0,"color":"#000000"},{"offset":1,"color":"#ffffff"}]}});
    }
    value["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"position","track":{"keys":[
            {"frame":0,"v":[x,0],"curvePoints":[]},{"frame":10,"v":[x,0]}]}}]}]);
    value
}
fn plan_of(value: &Value) -> (ObjectSnapshot, ObjectPaintPlan) {
    let snapshot = ObjectSnapshot::from_admitted(
        "style-refusal".into(),
        decode_storage_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap();
    let plan = {
        let pins = ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame: 7,
        };
        let packet = prepare_object_frame(&snapshot, &pins).unwrap();
        let scene = prepare_object_render_scene(&packet, &pins).unwrap();
        prepare_object_paint_plan(&scene, &pins).unwrap()
    };
    (snapshot, plan)
}
fn refusals(value: &Value) -> Vec<String> {
    let (snapshot, plan) = plan_of(value);
    let before = encode_storage_project(snapshot.document()).unwrap();
    assert_eq!(plan.commands().len(), 5, "every command still planned");
    let out = plan
        .commands()
        .map(|command| match prepare_command(command) {
            Ok(_) => String::new(),
            Err(error) => error.to_string(),
        })
        .collect();
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    out
}

#[test]
fn a_stored_gradient_reaches_the_production_preparation_as_a_gradient_never_a_flat_fill() {
    let (_snapshot, plan) = plan_of(&gradient_document(0.0));
    let mut gradients = 0;
    for command in plan.commands() {
        let is_gradient = command.style().unwrap().fill.gradient().is_some();
        gradients += usize::from(is_gradient);
        assert!(prepare_command(command).is_ok());
    }
    // Layer L0 carries the gradient on three of the five shapes; the other two are plain solids.
    assert_eq!(gradients, 3);
}

#[test]
fn strokes_the_style_contract_refuses_cannot_be_authored_so_admission_refuses_them_first() {
    let stroke = Some(json!({"kind":"solid","r":0.0,"g":0.0,"b":1.0,"a":1.0}));
    for body in [
        vector_body(stroke.clone(), -1.0, None),
        vector_body(stroke.clone(), 2.0, Some(json!([0, 0]))),
        {
            let mut inconsistent = vector_body(stroke.clone(), 2.0, None);
            inconsistent["hasRealStroke"] = json!(false);
            inconsistent
        },
    ] {
        let mut value = paint_test_document(&[7]);
        for record in value["objects"].as_array_mut().unwrap() {
            record["family"] = json!("editable-vector-path");
            record["vector"] = body.clone();
        }
        let error = decode_storage_project(&serde_json::to_vec(&value).unwrap()).unwrap_err();
        assert_eq!(
            error.kind(),
            crate::object_codec::ObjectCodecErrorKind::Invalid
        );
    }
    // A valid stroke prepares for every command.
    let mut value = paint_test_document(&[7]);
    for record in value["objects"].as_array_mut().unwrap() {
        record["family"] = json!("editable-vector-path");
        record["vector"] = vector_body(stroke.clone(), 2.0, None);
    }
    assert_eq!(refusals(&value), [""; 5]);
}
