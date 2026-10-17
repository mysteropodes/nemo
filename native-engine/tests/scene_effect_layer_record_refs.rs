//! L08-J reference tests: the legacy matte freeze, the uid remap and the kind-layer constructors against the
//! executed baseline rows (fbe4b8a3). Real codec documents; the controls file restates the same rules as
//! independent mutation models.
use super::oracle_rows::{document_text, layer_from, layer_text, pairs, rows};
use super::*;
use crate::object_codec::{decode_project, encode_project};
use serde_json::{json, Value};

#[test]
fn legacy_matte_sources_are_frozen_exactly_like_the_baseline_loop() {
    for row in rows("freeze") {
        let mut layers: Vec<ObjectLayer> = row["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(layer_from)
            .collect();
        freeze_legacy_matte_sources(&mut layers);
        let got: Vec<Option<&str>> = layers.iter().map(ObjectLayer::matte_source_uid).collect();
        let want: Vec<Option<&str>> = row["sources"]
            .as_array()
            .unwrap()
            .iter()
            .map(Value::as_str)
            .collect();
        assert_eq!(got, want, "{}", row["id"]);
    }
}

#[test]
fn uid_remap_matches_the_baseline_block_and_clears_a_matte_whose_source_is_gone() {
    for row in rows("remap") {
        let map = row["uidMap"].as_object().unwrap().clone();
        let resolve = |uid: &str| map.get(uid).and_then(Value::as_str).map(str::to_string);
        let mut layer = layer_from(&json!({
            "layerUid": "self", "matteMode": "alpha",
            "matteSourceLayerUid": row["input"]["matteSourceLayerUid"],
            "mattesMore": row["input"]["mattesMore"]}));
        remap_compositing_refs(&mut layer, &resolve);
        assert_eq!(
            layer.matte_source_uid(),
            row["matteSourceLayerUid"].as_str(),
            "{}",
            row["id"]
        );
        let got: Vec<(String, String)> = layer
            .mattes_more()
            .iter()
            .map(|m| (m.uid.clone(), m.mode.clone()))
            .collect();
        assert_eq!(got, pairs(&row["mattesMore"]), "{}", row["id"]);
        // The baseline never copies matteMode; native keeps it only while its source survives, and a layer
        // that never had a source keeps the mode it was given.
        if row["input"]["matteSourceLayerUid"].is_string() {
            assert_eq!(
                layer.matte_mode().is_some(),
                row["matteSourceLayerUid"].is_string(),
                "{}",
                row["id"]
            );
        } else {
            assert_eq!(layer.matte_mode(), Some("alpha"), "{}", row["id"]);
        }
    }
}

#[test]
fn remap_is_exact_for_the_identity_and_handles_the_borrowed_stack() {
    let text = document_text(&[
        layer_text(
            "L0",
            r#"{"matteMode":"luma","matteSourceLayerUid":"L1","mattesMore":[{"uid":"L2","mode":"alpha"}],"effectsFrom":"L2"}"#,
        ),
        layer_text("L1", "{}"),
        layer_text("L2", "{}"),
    ]);
    let doc = decode_project(text.as_bytes()).unwrap();
    let mut same = doc.layers[0].clone();
    remap_compositing_refs(&mut same, &|uid: &str| Some(uid.to_string()));
    assert_eq!(same, doc.layers[0]);
    let mut moved = doc.layers[0].clone();
    remap_compositing_refs(&mut moved, &|uid: &str| Some(format!("{uid}'")));
    assert_eq!(moved.compositing_reference_uids(), ["L1'", "L2'", "L2'"]);
    assert_eq!(
        (moved.matte_mode(), moved.mattes_more()[0].mode.as_str()),
        (Some("luma"), "alpha")
    );
    let mut lost = doc.layers[0].clone();
    remap_compositing_refs(&mut lost, &|uid: &str| {
        (uid == "L1").then(|| "N1".to_string())
    });
    assert_eq!(lost.matte_source_uid(), Some("N1"));
    assert!(lost.mattes_more.missing() && lost.effects_from.missing());
}

#[test]
fn effect_and_folder_layer_constructors_match_the_baseline_creation_rows() {
    let mut doc = decode_project(document_text(&[layer_text("L0", "{}")]).as_bytes()).unwrap();
    doc.layers.push(ObjectLayer::blank_effect_layer(
        "E".into(),
        "Effet 1".into(),
        1,
    ));
    doc.layers.push(ObjectLayer::blank_folder_layer(
        "F".into(),
        "Dossier 2".into(),
        2,
    ));
    let value: Value = serde_json::from_slice(&encode_project(&doc).unwrap()).unwrap();
    for row in rows("create") {
        let (layer, fields) = (
            if row["layer"] == "effect" {
                &value["layers"][1]
            } else {
                &value["layers"][2]
            },
            &row["fields"],
        );
        for (key, expected) in fields.as_object().unwrap() {
            if key == "isNullLayer" || key == "nullShape" {
                assert!(
                    layer.get(key).is_none(),
                    "native stores no null-layer record: {key}"
                );
            } else {
                assert_eq!(&layer[key], expected, "{} {key}", row["layer"]);
            }
        }
    }
    assert!(doc.layers[1].is_effect_layer());
    assert_eq!(doc.layers[1].effects().map(<[LayerEffect]>::len), Some(0));
    assert!(doc.layers[2].is_folder_layer() && !doc.layers[2].folder_collapsed());
    assert!(doc.layers[0].compositing_reference_uids().is_empty());
}
