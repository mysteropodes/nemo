//! Real project/resource and sealed-owner controls for the original brush-companion join.
use super::*;
use crate::history::NativeObjectHistory;
use crate::object_document::{AssetId, BitmapBrushFields};
use serde_json::{json, Value};
use std::sync::Arc;

fn project() -> Value {
    let mut drawing = BitmapBrushFields::test_drawing();
    drawing["layers"][0]["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,
        "family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    json!({"format":"nemo.native-project-document","formatVersion":2,
        "project":{"canvasW":48,"canvasH":48,"fps":24},"drawing":drawing,"viewportReference":null,
        "assets":[{"id":"ink","name":"Ink","kind":"image","source":{"kind":"embedded",
            "mediaType":"image/png","encoding":"base64","data":crate::object_codec::encoded_wire_test::PNG}}],
        "assetInventory":{"assetIds":["ink"],"placements":[]}})
}
fn history() -> NativeObjectHistory {
    NativeObjectHistory::from_project_file("selection-companion", project().to_string().as_bytes())
        .unwrap()
}
fn saved(owner: &NativeObjectHistory) -> Vec<u8> {
    owner
        .acquire_snapshot(owner.content_revision())
        .unwrap()
        .serialize_project_file()
        .unwrap()
}
fn allocation(owner: &NativeObjectHistory) -> Arc<Vec<u8>> {
    Arc::clone(
        owner
            .acquire_snapshot(owner.content_revision())
            .unwrap()
            .project()
            .unwrap()
            .asset_inventory()
            .unwrap()
            .get(&AssetId::new("ink".into()).unwrap())
            .unwrap()
            .source()
            .encoded_allocation()
            .unwrap(),
    )
}
fn selection(command: &str, frame: u32, mode: &str) -> Vec<u8> {
    json!({"command":command,"contextId":"scene-root","layerUid":"L","frame":frame,
        "mode":mode,"strokeIds":["anchor"]})
    .to_string()
    .into_bytes()
}
fn paste(clip: Value) -> Vec<u8> {
    json!({"command":"object.clipboard.paste","contextId":"scene-root","layerUid":"L",
        "frame":0,"mode":"motion","clip":clip})
    .to_string()
    .into_bytes()
}

#[test]
fn bitmap_cut_then_paste_uses_retained_origin_and_same_allocation_with_exact_undo_reopen() {
    let mut owner = history();
    let before = saved(&owner);
    let bytes = allocation(&owner);
    let (clip, candidate) = owner
        .prepare_clipboard_cut_json(&selection("object.clipboard.cut", 0, "motion"))
        .unwrap();
    let clip = clip.unwrap();
    assert_eq!(clip.rasters.value().unwrap().len(), 1);
    assert_eq!(clip.source.value().unwrap().content_revision, 0);
    assert_eq!(owner.commit_selection_edit(candidate), Ok(1));
    let deleted: Value = serde_json::from_slice(&saved(&owner)).unwrap();
    assert_eq!(deleted["drawing"]["objects"], json!([]));
    assert!(deleted["drawing"].get("items").is_none());
    let candidate = owner
        .prepare_clipboard_paste_json(&paste(serde_json::to_value(&clip).unwrap()))
        .unwrap();
    assert_eq!(owner.commit_selection_edit(candidate), Ok(2));
    let restored = saved(&owner);
    let after: Value = serde_json::from_slice(&restored).unwrap();
    let drawing = &after["drawing"];
    assert_eq!(drawing["objects"].as_array().unwrap().len(), 1);
    assert_eq!(drawing["items"]["raster"].as_array().unwrap().len(), 1);
    assert_ne!(drawing["objects"][0]["target"]["strokeId"], json!("anchor"));
    assert_ne!(
        drawing["items"]["raster"][0]["target"]["strokeId"],
        json!("companion")
    );
    assert_ne!(
        drawing["objects"][0]["vectorExt"]["bitmapBrush"]["brushGroupId"],
        json!("ink group:雪")
    );
    assert_eq!(
        drawing["objects"][0]["vectorExt"]["bitmapBrush"]["brushGroupId"],
        drawing["items"]["raster"][0]["item"]["brushGroupId"]
    );
    assert_eq!(
        drawing["objects"][0]["geometry"]["segments"][0]["point"]["x"],
        json!(16.0)
    );
    assert_eq!(drawing["items"]["raster"][0]["item"]["x"], json!(28.0));
    assert_eq!(after["assets"], project()["assets"]);
    assert!(Arc::ptr_eq(&bytes, &allocation(&owner)));
    owner.undo(2).unwrap();
    owner.undo(3).unwrap();
    assert_eq!(saved(&owner), before);
    owner.redo(4).unwrap();
    owner.redo(5).unwrap();
    assert_eq!(saved(&owner), restored);
    let reopened = NativeObjectHistory::from_project_file("selection-reopened", &restored).unwrap();
    assert_eq!(saved(&reopened), restored);
    assert_eq!(owner.history_depths(), (2, 0));
}

