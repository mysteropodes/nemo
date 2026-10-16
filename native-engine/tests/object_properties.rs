//! Literal Motion metadata and identity oracles through the production codecs.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use serde_json::{json, Value};

fn source(version: u32) -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
        "layerUid":"L0","strokeId":"ink"});
    let mut next = record.clone();
    next["target"]["frameScope"]["frame"] = json!(4);
    let mut doc = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":8,"layers":[{"layerUid":"L0"}],"objects":[record,next]});
    if version == 2 {
        let scopes: Vec<_> = [0, 4]
            .iter()
            .map(|frame| {
                json!({
            "frameScope":{"kind":"authored","frame":frame},
            "elements":[{"layerUid":"L0","strokeId":"ink","naturalPosition":0,"order":{}}]})
            })
            .collect();
        doc["compositionOrder"] = json!({"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"migration":"explicit-plan-required","referencePolicy":"unresolved",
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],"scopes":scopes}]});
    }
    doc
}
fn holders() -> Value {
    json!([
        {"target":{"contextId":"scene-root","layerUid":"L0"},
         "controls":[{"key":"xc_wave","name":"Wave","type":"number"},
             {"key":"xc_point","name":"Point","type":"point","default":[12,34]},
             {"key":"xc_color","name":"Color","type":"color"},
             {"key":"xc_on","name":"On","type":"checkbox","default":[1]}],
         "properties":[{"property":"position","static":[1.25,-0.0]},
             {"property":"scale","track":{"keys":[]}},
             {"property":"opacity","static":[75],"track":{"keys":[
                 {"frame":-0.5,"v":[25],"hold":true,"curvePoints":null},
                 {"frame":4,"v":[100],"curvePoints":[{"x":0,"y":0,"tx":0},{"x":1,"y":1}],"hIn":[0,0],"hOut":[0,0]}]},
              "expression":{"enabled":false,"code":"value"}},
             {"property":"xc_point","static":[-5,6]},
             {"property":"xc_on","track":{"keys":[{"frame":0,"v":[1],"hold":true}]}}]},
        {"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"ink"},
         "properties":[{"property":"fillColor","static":[255,12.5,0,255]}]}
    ])
}
fn admitted(value: &Value) -> ObjectDocument {
    decode_storage_project(value.to_string().as_bytes()).unwrap()
}
fn image(doc: &ObjectDocument) -> Value {
    serde_json::from_slice(&encode_storage_project(doc).unwrap()).unwrap()
}

#[test]
fn missing_empty_and_populated_tables_roundtrip_without_invented_values() {
    for version in [1, 2] {
        let original = source(version);
        let absent = admitted(&original);
        assert!(absent.property_holders().is_none());
        assert_eq!(image(&absent), original);
        let mut empty = original.clone();
        empty["propertyHolders"] = json!([]);
        assert_eq!(admitted(&empty).property_holders().unwrap().len(), 0);
        assert_eq!(image(&admitted(&empty)), empty);
        let mut full = original;
        full["propertyHolders"] = holders();
        let doc = admitted(&full);
        assert_eq!(image(&doc), full);
        let bytes = encode_storage_project(&doc).unwrap();
        assert_eq!(
            encode_storage_project(&decode_storage_project(&bytes).unwrap()).unwrap(),
            bytes
        );
        let values = doc.property_holders().unwrap();
        assert_eq!(values[0].target().stroke_id(), None);
        assert_eq!(values[1].target().stroke_id(), Some("ink"));
        assert_eq!(values[1].target().context_id(), "scene-root");
        assert_eq!(values[1].target().layer_uid(), "L0");
    }
}

