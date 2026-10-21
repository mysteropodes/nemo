//! Real Metal Text pixels through the shared scheduled viewport/export owner.
//! Expected geometry/probes reuse the shipped opentype/Paper oracle, never GPU output.
use super::*;
use crate::compositor::Compositor;
use crate::object_render_scene::{
    prepare_scheduled_object_composition_from_witness, ObjectCompositionWitness,
};
use crate::png_output::encode_scheduled_object_png;
use crate::request_receipts::OpacityRequest;
use crate::scheduler::{
    EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
};
use std::sync::Arc;
use vello::kurbo::Shape;

const FONT: &[u8] = include_bytes!("../../src/fonts/Roboto-Regular.ttf");
const FONT_VERSION: &str = "e9ff3730797df6aaf76d37c48234a6c4875032eaad8f1aaccd7daefa6dca9e7a";
const PNG_VERSION: &str = "273e60d11e47acf74c267b01fb653644c6ba2f3c9a4ff3554972999b6d3d33bb";
const WIDTH: usize = 320;
const HEIGHT: usize = 180;
const GLYPH_ORACLE: &str = include_str!("fixtures/text-glyph-source/oracle.txt");

fn metal() -> Compositor {
    let gpu = Compositor::new().expect("Text GPU acceptance requires a real Metal adapter");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    gpu
}
fn document() -> Value {
    let mut doc = drawing();
    doc["objects"] = json!([]);
    doc["items"]["raster"][0]["item"] = support::raster_item("img-1", 280, 150, 2, 2);
    doc["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = json!([
        {"layerUid":"L0","strokeId":"text","naturalPosition":0,"order":{}},
        {"layerUid":"L0","strokeId":"image","naturalPosition":1,"order":{}}]);
    doc
}
fn versions(w: &ObjectCompositionWitness, font_id: &str) {
    let resources = w
        .scene_resources()
        .expect("actual font and raster resources");
    assert_eq!(resources.len(), 2);
    assert_eq!(resources.get(font_id).unwrap().version(), FONT_VERSION);
    assert_eq!(
        resources.get(font_id).unwrap().charged_bytes(),
        FONT.len() as u64
    );
    assert_eq!(
        resources.get("raster-asset:img-1").unwrap().version(),
        PNG_VERSION
    );
    assert_eq!(
        resources.get("raster-asset:img-1").unwrap().charged_bytes(),
        16
    );
    assert_eq!(resources.total_charged_bytes(), FONT.len() as u64 + 16);
    let declared = w
        .plan()
        .scene_resource_versions()
        .unwrap()
        .into_iter()
        .collect();
    w.verify_scene_resources(&declared).unwrap();
}
fn decode_png(bytes: &[u8]) -> Vec<u8> {
    let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
        .read_info()
        .unwrap();
    assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);
    assert_eq!((reader.info().width, reader.info().height), (320, 180));
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    rgba.truncate(info.buffer_size());
    assert_eq!(rgba.len(), WIDTH * HEIGHT * 4);
    rgba
}
fn render(gpu: &mut Compositor, witness: &ObjectCompositionWitness) -> Vec<u8> {
    let packet = witness.plan().packet();
    let snapshot = packet.snapshot();
    let frame = packet.frame();
    let pins = ObjectSchedulePins {
        instance_id: snapshot.instance_id().into(),
        document_id: snapshot.document_id().into(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame,
        scope: ObjectScheduleScope::Authored,
    };
    let key = EvaluationKey::new(
        snapshot.snapshot_id(),
        "scene-root",
        frame,
        "final",
        OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap(),
        witness.plan().scene_resource_versions().unwrap(),
    )
    .unwrap();
    let mut scheduler = FrameScheduler::new();
    let work = scheduler
        .schedule_object(snapshot.clone(), pins, key)
        .unwrap();
    let bytes = {
        let live =
            prepare_scheduled_object_composition_from_witness(&scheduler, &work, witness).unwrap();
        let result = gpu.compose_scheduled_object(&live).unwrap();
        assert_eq!(
            result.identity().evaluation_key().document_snapshot_id(),
            snapshot.snapshot_id()
        );
        let pixels = gpu.readback_scheduled_object_rgba8(&result, &live).unwrap();
        assert_eq!(pixels.dimensions(), (320, 180));
        let bytes = pixels.bytes(&live).unwrap().to_vec();
        if witness.is_viewport() {
            assert!(
                encode_scheduled_object_png(gpu, &result, &live, 1024 * 1024).is_err(),
                "viewport witness must not become an artwork PNG"
            );
        } else {
            let png = encode_scheduled_object_png(gpu, &result, &live, 1024 * 1024).unwrap();
            assert_eq!(png.dimensions(), (320, 180));
            assert_eq!(
                decode_png(png.bytes(&live).unwrap()),
                bytes,
                "real encoded export must independently decode to GPU readback"
            );
        }
        bytes
    };
    scheduler.succeed(work.work_id()).unwrap();
    let leases = scheduler.lease_counters();
    assert_eq!(
        (leases.acquired(), leases.released(), leases.live()),
        (3, 3, 0)
    );
    bytes
}
fn pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[(y * WIDTH + x) * 4..(y * WIDTH + x + 1) * 4]
        .try_into()
        .unwrap()
}
fn witnesses(snapshot: &ObjectSnapshot) -> (ObjectCompositionWitness, ObjectCompositionWitness) {
    let p = plan(snapshot, 7);
    assert_eq!(
        p.commands().len(),
        0,
        "Text must really paint without a vector stand-in"
    );
    assert_eq!(labels(&p), ["scene-root:L0:text", "image"]);
    let pins = selector(snapshot, 7);
    (
        prepare_object_viewport_composition(&p, &pins, None).unwrap(),
        prepare_object_composition(&p, &pins).unwrap(),
    )
}
fn assert_a_bounds(w: &ObjectCompositionWitness) {
    let text = ordered_entries(w.plan())
        .find_map(|entry| match entry {
            PaintEntry::Text(text) => Some(text),
            _ => None,
        })
        .unwrap();
    assert_eq!(text.item.scene().contours().len(), 3);
    let bounds = text
        .item
        .scene()
        .contours()
        .iter()
        .map(|c| c.contour.path().bounding_box())
        .reduce(|a, b| a.union(b))
        .unwrap();
    // The exact existing bundled-A CPU/SVG oracle, not a new GPU-derived oracle.
    for (actual, expected) in [bounds.x0, bounds.y0, bounds.x1, bounds.y1]
        .into_iter()
        .zip([10.33984375, 22.1375, 25.3515625, 39.2])
    {
        assert!((actual - expected).abs() < 1e-6, "{actual} != {expected}");
    }
}
fn assert_a_ink(bytes: &[u8]) {
    assert!(
        (22..40).any(|y| (10..26).any(|x| pixel(bytes, x, y) != [255; 4])),
        "a blank/encoding-only Text path cannot pass equality"
    );
    assert_eq!(pixel(bytes, 0, 0), [255; 4]);
    assert_eq!(pixel(bytes, 319, 179), [255; 4]);
}