#[test]
fn bitmap_packet_tampering_foreign_origins_and_missing_proof_preserve_bytes_and_redo() {
    let mut owner = history();
    let original = owner
        .copy_selection_json(&selection("object.clipboard.copy", 0, "motion"))
        .unwrap()
        .unwrap();
    let good = serde_json::to_value(original).unwrap();
    let candidate = owner
        .prepare_selection_delete_json(&selection("object.selection.delete", 0, "motion"))
        .unwrap();
    owner.commit_selection_edit(candidate).unwrap();
    owner.undo(1).unwrap();
    let before = saved(&owner);
    let bytes = allocation(&owner);
    for case in 0..9 {
        let mut bad = good.clone();
        match case {
            0 => bad["source"]["instanceId"] = json!("foreign"),
            1 => bad["source"]["documentId"] = json!("foreign"),
            2 => bad["source"]["contentRevision"] = json!(999),
            3 => bad["source"]["strokeIds"] = json!([]),
            4 => bad["records"][0]["geometry"]["segments"][0]["point"]["x"] = json!(9),
            5 => bad["rasters"][0]["item"]["x"] = json!(17),
            6 => bad["rasters"][0]["item"]["assetId"] = json!("missing"),
            7 => {
                bad.as_object_mut().unwrap().remove("source");
            }
            _ => {
                bad.as_object_mut().unwrap().remove("rasters");
            }
        }
        assert!(
            owner.prepare_clipboard_paste_json(&paste(bad)).is_err(),
            "case {case}"
        );
        assert_eq!(saved(&owner), before);
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (2, (0, 1))
        );
        assert!(Arc::ptr_eq(&bytes, &allocation(&owner)));
    }
    let another = history();
    assert!(another.prepare_clipboard_paste_json(&paste(good)).is_err());
}

