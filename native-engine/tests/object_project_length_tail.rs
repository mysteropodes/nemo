//! The retained tail of the Frames field at document level (baseline setTotalFrames never truncates `ld.frames`):
//! what a cut takes off the end comes back, with raster images and text blocks, on every document shape.
use super::{extract, restore};
use crate::object_codec::encode_storage_project;
use crate::object_document::{FrameScopeKind, ObjectDocument};
use serde_json::{json, Value};

#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;

/// L0 keys at 0, 2 and 5; L1 keys at 0 and 3. Six frames.
const LAYERS: &str = "K=a.1.10;.;K=b.2.20;.;.;K=c.3.30|K;.;.;K;.;.";

fn target(layer: &str, frame: u32, id: &str) -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":id})
}
/// Every L0 key also holds an image and a text block.
fn with_items(build: build::Build) -> ObjectDocument {
    let mut value = build::document_json(&rows::parse_layers(LAYERS), build);
    let (mut raster, mut text) = (Vec::new(), Vec::new());
    for frame in [0u32, 2, 5] {
        raster.push(json!({"target":target("L0", frame, &format!("img{frame}")),
            "item":{"assetId":"img-1","x":100,"y":50,"width":40,"height":20}}));
        text.push(json!({"target":target("L0", frame, &format!("txt{frame}")),
            "block":{"text":"Hi","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":24,
                "color":"#112233","align":"left","fixedWidth":200,"letterSpacing":2,"anchorTopLeft":{"x":10,"y":20}}}));
        if build.v2() {
            let scopes = value["compositionOrder"]["contexts"][0]["scopes"]
                .as_array_mut()
                .unwrap();
            for scope in scopes
                .iter_mut()
                .filter(|s| s["frameScope"]["frame"] == frame)
            {
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
    value["items"] = json!({"raster":raster,"text":text});
    build::decode(&value)
}
fn cells(document: &ObjectDocument) -> usize {
    let items = document.item_records();
    let item_cells = items.map_or(0, |i| i.raster_entries().len() + i.text_entries().len());
    let vectors = document
        .objects
        .iter()
        .filter(|r| {
            r.target.frame_scope.kind == FrameScopeKind::Authored
                && r.target.context_id == "scene-root"
        })
        .count();
    item_cells + vectors
}

#[test]
fn a_cut_returns_every_record_item_scope_and_timing_entry_on_every_document_shape() {
    for build in build::Build::ALL {
        let full = with_items(build);
        let total = full.total_frames;
        for wanted in [1u32, 2, 3, 5] {
            let kept = extract(&full, wanted);
            let mut short = super::super::cut(&full, wanted, total - 1).unwrap();
            assert!(cells(&short) < cells(&full), "{build:?} {wanted}");
            assert!(short
                .item_records()
                .is_none_or(|i| i.targets().all(|t| t.frame_scope.frame < wanted)));
            // The cut document itself is storage-valid, and nothing in it sits past the new end.
            encode_storage_project(&short).unwrap();
            short.total_frames = total;
            let back = restore(&short, &kept, wanted).unwrap();
            assert_eq!(back, full, "{build:?} cut at {wanted}");
        }
    }
}

#[test]
fn cells_of_a_layer_that_is_gone_stay_gone_and_the_rest_return() {
    let full = with_items(build::Build::V2);
    let kept = extract(&full, 3);
    let mut short = super::super::cut(&full, 3, 5).unwrap();
    short.total_frames = 6;
    short.layers.retain(|layer| layer.layer_uid != "L0");
    short.objects.retain(|r| r.target.layer_uid != "L0");
    short.items = crate::object_document::ItemRecords::normalized(vec![], vec![]);
    if let crate::object_document::Presence::Present(plan) = &mut short.composition_order {
        plan.contexts[0].layers.retain(|l| l.layer_uid != "L0");
        plan.contexts[0]
            .scopes
            .iter_mut()
            .for_each(|s| s.elements.retain(|e| e.layer_uid != "L0"));
    }
    let back = restore(&short, &kept, 3).unwrap();
    assert!(back.objects.iter().all(|r| r.target.layer_uid != "L0"));
    assert!(back.item_records().is_none());
    // L1's second key (frame 3) was cut with the tail and is back.
    assert_eq!(
        back.layers[0]
            .timing
            .value()
            .unwrap()
            .key_frames()
            .collect::<Vec<_>>(),
        vec![0, 3]
    );
}
