//! D37: a layer's standing key-lock mode is a native read; absent and explicit null read as none.
use super::layer_members;
use crate::object_codec::decode_storage_project;
use serde_json::json;

#[test]
fn the_stored_key_lock_mode_is_reported_and_none_is_omitted() {
    let document = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4,"objects":[],
        "layers":[{"layerUid":"a","keyLock":"in"},{"layerUid":"b","keyLock":"out"},{"layerUid":"c","keyLock":"layer"},
            {"layerUid":"d","keyLock":null},{"layerUid":"e"}]});
    let rows = layer_members(&decode_storage_project(document.to_string().as_bytes()).unwrap());
    let modes: Vec<_> = rows.iter().map(|row| row.get("keyLock").cloned()).collect();
    assert_eq!(
        modes,
        vec![
            Some(json!("in")),
            Some(json!("out")),
            Some(json!("layer")),
            None,
            None
        ]
    );
}
