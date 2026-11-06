//! #2520/#2525: the baseline 3D layer switch is a native row fact, reported only while it is on.
use super::layer_members;
use crate::object_codec::decode_storage_project;
use serde_json::json;

#[test]
fn the_3d_switch_is_reported_only_while_it_is_on() {
    let document = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4,"objects":[],
        "layers":[{"layerUid":"a","threeD":true},{"layerUid":"b","threeD":false},{"layerUid":"c"}]});
    let rows = layer_members(&decode_storage_project(document.to_string().as_bytes()).unwrap());
    let flags: Vec<_> = rows.iter().map(|row| row.get("threeD").cloned()).collect();
    assert_eq!(flags, vec![Some(json!(true)), None, None]);
}
