//! Preview and export parity of the editable vector family (L01-I). Preview is the composition the viewport and the headless
//! compositor draw (`compose_object` over the paint plan); export is the scheduled composition that the PNG exporter encodes
//! (`compose_scheduled_object`, `encode_scheduled_object_png`). Both consume ONE paint plan of ONE pinned snapshot and frame,
//! and the plan consumes ONE scene that ONE evaluator produced, so open paths, strokes, dashes, paint order and opacity are
//! drawn by the same code: the frames must be identical byte for byte, and so must the decoded PNG. Declared from
//! object_compositor.rs under `export_job`; a missing real adapter is a test FAILURE, never a skip.
use super::ObjectOutputSpec;
use crate::compositor::{
    object_test_plan as export_plan, object_test_selector as export_selector,
    schedule_test_object as schedule, Compositor,
};
use crate::object_codec::encode_storage_project;
use crate::object_render_scene::{
    prepare_object_composition, prepare_scheduled_object_composition as prepare,
};
use crate::png_output::encode_scheduled_object_png;
use crate::revision::ObjectSnapshot;
use crate::scheduler::FrameScheduler;

#[path = "render_vector_support.rs"]
mod support;
use support::*;

fn headless() -> Compositor {
    Compositor::new().expect("L01-I needs a real headless Metal adapter")
}

fn decode(bytes: &[u8]) -> Vec<u8> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .expect("png header");
    assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    let mut rgba = vec![0; reader.output_buffer_size().expect("png size")];
    let info = reader.next_frame(&mut rgba).expect("png frame");
    rgba.truncate(info.buffer_size());
    rgba
}

/// The viewport/headless composition in the project output mode that the export uses.
fn preview(gpu: &mut Compositor, snapshot: &ObjectSnapshot) -> Vec<u8> {
    let plan = plan_of(snapshot);
    let prepared = prepare_object_composition(&plan, &selector(snapshot, 0)).expect("witness");
    let output = ObjectOutputSpec::new(64, 64, "project-srgb-normal-rgba8-area").expect("output");
    let result = gpu.compose_object(&prepared, output).expect("compose");
    gpu.readback_object_rgba8(&result)
        .expect("readback")
        .bytes()
        .to_vec()
}

/// The scheduled export: raw readback bytes and the decoded PNG of the same result.
fn export(gpu: &mut Compositor, snapshot: &ObjectSnapshot) -> (Vec<u8>, Vec<u8>) {
    let input = export_plan(snapshot, 0);
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, snapshot, 0);
    let (raw, png) = {
        let live = prepare(&scheduler, &work, &input, &export_selector(snapshot, 0)).expect("live");
        let result = gpu.compose_scheduled_object(&live).expect("compose");
        let png = encode_scheduled_object_png(gpu, &result, &live, 256 * 1024).expect("png");
        let raw = gpu
            .readback_scheduled_object_rgba8(&result, &live)
            .expect("raw");
        (
            raw.bytes(&live).expect("raw bytes").to_vec(),
            decode(png.bytes(&live).expect("png bytes")),
        )
    };
    scheduler.succeed(work.work_id()).expect("succeed");
    (raw, png)
}

#[test]
fn preview_export_readback_and_decoded_png_are_identical_for_every_oracle_scene() {
    let mut gpu = headless();
    let mut checked = 0;
    for row in scene_rows() {
        let snap = snapshot(&scene_doc(&row.records));
        let before = encode_storage_project(snap.document()).expect("encode");
        let shown = preview(&mut gpu, &snap);
        let (raw, png) = export(&mut gpu, &snap);
        assert_eq!(shown.len(), SIZE * SIZE * 4, "{}", row.id);
        assert_eq!(shown, raw, "{}: preview vs export readback", row.id);
        assert_eq!(raw, png, "{}: export readback vs decoded PNG", row.id);
        // the shared frame is the certified one, and rendering it did not touch the pinned document
        let failures = probe_failures(&png, &row.probes, 2);
        assert!(failures.is_empty(), "{}: {failures:?}", row.id);
        assert_eq!(
            encode_storage_project(snap.document()).expect("encode"),
            before,
            "{}",
            row.id
        );
        checked += 1;
    }
    assert_eq!(checked, 18);
}

#[test]
fn the_export_of_a_stroked_open_path_is_not_the_export_of_its_fill_alone() {
    let mut gpu = headless();
    let stroked = snapshot(&scene_doc(&scene_row("line-round-cap").records));
    // the same anchors without the stroke: an open horizontal line has no fill area, so the frame is blank
    let mut bare = scene_row("line-round-cap").records;
    bare[0]["vector"]["stroke"] = serde_json::Value::Null;
    bare[0]["vector"]["hasRealStroke"] = serde_json::json!(false);
    let blank = snapshot(&scene_doc(&bare));
    let (with, _) = export(&mut gpu, &stroked);
    let (without, _) = export(&mut gpu, &blank);
    assert_ne!(with, without);
    assert!(without.chunks(4).all(|p| p == [255, 255, 255, 255]));
    assert_eq!(pixel(&with, 32, 32), [255, 0, 0, 255]);
}
