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
    drawing["objects"] = json!([drawing["objects"][0].clone()]);
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
        OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap(),
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
        assert_eq!((reader.info().width, reader.info().height), (320, 180));
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
