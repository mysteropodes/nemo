//! #2248: the frame-structure edits (insert, remove, extend, duplicate) and the key retime move a layer's raster
//! images and text blocks with its vector records. Behavior source: baseline 1065e9d8, where a cell is one list of
//! strokes (vectors, rasters and text alike) and every frame edit shifts the whole cell.
use super::build::{decode, document_json, Build};
use super::rows::{parse_layers, OP_DUPLICATE, OP_EXTEND, OP_INSERT, OP_REMOVE};
use super::{bytes, dispatch, head, owner, request};
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

const LAYERS: &str = "K=a.1.10;.;K=b.2.20;.|K;.;.;K";

fn target(layer: &str, frame: u32, id: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":id})
}
/// L0 holds a vector, an image (bound to a mesh) and a text block on each of its keys, frames 0 and 2.
fn document_with_items(build: Build) -> ObjectDocument {
    let mut value = document_json(&parse_layers(LAYERS), build);
    let (mut raster, mut text) = (Vec::new(), Vec::new());
    for frame in [0u32, 2] {
        raster.push(json!({"target":target("L0", frame, &format!("img{frame}")),
            "item":{"assetId":"img-1","x":100,"y":50,"width":40,"height":20,"meshId":"mesh-1"}}));
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
                        scope["elements"].as_array_mut().unwrap()
                            .push(json!({"layerUid":"L0","strokeId":id,"naturalPosition":at + 1,"order":{}}));
                    }
                }
            }
        }
    }
    value["items"] = json!({"raster":raster,"text":text});
    decode(&value)
}
/// The frames of L0 that hold a vector record, an image and a text block, in stored order.
fn families(document: &ObjectDocument) -> (Vec<u32>, Vec<u32>, Vec<u32>) {
    let items = document.item_records();
    let raster = items.map_or(vec![], |i| {
        i.raster_entries()
            .iter()
            .map(|e| e.target.frame_scope.frame)
            .collect()
    });
    let text = items.map_or(vec![], |i| {
        i.text_entries()
            .iter()
            .map(|e| e.target.frame_scope.frame)
            .collect()
    });
    let vector = document
        .objects
        .iter()
        .filter(|r| {
            r.target.layer_uid == "L0"
                && r.target.context_id == "scene-root"
                && r.target.frame_scope.kind == crate::object_document::FrameScopeKind::Authored
        })
        .map(|r| r.target.frame_scope.frame)
        .collect();
    (vector, raster, text)
}
fn all_on(document: &ObjectDocument, frames: &[u32]) -> bool {
    let (vector, raster, text) = families(document);
    [vector, raster, text].iter().all(|list| list == frames)
}
fn run(o: &mut crate::history::NativeObjectHistory, operation: &str, payload: Value) {
    let reply = dispatch(
        o,
        &request(
            o,
            &format!("edit-{}", o.content_revision()),
            operation,
            payload,
        ),
    );
    assert_eq!(reply["ok"], true, "{operation} {reply}");
    assert_eq!(reply["result"]["applied"], true, "{reply}");
}

#[test]
fn insert_extend_duplicate_and_remove_move_images_and_text_like_vectors() {
    for build in [Build::V1, Build::V2] {
        let input = document_with_items(build);
        assert!(all_on(&input, &[0, 2]), "{build:?}: fixture");
        let mut o = owner(input.clone());
        let original = bytes(&o);
        // F5 at cell 0 on every layer: everything after frame 0 shifts one later.
        run(
            &mut o,
            OP_INSERT,
            json!({"command":"object.frames.insert","cell":0}),
        );
        assert!(
            all_on(head(&o).document(), &[0, 3]),
            "{build:?}: insert {:?}",
            families(head(&o).document())
        );
        // The mesh binding follows the image; ids are unchanged.
        let snapshot = head(&o);
        let moved = &snapshot.document().item_records().unwrap().raster_entries()[1];
        assert_eq!(
            (moved.target.stroke_id.as_str(), moved.item.mesh_id()),
            ("img2", Some("mesh-1"))
        );
        let after_insert = bytes(&o);
        // Remove frame 1 (the inserted slot) restores the original frames.
        run(
            &mut o,
            OP_REMOVE,
            json!({"command":"object.frames.remove","from":1,"to":1}),
        );
        assert!(all_on(head(&o).document(), &[0, 2]), "{build:?}: remove");
        // Removing the frame that holds the second key drops its image and text with its vector.
        run(
            &mut o,
            OP_REMOVE,
            json!({"command":"object.frames.remove","from":2,"to":2}),
        );
        assert!(
            all_on(head(&o).document(), &[0]),
            "{build:?}: remove key {:?}",
            families(head(&o).document())
        );
        o.undo(o.content_revision()).unwrap();
        o.undo(o.content_revision()).unwrap();
        assert_eq!(bytes(&o), after_insert, "{build:?}: undo is exact");
        o.undo(o.content_revision()).unwrap();
        assert_eq!(bytes(&o), original, "{build:?}: undo to the original");
        // Extend two frames at cell 0.
        run(
            &mut o,
            OP_EXTEND,
            json!({"command":"object.frames.extend","cell":0,"count":2}),
        );
        assert!(all_on(head(&o).document(), &[0, 4]), "{build:?}: extend");
        o.undo(o.content_revision()).unwrap();
        // Duplicate the key at cell 0: a copy of the cell lands on frame 1 and the rest shifts.
        run(
            &mut o,
            OP_DUPLICATE,
            json!({"command":"object.frames.duplicate","layerUid":"L0","cell":0}),
        );
        assert!(
            all_on(head(&o).document(), &[0, 3, 1]) || all_on(head(&o).document(), &[0, 1, 3]) || {
                let (v, r, t) = families(head(&o).document());
                let sorted = |mut l: Vec<u32>| {
                    l.sort_unstable();
                    l
                };
                sorted(v) == [0, 1, 3] && sorted(r) == [0, 1, 3] && sorted(t) == [0, 1, 3]
            },
            "{build:?}: duplicate {:?}",
            families(head(&o).document())
        );
        // The stored result reopens to the same document.
        let reopened =
            decode_storage_project(&encode_storage_project(head(&o).document()).unwrap()).unwrap();
        assert_eq!(&reopened, head(&o).document(), "{build:?}: save and reopen");
    }
}

#[test]
fn a_key_retime_moves_images_and_text_with_the_vectors() {
    for build in [Build::V1, Build::V2] {
        let mut o = owner(document_with_items(build));
        run(
            &mut o,
            "command.document.object.layer.key.move",
            json!({"command":"object.layer.key.move","layerUid":"L0","from":2,"to":3}),
        );
        assert!(
            all_on(head(&o).document(), &[0, 3]),
            "{build:?}: {:?}",
            families(head(&o).document())
        );
        o.undo(o.content_revision()).unwrap();
        assert!(all_on(head(&o).document(), &[0, 2]), "{build:?}: undo");
    }
}
