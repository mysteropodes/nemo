//! Handwritten bulk cell fixtures. The existing dense builder/parser supplies admitted
//! records, never expected results from the new move/shift implementation.
use super::{build, rows};
use crate::commands::ObjectFillOwner;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

pub(super) fn document(text: &str, form: build::Build) -> ObjectDocument {
    build::document(&rows::parse_layers(text), form)
}
pub(super) fn admit(value: Value) -> ObjectDocument {
    decode_storage_project(value.to_string().as_bytes()).expect("literal bulk fixture is admitted")
}
pub(super) fn value(document: &ObjectDocument) -> Value {
    serde_json::to_value(document).unwrap()
}
pub(super) fn owner(document: ObjectDocument) -> ObjectFillOwner {
    ObjectFillOwner::new("bulk-edit", document).unwrap()
}
pub(super) fn head(owner: &ObjectFillOwner) -> ObjectDocument {
    owner
        .acquire_snapshot(owner.content_revision())
        .unwrap()
        .document()
        .clone()
}
pub(super) fn bytes(owner: &ObjectFillOwner) -> Vec<u8> {
    encode_storage_project(&head(owner)).unwrap()
}
pub(super) fn moving(selection: &[(&str, u32)], layer: &str, frame: u32) -> Value {
    json!({"command":"object.frames.move","selection":selection.iter().map(|(layer,frame)|
        json!({"layerUid":layer,"frame":frame})).collect::<Vec<_>>(),"layerUid":layer,"frame":frame})
}
pub(super) fn shifting(layer: &str, delta: i64) -> Value {
    json!({"command":"object.layer.frames.shift","layerUid":layer,"delta":delta})
}
/// Strip exactly the root content/timing destinations tested independently below.
/// All optional presence, layer metadata, Reference/other-context values and holders remain.
fn immutable(document: &ObjectDocument) -> Value {
    let mut v = value(document);
    for layer in v["layers"].as_array_mut().unwrap() {
        layer.as_object_mut().unwrap().remove("timing");
    }
    v["objects"].as_array_mut().unwrap().retain(|record| {
        record["target"]["contextId"] != "scene-root"
            || record["target"]["frameScope"]["kind"] != "authored"
    });
    if let Some(contexts) = v
        .get_mut("compositionOrder")
        .and_then(|v| v.get_mut("contexts"))
        .and_then(Value::as_array_mut)
    {
        for context in contexts
            .iter_mut()
            .filter(|c| c["contextId"] == "scene-root")
        {
            context["scopes"] = json!([]);
        }
    }
    v
}
fn authored_records(document: &ObjectDocument) -> Vec<String> {
    let mut records = value(document)["objects"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|record| {
            record["target"]["contextId"] == "scene-root"
                && record["target"]["frameScope"]["kind"] == "authored"
        })
        .map(|record| serde_json::to_string(record).unwrap())
        .collect::<Vec<_>>();
    records.sort();
    records
}
pub(super) fn assert_rows(
    input: &ObjectDocument,
    output: &ObjectDocument,
    literal: &str,
    form: build::Build,
) {
    let cells = rows::parse_layers(literal);
    let expected = build::document(&cells, form);
    let encoded = encode_storage_project(output).unwrap();
    assert_eq!(
        value(&decode_storage_project(&encoded).unwrap()),
        value(output),
        "storage roundtrip retains full output"
    );
    assert_eq!(output.total_frames, input.total_frames);
    assert_eq!(
        immutable(output),
        immutable(input),
        "everything outside root content/timing survives"
    );
    assert_eq!(
        authored_records(output),
        authored_records(&expected),
        "full geometry/fill/identity, not just names"
    );
    for (layer, row) in cells.iter().enumerate() {
        assert_eq!(
            build::render_layer(output, layer),
            literal.split('|').nth(layer).unwrap()
        );
        assert_eq!(
            value(output)["layers"][layer]["timing"],
            build::timing_json(row)
        );
        if form.v2() {
            for (frame, cell) in row.iter().enumerate() {
                assert_eq!(
                    build::element_triples(output, &build::uid(layer), frame as u32),
                    build::expected_triples(cell),
                    "literal authored order at layer{layer}/frame{frame}"
                );
            }
        }
    }
}
pub(super) fn with_holder(document: ObjectDocument, stroke: bool) -> ObjectDocument {
    let mut v = value(&document);
    let mut target = json!({"contextId":"scene-root","layerUid":"L0"});
    if stroke {
        target["strokeId"] = json!("a");
    }
    v["propertyHolders"] = json!([{"target":target,"properties":[{"property":"rotation",
        "track":{"keys":[{"frame":1,"v":[13]}]}}]}]);
    admit(v)
}
/// Canonicalize only record storage order and root scope-list placement. Authored
/// element order, naturalPosition, tracks and every DTO field remain exact.
pub(super) fn canonical(mut v: Value) -> Value {
    if let Some(objects) = v.get_mut("objects").and_then(Value::as_array_mut) {
        objects.sort_by_key(|record| serde_json::to_string(record).unwrap());
    }
    if let Some(contexts) = v
        .get_mut("compositionOrder")
        .and_then(|v| v.get_mut("contexts"))
        .and_then(Value::as_array_mut)
    {
        for context in contexts
            .iter_mut()
            .filter(|c| c["contextId"] == "scene-root")
        {
            context["scopes"]
                .as_array_mut()
                .unwrap()
                .sort_by_key(|scope| scope["frameScope"]["frame"].as_u64().unwrap());
        }
    }
    v
}

