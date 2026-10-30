//! #2248: a copied cell carries its raster images and text blocks with its vector records, and a paste writes all
//! three. Behavior source: baseline 1065e9d8, where a cell is one list of strokes (vectors, rasters and text alike) and
//! copyFrames/pasteFrames deep-copy the whole cell. Asset and mesh references stay valid in the same document; a
//! reference that does not resolve refuses typed.
use super::build::{decode, document_json, Build};
use super::rows::{parse_layers, OP_PASTE};
use super::{bytes, dispatch, head, request};
use crate::commands::{packet_paste_cells, paste_payload, CellRef, PasteCell};
use crate::history::NativeObjectHistory;
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource, ObjectDocument, ProjectEnvelope,
    ProjectMetadata,
};
use serde_json::{json, Value};
use std::sync::Arc;

const LAYERS: &str = "K=a.1.10;.;K=b.2.20;.|K;.;.;K";
const PNG_HEX: &str = "89504e470d0a1a0a0000000d494844520000000200000002080600000072b60d240000001a49444154789c63105432feef129ad6c050de319361d5ee33ff01378a07ad4ff3864d0000000049454e44ae426082";

fn inventory() -> AssetInventory {
    let png: Vec<u8> = PNG_HEX
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect();
    let source =
        AssetSource::admitted_embedded("image/png".into(), Arc::new(png), 1 << 20).unwrap();
    let record = AssetRecord::new(
        AssetId::new("img-1".into()).unwrap(),
        "img-1.bin".into(),
        AssetKind::Image,
        source,
    )
    .unwrap();
    AssetInventory::new(vec![Arc::new(record)], Vec::new(), 16, 0).unwrap()
}
fn target(layer: &str, frame: u32, id: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":id})
}
/// L0 holds a vector, an image bound to a mesh and a text block on each of its keys (frames 0 and 2).
fn fixture(build: Build) -> ObjectDocument {
    let mut value = document_json(&parse_layers(LAYERS), build);
    let baseline: Value =
        serde_json::from_str(include_str!("../../tests/fixtures/mesh/project.json")).unwrap();
    value["imageMeshes"] = json!({"im_1": baseline["imageMeshes"]["im_1"].clone()});
    let (mut raster, mut text) = (Vec::new(), Vec::new());
    for frame in [0u32, 2] {
        raster.push(json!({"target":target("L0", frame, &format!("img{frame}")),
            "item":{"assetId":"img-1","x":100,"y":50,"width":40,"height":20,"meshId":"im_1"}}));
        text.push(json!({"target":target("L0", frame, &format!("txt{frame}")),
            "block":{"text":"Hi","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":24,
                "color":"#112233","align":"left","fixedWidth":200,"letterSpacing":2,"anchorTopLeft":{"x":10,"y":20}}}));
        if build.v2() {
            for scope in value["compositionOrder"]["contexts"][0]["scopes"]
                .as_array_mut()
                .unwrap()
            {
                if scope["frameScope"]["frame"] == frame {
                    for (at, id) in [format!("img{frame}"), format!("txt{frame}")]
                        .iter()
                        .enumerate()
                    {
                        scope["elements"].as_array_mut().unwrap().push(
                            json!({"layerUid":"L0","strokeId":id,"naturalPosition":at + 1,"order":{}}),
                        );
                    }
                }
            }
        }
    }
    value["items"] = json!({"raster":raster,"text":text});
    decode(&value)
}
fn owner(build: Build) -> NativeObjectHistory {
    let envelope =
        ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 24.).unwrap(), fixture(build))
            .with_asset_inventory(Some(inventory()));
    NativeObjectHistory::from_admitted_project("paste-items", envelope).unwrap()
}
fn cell(layer: &str, frame: u32) -> CellRef {
    CellRef {
        layer_uid: layer.into(),
        frame,
    }
}
/// Ids of the vector records, images and text blocks the document holds on `layer` at `frame`.
fn ids_at(
    document: &ObjectDocument,
    layer: &str,
    frame: u32,
) -> (Vec<String>, Vec<String>, Vec<String>) {
    let here = |t: &crate::object_document::ObjectTarget| {
        t.layer_uid == layer && t.frame_scope.frame == frame && t.context_id == "scene-root"
    };
    let items = document.item_records();
    (
        document
            .objects
            .iter()
            .filter(|r| here(&r.target))
            .map(|r| r.target.stroke_id.clone())
            .collect(),
        items.map_or(vec![], |i| {
            i.raster_entries()
                .iter()
                .filter(|e| here(&e.target))
                .map(|e| e.target.stroke_id.clone())
                .collect()
        }),
        items.map_or(vec![], |i| {
            i.text_entries()
                .iter()
                .filter(|e| here(&e.target))
                .map(|e| e.target.stroke_id.clone())
                .collect()
        }),
    )
}
fn paste(o: &mut NativeObjectHistory, id: &str, payload: Value) -> Value {
    let submitted = request(o, id, OP_PASTE, payload);
    dispatch(o, &submitted)
}