#[test]
fn gpu_real_bundled_text_matches_original_ink_and_viewport_export_png() {
    let mut gpu = metal();
    let o = owner(&document());
    let snapshot = support::head(&o);
    let before = support::state(&o);
    let (view, export) = witnesses(&snapshot);
    versions(&view, "font:Roboto-Regular");
    versions(&export, "font:Roboto-Regular");
    assert_a_bounds(&export);
    let shown = render(&mut gpu, &view);
    assert_a_ink(&shown);
    assert_eq!(shown, render(&mut gpu, &export));
    assert_eq!(support::state(&o), before);

    // Existing baseline CASE provides independent full H paths and solid interior probes.
    let case: Value = GLYPH_ORACLE
        .lines()
        .filter_map(|line| line.strip_prefix("CASE "))
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["name"] == "hello_roboto_100")
        .unwrap();
    assert_eq!(case["font"], "Roboto-Regular");
    assert_eq!(case["text"], "Hello");
    let mut doc = document();
    doc["items"]["text"][0]["block"] = json!({"text":case["text"],
        "font":{"kind":"bundled","fontKey":case["font"]}, "size":case["size"],
        "color":"#112233", "align":case["align"],
        "anchorTopLeft":{"x":case["tl"][0],"y":case["tl"][1]}});
    let hello = owner(&doc);
    let snap = support::head(&hello);
    let (view, export) = witnesses(&snap);
    let text = ordered_entries(export.plan())
        .find_map(|e| match e {
            PaintEntry::Text(text) => Some(text),
            _ => None,
        })
        .unwrap();
    assert_eq!(text.item.scene().contours().len(), 7);
    for (contour, expected) in text
        .item
        .scene()
        .contours()
        .iter()
        .take(3)
        .zip(case["glyphs"][0]["paths"].as_array().unwrap())
    {
        let segments = contour.contour.segments();
        let oracle = expected["segs"].as_array().unwrap();
        assert_eq!(segments.len(), oracle.len());
        for (segment, row) in segments.iter().zip(oracle) {
            let actual = [
                segment.point[0],
                segment.point[1],
                segment.handle_in[0],
                segment.handle_in[1],
                segment.handle_out[0],
                segment.handle_out[1],
            ];
            let expected: Vec<f64> = row
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_f64().unwrap())
                .collect();
            assert_eq!(
                actual.as_slice(),
                expected.as_slice(),
                "original literal H geometry"
            );
        }
    }
    let shown = render(&mut gpu, &view);
    assert_eq!(shown, render(&mut gpu, &export));
    // Strict solid interiors of H's pinned rectangles; no antialiased edge tolerance.
    assert_eq!(pixel(&shown, 12, 20), [17, 34, 51, 255]);
    assert_eq!(pixel(&shown, 30, 42), [17, 34, 51, 255]);
    assert_eq!(pixel(&shown, 30, 20), [255; 4]);
    assert_eq!(pixel(&shown, 319, 179), [255; 4]);
}

