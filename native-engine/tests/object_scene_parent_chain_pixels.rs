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
    // Persisted raster x/y are the centre (original serR). For the original
    // 2x2 PNG, centre(241,131) places its display top-left at(240,130),
    // outside every glyph; the parent's(40,0) then puts its first texel at(280,130).
    doc["items"]["raster"][0]["item"]["x"] = json!(241);
    doc["items"]["raster"][0]["item"]["y"] = json!(131);
    let raw_raster = &doc["items"]["raster"][0]["item"];
    assert_eq!(raw_raster["x"], 241);
    assert_eq!(raw_raster["y"], 131);
    assert_eq!(raw_raster["width"], 2);
    assert_eq!(raw_raster["height"], 2);
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
#[test]
fn rotated_and_scaled_parent_moves_text_and_raster_to_hand_computed_metal_pixels() {
    // Parent P (hidden 0..40 x 0..20, so pivot c = (20,10)): scale 200%, rotate +90 deg, position (150,30).
    // Hand map p' = c + R(2 (p - c)) + (150,30) with R(dx,dy) = (-dy,dx): x' = 190 - 2 py, y' = 2 px.
    // Source probes are on the Roboto H of "Hello" at the origin (stems x 8.25..17.68 / 53.52..62.89,
    // y 8.9..80, crossbar y 39.47..47.14 between them) and sit on pixel centres of the destination.
    let mut doc = drawing();
    doc["items"]["raster"][0]["item"]["x"] = json!(50);
    doc["items"]["raster"][0]["item"]["y"] = json!(90);
    let case: Value = include_str!("fixtures/text-glyph-source/oracle.txt")
        .lines()
        .filter_map(|l| l.strip_prefix("CASE "))
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .find(|v| v["name"] == "hello_roboto_100")
        .unwrap();
    doc["items"]["text"][0]["block"] = json!({"text":case["text"],"font":{"kind":"bundled","fontKey":case["font"]},
        "size":case["size"],"color":"#112233","align":case["align"],"anchorTopLeft":{"x":case["tl"][0],"y":case["tl"][1]}});
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"P"},"properties":[
        {"property":"position","static":[150,30]},{"property":"rotation","static":[90]},
        {"property":"scale","static":[200,200]}]}]);
    let o = owner(&doc);
    let s = snapshot(&o);
    let before = crate::object_codec::encode_storage_project(s.document()).unwrap();
    let p = plan(&s, 7);
    let chain = p.native_parent_chains().chain(0).unwrap();
    assert_eq!(chain.len(), 1);
    assert_eq!(chain[0].pivot, [20.0, 10.0]);
    let pins = pins(&s, 7);
    let view = prepare_object_viewport_composition(&p, &pins, None).unwrap();
    let export = prepare_object_composition(&p, &pins).unwrap();
    let mut gpu = Compositor::new().expect("rotated parent acceptance requires real Metal");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    let shown = render(&mut gpu, &view);
    assert_eq!(shown, render(&mut gpu, &export));
    let ink = [17, 34, 51, 255];
    // Glyph: stems, crossbar (solid) and the hole above the bar (white), plus the untouched far corner.
    assert_eq!(pixel(&shown, 129, 26), ink, "left stem (13.25,30.25)");
    assert_eq!(pixel(&shown, 129, 116), ink, "right stem (58.25,30.25)");
    assert_eq!(pixel(&shown, 103, 70), ink, "crossbar (35.25,43.25)");
    assert_eq!(pixel(&shown, 141, 70), [255; 4], "hole (35.25,24.25)");
    assert_eq!(pixel(&shown, 319, 179), [255; 4]);
    // Raster: the 2x2 PNG becomes a 4x4 px square centred on (10,100) rotated +90 deg, so source texel
    // (tx,ty) lands at centre offset R((2tx-1, 2ty-1)); its outermost corner pixel is pure (edge pad).
    let mut reader = png::Decoder::new(std::io::Cursor::new(support::png()))
        .read_info()
        .unwrap();
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let _ = reader.next_frame(&mut rgba).unwrap();
    for (tx, ty, x, y) in [
        (0, 0, 11, 98),
        (1, 0, 11, 101),
        (0, 1, 8, 98),
        (1, 1, 8, 101),
    ] {
        let t = &rgba[(ty * 2 + tx) * 4..(ty * 2 + tx) * 4 + 4];
        let a = f64::from(t[3]) / 255.0;
        let want: Vec<u8> = (0..3)
            .map(|k| (f64::from(t[k]) * a + 255.0 * (1.0 - a)).round() as u8)
            .chain([255])
            .collect();
        let got = pixel(&shown, x, y);
        assert!(
            got.iter().zip(&want).all(|(g, w)| g.abs_diff(*w) <= 1),
            "texel ({tx},{ty}) at ({x},{y}): {got:?} != {want:?}"
        );
    }
    assert_eq!(
        crate::object_codec::encode_storage_project(s.document()).unwrap(),
        before
    );
}