#[test]
fn a_pasted_cell_carries_its_vector_image_and_text_in_one_history_entry() {
    for build in [Build::V1, Build::V2] {
        let mut o = owner(build);
        let original = bytes(&o);
        let packet = o.copy_frame_clipboard(&[cell("L0", 0)]).unwrap();
        let cells = packet_paste_cells(&packet);
        assert_eq!(
            (
                cells[0].items.raster.len(),
                cells[0].items.text.len(),
                cells[0].records.len()
            ),
            (1, 1, 1),
            "{build:?}: the packet cell holds all three"
        );
        let payload = paste_payload(&packet, "L1", 1);
        assert_eq!(
            payload["cells"][0]["rasterItems"].as_array().unwrap().len(),
            1,
            "{payload}"
        );
        assert_eq!(
            payload["cells"][0]["textItems"].as_array().unwrap().len(),
            1,
            "{payload}"
        );
        let depth = o.history_depths();
        let reply = paste(&mut o, "paste", payload);
        assert_eq!(reply["ok"], true, "{build:?}: {reply}");
        assert_eq!(reply["result"]["applied"], true, "{reply}");
        assert_eq!(reply["result"]["historyEntriesAdded"], 1, "{reply}");
        assert_eq!(
            o.history_depths().0,
            depth.0 + 1,
            "{build:?}: one history entry"
        );
        let snapshot = head(&o);
        assert_eq!(
            ids_at(snapshot.document(), "L1", 1),
            (vec!["a".into()], vec!["img0".into()], vec!["txt0".into()]),
            "{build:?}: all three landed on L1 frame 1"
        );
        let landed = &snapshot.document().item_records().unwrap().raster_entries();
        let copy = landed.iter().find(|e| e.target.layer_uid == "L1").unwrap();
        assert_eq!(
            copy.item.mesh_id(),
            Some("im_1"),
            "{build:?}: the mesh binding is kept"
        );
        assert_eq!(
            ids_at(snapshot.document(), "L0", 0),
            (vec!["a".into()], vec!["img0".into()], vec!["txt0".into()]),
            "{build:?}: the source cell is untouched"
        );
        o.undo(o.content_revision()).unwrap();
        assert_eq!(bytes(&o), original, "{build:?}: undo is exact");
    }
}

#[test]
fn a_paste_replaces_the_items_the_target_cell_held() {
    let mut o = owner(Build::V2);
    let packet = o.copy_frame_clipboard(&[cell("L0", 0)]).unwrap();
    let reply = paste(&mut o, "paste", paste_payload(&packet, "L0", 2));
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(
        ids_at(head(&o).document(), "L0", 2),
        (vec!["a".into()], vec!["img0".into()], vec!["txt0".into()]),
        "the key at frame 2 now holds the copy, not its own image and text"
    );
}

#[test]
fn an_item_only_key_is_copied_as_a_key_not_a_blank() {
    let mut o = owner(Build::V1);
    // Drop L0's vector record at frame 2 so that key holds an image and a text block only.
    let mut document = head(&o).document().clone();
    document
        .objects
        .retain(|r| !(r.target.layer_uid == "L0" && r.target.frame_scope.frame == 2));
    let envelope =
        ProjectEnvelope::no_media(ProjectMetadata::new(320, 180, 24.).unwrap(), document)
            .with_asset_inventory(Some(inventory()));
    o = NativeObjectHistory::from_admitted_project("paste-items", envelope).unwrap();
    let packet = o.copy_frame_clipboard(&[cell("L0", 2)]).unwrap();
    assert_eq!(format!("{:?}", packet.cells()[0].kind()), "Key");
    let reply = paste(&mut o, "paste", paste_payload(&packet, "L1", 1));
    assert_eq!(reply["ok"], true, "{reply}");
    assert_eq!(
        ids_at(head(&o).document(), "L1", 1),
        (vec![], vec!["img2".into()], vec!["txt2".into()])
    );
}

#[test]
fn a_plain_cell_pastes_no_items() {
    let o = owner(Build::V1);
    // Frame 1 of L0 holds the key at frame 0: a held cell, which the baseline copies as a plain cell.
    let packet = o.copy_frame_clipboard(&[cell("L0", 1)]).unwrap();
    let cells: Vec<PasteCell> = packet_paste_cells(&packet);
    assert!(!cells[0].keyframe && cells[0].items.is_empty() && cells[0].records.is_empty());
}

#[test]
fn unresolved_references_and_foreign_documents_refuse_typed() {
    let mut o = owner(Build::V1);
    let original = bytes(&o);
    let packet = o.copy_frame_clipboard(&[cell("L0", 0)]).unwrap();
    let good = paste_payload(&packet, "L1", 1);
    let mut missing_asset = good.clone();
    missing_asset["cells"][0]["rasterItems"][0]["item"]["assetId"] = json!("img-9");
    assert_eq!(
        paste(&mut o, "asset", missing_asset)["error"]["code"],
        "not_found"
    );
    let mut missing_mesh = good.clone();
    missing_mesh["cells"][0]["rasterItems"][0]["item"]["meshId"] = json!("im_9");
    assert_eq!(
        paste(&mut o, "mesh", missing_mesh)["error"]["code"],
        "not_found"
    );
    let mut foreign = good.clone();
    foreign["documentId"] = json!("another-document");
    assert_eq!(
        paste(&mut o, "foreign", foreign)["error"]["code"],
        "wrong_document"
    );
    let mut unknown = good.clone();
    unknown["cells"][0]["rasterItems"][0]["bogus"] = json!(1);
    assert_eq!(
        paste(&mut o, "strict", unknown)["error"]["code"],
        "invalid_request"
    );
    let mut reference = good.clone();
    reference["cells"][0]["textItems"][0]["target"]["frameScope"]["kind"] = json!("reference");
    assert_eq!(
        paste(&mut o, "scope", reference)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(
        bytes(&o),
        original,
        "every refusal leaves the document untouched"
    );
    assert_eq!(paste(&mut o, "ok", good)["ok"], true);
}