#[test]
fn gpu_retained_document_font_and_raster_survive_owner_drop_with_original_pixels() {
    let mut o = owner(&document());
    let original = support::head(&o);
    let (old_view, old_export) = witnesses(&original);
    let mut gpu = metal();
    let before = render(&mut gpu, &old_view);
    assert_a_bounds(&old_export);
    assert_a_ink(&before);
    let request = OpacityRequest::history_stage(
        "gpu-font-import",
        o.instance_id(),
        o.document_id(),
        Some(o.content_revision()),
        "command.document.object.asset.font.import",
        json!({"command":"object.asset.font.import", "documentSnapshotId":original.snapshot_id(),
            "lifecycleGeneration":1,"source":{"kind":"bundled","fontKey":"Roboto-Regular"}}),
    );
    let response = o.dispatch_font_import(&request, FONT).unwrap();
    assert!(response.ok, "{:?}", response.error);
    let asset = response.result.unwrap()["assetId"]
        .as_str()
        .unwrap()
        .to_owned();
    support::run(
        &mut o,
        &json!({"command":"object.text.edit", "stableTarget":support::target("L0","text"),
        "patch":{"font":{"kind":"asset","assetId":asset}}}),
    )
    .unwrap();
    let snap = support::head(&o);
    let (view, export) = witnesses(&snap);
    let font_id = format!("font:{asset}");
    versions(&view, &font_id);
    versions(&export, &font_id);
    assert_a_bounds(&export);
    assert_ne!(
        export.document_snapshot_id(),
        old_export.document_snapshot_id()
    );
    let font = Arc::downgrade(
        export
            .scene_resources()
            .unwrap()
            .get(&font_id)
            .unwrap()
            .retention(),
    );
    let image = export.raster_images().next().unwrap().data.clone();
    let image_id = image.id();
    let view_image_id = view.raster_images().next().unwrap().data.id();
    let (image_storage, _) = image.into_raw_parts();
    let pixels = Arc::downgrade(&image_storage);
    assert_eq!(
        view.raster_images().next().unwrap().data.as_ref(),
        export.raster_images().next().unwrap().data.as_ref()
    );
    let mut wrong = export
        .plan()
        .scene_resource_versions()
        .unwrap()
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();
    wrong.insert(font_id.clone(), "foreign-font-version".into());
    assert!(export.verify_scene_resources(&wrong).is_err());
    drop(image_storage);
    drop(snap);
    drop(original);
    drop(o);
    assert!(font.upgrade().is_some() && pixels.upgrade().is_some());
    assert_eq!(
        view.raster_images().next().unwrap().data.id(),
        view_image_id
    );
    assert_eq!(export.raster_images().next().unwrap().data.id(), image_id);
    let shown = render(&mut gpu, &view);
    assert_a_ink(&shown);
    assert_eq!(
        shown, before,
        "same encoded font and immutable block after native font edit"
    );
    assert_eq!(shown, render(&mut gpu, &export));
    assert_eq!(
        render(&mut gpu, &old_export),
        before,
        "old witness remains unchanged after owner disposal"
    );
    drop(old_view);
    drop(old_export);
    drop(view);
    drop(export);
    // Vello may still retain its real raster Blob; the compositor is an actual holder.
    drop(gpu);
    assert!(
        font.upgrade().is_none(),
        "last font retention owner released"
    );
    assert!(
        pixels.upgrade().is_none(),
        "last raster Blob/charge holder released"
    );
}

