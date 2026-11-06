//! #2371 D36: a guide layer reads as `guide`, a folder (also a Null by its pivot) as `folder`, a Null as `null`; none carries a
//! member another kind owns.
use super::layer_members;
use crate::object_codec::decode_storage_project;
use serde_json::json;

#[test]
fn the_layer_kind_of_a_content_free_layer_follows_its_stored_flags() {
    let layer = |uid: &str, extra: serde_json::Value| {
        let mut row = json!({"layerUid":uid,"name":uid});
        for (key, member) in extra.as_object().unwrap() {
            row[key] = member.clone();
        }
        row
    };
    let document = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4,"objects":[],
        "layers":[
            layer("guide", json!({"isGuideLayer":true,"guidePos":[10,20],"guideOrientation":"horizontal"})),
            layer("folder", json!({"isNullLayer":true,"nullPos":[1,2],"isFolderLayer":true,"folderCollapsed":false})),
            layer("null", json!({"isNullLayer":true,"nullPos":[1,2]})),
            layer("plain", json!({}))]});
    let document =
        decode_storage_project(document.to_string().as_bytes()).expect("admissible fixture");
    let kinds: Vec<String> = layer_members(&document)
        .iter()
        .map(|row| row["kind"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(kinds, ["guide", "folder", "null", "draw"]);
    let with_objects = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":4,
        "layers":[layer("guide", json!({"isGuideLayer":true,"guidePos":[10,20],"guideOrientation":"horizontal"}))],
        "objects":[{"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
            "target":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"guide","strokeId":"s"},
            "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world","handleSpace":"relative","segments":[]},
            "fill":{"kind":"solid","r":0,"g":0,"b":0,"a":1}}]});
    assert!(
        decode_storage_project(with_objects.to_string().as_bytes()).is_err(),
        "a guide carries no objects"
    );
}