pub(super) fn malformed_payloads() -> Vec<Value> {
    let mut cases = vec![
        Value::Null,
        json!([]),
        json!({}),
        moving(&[("L0", 0), ("L0", 0)], "L0", 1),
        json!({"command":"object.frames.move","selection":null,"layerUid":"L0","frame":1}),
        json!({"command":"object.frames.move","selection":[{"layerUid":"L0","frame":0,"extra":0}],"layerUid":"L0","frame":1}),
    ];
    for (field, bad) in [
        ("layerUid", json!("")),
        ("frame", json!(-1)),
        ("frame", json!(0.5)),
        ("frame", json!(4294967296u64)),
        ("extra", json!(true)),
        ("command", json!("object.frames.unknown")),
    ] {
        let mut value = moving(&[("L0", 0)], "L0", 1);
        value[field] = bad;
        cases.push(value);
    }
    for delta in [
        Value::Null,
        json!(true),
        json!("1"),
        json!(0.5),
        json!(9007199254740992i64),
        json!(-9007199254740992i64),
    ] {
        let mut value = shifting("L0", 1);
        value["delta"] = delta;
        cases.push(value);
    }
    cases
}

/// One literal fixture expectation: root L0/frame0 -> L1/frame1.
/// This is not the bulk move implementation or a general mapping oracle.
pub(super) fn expected_mixed_move(input: &ObjectDocument) -> Value {
    let mut expected = value(input);
    expected["layers"][0]["timing"] = build::timing_json(&rows::parse_layers(".;.;.;K=b.2.20")[0]);
    expected["layers"][1]["timing"] =
        build::timing_json(&rows::parse_layers("K=c.3.30;K;.;K=d.4.40")[0]);
    for record in expected["objects"].as_array_mut().unwrap() {
        let target = &mut record["target"];
        if target["contextId"] == "scene-root"
            && target["layerUid"] == "L0"
            && target["frameScope"]["kind"] == "authored"
            && target["frameScope"]["frame"] == 0
        {
            target["layerUid"] = json!("L1");
            target["frameScope"]["frame"] = json!(1);
        }
    }
    for kind in ["raster", "text"] {
        expected["items"][kind][0]["target"]["layerUid"] = json!("L1");
        expected["items"][kind][0]["target"]["frameScope"]["frame"] = json!(1);
    }
    let scopes = expected["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .unwrap();
    let original = scopes
        .iter_mut()
        .find(|scope| scope["frameScope"]["frame"] == 0)
        .unwrap();
    let mut moved = original["elements"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["layerUid"] == "L0")
        .cloned()
        .collect::<Vec<_>>();
    original["elements"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["layerUid"] != "L0");
    for element in &mut moved {
        element["layerUid"] = json!("L1");
    }
    scopes.push(json!({"frameScope":{"kind":"authored","frame":1},"elements":moved}));
    expected
}

/// A genuinely malformed input for the existing storage guard. Never passed through admit().
pub(super) fn empty_item_table_wire(kind: &str) -> Value {
    let mut raw = value(&super::f7::mixed_items());
    raw["items"][kind] = json!([]);
    raw
}
