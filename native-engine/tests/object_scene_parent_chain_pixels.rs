//! Original parent-chain Metal controls, kept in the same actual_pixels module.
use super::*;
use crate::compositor::Compositor;
use crate::object_render_scene::prepare_scheduled_object_composition_from_witness;
use crate::png_output::encode_scheduled_object_png;
use crate::scheduler::{
    EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
};
fn render(gpu: &mut Compositor, w: &ObjectCompositionWitness) -> Vec<u8> {
    let s = w.plan().packet().snapshot();
    let f = w.plan().packet().frame();
    let pins = ObjectSchedulePins {
        instance_id: s.instance_id().into(),
        document_id: s.document_id().into(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame: f,
        scope: ObjectScheduleScope::Authored,
    };
    let key = EvaluationKey::new(
        s.snapshot_id(),
        "scene-root",
        f,
        "final",
        OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap(),
        w.plan().scene_resource_versions().unwrap(),
    )
    .unwrap();
    let mut scheduler = FrameScheduler::new();
    let work = scheduler.schedule_object(s.clone(), pins, key).unwrap();
    let bytes = {
        let live = prepare_scheduled_object_composition_from_witness(&scheduler, &work, w).unwrap();
        let result = gpu.compose_scheduled_object(&live).unwrap();
        let read = gpu.readback_scheduled_object_rgba8(&result, &live).unwrap();
        let bytes = read.bytes(&live).unwrap().to_vec();
        if !w.is_viewport() {
            let png = encode_scheduled_object_png(gpu, &result, &live, 1024 * 1024).unwrap();
            let mut d = png::Decoder::new(std::io::Cursor::new(png.bytes(&live).unwrap()))
                .read_info()
                .unwrap();
            let mut rgba = vec![0; d.output_buffer_size().unwrap()];
            let n = d.next_frame(&mut rgba).unwrap();
            rgba.truncate(n.buffer_size());
            assert_eq!(
                rgba, bytes,
                "actualparentedviewport/export PNG pixel parity"
            );
        }
        bytes
    };
    scheduler.succeed(work.work_id()).unwrap();
    let c = scheduler.lease_counters();
    assert_eq!((c.acquired(), c.released(), c.live()), (3, 3, 0));
    bytes
}
fn pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * 320 + x) * 4..(y * 320 + x + 1) * 4]
        .try_into()
        .unwrap()
}
#[test]
fn real_v1_explicit_migration_metal_pixels_equal_existing_unparented_scene() {
    let original = owner(&v1_drawing());
    let original_s = snapshot(&original);
    let original_bytes =
        crate::object_codec::encode_storage_project(original_s.document()).unwrap();
    let baseline = super::super::tests::drawing();
    // Use the existing explicit migration port and the original baseline
    // plan. Production gets no implicit order or raw-v1 paint fallback.
    let plan_bytes = serde_json::to_vec(&baseline["compositionOrder"]).unwrap();
    let migrated =
        crate::object_codec::migrate_ordered_project(original_s.document(), &plan_bytes).unwrap();
    let o = owner(&serde_json::to_value(migrated).unwrap());
    let s = snapshot(&o);
    let p = direct_plan(&s);
    let pin = pins(&s, 0);
    let base = super::super::tests::owner(&baseline);
    let base_s = snapshot(&base);
    let base_plan = super::super::tests::plan(&base_s, 0);
    let base_pin = pins(&base_s, 0);
    let mut gpu = Compositor::new().expect("v1 explicit migration regression requires real Metal");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    let expected = render(
        &mut gpu,
        &prepare_object_viewport_composition(&base_plan, &base_pin, None).unwrap(),
    );
    assert_eq!(
        render(
            &mut gpu,
            &prepare_object_viewport_composition(&p, &pin, None).unwrap()
        ),
        expected
    );
    assert_eq!(
        render(&mut gpu, &prepare_object_composition(&p, &pin).unwrap()),
        expected
    );
    assert_eq!(original_s.document().format_version(), 1);
    assert!(original_s.document().composition_order.value().is_none());
    assert_eq!(
        crate::object_codec::encode_storage_project(original_s.document()).unwrap(),
        original_bytes
    );
    assert_eq!(original.history_depths(), (0, 0));
}
#[test]
fn real_parented_text_and_raster_metal_viewport_export_and_original_h_oracle() {
    let mut doc = drawing();
    // Keep the raster probes outside every glyph: the original image location
    // would overlap the translated H crossbar and could hide a raster-join failure.
    doc["items"]["raster"][0]["item"]["x"] = json!(240);
    doc["items"]["raster"][0]["item"]["y"] = json!(130);
    let case: Value = include_str!("fixtures/text-glyph-source/oracle.txt")
        .lines()
        .filter_map(|l| l.strip_prefix("CASE "))
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .find(|v| v["name"] == "hello_roboto_100")
        .unwrap();
    doc["items"]["text"][0]["block"] = json!({"text":case["text"],"font":{"kind":"bundled","fontKey":case["font"]},
        "size":case["size"],"color":"#112233","align":case["align"],"anchorTopLeft":{"x":case["tl"][0],"y":case["tl"][1]}});
    let o = owner(&doc);
    let s = snapshot(&o);
    let before = crate::object_codec::encode_storage_project(s.document()).unwrap();
    let p = plan(&s, 7);
    assert_eq!(
        p.commands().len(),
        0,
        "hiddenparentgeometrymustnotpaintastand-in"
    );
    let pins = pins(&s, 7);
    let view = prepare_object_viewport_composition(&p, &pins, None).unwrap();
    let export = prepare_object_composition(&p, &pins).unwrap();
    let mut gpu = Compositor::new().expect("parentTextacceptance requires real Metal");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    let shown = render(&mut gpu, &view);
    assert_eq!(shown, render(&mut gpu, &export));
    // Existing literal H solid/hole probes, translated by actualparentPosition(40,0).
    assert_eq!(pixel(&shown, 52, 20), [17, 34, 51, 255]);
    assert_eq!(pixel(&shown, 70, 42), [17, 34, 51, 255]);
    assert_eq!(pixel(&shown, 70, 20), [255; 4]);
    assert_eq!(pixel(&shown, 319, 179), [255; 4]);
    // Actual immutable source PNG's opaque first pixel moved from(240,130)to(280,130), outside all glyphs.
    let mut reader = png::Decoder::new(std::io::Cursor::new(support::png()))
        .read_info()
        .unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let _ = reader.next_frame(&mut rgba).unwrap();
    assert_eq!(&rgba[..4], &[17, 34, 51, 255]);
    assert_eq!(pixel(&shown, 280, 130), [17, 34, 51, 255]);
    assert_eq!(pixel(&shown, 240, 130), [255; 4]);
    assert_eq!(
        crate::object_codec::encode_storage_project(s.document()).unwrap(),
        before
    );
}
