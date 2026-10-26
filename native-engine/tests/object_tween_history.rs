//! #2358: independent production frame outputs through the real dispatcher, history and storage codec.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use crate::request_receipts::OpacityRequest;
use serde_json::{json, Value};
fn fixture_cases() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!("fixtures/tween-baseline/oracle.json")).unwrap()
        ["cases"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["operation"] == "generate")
        .cloned()
        .collect()
}
fn paint(v: &Value) -> Value {
    let Some(s) = v.as_str() else {
        return Value::Null;
    };
    let rgb = crate::object_document::bitmap_brush::rgba(s).unwrap();
    json!({"kind":"solid","r":rgb[0] as f64/255.,"g":rgb[1] as f64/255.,"b":rgb[2] as f64/255.,"a":rgb[3] as f64/255.})
}
fn record(s: &Value, frame: usize) -> Value {
    let fallback = |key: &str, default: Value| s.get(key).cloned().unwrap_or(default);
    let segments: Vec<_> = s["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            let p = |key: &str| json!({"x":s[key][0],"y":s[key][1]});
            json!({"point":p("point"),"handleIn":p("handleIn"),"handleOut":p("handleOut")})
        })
        .collect();
    let mut vector = json!({"stroke":paint(&s["strokeColor"]),"hasRealStroke":s["hasRealStroke"].as_bool().unwrap_or(s["strokeColor"].is_string()),
        "strokeWidth":fallback("strokeWidth",json!(3)),"strokeCap":fallback("strokeCap",json!("round")),"strokeJoin":fallback("strokeJoin",json!("round")),"opacity":fallback("opacity",json!(1))});
    for key in ["dashArray", "dashOffset", "miterLimit", "paintOrder"] {
        if let Some(v) = s.get(key) {
            vector[key] = v.clone();
        }
    }
    let mut record = json!({"schemaVersion":1,"family":"editable-vector-path","target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":"L0","strokeId":fallback("strokeId",json!("native-dab-id"))},
        "geometry":{"kind":"cubic-path","closed":s["closed"].as_bool().unwrap_or(false),"coordinateSpace":"document-world","handleSpace":"relative","segments":segments},"fill":paint(&s["fillColor"]),"vector":vector});
    let mut texture = json!({});
    for (old, new) in [
        ("brushTexturePreset", "preset"),
        ("brushGroupId", "groupId"),
        ("isBrushTextureCopy", "isCopy"),
        ("preTextureStroke", "preStroke"),
        ("preTextureOpacity", "preOpacity"),
    ] {
        if let Some(value) = s.get(old) {
            texture[new] = value.clone();
        }
    }
    if !texture.as_object().unwrap().is_empty() {
        record["vectorExt"] = json!({"texture":texture});
    }
    record
}
fn document(frames: &Value, v2: bool) -> ObjectDocument {
    let frames = frames.as_array().unwrap();
    let mut objects = vec![];
    let mut scopes = vec![];
    let mut keys = vec![];
    let mut samples = vec![];
    for (i, f) in frames.iter().enumerate() {
        if f["isKeyframe"] == true {
            keys.push(json!({"frame":i}));
        } else if f["isInterpolated"] == true {
            let mut s = json!({"frame":i});
            if let Some(v) = f.get("isManualEdit") {
                s["manualEdit"] = v.clone();
            }
            samples.push(s);
        }
        let strokes = f["strokes"].as_array().unwrap();
        objects.extend(strokes.iter().map(|s| record(s, i)));
        if !strokes.is_empty() {
            scopes.push(json!({"frameScope":{"kind":"authored","frame":i},"elements":strokes.iter().enumerate().map(|(j,s)|json!({"layerUid":"L0","strokeId":s["strokeId"],"naturalPosition":j,"order":{}})).collect::<Vec<_>>()}));
        }
    }
    let mut value = json!({"format":"nemo.native-object-document","formatVersion":if v2{2}else{1},"totalFrames":frames.len(),"layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":keys,"samples":samples}}],"objects":objects});
    if v2 {
        value["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],"scopes":scopes}]});
    }
    serde_json::from_value(value).unwrap()
}
fn head(o: &NativeObjectHistory) -> ObjectDocument {
    o.acquire_snapshot(o.content_revision())
        .unwrap()
        .document()
        .clone()
}
fn bytes(o: &NativeObjectHistory) -> Vec<u8> {
    encode_storage_project(&head(o)).unwrap()
}
fn request(o: &NativeObjectHistory, id: &str, op: &str, payload: Value) -> OpacityRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":id,"instanceId":o.instance_id(),"documentId":o.document_id(),"expectedRevision":o.content_revision(),"operation":format!("command.document.object.{op}"),"payload":payload})).unwrap()
}
fn dispatch(o: &mut NativeObjectHistory, q: &OpacityRequest) -> Value {
    serde_json::to_value(o.try_dispatch_object_command(q).unwrap().unwrap()).unwrap()
}
fn numeric(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Number(a), Value::Number(b)) => assert!(
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-8,
            "{path}: {a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                numeric(a, b, &format!("{path}[{i}]"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len(), "{path}: {a:?} != {b:?}");
            for (k, v) in b {
                numeric(&a[k], v, &format!("{path}.{k}"));
            }
        }
        _ => assert_eq!(actual, expected, "{path}"),
    }
}
fn assert_frames(doc: &ObjectDocument, expected: &Value, name: &str) {
    for (frame, f) in expected.as_array().unwrap().iter().enumerate() {
        let mut records: Vec<_> = doc
            .objects
            .iter()
            .filter(|r| r.target.frame_scope.frame == frame as u32)
            .collect();
        if let Some(order) = doc.composition_order.value() {
            if let Some(scope) = order.contexts[0]
                .scopes
                .iter()
                .find(|s| s.frame_scope.frame == frame as u32)
            {
                records.sort_by_key(|r| {
                    scope
                        .elements
                        .iter()
                        .find(|e| e.stroke_id == r.target.stroke_id)
                        .unwrap()
                        .natural_position
                });
            }
        }
        let wanted = f["strokes"].as_array().unwrap();
        assert_eq!(records.len(), wanted.len(), "{name} frame {frame}");
        for (r, s) in records.into_iter().zip(wanted) {
            let wanted = record(s, frame);
            let mut actual = serde_json::to_value(r).unwrap();
            // Baseline generated dabs have no id; the native record requires a stable scoped id.
            if s.get("strokeId").is_none() {
                actual["target"]["strokeId"] = wanted["target"]["strokeId"].clone();
            }
            numeric(&actual, &wanted, &format!("{name}:{frame}"));
        }
        let exposure = doc.layers[0].timing.value().unwrap().exposure(frame as u32);
        assert_eq!(
            exposure.0,
            if f["isKeyframe"] == true {
                "key"
            } else if f["isInterpolated"] == true {
                "interpolated"
            } else {
                "held"
            },
            "{name}:{frame}"
        );
        assert_eq!(exposure.2, f["isManualEdit"].as_bool().unwrap_or(false));
    }
}
#[test]
fn native_tween_generation_matches_baseline_with_one_undo_redo_and_reopen() {
    for c in fixture_cases() {
        for v2 in [false, true] {
            let input = &c["input"];
            let name = input["name"].as_str().unwrap();
            let mut o = NativeObjectHistory::new("tween", document(&input["frames"], v2)).unwrap();
            let before = bytes(&o);
            let retained = o.acquire_snapshot(0).unwrap();
            let op = if name == "force" {
                "tween.reassign"
            } else {
                "tween.generate"
            };
            let mut payload = json!({"command":format!("object.{op}"),"layerUid":"L0","options":input["options"]});
            if name == "force" {
                payload["span"] = input["span"].clone();
            }
            let q = request(&o, "generate", op, payload);
            let reply = dispatch(&mut o, &q);
            assert_eq!(reply["ok"], true, "{name}: {reply}");
            assert_eq!(o.history_depths(), (1, 0));
            assert_eq!(o.content_revision(), 1);
            assert_frames(&head(&o), &c["expected"], name);
            let after = bytes(&o);
            assert_eq!(dispatch(&mut o, &q), reply);
            assert_eq!(bytes(&o), after);
            assert_eq!(o.history_depths(), (1, 0));
            for (op, want) in [("undo", &before), ("redo", &after)] {
                let q = request(&o, op, op, json!({"command":format!("object.{op}")}));
                assert_eq!(dispatch(&mut o, &q)["ok"], true);
                assert_eq!(&bytes(&o), want);
            }
            assert_eq!(encode_storage_project(retained.document()).unwrap(), before);
            let reopened =
                NativeObjectHistory::new("reopened", decode_storage_project(&after).unwrap())
                    .unwrap();
            assert_eq!(bytes(&reopened), after);
        }
    }
}
