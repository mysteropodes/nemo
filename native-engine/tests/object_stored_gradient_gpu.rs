//! Original four literal gradient rows through the real shared scheduled viewport/export consumer.
use super::gradient_record;
use super::{drawing, edit, owner, pins, plan, rows, saved, snapshot};
use crate::compositor::Compositor;
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::FillResolution;
use crate::history::NativeObjectHistory;
use crate::object_codec::encode_storage_project;
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_viewport_composition,
    prepare_scheduled_object_composition_from_witness, ObjectCompositionWitness,
};
use crate::png_output::encode_scheduled_object_png;
use crate::scheduler::{
    EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
};
use serde_json::json;

fn render(gpu: &mut Compositor, witness: &ObjectCompositionWitness) -> Vec<u8> {
    let s = witness.plan().packet().snapshot();
    let frame = witness.plan().packet().frame();
    let pins = ObjectSchedulePins {
        instance_id: s.instance_id().into(),
        document_id: s.document_id().into(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame,
        scope: ObjectScheduleScope::Authored,
    };
    let key = EvaluationKey::new(
        s.snapshot_id(),
        "scene-root",
        frame,
        "final",
        OutputSpec::new("frame", "rgba8", 32, 32, "srgb", "straight").unwrap(),
        witness.plan().scene_resource_versions().unwrap(),
    )
    .unwrap();
    let mut scheduler = FrameScheduler::new();
    let work = scheduler.schedule_object(s.clone(), pins, key).unwrap();
    // Stored gradients are paint: one snapshot lease, no external-resource lease.
    assert!(scheduler.lease_ids(work.work_id()).unwrap().1.is_empty());
    let active = scheduler.lease_counters();
    assert_eq!(
        (active.acquired(), active.released(), active.live()),
        (1, 0, 1)
    );
    let live =
        prepare_scheduled_object_composition_from_witness(&scheduler, &work, witness).unwrap();
    let result = gpu.compose_scheduled_object(&live).unwrap();
    let read = gpu.readback_scheduled_object_rgba8(&result, &live).unwrap();
    let bytes = read.bytes(&live).unwrap().to_vec();
    assert_eq!(bytes.len(), 32 * 32 * 4);
    if !witness.is_viewport() {
        assert!(encode_scheduled_object_png(gpu, &result, &live, 1).is_err());
        let png = encode_scheduled_object_png(gpu, &result, &live, 1024 * 1024).unwrap();
        let mut decoder = png::Decoder::new(std::io::Cursor::new(png.bytes(&live).unwrap()))
            .read_info()
            .unwrap();
        assert_eq!((decoder.info().width, decoder.info().height), (32, 32));
        assert_eq!(decoder.info().color_type, png::ColorType::Rgba);
        let mut decoded = vec![0; decoder.output_buffer_size().unwrap()];
        let n = decoder.next_frame(&mut decoded).unwrap();
        decoded.truncate(n.buffer_size());
        assert_eq!(
            decoded, bytes,
            "actual PNG encoding and independent decode retain the export pixels"
        );
    }
    drop(read);
    drop(result);
    drop(live);
    scheduler.succeed(work.work_id()).unwrap();
    let c = scheduler.lease_counters();
    assert_eq!((c.acquired(), c.released(), c.live()), (1, 1, 0));
    bytes
}
#[test]
fn stored_two_stop_original_gradient_oracles_use_real_metal_viewport_export_and_decoded_png() {
    let mut gpu = Compositor::new().expect("stored-gradient pixels require real Metal");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    for row in rows() {
        assert_eq!(row["size"], json!([32, 32]));
        let mut o = owner(&drawing(vec![]));
        edit(&mut o, "object.vector.create", gradient_record(&row));
        let s = snapshot(&o);
        let source = encode_storage_project(s.document()).unwrap();
        let p = plan(&s, 0);
        assert!(matches!(
            p.commands().next().unwrap().style().unwrap().fill,
            FillResolution::Gradient(_)
        ));
        let view = prepare_object_viewport_composition(&p, &pins(&s, 0), None).unwrap();
        let export = prepare_object_composition(&p, &pins(&s, 0)).unwrap();
        let expected_versions = p.scene_resource_versions().unwrap();
        assert!(
            expected_versions.is_empty(),
            "stored gradient is paint, not a synthetic image resource"
        );
        let shown = render(&mut gpu, &view);
        assert!(
            shown.chunks_exact(4).any(|p| p != [255, 255, 255, 255]),
            "equal blank surfaces cannot pass"
        );
        assert_eq!(
            shown,
            render(&mut gpu, &export),
            "shared viewport/export pixels"
        );
        // Original literal probes and their original per-probe tolerances, no new goldens.
        for probe in row["probes"].as_array().unwrap() {
            let x = probe[0].as_u64().unwrap() as usize;
            let y = probe[1].as_u64().unwrap() as usize;
            let rgba = probe[2].as_array().unwrap();
            let tolerance = probe[3].as_u64().unwrap() as u8;
            for channel in 0..4 {
                assert!(
                    shown[(y * 32 + x) * 4 + channel]
                        .abs_diff(rgba[channel].as_u64().unwrap() as u8)
                        <= tolerance,
                    "{} probe {probe} channel {channel}",
                    row["id"]
                );
            }
        }
        let saved = saved(&o);
        drop(o);
        let reopened = NativeObjectHistory::from_project_file("gradient-reopened", &saved).unwrap();
        let reopened_s = snapshot(&reopened);
        let reopened_p = plan(&reopened_s, 0);
        let fresh = prepare_object_composition(&reopened_p, &pins(&reopened_s, 0)).unwrap();
        assert_eq!(
            render(&mut gpu, &fresh),
            shown,
            "real save/fresh-owner reopen preserves gradient pixels"
        );
        assert_eq!(
            render(&mut gpu, &export),
            shown,
            "old immutable owner-independent witness stays readable"
        );
        assert_eq!(
            export.plan().scene_resource_versions().unwrap(),
            expected_versions
        );
        assert_eq!(encode_storage_project(s.document()).unwrap(), source);
    }
}
