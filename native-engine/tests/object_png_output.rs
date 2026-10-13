//! Real retained object pixels; private PNG bytes are never an export artifact.
use super::{encode_scheduled_object_png, BoundedPngBytes};
use crate::compositor::{
    object_test_owner, object_test_plan as plan, object_test_selector as selector,
    schedule_test_object as schedule, Compositor,
};
use crate::object_render_scene::prepare_scheduled_object_composition as prepare;
use crate::png_output::{encode_rgba8, encode_rgba8_into};
use crate::render_scene::ScheduledFrameIdentity;
use crate::scheduler::{
    EvaluationKey, FrameFailure, FrameFailureKind, FrameScheduler, ObjectSchedulePins,
    ObjectScheduleScope, OutputSpec,
};
use serde_json::json;
use std::io::{Cursor, Write};

fn decode(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    rgba.truncate(info.buffer_size());
    (info.width, info.height, rgba)
}

#[test]
fn cpu_writer_rejects_over_budget_before_growth_and_keeps_exact_boundary() {
    assert!(BoundedPngBytes::new(0).is_err());
    let mut sink = BoundedPngBytes::new(5).unwrap();
    sink.write_all(&[1, 2]).unwrap();
    let before = (sink.bytes.clone(), sink.bytes.capacity());
    assert!(sink.write_all(&[3, 4, 5, 6]).is_err());
    assert_eq!((sink.bytes.clone(), sink.bytes.capacity()), before);
    sink.write_all(&[3, 4, 5]).unwrap();
    assert_eq!(sink.bytes, [1, 2, 3, 4, 5]);
    assert!(sink.write_all(&[6]).is_err());
    assert_eq!(sink.write(&[]).unwrap(), 0);
    sink.flush().unwrap();
    assert_eq!(sink.bytes.len(), 5);
}

#[test]
fn cpu_shared_profile_preserves_straight_and_zero_alpha_rgba_without_partial_shape_output() {
    // Generic PNG profile preserves straight RGB even at alpha zero. D2 object
    // composition itself remains the separately admitted opaque white mode.
    let rgba = [17, 34, 51, 0, 17, 34, 51, 127];
    let original = encode_rgba8(2, 1, &rgba).unwrap();
    let mut sink = BoundedPngBytes::new(original.len()).unwrap();
    encode_rgba8_into(2, 1, &rgba, &mut sink).unwrap();
    assert_eq!(sink.bytes, original);
    assert_eq!(decode(&sink.bytes), (2, 1, rgba.to_vec()));
    let mut short = BoundedPngBytes::new(original.len() - 1).unwrap();
    assert!(encode_rgba8_into(2, 1, &rgba, &mut short).is_err());
    assert!(short.bytes.len() < original.len());
    for (w, h, pixels) in [(0, 1, rgba.as_slice()), (2, 1, &rgba[..4])] {
        let mut empty = BoundedPngBytes::new(1024).unwrap();
        assert!(encode_rgba8_into(w, h, pixels, &mut empty).is_err());
        assert!(empty.bytes.is_empty());
    }
}