#[test]
fn metadata_preserves_baseline_dimensions_defaults_units_and_detached_controls() {
    let mut value = source(2);
    value["propertyHolders"] = holders();
    let doc = admitted(&value);
    let holder = &doc.property_holders().unwrap()[0];
    for (name, dim, unit, default, animated) in [
        ("position", 2, "px", json!([0, 0]), true),
        ("scale", 2, "%", json!([100, 100]), true),
        ("rotation", 1, "°", json!([0]), true),
        ("opacity", 1, "%", json!([100]), true),
        ("arcSweep", 1, "°", json!([359.9]), true),
        ("starInner", 1, "%", json!([50]), true),
        ("trimEnd", 1, "%", json!([100]), true),
        ("volume", 1, "", json!([1]), true),
        ("timeLinkInOffset", 1, "f", json!([0]), false),
        ("xc_wave", 1, "", json!([0]), true),
        ("xc_point", 2, "px", json!([12, 34]), true),
        ("xc_color", 4, "", json!([255, 255, 255, 255]), true),
        ("xc_on", 1, "", json!([1]), true),
    ] {
        let meta = holder.metadata(name).unwrap();
        assert_eq!(
            (meta.dimension(), meta.unit(), meta.animatable()),
            (dim, unit, animated),
            "{name}"
        );
        assert_eq!(
            serde_json::to_value(meta.default_value()).unwrap(),
            default,
            "{name}"
        );
    }
    assert!(holder.metadata("unknown").is_none());
    let color = holder.metadata("fillColor").unwrap();
    assert_eq!(color.dimension(), 4);
    assert!(
        color.default_value().is_none(),
        "paint inherits its actual source, not an invented color"
    );
    let mut detached = serde_json::to_value(holder.metadata("xc_point").unwrap()).unwrap();
    detached["defaultValue"] = json!([999, 999]);
    assert_eq!(image(&doc), value);
}

#[test]
fn malformed_dimensions_targets_keys_curves_and_unsupported_records_are_rejected() {
    for version in [1, 2] {
        for (pointer, value) in [
            ("/propertyHolders", Value::Null),
            ("/propertyHolders/0/target/strokeId", Value::Null),
            ("/propertyHolders/0/target/layerUid", json!("missing")),
            ("/propertyHolders/0/target/contextId", json!("missing")),
            ("/propertyHolders/1/target/strokeId", json!("missing")),
            ("/propertyHolders/0/properties/0/static", json!([1])),
            ("/propertyHolders/0/properties/0/static", json!([1, null])),
            ("/propertyHolders/0/properties/0/property", json!("order")),
            (
                "/propertyHolders/0/properties/0/property",
                json!("timeRemap"),
            ),
            ("/propertyHolders/0/properties/0/property", json!("typo")),
            (
                "/propertyHolders/0/properties/2/track/keys/1/frame",
                json!(-0.5),
            ),
            (
                "/propertyHolders/0/properties/2/track/keys/1/hIn",
                json!([0]),
            ),
            (
                "/propertyHolders/0/properties/2/track/keys/1/curvePoints",
                json!([{"x":0,"y":0},{"x":0,"y":1}]),
            ),
            ("/propertyHolders/0/controls/1/default", json!([0])),
            ("/propertyHolders/0/controls/1/type", json!("unknown")),
            ("/propertyHolders/0/controls/1/key", json!("position")),
            (
                "/propertyHolders/0/controls/1/name",
                json!(" \u{feff}WAVE\u{feff} "),
            ),
        ] {
            let mut doc = source(version);
            doc["propertyHolders"] = holders();
            if pointer.ends_with("/strokeId") && pointer.contains("/0/") {
                doc["propertyHolders"][0]["target"]["strokeId"] = value;
            } else {
                *doc.pointer_mut(pointer).unwrap() = value;
            }
            assert!(
                decode_storage_project(doc.to_string().as_bytes()).is_err(),
                "{pointer}: {doc}"
            );
        }
        let mut doc = source(version);
        doc["propertyHolders"] = holders();
        doc["propertyHolders"][0]["properties"][0] =
            json!({"property":"timeLinkInOffset","track":{"keys":[]}});
        assert!(decode_storage_project(doc.to_string().as_bytes()).is_err());
        let mut overflowing = source(version);
        overflowing["propertyHolders"] = holders();
        let original = overflowing.to_string();
        let raw = original.replace("[1.25,-0.0]", "[1e400,0]");
        assert_ne!(raw, original);
        assert!(decode_storage_project(raw.as_bytes()).is_err());
    }
}

