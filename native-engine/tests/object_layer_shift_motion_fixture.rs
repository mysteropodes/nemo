// Included in the original test module: imports, fixture/support items and
// any baseline fixture-admission test keep their exact lexical namespace.
use super::{locked_keys, shift_layer, shift_selected, validate_selected, SelectedMotionKey};
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::{KeyLockMode, ObjectDocument};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
use build::{Build, Cell, Stroke};

fn fixture() -> Value {
    let row: Vec<Cell> = (0..10)
        .map(|frame| Cell {
            flag: if frame == 0 || frame == 3 { 'K' } else { '.' },
            strokes: match frame {
                0 => vec![Stroke {
                    id: "a".into(),
                    z: 1,
                    g: 10,
                }],
                3 => vec![Stroke {
                    id: "b".into(),
                    z: 2,
                    g: 20,
                }],
                _ => vec![],
            },
        })
        .collect();
    let mut empty = row.clone();
    for cell in &mut empty {
        cell.strokes.clear();
    }
    let mut value = build::document_json(&[row, empty], Build::V2Extras);
    value["layers"][0]["locked"] = json!(true);
    value["layers"][0]["keyLock"] = json!("layer");
    value["motionArcs"] = json!({"L0:0-3-0":{"out":[1,2]},"opaque":null});
    value["propertyHolders"] = json!([
        {"target":{"contextId":"scene-root","layerUid":"L0"},"properties":[
            {"property":"opacity","static":[17],"expression":{"enabled":false,"code":"7"},"track":{"keys":[
                {"frame":-2.5,"v":[1],"hold":true,"curvePoints":null},
                {"frame":0,"v":[2],"lockTo":"in","hIn":[3,4],"hOut":[5,6]},
                {"frame":2.25,"v":[3],"lockTo":null,"curvePoints":[]},
                {"frame":8,"v":[4],"lockTo":"out"},
                {"frame":12,"v":[5],"lockTo":"layer"}]}},
            {"property":"rotation","static":[90],"track":{"keys":[]}}]},
        {"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"a"},"properties":[
            {"property":"rotation","track":{"keys":[{"frame":1,"v":[15],"lockTo":"in"},{"frame":9,"v":[25],"lockTo":"layer"}]}}]},
        {"target":{"contextId":"ctx-x","layerUid":"L0","strokeId":"x0"},"properties":[
            {"property":"opacity","static":[11],"track":{"keys":[{"frame":0.5,"v":[40]},{"frame":4.5,"v":[60]}]}}]}
    ]);
    value["compositionOrder"]["contexts"][0]["layers"][0]["order"] = json!({"static":[5],
        "expression":{"enabled":false,"code":"9"},"track":{"keys":[{"frame":0,"v":[1],"lockTo":"in"},{"frame":6,"v":[2],"lockTo":"out"}]}});
    value["compositionOrder"]["contexts"][0]["scopes"][0]["elements"][0]["order"]["track"] =
        json!({"keys":[{"frame":2,"v":[7],"lockTo":"layer"},{"frame":8.5,"v":[8],"lockTo":null}]});
    value["compositionOrder"]["contexts"][1]["layers"][0]["order"]["track"] =
        json!({"keys":[{"frame":1,"v":[3]}]});
    value["compositionOrder"]["contexts"][1]["scopes"][0]["elements"][0]["order"]["track"] =
        json!({"keys":[{"frame":3,"v":[4]},{"frame":5,"v":[5]}]});
    let inside = build::record_json("symbol", "authored", "inside", "inner", 0, 3, 30);
    value["componentTables"] = json!({"components":{"format":"nemo.native-component-set","formatVersion":1,
        "components":[{"symbolId":"symbol","totalFrames":6,"layers":[{"layerUid":"inside"}],"objects":[inside]}],
        "instances":[{"contextId":"scene-root","layerUid":"L1","symbolId":"symbol","symSpeed":2,
            "timeRemap":{"keys":[{"frame":0,"v":[1],"lockTo":"in"},{"frame":1,"v":[2]},{"frame":8,"v":[4]}]}}]}});
    value
}
fn admit(value: Value) -> ObjectDocument {
    decode_storage_project(value.to_string().as_bytes())
        .expect("actual native Motion fixture admission")
}
fn bytes(document: &ObjectDocument) -> Vec<u8> {
    encode_storage_project(document).unwrap()
}
fn wire(document: &ObjectDocument) -> Value {
    serde_json::from_slice(&bytes(document)).unwrap()
}
fn key(target: Value, frame: Value) -> SelectedMotionKey {
    serde_json::from_value(json!({"target":target,"frame":frame})).unwrap()
}
fn property(context: &str, layer: &str, stroke: Option<&str>, property: &str) -> Value {
    let mut target = json!({"contextId":context,"layerUid":layer});
    if let Some(stroke) = stroke {
        target["strokeId"] = json!(stroke);
    }
    json!({"kind":"property","target":target,"property":property})
}
fn layer_order(context: &str, layer: &str) -> Value {
    json!({"kind":"layerOrder","contextId":context,"layerUid":layer})
}
fn element_order(context: &str, layer: &str, frame: u32, stroke: &str) -> Value {
    json!({"kind":"elementOrder","contextId":context,"layerUid":layer,"frameScope":{"kind":"authored","frame":frame},"strokeId":stroke})
}
fn remap() -> Value {
    json!({"kind":"timeRemap","contextId":"scene-root","layerUid":"L1"})
}
fn set_frames(value: &mut Value, path: &str, frames: &[f64]) {
    let keys = value.pointer_mut(path).unwrap().as_array_mut().unwrap();
    assert_eq!(keys.len(), frames.len());
    for (key, frame) in keys.iter_mut().zip(frames) {
        key["frame"] = json!(frame);
    }
}
const PROPERTY: &str = "/propertyHolders/0/properties/0/track/keys";
const ELEMENT: &str = "/propertyHolders/1/properties/0/track/keys";
const LAYER_ORDER: &str = "/compositionOrder/contexts/0/layers/0/order/track/keys";
const ELEMENT_ORDER: &str = "/compositionOrder/contexts/0/scopes/0/elements/0/order/track/keys";
const REMAP: &str = "/componentTables/components/instances/0/timeRemap/keys";