#[test]
fn gpu_live_png_is_deterministic_bounded_and_retains_real_edit_undo_drop_pixels() {
    let mut gpu = Compositor::new().expect("D3C1 requires real headless Metal");
    let mut owner = object_test_owner("png-retention");
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, 7);
    let (result, first) = {
        let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
        let result = gpu.compose_scheduled_object(&live).unwrap();
        let png = encode_scheduled_object_png(&gpu, &result, &live, 64 * 1024).unwrap();
        (result, png)
    };
    let fill=owner.prepare_fill_json(&serde_json::to_vec(&json!({"command":"object.fill.set",
        "stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},"layerUid":"L2","strokeId":"a"},
        "fill":{"kind":"solid","r":1,"g":1,"b":0,"a":1}})).unwrap()).unwrap();
    assert_eq!(owner.commit_fill(fill), Ok(1));
    assert_eq!(owner.undo(1), Ok(2));
    drop(owner);
    let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
    let length = first.bytes(&live).unwrap().len();
    let exact = encode_scheduled_object_png(&gpu, &result, &live, length).unwrap();
    assert_eq!(exact.bytes(&live).unwrap(), first.bytes(&live).unwrap());
    assert_eq!(
        exact.identity(),
        &ScheduledFrameIdentity::from_scheduled(&work)
    );
    assert_eq!(exact.dimensions(), (64, 64));
    assert!(encode_scheduled_object_png(&gpu, &result, &live, length - 1).is_err());
    assert!(encode_scheduled_object_png(&gpu, &result, &live, 0).is_err());
    let (width, height, rgba) = decode(exact.bytes(&live).unwrap());
    assert_eq!((width, height), (64, 64));
    for (x, y, expected) in [(8, 8, [64u8, 96, 159, 255]), (40, 40, [255, 255, 255, 255])] {
        let actual = &rgba[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert!(
            actual
                .iter()
                .zip(expected)
                .all(|(&a, b)| a.abs_diff(b) <= 1),
            "{actual:?} != {expected:?}"
        );
    }
    let raw = gpu.readback_scheduled_object_rgba8(&result, &live).unwrap();
    assert_eq!(rgba, raw.bytes(&live).unwrap());
    assert_eq!(
        exact.bytes(&live).unwrap(),
        encode_rgba8(64, 64, &rgba).unwrap()
    );
    assert_eq!(scheduler.lease_counters().acquired(), 1);
    drop(live);
    scheduler.succeed(work.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn gpu_equal_number_foreign_scheduler_gpu_and_other_source_return_no_png() {
    let mut gpu = Compositor::new().expect("D3C1 requires real headless Metal");
    let foreign_gpu = Compositor::new().unwrap();
    let snapshot = object_test_owner("png-foreign")
        .acquire_snapshot(0)
        .unwrap();
    let input = plan(&snapshot, 7);
    let mut a = FrameScheduler::new();
    let mut b = FrameScheduler::new();
    let wa = schedule(&mut a, &snapshot, 7);
    let wb = schedule(&mut b, &snapshot, 7);
    assert_eq!(wa.work_id(), wb.work_id());
    assert_eq!(wa.view_generation(), wb.view_generation());
    assert_eq!(wa.key(), wb.key());
    let live = prepare(&a, &wa, &input, &selector(&snapshot, 7)).unwrap();
    let foreign = prepare(&b, &wb, &input, &selector(&snapshot, 7)).unwrap();
    let result = gpu.compose_scheduled_object(&live).unwrap();
    let png = encode_scheduled_object_png(&gpu, &result, &live, 64 * 1024).unwrap();
    assert!(encode_scheduled_object_png(&gpu, &result, &foreign, 64 * 1024).is_err());
    assert!(encode_scheduled_object_png(&foreign_gpu, &result, &live, 64 * 1024).is_err());
    assert!(png.bytes(&foreign).is_err());
    let other = object_test_owner("another-document")
        .acquire_snapshot(0)
        .unwrap();
    assert!(prepare(&a, &wa, &plan(&other, 7), &selector(&other, 7)).is_err());
    let mut wrong = selector(&snapshot, 7);
    wrong.context_id = "foreign";
    assert!(prepare(&a, &wa, &input, &wrong).is_err());
    drop(live);
    drop(foreign);
    a.cancel(wa.work_id()).unwrap();
    b.cancel(wb.work_id()).unwrap();
}

#[test]
fn gpu_stale_cancel_fail_replace_success_reject_encoding_and_retained_png_consumption() {
    let mut gpu = Compositor::new().expect("D3C1 requires real headless Metal");
    for terminal in 0..5 {
        let snapshot = object_test_owner("png-terminal")
            .acquire_snapshot(0)
            .unwrap();
        let input = plan(&snapshot, 7);
        let mut scheduler = FrameScheduler::new();
        let old = schedule(&mut scheduler, &snapshot, 7);
        let (result, png) = {
            let live = prepare(&scheduler, &old, &input, &selector(&snapshot, 7)).unwrap();
            let result = gpu.compose_scheduled_object(&live).unwrap();
            let png = encode_scheduled_object_png(&gpu, &result, &live, 64 * 1024).unwrap();
            (result, png)
        };
        let replacement = object_test_owner("png-replacement")
            .acquire_snapshot(0)
            .unwrap();
        match terminal {
            1 => {
                scheduler.cancel(old.work_id()).unwrap();
            }
            2 => {
                scheduler
                    .fail(
                        old.work_id(),
                        FrameFailure::new(FrameFailureKind::Evaluation, "refused"),
                    )
                    .unwrap();
            }
            3 => {
                scheduler
                    .replace_object_document(
                        &replacement,
                        &ObjectSchedulePins {
                            instance_id: replacement.instance_id().into(),
                            document_id: replacement.document_id().into(),
                            content_revision: replacement.content_revision(),
                            document_snapshot_id: replacement.snapshot_id().into(),
                            context_id: "scene-root".into(),
                            frame: 0,
                            scope: ObjectScheduleScope::Authored,
                        },
                    )
                    .unwrap();
            }
            4 => {
                scheduler.succeed(old.work_id()).unwrap();
            }
            _ => {}
        }
        if terminal != 0 {
            assert!(prepare(&scheduler, &old, &input, &selector(&snapshot, 7)).is_err());
        }
        let current = if terminal == 3 {
            &replacement
        } else {
            &snapshot
        };
        let next = schedule(&mut scheduler, current, 0);
        let next_plan = plan(current, 0);
        let live = prepare(&scheduler, &next, &next_plan, &selector(current, 0)).unwrap();
        assert!(prepare(&scheduler, &old, &input, &selector(&snapshot, 7)).is_err());
        assert!(encode_scheduled_object_png(&gpu, &result, &live, 64 * 1024).is_err());
        assert!(png.bytes(&live).is_err());
        let current_result = gpu.compose_scheduled_object(&live).unwrap();
        let current_png =
            encode_scheduled_object_png(&gpu, &current_result, &live, 64 * 1024).unwrap();
        assert_eq!(
            current_png.identity(),
            &ScheduledFrameIdentity::from_scheduled(&next)
        );
        drop(live);
        let retained = scheduler.cancel(old.work_id()).unwrap();
        assert_eq!(scheduler.succeed(old.work_id()).unwrap(), retained);
        scheduler.cancel(next.work_id()).unwrap();
        assert_eq!(scheduler.lease_counters().live(), 0);
        assert_eq!(
            scheduler.lease_counters().acquired(),
            scheduler.lease_counters().released()
        );
    }
}

#[test]
fn cpu_unsupported_output_cannot_create_a_png_consumption_permit() {
    let snapshot = object_test_owner("png-output").acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = scheduler
        .schedule_object(
            snapshot.clone(),
            ObjectSchedulePins {
                instance_id: snapshot.instance_id().into(),
                document_id: snapshot.document_id().into(),
                content_revision: snapshot.content_revision(),
                document_snapshot_id: snapshot.snapshot_id().into(),
                context_id: "scene-root".into(),
                frame: 7,
                scope: ObjectScheduleScope::Authored,
            },
            EvaluationKey::new(
                snapshot.snapshot_id(),
                "scene-root",
                7,
                "final",
                OutputSpec::new("frame", "rgba16f", 64, 64, "linear", "premultiplied").unwrap(),
                [],
            )
            .unwrap(),
        )
        .unwrap();
    assert!(prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).is_err());
    scheduler.cancel(work.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
}