#[test]
fn duplicate_json_members_and_holder_property_control_identities_never_overwrite() {
    for version in [1, 2] {
        let mut doc = source(version);
        doc["propertyHolders"] = holders();
        for (name, value) in [
            ("target", json!({"contextId":"scene-root","layerUid":"L0"})),
            ("property", json!("position")),
            ("static", json!([1.25, -0.0])),
            ("track", json!({"keys":[]})),
            ("controls", holders()[0]["controls"].clone()),
            ("strokeId", json!("ink")),
        ] {
            let index = usize::from(name == "strokeId");
            let raw = doc["propertyHolders"][index].to_string();
            let marker = format!("\"{name}\":");
            // Both values are valid: only actual duplicate rejection can fail this case.
            let duplicate = raw.replacen(&marker, &format!("\"{name}\":{value},{marker}"), 1);
            assert_ne!(duplicate, raw);
            assert!(
                serde_json::from_str::<PropertyHolder>(&duplicate).is_err(),
                "{name}"
            );
        }
        let raw = doc.to_string().replacen(
            "\"propertyHolders\":",
            "\"propertyHolders\":[],\"propertyHolders\":",
            1,
        );
        assert!(decode_storage_project(raw.as_bytes()).is_err());
        for pointer in [
            "/propertyHolders",
            "/propertyHolders/0/properties",
            "/propertyHolders/0/controls",
        ] {
            let mut duplicate = doc.clone();
            let list = duplicate
                .pointer_mut(pointer)
                .unwrap()
                .as_array_mut()
                .unwrap();
            list.push(list[0].clone());
            assert!(
                decode_storage_project(duplicate.to_string().as_bytes()).is_err(),
                "{pointer}"
            );
        }
    }
}

#[cfg(feature = "history")]
#[test]
fn existing_fill_owner_history_and_immutable_pins_retain_holder_values() {
    use crate::history::object_history::NativeObjectHistory;
    for version in [1, 2] {
        let mut value = source(version);
        value["propertyHolders"] = holders();
        let mut owner = NativeObjectHistory::new("properties", admitted(&value)).unwrap();
        let before = owner.acquire_snapshot(0).unwrap();
        let payload = json!({"command":"object.fill.set","stableTarget":value["objects"][0]["target"],
            "fill":{"kind":"solid","r":0,"g":1,"b":0,"a":1}});
        let edit = owner
            .prepare_fill_json(payload.to_string().as_bytes())
            .unwrap();
        owner.commit_fill(edit).unwrap();
        let after = owner.acquire_snapshot(1).unwrap();
        assert_eq!(
            image(after.document())["propertyHolders"],
            value["propertyHolders"]
        );
        owner.undo(1).unwrap();
        assert_eq!(image(owner.acquire_snapshot(2).unwrap().document()), value);
        owner.redo(2).unwrap();
        assert_eq!(
            image(owner.acquire_snapshot(3).unwrap().document()),
            image(after.document())
        );
        assert_eq!(image(before.document()), value);
        assert_eq!(owner.history_depths(), (1, 0));
    }
}

#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[test]
fn full_project_save_reopen_and_retained_reads_preserve_control_and_holder_presence() {
    use crate::history::object_history::NativeObjectHistory;
    for version in [1, 2] {
        let mut drawing = source(version);
        drawing["propertyHolders"] = holders();
        let bytes = json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":320,"canvasH":180,"fps":29.97},"drawing":drawing,
            "viewportReference":null,"assets":[]})
        .to_string()
        .into_bytes();
        let owner = NativeObjectHistory::from_project_file("properties-file", &bytes).unwrap();
        let pinned = owner.acquire_snapshot(0).unwrap();
        let saved = pinned.serialize_project_file().unwrap();
        let reopened =
            NativeObjectHistory::from_project_file("properties-reopened", &saved).unwrap();
        let next = reopened.acquire_snapshot(0).unwrap();
        assert_eq!(next.serialize_project_file().unwrap(), saved);
        assert_eq!(image(next.document()), drawing);
        assert_ne!(owner.document_id(), reopened.document_id());
        drop(owner);
        assert_eq!(pinned.serialize_project_file().unwrap(), saved);
    }
}