#[test]
fn gpu_stored_text_animator_and_layer_motion_share_original_glyphs_and_export_pixels() {
    let case: Value = GLYPH_ORACLE
        .lines()
        .filter_map(|line| line.strip_prefix("CASE "))
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .find(|row| row["name"] == "hello_roboto_100")
        .unwrap();
    let mut doc = document();
    doc["items"]["text"][0]["block"] = json!({"text":case["text"],
        "font":{"kind":"bundled","fontKey":case["font"]}, "size":case["size"],
        "color":"#112233", "align":case["align"],
        "anchorTopLeft":{"x":case["tl"][0],"y":case["tl"][1]}});
    doc["layers"][0]["textAnimators"] = json!([{"id":"retained", "selector":{},
        "props":{"position":[20,0]}, "properties":[{"property":"amount","track":{"keys":[
            {"frame":0,"v":[0]},{"frame":4,"v":[100]}]}}]}]);
    doc["propertyHolders"] = json!([{"target":{"contextId":"scene-root","layerUid":"L0"},
        "properties":[{"property":"position","static":[7,9]}]}]);
    let o = owner(&doc);
    let before = support::state(&o);
    let snapshot = support::head(&o);
    let (view, export) = witnesses(&snapshot);
    versions(&view, "font:Roboto-Regular");
    versions(&export, "font:Roboto-Regular");
    let text = ordered_entries(export.plan())
        .find_map(|entry| match entry {
            PaintEntry::Text(text) => Some(text),
            _ => None,
        })
        .unwrap();
    assert_eq!(text.item.scene().contours().len(), 7);
    for (contour, expected) in text
        .item
        .scene()
        .contours()
        .iter()
        .take(3)
        .zip(case["glyphs"][0]["paths"].as_array().unwrap())
    {
        let oracle = expected["segs"].as_array().unwrap();
        assert_eq!(contour.contour.segments().len(), oracle.len());
        for (segment, row) in contour.contour.segments().iter().zip(oracle) {
            let numbers = row
                .as_array()
                .unwrap()
                .iter()
                .map(|n| n.as_f64().unwrap())
                .collect::<Vec<_>>();
            // Original literal H geometry, TA +20 then own layer +7,+9 exactly once.
            assert_eq!(segment.point, [numbers[0] + 27.0, numbers[1] + 9.0]);
            assert_eq!(segment.handle_in, [numbers[2], numbers[3]]);
            assert_eq!(segment.handle_out, [numbers[4], numbers[5]]);
        }
    }
    let mut gpu = metal();
    let shown = render(&mut gpu, &view);
    assert_eq!(shown, render(&mut gpu, &export));
    assert_eq!(pixel(&shown, 39, 29), [17, 34, 51, 255]);
    assert_eq!(pixel(&shown, 57, 51), [17, 34, 51, 255]);
    assert_eq!(pixel(&shown, 57, 29), [255; 4]);
    assert_eq!(pixel(&shown, 12, 20), [255; 4]);
    assert_eq!(pixel(&shown, 319, 179), [255; 4]);
    assert_eq!(support::state(&o), before);
    // Same original Hello/TA/own-Motion fixture, now through a genuine content-free Null.
    doc["layers"][0]["parentLayerUid"] = json!("N");
    doc["layers"].as_array_mut().unwrap().push(json!({"layerUid":"N","visible":false,
        "isNullLayer":true,"nullPos":[100,80]}));
    doc["compositionOrder"]["contexts"][0]["layers"].as_array_mut().unwrap()
        .push(json!({"layerUid":"N","naturalPosition":1,"order":{}}));
    doc["propertyHolders"].as_array_mut().unwrap().push(json!({
        "target":{"contextId":"scene-root","layerUid":"N"},
        "properties":[{"property":"rotation","static":[180]}]}));
    let null_owner = owner(&doc);
    let null_before = support::state(&null_owner);
    let null_snapshot = support::head(&null_owner);
    let (null_view, null_export) = witnesses(&null_snapshot);
    versions(&null_export, "font:Roboto-Regular");
    let reflected = render(&mut gpu, &null_view);
    assert_eq!(reflected, render(&mut gpu, &null_export));
    // Original ink cell(39,29)/(57,51), rotated180 around(100,80):
    // destination sample cells are(160,130)/(142,108), not rounded edge guesses.
    assert_eq!(pixel(&reflected,160,130), [17,34,51,255]);
    assert_eq!(pixel(&reflected,142,108), [17,34,51,255]);
    assert_eq!(pixel(&reflected,142,130), [255;4]);
    assert_eq!(pixel(&reflected,39,29), [255;4]);
    assert_eq!(support::state(&null_owner), null_before);
    drop(null_snapshot); drop(null_owner);
    assert_eq!(reflected, render(&mut gpu, &null_export));
    drop(snapshot);
    drop(o);
    assert_eq!(shown, render(&mut gpu, &export));
}
