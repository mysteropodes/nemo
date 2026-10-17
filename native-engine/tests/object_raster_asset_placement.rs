//! Holder queries of Image asset placements (L06-I, G02 / G27) and the shared builders of the sibling test files.
//!
//! Documents are decoded by the real storage codec (so raster items and text blocks are the real L06-L records) and
//! catalogs are built with the real `AssetInventory` constructor. The oracle (`fixtures/object-raster-asset-placement/
//! oracle.txt`, generator `tools/gen.js`) holds the baseline media-library holder binding; its tests are in `join`.

use super::*;
use crate::object_codec::decode_storage_project;
use crate::object_document::{AssetKind, AssetRecord, AssetSource};
use serde_json::{json, Value};
use std::sync::Arc;

#[path = "object_raster_asset_placement_controls.rs"]
mod controls;
#[path = "object_raster_asset_placement_flow.rs"]
mod flow;
#[path = "object_raster_asset_placement_join.rs"]
mod join;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-raster-asset-placement/oracle.txt");
const TEXT: &str = r##"{"text":"Hi","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":48,"color":"#112233","align":"left","anchorTopLeft":{"x":10,"y":20}}"##;

/// The oracle rows with this tag, split on tabs.
pub(crate) fn rows(tag: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|line| line.starts_with(&format!("{tag}\t")))
        .map(|line| line.split('\t').collect())
        .collect()
}
/// Base64 of the named valid PNG row (`blue` or `red`).
pub(crate) fn png_base64(name: &str) -> &'static str {
    rows("P").into_iter().find(|row| row[1] == name).unwrap()[2]
}

pub(crate) fn id(text: &str) -> AssetId {
    AssetId::new(text.to_owned()).unwrap()
}
pub(crate) fn image_record(asset: &str) -> Arc<AssetRecord> {
    let source =
        AssetSource::admitted_embedded("image/png".to_owned(), Arc::new(vec![1, 2, 3]), 64)
            .unwrap();
    Arc::new(AssetRecord::new(id(asset), "plate".to_owned(), AssetKind::Image, source).unwrap())
}
pub(crate) fn linked_record(kind: AssetKind, asset: &str) -> Arc<AssetRecord> {
    let source = AssetSource::linked(format!("media/{asset}.bin"), 4096).unwrap();
    Arc::new(AssetRecord::new(id(asset), format!("{asset} name"), kind, source).unwrap())
}
pub(crate) fn layer_target(layer_uid: &str) -> AssetTarget {
    AssetTarget::Drawing {
        context_id: "scene-root".to_owned(),
        layer_uid: layer_uid.to_owned(),
        stroke_id: None,
    }
}
pub(crate) fn element_target(layer_uid: &str, stroke_id: &str) -> AssetTarget {
    AssetTarget::Drawing {
        context_id: "scene-root".to_owned(),
        layer_uid: layer_uid.to_owned(),
        stroke_id: Some(stroke_id.to_owned()),
    }
}
pub(crate) fn text_target(layer_uid: &str) -> AssetTarget {
    AssetTarget::Text {
        context_id: "scene-root".to_owned(),
        layer_uid: layer_uid.to_owned(),
        stroke_id: None,
    }
}
pub(crate) fn audio_target(track_uid: &str) -> AssetTarget {
    AssetTarget::AudioTrack {
        track_uid: track_uid.to_owned(),
    }
}
pub(crate) fn placement(
    placement_id: &str,
    asset: &str,
    target: AssetTarget,
    offset: i64,
) -> AssetPlacement {
    AssetPlacement::new(placement_id.to_owned(), id(asset), target, offset).unwrap()
}
pub(crate) fn ids(placements: &[&AssetPlacement]) -> Vec<String> {
    placements.iter().map(|p| p.id().to_owned()).collect()
}

fn record_target(layer: &str, stroke: &str, scope: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":scope,"frame":0},
        "layerUid":layer,"strokeId":stroke})
}
fn path_record(layer: &str, stroke: &str, scope: &str) -> Value {
    json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
        "target":record_target(layer, stroke, scope),
        "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":[
            {"point":{"x":10.125,"y":20},"handleIn":{"x":-2,"y":0},"handleOut":{"x":3.25,"y":-1}},
            {"point":{"x":40,"y":20},"handleIn":{"x":-3,"y":2},"handleOut":{"x":0,"y":6}},
            {"point":{"x":25,"y":50},"handleIn":{"x":4,"y":-2},"handleOut":{"x":-5,"y":-3}}]},
        "fill":{"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.75}})
}
fn raster_item(asset: &str) -> Value {
    json!({"assetId":asset,"x":200,"y":150,"width":64,"height":32,"opacity":1})
}
/// A version 1 drawing with layers `L` and `M`: authored path `L/ink-1`, REFERENCE-scope path `M/ref-1`, authored
/// raster items `L/r0` and `M/r1` (asset `img`) and the authored text block `M/t0`.
pub(crate) fn drawing() -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":24,
        "layers":[{"layerUid":"L"},{"layerUid":"M"}],
        "objects":[path_record("L","ink-1","authored"), path_record("M","ref-1","reference")],
        "items":{
            "raster":[
                {"target":record_target("L","r0","authored"),"item":raster_item("img")},
                {"target":record_target("M","r1","authored"),"item":raster_item("img")}],
            "text":[
                {"target":record_target("M","t0","authored"),
                 "block":serde_json::from_str::<Value>(TEXT).unwrap()}]}})
}
pub(crate) fn document(drawing: &Value) -> ObjectDocument {
    decode_storage_project(drawing.to_string().as_bytes()).unwrap()
}
/// Records `img`, `img2` (images), `clip` (video), `face` (font), `score` (audio) and six placements.
pub(crate) fn catalog() -> AssetInventory {
    let records = vec![
        image_record("img"),
        image_record("img2"),
        linked_record(AssetKind::Video, "clip"),
        linked_record(AssetKind::Font, "face"),
        linked_record(AssetKind::Audio, "score"),
    ];
    let placements = vec![
        placement("p1", "img", layer_target("L"), 5),
        placement(
            "p2",
            "img",
            element_target("L", "r0"),
            -9_007_199_254_740_991,
        ),
        placement("p3", "clip", layer_target("M"), 0),
        placement(
            "p4",
            "img2",
            element_target("M", "r1"),
            9_007_199_254_740_991,
        ),
        placement("p5", "face", text_target("M"), 2),
        placement("p6", "score", audio_target("t1"), 7),
    ];
    AssetInventory::new(records, placements, 8, 8).unwrap()
}

