//! Real retained catalog at the existing save-envelope boundary; no mock budget.
#![cfg(all(feature = "history", feature = "project_media_codec"))]
use super::storage;
use crate::history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode;
use base64::Engine;
use serde_json::{json, Value};

const ENVELOPE_LIMIT: usize = 32 * 1024 * 1024;

fn rgba_png(width: u32, height: u32) -> Vec<u8> {
    let pixels = vec![0; width as usize * height as usize * 4];
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::NoCompression);
        encoder.set_filter(png::Filter::NoFilter);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&pixels).unwrap();
        writer.finish().unwrap();
    }
    assert!(
        encoded.len() < 16 * 1024 * 1024,
        "each real PNG fits existing source admission"
    );
    encoded
}

fn asset(id: &str, width: u32, height: u32) -> Value {
    json!({"id":id,"name":id,"kind":"image","source":{"kind":"embedded",
        "mediaType":"image/png","encoding":"base64",
        "data":base64::engine::general_purpose::STANDARD.encode(rgba_png(width, height))}})
}

fn near_limit_catalog() -> Vec<u8> {
    let mut source = storage::wire(2, json!({"canvasW":320,"canvasH":180,"fps":24.0}));
    source["assets"] = json!([
        asset("large-a", 2048, 1535),
        asset("large-b", 2048, 1535),
        asset("small", 1, 1)
    ]);
    source["assetInventory"] = json!({"assetIds":["large-a","large-b","small"],"placements":[]});
    let initial_length = serde_json::to_vec(&source).unwrap().len();
    let remaining = ENVELOPE_LIMIT
        .checked_sub(512 + initial_length)
        .expect("the two actual stored PNGs must leave calibration room");
    // RGBA bytes grow by four per pixel and base64 by four per three bytes.
    // One stored row stays below a zlib block; only its width is calibrated.
    let width = u32::try_from(1 + remaining * 3 / 16).unwrap();
    assert!(width <= 8192 && width as usize * 4 + 1 < 65535);
    source["assets"][2] = asset("small", width, 1);
    let raw = serde_json::to_vec(&source).unwrap();
    assert!(
        raw.len() <= ENVELOPE_LIMIT && raw.len() > ENVELOPE_LIMIT - 768,
        "one calculated width must place the actual catalog within the stated margin"
    );
    raw
}

#[test]
fn composition_candidate_save_budget_refuses_before_publication_but_small_edit_succeeds() {
    let input = near_limit_catalog();
    // The real strict owner validates every PNG/catalog entry and retains its bytes.
    let mut owner =
        NativeObjectHistory::from_project_file("composition-save-budget", &input).unwrap();
    drop(input);
    let old = owner.acquire_snapshot(0).unwrap();
    let before = owner.serialize_project_file(0, old.snapshot_id()).unwrap();
    assert!(
        before.len() <= ENVELOPE_LIMIT && before.len() > ENVELOPE_LIMIT - 768,
        "the actual save encoder, not only source JSON, is near its unchanged limit"
    );
    assert_eq!(owner.history_depths(), (0, 0));
    let request = json!({"command":"object.project.composition.edit",
        "action":{"kind":"setCompositionMetadata","markers":[{"frame":0.0,"name":"x".repeat(1024)}]}});
    let raw = serde_json::to_vec(&request).unwrap();
    assert!(
        raw.len() < 4096,
        "the legal bounded request must reach candidate save admission"
    );
    assert_eq!(
        owner.prepare_composition_metadata_json(&raw).unwrap_err(),
        DispatchErrorCode::Unavailable
    );
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(owner.history_depths(), (0, 0));
    assert!(
        owner.serialize_project_file(0, old.snapshot_id()).unwrap() == before,
        "refusal preserves the exact still-saveable owner bytes"
    );
    assert!(old
        .project()
        .unwrap()
        .project()
        .composition_markers()
        .is_none());

    let small = json!({"command":"object.project.composition.edit",
        "action":{"kind":"setCompositionMetadata","bpm":140.0}});
    let candidate = owner
        .prepare_composition_metadata_json(&serde_json::to_vec(&small).unwrap())
        .unwrap();
    assert_eq!(owner.commit_project_settings(candidate), Ok(1));
    assert_eq!(owner.history_depths(), (1, 0));
    let next = owner.acquire_snapshot(1).unwrap();
    assert_eq!(
        next.project().unwrap().project().composition_bpm(),
        (Some(140.0), None, None)
    );
    assert!(std::ptr::eq(old.document(), next.document()));
    assert!(
        owner
            .serialize_project_file(1, next.snapshot_id())
            .unwrap()
            .len()
            <= ENVELOPE_LIMIT
    );
    assert!(
        old.serialize_project_file().unwrap() == before,
        "the held old snapshot remains exactly saveable"
    );
    assert_eq!(
        old.project().unwrap().project().composition_bpm(),
        (None, None, None)
    );
}
