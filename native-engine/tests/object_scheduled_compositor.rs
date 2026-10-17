//! Real owner/scheduler/Metal controls; no installed-display acceptance.
use crate::compositor::Compositor;
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::{
    paint_test_document, prepare_object_paint_plan, prepare_object_render_scene,
    prepare_scheduled_object_composition as prepare, ObjectPaintPlan, ScheduledObjectSceneError,
};
use crate::render_scene::ScheduledFrameIdentity;
use crate::revision::ObjectSnapshot;
use crate::scheduler::{
    EvaluationKey, FrameFailure, FrameFailureKind, FrameScheduler, ObjectSchedulePins,
    ObjectScheduleScope, OutputSpec, ScheduledFrame,
};
use serde_json::json;

pub(crate) fn owner(instance: &str) -> NativeObjectHistory {
    owner_with_order(instance, None)
}

fn owner_with_order(instance: &str, order: Option<serde_json::Value>) -> NativeObjectHistory {
    owner_with_visibility(instance, order, None)
}
fn owner_with_visibility(
    instance: &str,
    order: Option<serde_json::Value>,
    visibility: Option<[bool; 3]>,
) -> NativeObjectHistory {
    let mut source = paint_test_document(&[0, 7]);
    if let Some(flags) = visibility {
        let flags = [flags[2], flags[0], flags[1]]; // Stored L2/L0/L1, not paint order.
        for (layer, flag) in source["layers"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .zip(flags)
        {
            layer["visible"] = json!(flag);
        }
    }
    source["objects"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["target"]["strokeId"] == "a");
    for scope in source["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .unwrap()
    {
        scope["elements"]
            .as_array_mut()
            .unwrap()
            .retain(|e| e["strokeId"] == "a");
    }
    for record in source["objects"].as_array_mut().unwrap() {
        record["geometry"]["segments"] = json!([[4,4],[28,4],[28,28],[4,28]].map(|[x,y]|
            json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
        let color = match record["target"]["layerUid"].as_str().unwrap() {
            "L0" => [1, 0, 0],
            "L1" => [0, 1, 0],
            _ => [0, 0, 1],
        };
        record["fill"] = json!({"kind":"solid","r":color[0],"g":color[1],"b":color[2],"a":0.5});
    }
    if let Some(order) = order {
        source["compositionOrder"]["contexts"][0]["layers"][0]["order"] = order;
    }
    NativeObjectHistory::new(
        instance,
        decode_storage_project(&serde_json::to_vec(&source).unwrap()).unwrap(),
    )
    .unwrap()
}

#[test]
fn gpu_layer_visibility_filters_paint_and_real_png_without_deleting_source_data() {
    let mut gpu = Compositor::new().expect("visibility requires real headless Metal");
    for (flags, expected, indices) in [
        (Some([true, false, true]), [128, 64, 191, 255], vec![0, 1]),
        (Some([false, false, false]), [255, 255, 255, 255], vec![]),
    ] {
        let owner = owner_with_visibility("layer-visibility", None, flags);
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let before = crate::object_codec::encode_storage_project(snapshot.document()).unwrap();
        let input = plan(&snapshot, 7);
        assert_eq!(
            input
                .packet()
                .records()
                .map(|r| r.source_layer_index())
                .collect::<Vec<_>>(),
            indices
        );
        let mut scheduler = FrameScheduler::new();
        let work = schedule(&mut scheduler, &snapshot, 7);
        let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
        let result = gpu.compose_scheduled_object(&live).unwrap();
        let pixels = gpu.readback_scheduled_object_rgba8(&result, &live).unwrap();
        assert_pixel(pixels.bytes(&live).unwrap(), 8, 8, expected);
        assert_pixel(pixels.bytes(&live).unwrap(), 40, 40, [255, 255, 255, 255]);
        #[cfg(feature = "export_job")]
        {
            let png = crate::png_output::encode_scheduled_object_png(&gpu, &result, &live, 65536)
                .unwrap();
            let bytes = png.bytes(&live).unwrap();
            let mut reader = png::Decoder::new(std::io::Cursor::new(&*bytes))
                .read_info()
                .unwrap();
            let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
            let frame = reader.next_frame(&mut rgba).unwrap();
            assert_eq!((frame.width, frame.height), (64, 64));
            assert_pixel(&rgba, 8, 8, expected);
            assert_pixel(&rgba, 40, 40, [255, 255, 255, 255]);
        }
        assert_eq!(
            crate::object_codec::encode_storage_project(snapshot.document()).unwrap(),
            before
        );
        drop(live);
        scheduler.succeed(work.work_id()).unwrap();
        assert_eq!(scheduler.lease_counters().live(), 0);
    }
}
pub(crate) fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
pub(crate) fn plan(snapshot: &ObjectSnapshot, frame: u32) -> ObjectPaintPlan {
    let pins = selector(snapshot, frame);
    let packet = prepare_object_frame(snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    prepare_object_paint_plan(&scene, &pins).unwrap()
}
fn output() -> OutputSpec {
    OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap()
}
fn key(snapshot: &ObjectSnapshot, frame: u32) -> EvaluationKey {
    EvaluationKey::new(
        snapshot.snapshot_id(),
        "scene-root",
        frame,
        "final",
        output(),
        [],
    )
    .unwrap()
}
fn pins(snapshot: &ObjectSnapshot, frame: u32) -> ObjectSchedulePins {
    ObjectSchedulePins {
        instance_id: snapshot.instance_id().into(),
        document_id: snapshot.document_id().into(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame,
        scope: ObjectScheduleScope::Authored,
    }
}
pub(crate) fn schedule(
    scheduler: &mut FrameScheduler,
    snapshot: &ObjectSnapshot,
    frame: u32,
) -> ScheduledFrame {
    scheduler
        .schedule_object(
            snapshot.clone(),
            pins(snapshot, frame),
            key(snapshot, frame),
        )
        .unwrap()
}
fn edit(owner: &mut NativeObjectHistory) {
    let fill = owner.prepare_fill_json(&serde_json::to_vec(&json!({"command":"object.fill.set",
        "stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},"layerUid":"L2","strokeId":"a"},
        "fill":{"kind":"solid","r":1,"g":1,"b":0,"a":1}})).unwrap()).unwrap();
    assert_eq!(owner.commit_fill(fill), Ok(1));
}

#[test]
fn cpu_scheduler_provenance_rejects_equal_numeric_ids_and_survives_owner_moves() {
    let owner = owner("scheduler-collision");
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let mut a = FrameScheduler::new();
    let mut b = FrameScheduler::new();
    let first = schedule(&mut a, &snapshot, 7);
    let foreign = schedule(&mut b, &snapshot, 7);
    assert_eq!(first.work_id(), foreign.work_id());
    assert_eq!(first.view_generation(), foreign.view_generation());
    assert_eq!(first.key(), foreign.key());
    assert_ne!(
        ScheduledFrameIdentity::from_scheduled(&first),
        ScheduledFrameIdentity::from_scheduled(&foreign)
    );
    assert!(!first.same_work_as(&foreign));
    assert!(!a.is_current(&foreign));
    assert!(!b.is_current(&first));
    assert!(matches!(
        prepare(&b, &first, &input, &selector(&snapshot, 7)),
        Err(ScheduledObjectSceneError::Inactive)
    ));
    let duplicate = schedule(&mut a, &snapshot, 7);
    assert!(first.same_work_as(&duplicate));
    assert_ne!(first.admission(), duplicate.admission());
    let moved = Box::new(a);
    let live = prepare(&moved, &duplicate, &input, &selector(&snapshot, 7)).unwrap();
    let other = prepare(&b, &foreign, &input, &selector(&snapshot, 7)).unwrap();
    assert_eq!(
        live.validate_result(other.prepared()),
        Err(ScheduledObjectSceneError::ForeignResult)
    );
    assert_eq!(moved.lease_counters().acquired(), 1);
    assert_eq!(b.lease_counters().acquired(), 1);
}

#[test]
fn cpu_all_selector_pins_and_actual_retained_owner_revision_are_required() {
    let mut owner = owner("pin-checks");
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, 7);
    for field in 0..7 {
        let mut expected = selector(&snapshot, 7);
        match field {
            0 => expected.instance_id = "other",
            1 => expected.document_id = "other",
            2 => expected.content_revision = 1,
            3 => expected.document_snapshot_id = "other",
            4 => expected.context_id = "other",
            5 => expected.scope_kind = FrameScopeKind::Reference,
            _ => expected.frame = 0,
        }
        assert!(matches!(
            prepare(&scheduler, &work, &input, &expected),
            Err(ScheduledObjectSceneError::Pins(_))
        ));
    }
    let wrong_frame = plan(&snapshot, 0);
    assert!(matches!(
        prepare(&scheduler, &work, &wrong_frame, &selector(&snapshot, 0)),
        Err(ScheduledObjectSceneError::RetainedSourceMismatch)
    ));
    edit(&mut owner);
    let revised = owner.acquire_snapshot(1).unwrap();
    let revised_plan = plan(&revised, 7);
    assert!(matches!(
        prepare(&scheduler, &work, &revised_plan, &selector(&revised, 7)),
        Err(ScheduledObjectSceneError::RetainedSourceMismatch)
    ));
    let other = self::owner("other-instance").acquire_snapshot(0).unwrap();
    assert!(matches!(
        prepare(&scheduler, &work, &plan(&other, 7), &selector(&other, 7)),
        Err(ScheduledObjectSceneError::RetainedSourceMismatch)
    ));
    assert!(scheduler.is_current(&work));
    assert_eq!(scheduler.lease_counters().acquired(), 1);
    assert_eq!(scheduler.lease_counters().released(), 0);
}

#[test]
fn cpu_only_existing_d2_output_and_embedded_resource_contract_is_admitted() {
    let snapshot = owner("output-contract").acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    for field in 0..5 {
        let mut scheduler = FrameScheduler::new();
        let spec = match field {
            0 => OutputSpec::new("tile", "rgba8", 64, 64, "srgb", "straight"),
            1 => OutputSpec::new("frame", "rgba16f", 64, 64, "srgb", "straight"),
            2 => OutputSpec::new("frame", "rgba8", 64, 64, "linear", "straight"),
            3 => OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "premultiplied"),
            _ => Ok(output()),
        }
        .unwrap();
        let key = EvaluationKey::new(
            snapshot.snapshot_id(),
            "scene-root",
            7,
            if field == 4 { "draft" } else { "final" },
            spec,
            [],
        )
        .unwrap();
        let work = scheduler
            .schedule_object(snapshot.clone(), pins(&snapshot, 7), key)
            .unwrap();
        let counters = scheduler.lease_counters();
        assert!(matches!(
            prepare(&scheduler, &work, &input, &selector(&snapshot, 7)),
            Err(ScheduledObjectSceneError::UnsupportedOutput)
        ));
        assert_eq!(scheduler.lease_counters(), counters);
        scheduler.cancel(work.work_id()).unwrap();
        assert_eq!(scheduler.lease_counters().live(), 0);
    }
}

fn assert_pixel(bytes: &[u8], x: usize, y: usize, expected: [u8; 4]) {
    let actual = &bytes[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
    assert!(
        actual
            .iter()
            .zip(expected)
            .all(|(&a, b)| a.abs_diff(b) <= 1),
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn gpu_scheduled_pixels_retain_actual_edit_undo_drop_source_and_gpu_owner() {
    let mut gpu = Compositor::new().expect("D3A2B requires real headless Metal");
    let mut owner = owner("retained-pixels");
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, 7);
    let result = {
        let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
        gpu.compose_scheduled_object(&live).unwrap()
    };
    edit(&mut owner);
    assert_eq!(owner.undo(1), Ok(2));
    drop(owner);
    let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
    let readback = gpu.readback_scheduled_object_rgba8(&result, &live).unwrap();
    assert_eq!(readback.identity(), result.identity());
    assert_eq!(
        readback.identity(),
        &ScheduledFrameIdentity::from_scheduled(&work)
    );
    assert_eq!(readback.dimensions(), (64, 64));
    // Independent D2 literal: red/green/blue at alpha .5, over white.
    assert_pixel(readback.bytes(&live).unwrap(), 8, 8, [64, 96, 159, 255]);
    assert_pixel(readback.bytes(&live).unwrap(), 40, 40, [255, 255, 255, 255]);
    let other_gpu = Compositor::new().expect("foreign real GPU context");
    assert!(other_gpu
        .readback_scheduled_object_rgba8(&result, &live)
        .is_err());
    let mut other_scheduler = FrameScheduler::new();
    let foreign_work = schedule(&mut other_scheduler, &snapshot, 7);
    let foreign = prepare(
        &other_scheduler,
        &foreign_work,
        &input,
        &selector(&snapshot, 7),
    )
    .unwrap();
    assert!(gpu
        .readback_scheduled_object_rgba8(&result, &foreign)
        .is_err());
    assert!(readback.bytes(&foreign).is_err());
    drop(live);
    let terminal = scheduler.succeed(work.work_id()).unwrap();
    assert_eq!(scheduler.succeed(work.work_id()).unwrap(), terminal);
    assert_eq!(scheduler.lease_counters().acquired(), 1);
    assert_eq!(scheduler.lease_counters().released(), 1);
    assert!(matches!(
        prepare(&scheduler, &work, &input, &selector(&snapshot, 7)),
        Err(ScheduledObjectSceneError::Inactive)
    ));
}

#[test]
fn gpu_stale_cancel_fail_replace_and_success_refuse_old_texture_and_bytes() {
    let mut gpu = Compositor::new().expect("D3A2B requires real headless Metal");
    for terminal in 0..5 {
        let snapshot = owner("terminal-pixels").acquire_snapshot(0).unwrap();
        let input = plan(&snapshot, 7);
        let mut scheduler = FrameScheduler::new();
        let old = schedule(&mut scheduler, &snapshot, 7);
        let (result, bytes) = {
            let live = prepare(&scheduler, &old, &input, &selector(&snapshot, 7)).unwrap();
            let result = gpu.compose_scheduled_object(&live).unwrap();
            let bytes = gpu.readback_scheduled_object_rgba8(&result, &live).unwrap();
            (result, bytes)
        };
        let replacement = owner("replacement-pixels").acquire_snapshot(0).unwrap();
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
                    .replace_object_document(&replacement, &pins(&replacement, 0))
                    .unwrap();
            }
            4 => {
                scheduler.succeed(old.work_id()).unwrap();
            }
            _ => {}
        }
        if terminal != 0 {
            assert!(!scheduler.is_current(&old));
            assert!(matches!(
                prepare(&scheduler, &old, &input, &selector(&snapshot, 7)),
                Err(ScheduledObjectSceneError::Inactive)
            ));
            assert_eq!(scheduler.lease_counters().live(), 0);
        }
        let next_snapshot = if terminal == 3 {
            &replacement
        } else {
            &snapshot
        };
        let next = schedule(&mut scheduler, next_snapshot, 0);
        let next_plan = plan(next_snapshot, 0);
        assert!(matches!(
            prepare(&scheduler, &old, &input, &selector(&snapshot, 7)),
            Err(ScheduledObjectSceneError::Inactive)
        ));
        let live = prepare(&scheduler, &next, &next_plan, &selector(next_snapshot, 0)).unwrap();
        assert!(gpu.readback_scheduled_object_rgba8(&result, &live).is_err());
        assert!(bytes.bytes(&live).is_err());
        let latest = gpu.compose_scheduled_object(&live).unwrap();
        assert_eq!(
            latest.identity(),
            &ScheduledFrameIdentity::from_scheduled(&next)
        );
        drop(live);
        let old_receipt = scheduler.cancel(old.work_id()).unwrap();
        assert_eq!(scheduler.succeed(old.work_id()).unwrap(), old_receipt);
        scheduler.cancel(next.work_id()).unwrap();
        assert_eq!(scheduler.lease_counters().live(), 0);
        assert_eq!(scheduler.lease_counters().acquired(), 2);
        assert_eq!(scheduler.lease_counters().released(), 2);
    }
}

#[cfg(feature = "object_expression_runtime")]
fn expression_composition(
    owner: &NativeObjectHistory,
    snapshot: &ObjectSnapshot,
) -> Result<
    crate::object_render_scene::ObjectCompositionWitness,
    crate::object_expression_runtime::ExpressionCompositionRefusal,
> {
    crate::object_expression_runtime::prepare_expression_object_composition_for_owner(
        owner,
        snapshot,
        &selector(snapshot, 7),
        crate::object_expression_runtime::ExpressionLimits::new(4096, 8_388_608, 262_144, 32)
            .unwrap(),
    )
}

#[cfg(feature = "object_expression_runtime")]
#[test]
fn gpu_expression_composition_uses_original_scheduler_and_literal_source_over_pixels() {
    for (code, expected) in [
        ("value", [159, 64, 96, 255]),
        ("[value,Infinity]", [159, 64, 96, 255]),
        ("[3]", [64, 96, 159, 255]),
    ] {
        let owner = owner_with_order(
            "expression-composition",
            Some(json!({"static":[0],
            "expression":{"enabled":true,"code":code}})),
        );
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let before = crate::object_codec::encode_storage_project(snapshot.document()).unwrap();
        let witness = expression_composition(&owner, &snapshot).unwrap();
        let mut scheduler = FrameScheduler::new();
        let work = schedule(&mut scheduler, &snapshot, 7);
        let mut gpu = Compositor::new().expect("expression consumer needs real headless Metal");
        let live = crate::object_render_scene::prepare_scheduled_object_composition_from_witness(
            &scheduler, &work, &witness,
        )
        .unwrap();
        let result = gpu.compose_scheduled_object(&live).unwrap();
        let pixels = gpu.readback_scheduled_object_rgba8(&result, &live).unwrap();
        assert_pixel(pixels.bytes(&live).unwrap(), 8, 8, expected);
        assert_pixel(pixels.bytes(&live).unwrap(), 40, 40, [255, 255, 255, 255]);
        assert_eq!(
            result.identity(),
            &ScheduledFrameIdentity::from_scheduled(&work)
        );
        assert_eq!(scheduler.lease_counters().acquired(), 1);
        assert_eq!(owner.history_depths(), (0, 0));
        assert_eq!(
            crate::object_codec::encode_storage_project(
                owner.acquire_snapshot(0).unwrap().document()
            )
            .unwrap(),
            before
        );
        drop(live);
        scheduler.succeed(work.work_id()).unwrap();
        assert_eq!(scheduler.lease_counters().live(), 0);
    }
}