#[test]
fn compound_held_delete_promotes_once_in_animation_and_edits_inherited_key_in_motion() {
    for mode in ["animation2d", "motion"] {
        let mut owner = history();
        let before = saved(&owner);
        let candidate = owner
            .prepare_selection_delete_json(&selection("object.selection.delete", 4, mode))
            .unwrap();
        assert_eq!(candidate.promoted(), mode == "animation2d");
        assert_eq!(owner.commit_selection_edit(candidate), Ok(1));
        let after: Value = serde_json::from_slice(&saved(&owner)).unwrap();
        if mode == "animation2d" {
            assert_eq!(after["drawing"]["objects"].as_array().unwrap().len(), 1);
            assert_eq!(
                after["drawing"]["objects"][0]["target"]["frameScope"]["frame"],
                json!(0)
            );
            assert_eq!(
                after["drawing"]["items"]["raster"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            assert_eq!(
                after["drawing"]["layers"][0]["timing"]["keys"],
                json!([{"frame":0},{"frame":4}])
            );
        } else {
            assert_eq!(after["drawing"]["objects"], json!([]));
            assert!(after["drawing"].get("items").is_none());
        }
        assert_eq!(owner.history_depths(), (1, 0));
        owner.undo(1).unwrap();
        assert_eq!(saved(&owner), before);
    }
}

#[test]
fn sealed_proof_rejects_edited_plan_stale_owner_and_foreign_owner() {
    let mut owner = history();
    let before = saved(&owner);
    let mut forged = owner
        .prepare_selection_delete_json(&selection("object.selection.delete", 0, "motion"))
        .unwrap();
    forged.plan.document.total_frames += 1;
    assert_eq!(
        owner.commit_selection_edit(forged),
        Err(DispatchErrorCode::InvalidRequest)
    );
    assert_eq!(saved(&owner), before);
    let stale = owner
        .prepare_selection_delete_json(&selection("object.selection.delete", 0, "motion"))
        .unwrap();
    let fresh = owner
        .prepare_selection_delete_json(&selection("object.selection.delete", 0, "motion"))
        .unwrap();
    let mut other = history();
    assert!(other.commit_selection_edit(fresh.clone()).is_err());
    assert_eq!(saved(&other), before);
    owner.commit_selection_edit(fresh).unwrap();
    assert_eq!(
        owner.commit_selection_edit(stale),
        Err(DispatchErrorCode::StaleRevision)
    );
}

#[test]
fn public_compound_cut_receipt_replays_without_second_mutation() {
    let mut owner = history();
    let request = json!({"apiVersion":2,"requestId":"compound-cut","instanceId":owner.instance_id(),
        "documentId":owner.document_id(),"expectedRevision":0,"operation":"command.document.object.clipboard.cut",
        "payload":serde_json::from_slice::<Value>(&selection("object.clipboard.cut",0,"motion")).unwrap()});
    let request = serde_json::from_value(request).unwrap();
    let first = owner
        .try_dispatch_object_command(&request)
        .unwrap()
        .unwrap();
    let response = serde_json::to_value(&first).unwrap();
    assert_eq!(response["ok"], json!(true));
    assert_eq!(
        response["result"]["clipboard"]["rasters"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        response["result"]["clipboard"]["source"]["contentRevision"],
        json!(0)
    );
    assert!(serde_json::to_vec(&response).unwrap().len() <= 4096);
    let before = saved(&owner);
    let replay = owner
        .try_dispatch_object_command(&request)
        .unwrap()
        .unwrap();
    assert_eq!(serde_json::to_value(replay).unwrap(), response);
    assert_eq!(saved(&owner), before);
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (1, (1, 0))
    );
}

#[test]
fn same_asset_id_and_equal_png_bytes_with_a_new_allocation_do_not_authorize_old_packet() {
    let mut wire = project();
    wire["assets"][0]["id"] = json!("asset-ink");
    wire["assetInventory"]["assetIds"] = json!(["asset-ink"]);
    wire["drawing"]["items"]["raster"][0]["item"]["assetId"] = json!("asset-ink");
    let mut owner =
        ObjectFillOwner::from_project_file("resource-origin".into(), wire.to_string().as_bytes())
            .unwrap();
    let id = AssetId::new("asset-ink".into()).unwrap();
    let initial = owner.current_snapshot();
    let original_bytes = Arc::clone(
        initial
            .project()
            .unwrap()
            .asset_inventory()
            .unwrap()
            .get(&id)
            .unwrap()
            .source()
            .encoded_allocation()
            .unwrap(),
    );
    let (clip, cut) = owner
        .prepare_clipboard_cut_json(&selection("object.clipboard.cut", 0, "motion"))
        .unwrap();
    owner.commit_selection_edit(cut).unwrap();
    let remove = owner
        .current_snapshot()
        .prepare_asset_removal("asset-ink")
        .unwrap();
    owner.commit_asset_edit(remove).unwrap();
    let packet = serde_json::to_value(clip.unwrap()).unwrap();
    assert!(
        owner
            .prepare_clipboard_paste_json(&paste(packet.clone()))
            .is_err(),
        "removed asset"
    );
    let attach = owner
        .current_snapshot()
        .prepare_asset_attachment("ink", "Ink", "image/png", &original_bytes)
        .unwrap();
    owner.commit_asset_edit(attach).unwrap();
    let replaced = owner.current_snapshot();
    let replacement_bytes = replaced
        .project()
        .unwrap()
        .asset_inventory()
        .unwrap()
        .get(&id)
        .unwrap()
        .source()
        .encoded_allocation()
        .unwrap();
    assert_eq!(original_bytes.as_slice(), replacement_bytes.as_slice());
    assert!(!Arc::ptr_eq(&original_bytes, replacement_bytes));
    let before = replaced.serialize_project_file().unwrap();
    assert_eq!(
        owner
            .prepare_clipboard_paste_json(&paste(packet))
            .unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(
        owner.current_snapshot().serialize_project_file().unwrap(),
        before
    );
    assert_eq!(owner.content_revision(), 3);
}
