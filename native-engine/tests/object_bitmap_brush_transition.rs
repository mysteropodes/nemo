//! Real revision/history boundary controls for ordinary brush mutations.
use super::*;
use crate::history::NativeObjectHistory;
use crate::object_document::BitmapBrushFields;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

fn project() -> Value {
    json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":48,"canvasH":48,"fps":24},
        "drawing":BitmapBrushFields::test_drawing(),"viewportReference":null,
        "assets":[{"id":"ink","name":"Ink","kind":"image","source":{
            "kind":"embedded","mediaType":"image/png","encoding":"base64",
            "data":crate::object_codec::encoded_wire_test::PNG}}],
        "assetInventory":{"assetIds":["ink"]}})
}
fn history() -> NativeObjectHistory {
    NativeObjectHistory::from_project_file("compound-guard", project().to_string().as_bytes())
        .unwrap()
}
fn saved(history: &NativeObjectHistory) -> Vec<u8> {
    history
        .acquire_snapshot(history.content_revision())
        .unwrap()
        .serialize_project_file()
        .unwrap()
}
fn vector_payload(case: &str) -> Value {
    let source = BitmapBrushFields::test_drawing();
    let mut record = source["objects"][0].clone();
    match case {
        "delete" => {
            return json!({"command":"object.vector.delete","stableTarget":record["target"]})
        }
        "geometry" => record["geometry"]["segments"][0]["point"]["x"] = json!(6),
        "spec" => {
            record["vectorExt"]["bitmapBrush"]["bitmapBrushSpec"]["color"] = json!("#ff0000ff")
        }
        "create" => {
            record["target"]["strokeId"] = json!("");
            record["vectorExt"]["bitmapBrush"]["brushGroupId"] = json!("new-ink");
        }
        _ => panic!("unknown fixed test case"),
    }
    json!({"command":if case=="create" {"object.vector.create"} else {"object.vector.replace"},"record":record})
}
#[test]
fn geometry_spec_anchor_deletion_and_creation_refuse_at_shared_publication() {
    for case in ["geometry", "spec", "delete", "create"] {
        let mut history = history();
        let before = saved(&history);
        let retained = history.acquire_snapshot(0).unwrap();
        // Preparation MUST succeed: this drives the publication boundary,
        // rather than obtaining a green result from an invalid request.
        let candidate = history
            .prepare_vector_edit_json(vector_payload(case).to_string().as_bytes())
            .unwrap();
        assert_eq!(
            history.commit_vector_edit(candidate),
            Err(DispatchErrorCode::Unavailable),
            "{case}"
        );
        assert_eq!(history.content_revision(), 0, "{case}");
        assert_eq!(saved(&history), before, "{case}");
        assert_eq!(retained.serialize_project_file().unwrap(), before, "{case}");
        assert_eq!(
            history.undo(0),
            Err(DispatchErrorCode::Unavailable),
            "{case}"
        );
    }
}
#[test]
fn companion_move_resize_rotate_and_delete_refuse_without_asset_geometry_split() {
    let source = BitmapBrushFields::test_drawing();
    let target = source["items"]["raster"][0]["target"].clone();
    for payload in [
        json!({"command":"object.raster.move","stableTarget":target,"x":20,"y":20}),
        json!({"command":"object.raster.resize","stableTarget":target,"width":12,"height":12}),
        json!({"command":"object.raster.rotate","stableTarget":target,"rotation":20}),
        json!({"command":"object.raster.delete","stableTarget":target}),
    ] {
        let mut history = history();
        let before = saved(&history);
        let candidate = history
            .prepare_item_edit_json(payload.to_string().as_bytes())
            .unwrap();
        assert_eq!(
            history.commit_item_edit(candidate),
            Err(DispatchErrorCode::Unavailable)
        );
        assert_eq!(history.content_revision(), 0);
        assert_eq!(saved(&history), before);
    }
}
#[test]
fn independent_anchor_fill_ink_opacity_and_plain_geometry_keep_existing_ink() {
    let mut history = history();
    let before: Value = serde_json::from_slice(&saved(&history)).unwrap();
    let target = before["drawing"]["items"]["raster"][0]["target"].clone();
    let payload = json!({"command":"object.raster.opacity","stableTarget":target,"opacity":0.5});
    let candidate = history
        .prepare_item_edit_json(payload.to_string().as_bytes())
        .unwrap();
    assert_eq!(history.commit_item_edit(candidate), Ok(1));
    let mut record = before["drawing"]["objects"][0].clone();
    record["target"]["strokeId"] = json!("");
    record.as_object_mut().unwrap().remove("vectorExt");
    let candidate = history
        .prepare_vector_edit_json(
            json!({"command":"object.vector.create","record":record})
                .to_string()
                .as_bytes(),
        )
        .unwrap();
    assert_eq!(history.commit_vector_edit(candidate), Ok(2));
    let after: Value = serde_json::from_slice(&saved(&history)).unwrap();
    assert_eq!(
        after["drawing"]["objects"][0],
        before["drawing"]["objects"][0]
    );
    assert_eq!(after["assets"], before["assets"]);
    assert_eq!(
        after["drawing"]["items"]["raster"][0]["item"]["assetId"],
        json!("ink")
    );
    assert_eq!(
        after["drawing"]["items"]["raster"][0]["item"]["opacity"],
        json!(0.5)
    );
    assert_eq!(history.undo(2), Ok(3));
    assert_eq!(history.redo(3), Ok(4));
    let reopened =
        NativeObjectHistory::from_project_file("compound-reopen", &saved(&history)).unwrap();
    assert_eq!(saved(&reopened), saved(&history));
}
#[test]
fn failed_compound_mutation_does_not_clear_redo_or_retained_project_pixels() {
    let mut history = history();
    let original = saved(&history);
    let source = BitmapBrushFields::test_drawing();
    let candidate = history
        .prepare_fill_json(
            json!({"command":"object.fill.set",
        "stableTarget":source["objects"][0]["target"],
        "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}})
            .to_string()
            .as_bytes(),
        )
        .unwrap();
    assert_eq!(history.commit_fill(candidate), Ok(1));
    let changed = saved(&history);
    assert_ne!(changed, original);
    assert_eq!(history.undo(1), Ok(2));
    assert_eq!(saved(&history), original);
    let candidate = history
        .prepare_vector_edit_json(vector_payload("geometry").to_string().as_bytes())
        .unwrap();
    assert_eq!(
        history.commit_vector_edit(candidate),
        Err(DispatchErrorCode::Unavailable)
    );
    assert_eq!(history.content_revision(), 2);
    assert_eq!(saved(&history), original);
    assert_eq!(history.redo(2), Ok(3));
    assert_eq!(saved(&history), changed);
    assert_eq!(
        history
            .acquire_snapshot(0)
            .unwrap()
            .serialize_project_file()
            .unwrap(),
        original
    );
}
#[test]
fn alternate_drawing_writer_cannot_remove_anchor_or_retag_companion() {
    for case in ["erase-anchor", "remove-flags", "retag", "new-asset"] {
        let history = history();
        let origin = history.acquire_snapshot(0).unwrap();
        let before = origin.serialize_project_file().unwrap();
        let mut drawing = BitmapBrushFields::test_drawing();
        match case {
            "erase-anchor" => {
                drawing["objects"] = json!([]);
                let elements = drawing["compositionOrder"]["contexts"][0]["scopes"][0]["elements"]
                    .as_array_mut()
                    .unwrap();
                elements.remove(0);
                elements[0]["naturalPosition"] = json!(0);
            }
            "remove-flags" => {
                drawing["items"]["raster"][0]["item"]
                    .as_object_mut()
                    .unwrap()
                    .remove("isBitmapBrush");
            }
            "retag" => drawing["items"]["raster"][0]["item"]["brushGroupId"] = json!("other"),
            _ => drawing["items"]["raster"][0]["item"]["assetId"] = json!("other"),
        }
        let drawing =
            crate::object_codec::decode_storage_project(drawing.to_string().as_bytes()).unwrap();
        let mut owner = crate::revision::ObjectRevisionOwner::from_admitted(origin);
        assert_eq!(
            owner.publish(drawing),
            Err(BRUSH_EDIT_UNAVAILABLE),
            "{case}"
        );
        assert_eq!(owner.content_revision(), 0);
        assert_eq!(
            owner.acquire(0).unwrap().serialize_project_file().unwrap(),
            before
        );
    }
}
