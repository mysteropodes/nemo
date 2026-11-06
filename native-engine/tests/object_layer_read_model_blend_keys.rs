//! #2257: the hold-only blend keys of a layer row. Sibling of `object_layer_read_model.rs`.
use super::layer_members;
use crate::object_codec::decode_storage_project;
use serde_json::{json, Value};

fn rows(layers: Value) -> Vec<serde_json::Map<String, Value>> {
    let document = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4,
        "layers":layers,"objects":[]});
    layer_members(&decode_storage_project(document.to_string().as_bytes()).expect("admissible fixture"))
}

#[test]
fn hold_only_blend_keys_are_listed_in_frame_order_and_a_long_list_names_its_oversize() {
    let keys = |count: u32| -> Vec<Value> {
        (0..count)
            .map(|frame| json!({"frame": frame, "mode": "screen"}))
            .collect()
    };
    let rows = rows(json!([
        {"layerUid":"plain"},
        {"layerUid":"keyed","blendMode":"multiply","blendKeys":[{"frame":2,"mode":"hue"},{"frame":9,"mode":"normal"}]},
        {"layerUid":"edge","blendKeys":keys(64)},
        {"layerUid":"long","blendKeys":keys(65)}]));
    assert!(rows[0].get("blendKeys").is_none() && rows[0].get("blendKeysOversize").is_none());
    assert_eq!(
        rows[1]["blendKeys"],
        json!([{"frame":2,"mode":"hue"},{"frame":9,"mode":"normal"}])
    );
    assert_eq!(rows[1]["blendMode"], json!("multiply"), "the static blend is still listed beside the keys");
    assert_eq!(rows[2]["blendKeys"].as_array().unwrap().len(), 64);
    assert_eq!(rows[3]["blendKeysOversize"], json!(true));
    assert!(rows[3].get("blendKeys").is_none());
}
