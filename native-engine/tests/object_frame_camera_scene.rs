//! Original stored-camera matrix -> actual retained SVG and scheduled Metal/PNG.
use super::*;
use crate::compositor::Compositor;
use crate::export_job::native_vector_export::native_vector_export_svg::{
    reader, serialize_svg_frame, SvgFrameSpec,
};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_exposure_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
    prepare_object_viewport_composition, prepare_scheduled_object_composition_from_witness,
    ObjectCompositionWitness, ObjectPaintPlan,
};
use crate::png_output::encode_scheduled_object_png;
use crate::scheduler::{
    EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
};

fn document() -> Value {
    let mut value = source();
    let mut drawing = crate::object_render_scene::paint_test_document(&[0]);
    // The original shared fixture reverses encounter order; select the actual
    // L0:a record named by this retained composition plan, not index zero (L2:a).
    let record = drawing["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|record| record["target"]["layerUid"] == "L0" && record["target"]["strokeId"] == "a")
        .unwrap()
        .clone();
    drawing["objects"] = json!([record]);
    drawing["objects"][0]["geometry"]["segments"] = json!([[100, 60], [120, 60], [100, 80]].map(
        |[x, y]| json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})
    ));
    drawing["objects"][0]["fill"] =
        json!({"kind":"solid","r":17.0/255.0,"g":34.0/255.0,"b":51.0/255.0,"a":1});
    drawing["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] =
        json!([{"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}}]);
    for layer in drawing["layers"].as_array_mut().unwrap() {
        layer["timing"] = json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]});
    }
    value["drawing"] = drawing;
    value
}
fn selector(s: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
fn plan(s: &ObjectSnapshot, frame: u32) -> ObjectPaintPlan {
    let pins = selector(s, frame);
    let packet = prepare_object_exposure_frame(s, &pins).unwrap();
    prepare_object_paint_plan(&prepare_object_render_scene(&packet, &pins).unwrap(), &pins).unwrap()
}
fn svg(p: &ObjectPaintPlan) -> Vec<u8> {
    serialize_svg_frame(p, SvgFrameSpec::new(320, 180, 1024 * 1024).unwrap())
        .unwrap()
        .bytes
}
#[test]
fn stored_camera_svg_uses_original_matrix_and_reopen_without_editing_plan_geometry() {
    let o = owner(&document());
    let before = saved(&o);
    let s = snapshot(&o);
    let p = plan(&s, 4);
    assert_eq!(
        p.commands().next().unwrap().contour().segments()[0].point,
        [100., 60.]
    );
    let output = svg(&p);
    assert!(
        std::str::from_utf8(&output)
            .unwrap()
            .contains("overflow=\"hidden\""),
        "camera artwork is explicitly clipped to the output viewport"
    );
    let parsed = reader::parse(&output).unwrap();
    assert_eq!(parsed.paths.len(), 1);
    assert_eq!(parsed.camera_clip, Some([320, 180]));
    assert_eq!(parsed.paths[0].start, [40., 30.]);
    assert_eq!(
        parsed.paths[0]
            .curves
            .iter()
            .map(|c| c[2])
            .collect::<Vec<_>>(),
        [[80., 30.], [40., 70.], [40., 30.]]
    );
    assert_eq!(parsed.paths[0].alpha, 1.);
    let pins = selector(&s, 4);
    let export = prepare_object_composition(&p, &pins).unwrap();
    assert_eq!(export.camera_matrix(), Some([2., 0., 0., 2., -160., -90.]));
    let viewport = prepare_object_viewport_composition(&p, &pins, None).unwrap();
    assert_eq!(
        viewport.camera_matrix(),
        None,
        "unlocked Canvas is distinct from stored-camera export"
    );
    let mut wrong = pins.clone();
    wrong.frame = 3;
    assert!(prepare_object_composition(&p, &wrong).is_err());
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
    let reopened = NativeObjectHistory::from_project_file("camera-reopen", &before).unwrap();
    assert_eq!(svg(&plan(&snapshot(&reopened), 4)), output);
    drop(o);
    assert_eq!(svg(&p), output);
}

fn pixels(gpu: &mut Compositor, witness: &ObjectCompositionWitness) -> Vec<u8> {
    sized_pixels(gpu, witness, 320, 180)
}
fn sized_pixels(
    gpu: &mut Compositor,
    witness: &ObjectCompositionWitness,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let p = witness.plan().packet();
    let s = p.snapshot();
    let pins = ObjectSchedulePins {
        instance_id: s.instance_id().into(),
        document_id: s.document_id().into(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame: p.frame(),
        scope: ObjectScheduleScope::Authored,
    };
    let key = EvaluationKey::new(
        s.snapshot_id(),
        "scene-root",
        p.frame(),
        "final",
        OutputSpec::new("frame", "rgba8", width, height, "srgb", "straight").unwrap(),
        witness.plan().scene_resource_versions().unwrap(),
    )
    .unwrap();
    let mut scheduler = FrameScheduler::new();
    let work = scheduler.schedule_object(s.clone(), pins, key).unwrap();
    let live =
        prepare_scheduled_object_composition_from_witness(&scheduler, &work, witness).unwrap();
    let result = gpu.compose_scheduled_object(&live).unwrap();
    let read = gpu.readback_scheduled_object_rgba8(&result, &live).unwrap();
    let bytes = read.bytes(&live).unwrap().to_vec();
    if !witness.is_viewport() {
        let encoded = encode_scheduled_object_png(gpu, &result, &live, 1024 * 1024).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(encoded.bytes(&live).unwrap()))
            .read_info()
            .unwrap();
        assert_eq!((reader.info().width, reader.info().height), (width, height));
        let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut decoded).unwrap();
        decoded.truncate(info.buffer_size());
        assert_eq!(
            decoded, bytes,
            "independent PNG decode equals actual Metal readback"
        );
    }
    bytes
}
fn at(bytes: &[u8], x: usize, y: usize) -> &[u8] {
    &bytes[(y * 320 + x) * 4..(y * 320 + x) * 4 + 4]
}
#[test]
fn stored_camera_whole_paint_matches_original_literal_pixels_and_canvas_bypass() {
    let o = owner(&document());
    let s = snapshot(&o);
    let p = plan(&s, 4);
    let pins = selector(&s, 4);
    let mut gpu = Compositor::new().expect("camera acceptance requires actual Metal adapter");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    let export = prepare_object_composition(&p, &pins).unwrap();
    let viewport = prepare_object_viewport_composition(&p, &pins, None).unwrap();
    let e = pixels(&mut gpu, &export);
    let v = pixels(&mut gpu, &viewport);
    assert_eq!(at(&e, 50, 40), [17, 34, 51, 255]);
    assert_eq!(at(&e, 105, 65), [255; 4]);
    assert_eq!(at(&v, 105, 65), [17, 34, 51, 255]);
    assert_eq!(at(&v, 50, 40), [255; 4]);
    assert_eq!(at(&e, 0, 0), [255; 4]);
    assert_eq!(at(&e, 319, 179), [255; 4]);
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn camera_clip_stays_project_sized_in_larger_real_png_and_svg_outputs() {
    let mut source = document();
    for segment in source["drawing"]["objects"][0]["geometry"]["segments"]
        .as_array_mut()
        .unwrap()
    {
        let x = segment["point"]["x"].as_i64().unwrap();
        segment["point"]["x"] = json!(x + 220);
    }
    let o = owner(&source);
    let before = saved(&o);
    let s = snapshot(&o);
    let p = plan(&s, 0);
    let pins = selector(&s, 0);
    let output =
        serialize_svg_frame(&p, SvgFrameSpec::new(640, 360, 1024 * 1024).unwrap()).unwrap();
    let parsed = reader::parse(&output.bytes).unwrap();
    assert_eq!((parsed.width, parsed.height), (640, 360));
    assert_eq!(parsed.camera_clip, Some([320, 180]));
    assert_eq!(parsed.paths[0].start, [320., 60.]);
    let export = prepare_object_composition(&p, &pins).unwrap();
    let canvas = prepare_object_viewport_composition(&p, &pins, None).unwrap();
    let mut gpu = Compositor::new().expect("clip acceptance requires actual Metal");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    let e = sized_pixels(&mut gpu, &export, 640, 360);
    let v = sized_pixels(&mut gpu, &canvas, 640, 360);
    let outside = (65 * 640 + 325) * 4;
    assert_eq!(
        &e[outside..outside + 4],
        [255; 4],
        "project clip applies before larger export output"
    );
    assert_eq!(
        &v[outside..outside + 4],
        [17, 34, 51, 255],
        "ordinary Canvas keeps the same outside artwork visible"
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn explicit_camera_viewport_uses_same_retained_camera_without_relabeling_export_or_canvas() {
    use crate::object_render_scene::prepare_object_camera_viewport_composition;
    let o = owner(&document());
    let before = saved(&o);
    let s = snapshot(&o);
    let p = plan(&s, 4);
    let pins = selector(&s, 4);
    let camera = prepare_object_camera_viewport_composition(&p, &pins, None).unwrap();
    assert_eq!(
        camera.viewport_document_transform(),
        Some([2., 0., 0., 2., -160., -90.])
    );
    let canvas = prepare_object_viewport_composition(&p, &pins, Some(&camera)).unwrap();
    assert_eq!(
        canvas.viewport_document_transform(),
        Some([1., 0., 0., 1., 0., 0.])
    );
    let export = prepare_object_composition(&p, &pins).unwrap();
    assert_eq!(export.viewport_document_transform(), None);
    assert_eq!(export.camera_matrix(), camera.camera_matrix());
    let mut wrong = pins.clone();
    wrong.document_snapshot_id = "foreign";
    assert!(prepare_object_camera_viewport_composition(&p, &wrong, None).is_err());
    assert_eq!(
        p.commands().next().unwrap().contour().segments()[0].point,
        [100., 60.]
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
    drop(o);
    assert_eq!(
        camera.viewport_document_transform(),
        Some([2., 0., 0., 2., -160., -90.])
    );
}

#[test]
fn camera_view_keeps_real_reference_fit_and_image_allocation_bound_to_the_same_witness() {
    use crate::object_render_scene::prepare_object_camera_viewport_composition;
    let mut value = document();
    value["viewportReference"] = json!({"kind":"still-image","assetId":"ref","name":"reference.png","visible":true,"opacity":0.5,"offsetFrames":0});
    // Existing literal 2x1 BLUE PNG from native_object_reference_preview_tests.rs.
    value["assets"] = json!([{"id":"ref","mediaType":"image/png","encoding":"base64","data":"iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADUlEQVR4nGNgYPgPBgAU9AX7uVYDfQAAAABJRU5ErkJggg=="}]);
    let o = owner(&value);
    let before = saved(&o);
    let s = snapshot(&o);
    let p = plan(&s, 4);
    let pins = selector(&s, 4);
    let canvas = prepare_object_viewport_composition(&p, &pins, None).unwrap();
    let camera = prepare_object_camera_viewport_composition(&p, &pins, Some(&canvas)).unwrap();
    let (normal, normal_fit, opacity) = canvas.reference_image().unwrap();
    let (locked, locked_fit, locked_opacity) = camera.reference_image().unwrap();
    assert_eq!(normal_fit.as_coeffs(), [160., 0., 0., 160., 0., 10.]);
    assert_eq!(locked_fit.as_coeffs(), [320., 0., 0., 320., -160., -70.]);
    assert_eq!(normal.data.id(), locked.data.id());
    assert_eq!((opacity, locked_opacity), (0.5, 0.5));
    assert!(camera.reference_matches_plan(&p));
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
    let unlocked = prepare_object_viewport_composition(&p, &pins, Some(&camera)).unwrap();
    assert_eq!(
        unlocked.reference_image().unwrap().1.as_coeffs(),
        [160., 0., 0., 160., 0., 10.]
    );
    assert_eq!(
        unlocked.reference_image().unwrap().0.data.id(),
        locked.data.id()
    );
}

#[test]
fn session_navigation_uses_the_sealed_camera_and_reference_once_without_export_or_source_edits() {
    use crate::object_render_scene::prepare_object_navigated_viewport_composition;
    let mut value = document();
    value["viewportReference"] = json!({"kind":"still-image","assetId":"ref","name":"reference.png","visible":true,"opacity":0.5,"offsetFrames":0});
    value["assets"] = json!([{"id":"ref","mediaType":"image/png","encoding":"base64","data":"iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADUlEQVR4nGNgYPgPBgAU9AX7uVYDfQAAAABJRU5ErkJggg=="}]);
    let o = owner(&value);
    let before = saved(&o);
    let s = snapshot(&o);
    let p = plan(&s, 4);
    let pins = selector(&s, 4);
    let base = prepare_object_viewport_composition(&p, &pins, None).unwrap();
    let manual =
        prepare_object_navigated_viewport_composition(&p, &pins, Some(&base), true, 2., [10., -5.])
            .unwrap();
    // Original camera [2,0,0,2,-160,-90]; centred 2x over world pan [10,-5].
    assert_eq!(
        manual.viewport_document_transform(),
        Some([4., 0., 0., 4., -520., -250.])
    );
    let (image, fit, opacity) = manual.reference_image().unwrap();
    assert_eq!(fit.as_coeffs(), [640., 0., 0., 640., -520., -210.]);
    assert_eq!(image.data.id(), base.reference_image().unwrap().0.data.id());
    assert_eq!(opacity, 0.5);
    let p0 = plan(&s, 0);
    let pins0 = selector(&s, 0);
    let scrubbed = prepare_object_navigated_viewport_composition(
        &p0,
        &pins0,
        Some(&manual),
        true,
        2.,
        [10., -5.],
    )
    .unwrap();
    // Same override survives scrub onto original identity-width camera.
    assert_eq!(
        scrubbed.viewport_document_transform(),
        Some([2., 0., 0., 2., -180., -80.])
    );
    let export = prepare_object_composition(&p, &pins).unwrap();
    assert_eq!(export.camera_matrix(), Some([2., 0., 0., 2., -160., -90.]));
    assert_eq!(export.viewport_document_transform(), None);
    for (zoom, pan) in [
        (0., [0., 0.]),
        (-1., [0., 0.]),
        (f64::NAN, [0., 0.]),
        (1., [f64::INFINITY, 0.]),
        (f64::MAX, [f64::MAX, 0.]),
    ] {
        assert!(prepare_object_navigated_viewport_composition(
            &p,
            &pins,
            Some(&manual),
            true,
            zoom,
            pan
        )
        .is_err());
    }
    let mut wrong = pins.clone();
    wrong.document_snapshot_id = "foreign";
    assert!(prepare_object_navigated_viewport_composition(
        &p,
        &wrong,
        Some(&manual),
        true,
        2.,
        [10., -5.]
    )
    .is_err());
    assert_eq!(
        p.commands().next().unwrap().contour().segments()[0].point,
        [100., 60.]
    );
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn scheduled_navigation_requires_exact_sealed_view_quality_and_preserves_artwork_final() {
    use crate::object_render_scene::{
        object_viewport_quality, prepare_object_navigated_viewport_composition,
    };
    let o = owner(&document());
    let s = snapshot(&o);
    let p = plan(&s, 4);
    let expected = selector(&s, 4);
    let manual =
        prepare_object_navigated_viewport_composition(&p, &expected, None, true, 2., [10., -5.])
            .unwrap();
    let artwork = prepare_object_composition(&p, &expected).unwrap();
    let manual_quality = object_viewport_quality(true, 2., [10., -5.]);
    assert_eq!(
        manual_quality,
        "final-camera-view:4000000000000000:4024000000000000:c014000000000000"
    );
    for quality in [
        "final".to_string(),
        "final-camera-view".into(),
        "final-camera-view:bogus".into(),
        object_viewport_quality(false, 2., [10., -5.]),
        object_viewport_quality(true, 2., [10., 5.]),
        manual_quality.clone(),
    ] {
        let pins = ObjectSchedulePins {
            instance_id: s.instance_id().into(),
            document_id: s.document_id().into(),
            content_revision: s.content_revision(),
            document_snapshot_id: s.snapshot_id().into(),
            context_id: "scene-root".into(),
            frame: 4,
            scope: ObjectScheduleScope::Authored,
        };
        let key = EvaluationKey::new(
            s.snapshot_id(),
            "scene-root",
            4,
            quality.clone(),
            OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap(),
            p.scene_resource_versions().unwrap(),
        )
        .unwrap();
        let mut scheduler = FrameScheduler::new();
        let work = scheduler.schedule_object(s.clone(), pins, key).unwrap();
        assert_eq!(
            prepare_scheduled_object_composition_from_witness(&scheduler, &work, &manual).is_ok(),
            quality == manual_quality
        );
        assert_eq!(
            prepare_scheduled_object_composition_from_witness(&scheduler, &work, &artwork).is_ok(),
            quality == "final"
        );
    }
}