#[test]
fn only_authored_path_records_and_raster_items_are_image_elements() {
    let doc = document(&drawing());
    let cases = [
        ("L", "ink-1", true),
        ("L", "r0", true),
        ("M", "r1", true),
        ("M", "ref-1", false),
        ("M", "t0", false),
        ("M", "ink-1", false),
        ("L", "r1", false),
        ("L", "ghost", false),
        ("L", "", false),
        ("nope", "ink-1", false),
    ];
    for (layer, stroke, expected) in cases {
        assert_eq!(
            image_element_exists(&doc, "scene-root", layer, stroke),
            expected,
            "{layer}/{stroke}"
        );
    }
    assert!(!image_element_exists(&doc, "shot-9", "L", "ink-1"));
    assert!(!image_element_exists(&doc, "", "L", "ink-1"));
}

#[test]
fn a_document_without_items_still_has_its_path_records_as_image_elements() {
    let mut plain = drawing();
    plain.as_object_mut().unwrap().remove("items");
    let doc = document(&plain);
    assert!(doc.item_records().is_none());
    assert!(image_element_exists(&doc, "scene-root", "L", "ink-1"));
    assert!(!image_element_exists(&doc, "scene-root", "L", "r0"));
    assert!(!image_element_exists(&doc, "scene-root", "M", "r1"));
}

#[test]
fn the_layer_query_returns_whole_layer_and_element_placements_in_table_order() {
    let inventory = catalog();
    let on = |layer: &str| ids(&placements_for_layer(&inventory, layer));
    assert_eq!(on("L"), ["p1", "p2"]);
    // A video, an image element and a text placement all sit on M; the audio track sits on no layer.
    assert_eq!(on("M"), ["p3", "p4", "p5"]);
    assert!(on("nope").is_empty());
    assert!(on("").is_empty());
    assert!(on("t1").is_empty());
    let reversed = AssetInventory::new(
        inventory.records().to_vec(),
        inventory.placements().iter().rev().cloned().collect(),
        8,
        8,
    )
    .unwrap();
    assert_eq!(
        ids(&placements_for_layer(&reversed, "M")),
        ["p5", "p4", "p3"]
    );
}

#[test]
fn the_target_query_finds_exactly_the_placement_of_that_target() {
    let inventory = catalog();
    let at = |target: AssetTarget| {
        placement_for_target(&inventory, &target).map(|p| (p.id().to_owned(), p.offset_frames()))
    };
    assert_eq!(at(layer_target("L")), Some(("p1".to_owned(), 5)));
    assert_eq!(
        at(element_target("L", "r0")),
        Some(("p2".to_owned(), -9_007_199_254_740_991))
    );
    assert_eq!(
        at(element_target("M", "r1")),
        Some(("p4".to_owned(), 9_007_199_254_740_991))
    );
    assert_eq!(at(text_target("M")), Some(("p5".to_owned(), 2)));
    assert_eq!(at(audio_target("t1")), Some(("p6".to_owned(), 7)));
    // The same layer as a text target, an unknown element, another context: none of those is a placement.
    assert_eq!(at(text_target("L")), None);
    assert_eq!(at(element_target("L", "ghost")), None);
    assert_eq!(at(audio_target("t2")), None);
    let other_context = AssetTarget::Drawing {
        context_id: "shot-2".to_owned(),
        layer_uid: "L".to_owned(),
        stroke_id: None,
    };
    assert_eq!(at(other_context), None);
}

#[test]
fn the_asset_query_lists_every_target_of_one_asset() {
    let inventory = catalog();
    let of = |asset: &str| ids(&placements_for_asset(&inventory, &id(asset)));
    assert_eq!(of("img"), ["p1", "p2"]);
    assert_eq!(of("img2"), ["p4"]);
    assert_eq!(of("clip"), ["p3"]);
    assert_eq!(of("score"), ["p6"]);
    assert!(of("missing").is_empty());
}

#[test]
fn only_an_audio_track_sits_on_no_layer() {
    assert_eq!(target_layer(&layer_target("L")), Some(("scene-root", "L")));
    assert_eq!(
        target_layer(&element_target("M", "r1")),
        Some(("scene-root", "M"))
    );
    assert_eq!(target_layer(&text_target("M")), Some(("scene-root", "M")));
    assert_eq!(target_layer(&audio_target("t1")), None);
}

#[test]
fn queries_borrow_the_catalog_and_leave_it_unchanged() {
    let inventory = catalog();
    let before = inventory.clone();
    let _ = placements_for_layer(&inventory, "L");
    let _ = placements_for_asset(&inventory, &id("img"));
    let _ = placement_for_target(&inventory, &layer_target("L"));
    assert_eq!(inventory, before);
    assert_eq!(inventory.placements().len(), 6);
}
