//! G39 (#2376): the Motion preset apply is one owner transaction with one exact history entry.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};
fn target(layer: &str) -> Value {
    json!({"contextId":"scene-root","layerUid":layer})
}
fn fixture() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":40,
    "layers":[{"layerUid":"A","name":"first"},{"layerUid":"B","name":"second"}],"objects":[],
    "propertyHolders":[
      {"target":target("A"),"properties":[
        {"property":"opacity","static":[17]},
        {"property":"rotation","track":{"keys":[{"frame":0,"v":[0]},{"frame":12,"v":[90]}]}}]}
    ]})
}
fn from(value: Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "g39-owner",
        decode_storage_project(value.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn wire(o: &NativeObjectHistory) -> Value {
    let doc = o.acquire_snapshot(o.content_revision()).unwrap();
    serde_json::from_slice(&encode_storage_project(doc.document()).unwrap()).unwrap()
}
fn holder<'a>(w: &'a Value, layer: &str) -> Option<&'a Value> {
    w["propertyHolders"]
        .as_array()?
        .iter()
        .find(|h| h["target"]["layerUid"] == layer)
}
fn payload(layer: &str, properties: Value) -> Value {
    json!({"command":"object.property.preset.apply","target":target(layer),"properties":properties})
}
fn apply(o: &mut NativeObjectHistory, p: Value) -> Result<u64, DispatchErrorCode> {
    let candidate = o
        .owner
        .prepare_property_preset_apply_json(p.to_string().as_bytes())?;
    o.commit_property_edit(candidate)
}
fn preset() -> Value {
    json!([
      {"property":"position","keys":[
        {"frame":0,"v":[10,20],"curvePoints":[{"x":0,"y":0},{"x":1,"y":1}],"hIn":[0,0],"hOut":[1,2]},
        {"frame":24,"v":[300,40],"hold":true}]},
      {"property":"rotation","keys":[{"frame":0,"v":[5]},{"frame":24,"v":[45]}]},
      {"property":"opacity","keys":[],"static":[75]}
    ])
}
#[test]
fn apply_to_another_layer_writes_every_key_in_one_history_entry_and_undo_is_exact() {
    let initial = fixture();
    let mut o = from(initial.clone());
    assert_eq!(o.history_depths(), (0, 0));
    assert_eq!(apply(&mut o, payload("B", preset())).unwrap(), 1);
    assert_eq!(o.history_depths(), (1, 0), "one entry for three properties");
    let w = wire(&o);
    let b = holder(&w, "B").expect("the target layer gained its holder");
    let by = |name: &str| {
        b["properties"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["property"] == name)
            .unwrap()
            .clone()
    };
    let position = by("position")["track"]["keys"].clone();
    assert_eq!(position[0]["v"], json!([10, 20]));
    assert_eq!(position[0]["hOut"], json!([1, 2]));
    assert_eq!(position[1]["hold"], json!(true));
    assert_eq!(by("rotation")["track"]["keys"][1]["v"], json!([45]));
    assert_eq!(by("opacity")["static"], json!([75]));
    assert!(
        by("opacity").get("track").is_none(),
        "no empty track is kept"
    );
    assert_eq!(
        holder(&w, "A"),
        holder(&initial, "A"),
        "another layer is untouched"
    );
    assert_eq!(o.undo(1).unwrap(), 2);
    assert_eq!(wire(&o), initial, "undo restores the exact document");
    assert_eq!(o.redo(2).unwrap(), 3);
    assert_eq!(wire(&o), w, "redo restores the exact edit");
}
#[test]
fn applying_replaces_the_listed_properties_like_the_baseline_object_replacement() {
    let mut o = from(fixture());
    // Layer A animates rotation and holds a static opacity; the preset keys position and fixes opacity at 75.
    assert_eq!(apply(&mut o, payload("A", preset())).unwrap(), 1);
    let w = wire(&o);
    let a = holder(&w, "A").unwrap()["properties"]
        .as_array()
        .unwrap()
        .clone();
    let rot = a.iter().find(|r| r["property"] == "rotation").unwrap();
    assert_eq!(
        rot["track"]["keys"][0]["v"],
        json!([5]),
        "rotation keys replaced, not merged"
    );
    assert_eq!(rot["track"]["keys"].as_array().unwrap().len(), 2);
    let op = a.iter().find(|r| r["property"] == "opacity").unwrap();
    assert_eq!(op["static"], json!([75]));
    // A listed property with neither keys nor static is cleared, and an empty holder is dropped.
    let clear = payload(
        "A",
        json!([
        {"property":"position","keys":[]},{"property":"rotation","keys":[]},{"property":"opacity","keys":[]}]),
    );
    assert_eq!(apply(&mut o, clear).unwrap(), 2);
    assert!(holder(&wire(&o), "A").is_none());
    assert_eq!(o.history_depths(), (2, 0));
}
#[test]
fn an_identical_preset_adds_no_history_entry() {
    let mut o = from(fixture());
    assert_eq!(apply(&mut o, payload("B", preset())).unwrap(), 1);
    assert_eq!(
        apply(&mut o, payload("B", preset())).unwrap(),
        1,
        "no new revision"
    );
    assert_eq!(o.history_depths(), (1, 0));
}
#[test]
fn a_refused_preset_publishes_nothing() {
    let mut o = from(fixture());
    let bad = [
        payload(
            "B",
            json!([{"property":"position","keys":[{"frame":0,"v":[1]}]}]),
        ), // wrong dimension
        payload(
            "B",
            json!([{"property":"rotation","keys":[{"frame":4,"v":[1]},{"frame":4,"v":[2]}]}]),
        ), // duplicate frame
        payload(
            "B",
            json!([{"property":"rotation","keys":[{"frame":8,"v":[1]},{"frame":4,"v":[2]}]}]),
        ), // unsorted
        payload("B", json!([{"property":"order","keys":[]}])), // another owner
        payload(
            "B",
            json!([{"property":"rotation","keys":[]},{"property":"rotation","keys":[]}]),
        ), // repeated
        payload("B", json!([])),
        payload(
            "B",
            json!([{"property":"rotation","keys":[{"frame":0,"v":[1]}],"extra":1}]),
        ),
        payload(
            "B",
            json!([{"property":"opacity","keys":[],"static":[1,2]}]),
        ),
    ];
    for p in bad {
        assert_eq!(
            apply(&mut o, p.clone()).unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{p}"
        );
    }
    assert_eq!(
        apply(&mut o, payload("MISSING", preset())).unwrap_err(),
        DispatchErrorCode::NotFound
    );
    // One bad property refuses the whole preset: no partial publication.
    let mixed = payload(
        "B",
        json!([
        {"property":"rotation","keys":[{"frame":0,"v":[1]}]},
        {"property":"opacity","keys":[],"static":[1,2]}]),
    );
    assert!(apply(&mut o, mixed).is_err());
    assert_eq!(o.history_depths(), (0, 0));
    assert_eq!(wire(&o), fixture());
}
/// Every layer kind owns all five transform properties: the catalog and the holder admission do not look at the layer
/// kind (baseline `propsFor` lists the same `PROPS` for every layer holder), so a preset applies to Null, Folder,
/// Effect, 3D and folder-member layers alike, never turning the whole apply Unavailable.
#[test]
fn a_preset_applies_to_every_layer_kind_with_all_five_transform_properties() {
    let all = json!([
      {"property":"position","keys":[{"frame":0,"v":[1,2]}]},
      {"property":"anchor","keys":[],"static":[3,4]},
      {"property":"rotation","keys":[{"frame":0,"v":[5]},{"frame":10,"v":[9]}]},
      {"property":"scale","keys":[],"static":[50,60]},
      {"property":"opacity","keys":[{"frame":0,"v":[40]}]}
    ]);
    let layers = json!([
      {"layerUid":"null","name":"null","isNullLayer":true,"nullPos":[0,0]},
      {"layerUid":"folder","name":"folder","isFolderLayer":true},
      {"layerUid":"member","name":"member","folderId":"folder"},
      {"layerUid":"effect","name":"effect","isEffectLayer":true},
      {"layerUid":"d3","name":"d3","threeD":true}
    ]);
    let doc = json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":40,
      "layers":layers,"layerFolders":{"folder":{"name":"folder"}},"objects":[]});
    let mut o = from(doc);
    for (n, layer) in ["null", "folder", "member", "effect", "d3"]
        .iter()
        .enumerate()
    {
        assert_eq!(
            apply(&mut o, payload(layer, all.clone())).unwrap(),
            n as u64 + 1,
            "{layer}"
        );
        let w = wire(&o);
        let props = holder(&w, layer).unwrap()["properties"].as_array().unwrap();
        assert_eq!(props.len(), 5, "{layer}: all five properties written");
    }
    assert_eq!(o.history_depths(), (5, 0));
}
