//! Bitmap orchestration uses the real charged baker, PNG resource admission and one project publication.
use super::*;
pub(super) fn assert_native_bitmap_resources(owner: &NativeObjectHistory, expected: &Value) {
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    for (frame, f) in expected.as_array().unwrap().iter().enumerate() {
        for record in f["strokes"].as_array().unwrap() {
            let Some(size) = record["src"]
                .as_str()
                .and_then(|s| s.strip_prefix("baseline-bitmap:"))
            else {
                continue;
            };
            let (w, h) = size.split_once('x').unwrap();
            let dimensions = (w.parse::<u32>().unwrap(), h.parse::<u32>().unwrap());
            let item = snapshot
                .document()
                .items
                .value()
                .unwrap()
                .raster_entries()
                .iter()
                .find(|r| {
                    r.target.frame_scope.frame == frame as u32
                        && r.item.brush_group_id() == record["brushGroupId"].as_str()
                })
                .unwrap();
            let asset = snapshot
                .project()
                .unwrap()
                .asset_inventory()
                .unwrap()
                .records()
                .iter()
                .find(|r| r.id() == item.item.asset_id())
                .unwrap();
            let png = asset.source().encoded_allocation().unwrap();
            let decoder = png::Decoder::new(std::io::Cursor::new(png.as_slice()));
            let mut reader = decoder.read_info().unwrap();
            let mut bytes = vec![0; reader.output_buffer_size().unwrap()];
            let output = reader.next_frame(&mut bytes).unwrap();
            assert_eq!((output.width, output.height), dimensions);
            assert_eq!(output.color_type, png::ColorType::Rgba);
            assert!(bytes[..output.buffer_size()]
                .chunks_exact(4)
                .any(|p| p[3] > 0));
        }
    }
}
fn owner() -> NativeObjectHistory {
    let case = fixture_cases()
        .into_iter()
        .find(|c| c["input"]["name"] == "bitmap-both")
        .unwrap();
    let doc = document(&case["input"]["frames"], true);
    let p = crate::object_document::ProjectEnvelope::no_media(
        crate::object_document::ProjectMetadata::new(320, 180, 24.).unwrap(),
        doc,
    )
    .with_asset_inventory(Some(assets::images_only()));
    NativeObjectHistory::from_admitted_project("bitmap", p).unwrap()
}
#[test]
fn native_tween_bitmap_bakes_are_atomic_stale_safe_and_reuse_identical_png_resources() {
    let mut o = owner();
    let snapshot = o.acquire_snapshot(0).unwrap();
    let initial = snapshot.serialize_project_file().unwrap();
    let payload = json!({"command":"object.tween.generate","layerUid":"L0","options":{"resamplePoints":12,"step":1,"skipManual":true,"easing":[]}});
    let sealed = o
        .owner
        .prepare_tween_json(&serde_json::to_vec(&payload).unwrap())
        .unwrap();
    assert_eq!(o.content_revision(), 0);
    assert_eq!(snapshot.serialize_project_file().unwrap(), initial);
    let q = request(&o, "generate", "tween.generate", payload.clone());
    assert_eq!(dispatch(&mut o, &q)["ok"], true);
    let generated = o
        .acquire_snapshot(1)
        .unwrap()
        .serialize_project_file()
        .unwrap();
    assert_eq!(
        o.owner.commit_tween(sealed),
        Err(crate::request_receipts::DispatchErrorCode::StaleRevision)
    );
    assert_eq!(
        o.acquire_snapshot(1)
            .unwrap()
            .serialize_project_file()
            .unwrap(),
        generated
    );
    let count = o
        .acquire_snapshot(1)
        .unwrap()
        .project()
        .unwrap()
        .asset_inventory()
        .unwrap()
        .records()
        .len();
    let q = request(&o, "regenerate", "tween.generate", payload);
    assert_eq!(dispatch(&mut o, &q)["ok"], true);
    assert_eq!(
        o.acquire_snapshot(2)
            .unwrap()
            .project()
            .unwrap()
            .asset_inventory()
            .unwrap()
            .records()
            .len(),
        count
    );
    assert_eq!(
        o.acquire_snapshot(2)
            .unwrap()
            .serialize_project_file()
            .unwrap(),
        generated
    );
    assert_eq!(snapshot.serialize_project_file().unwrap(), initial);
}

#[test]
fn native_tween_bitmap_ghost_admission_materializes_through_existing_brush_runtime() {
    use crate::application::ObjectBrushRuntime;
    use crate::object_render_scene::{ordered_entries, PaintEntry};
    let mut owner = owner();
    let command = request(
        &owner,
        "generate-scene",
        "tween.generate",
        json!({"command":"object.tween.generate","layerUid":"L0"}),
    );
    assert_eq!(dispatch(&mut owner, &command)["ok"], true);
    let snapshot = owner.acquire_snapshot(1).unwrap();
    let q = json!({"atRevision":1,"documentSnapshotId":snapshot.snapshot_id(),"contextId":"scene-root",
        "layerUid":"L0","frame":0,"onion":true,"ghostAll":false,"from":0,"to":2,
        "previousOpacity":30,"nextOpacity":30,"mode":"default"});
    let read = snapshot
        .query_tween_ghosts_json(&serde_json::to_vec(&q).unwrap())
        .unwrap();
    drop(owner);
    let mut runtime = ObjectBrushRuntime::default();
    for index in 0..read.frames().len() {
        let admission = read.prepare_brush_admission(index).unwrap();
        let plan = runtime.materialize(&admission, &mut |_| true).unwrap();
        assert_eq!(
            ordered_entries(&plan)
                .filter(|e| matches!(e, PaintEntry::Brush(_)))
                .count(),
            1
        );
        assert_eq!(plan.packet().document_snapshot_id(), snapshot.snapshot_id());
        assert!(!plan.scene_resource_versions().unwrap().is_empty());
    }
}
