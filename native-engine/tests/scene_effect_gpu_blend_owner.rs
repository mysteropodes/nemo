//! L08-G on the real native owner: the layered path consumes the same
//! `ObjectPaintPlan` / `ObjectCompositionWitness` as `compose_object`, so a
//! plan with no layered feature must come out of both paths as the same image.
//!
//! The input construction (owner, snapshot, selector, paint plan) is the
//! accepted D2 recipe of `tests/object_compositor.rs`; expected pixels are the
//! fast path's own readback of the same witness, never a mock.
use super::support::{complete, textures, worst_mismatch};
use crate::compositor::object_compositor::ObjectOutputSpec;
use crate::compositor::scene_effect_gpu_blend::{
    layer_runs_from_plan, GpuLayerInput, GpuLayerStack,
};
use crate::compositor::{
    object_test_owner, object_test_plan, object_test_selector, schedule_test_object, Compositor,
};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
use crate::object_render_scene::scene_effect_kernels::scene_effect_order::{
    plan_composition, LayerComposition,
};
use crate::object_render_scene::{
    paint_test_document, prepare_object_composition, prepare_object_paint_plan,
    prepare_object_render_scene, prepare_scheduled_object_composition, ObjectPaintPlan,
};
use crate::render_scene::ScheduledFrameIdentity;
use crate::revision::ObjectSnapshot;
use crate::scheduler::{
    FrameFailure, FrameFailureKind, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope,
};
use serde_json::{json, Value};
use vello::kurbo::BezPath;

fn rectangle(record: &mut Value) {
    record["geometry"]["segments"] =
        json!([[4, 4], [28, 4], [28, 28], [4, 28]].map(|[x, y]| json!({
            "point": {"x": x, "y": y},
            "handleIn": {"x": 0, "y": 0},
            "handleOut": {"x": 0, "y": 0}
        })));
}

fn document(frames: &[u32], alpha: f64) -> Value {
    let mut value = paint_test_document(frames);
    value["objects"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["target"]["strokeId"] == "a");
    for scope in value["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .unwrap()
    {
        scope["elements"]
            .as_array_mut()
            .unwrap()
            .retain(|r| r["strokeId"] == "a");
    }
    for record in value["objects"].as_array_mut().unwrap() {
        rectangle(record);
        let (r, g, b) = match record["target"]["layerUid"].as_str().unwrap() {
            "L0" => (1, 0, 0),
            "L1" => (0, 1, 0),
            _ => (0, 0, 1),
        };
        record["fill"] = json!({"kind": "solid", "r": r, "g": g, "b": b, "a": alpha});
    }
    value
}

fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
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

fn plan(snapshot: &ObjectSnapshot, frame: u32) -> ObjectPaintPlan {
    let pins = selector(snapshot, frame);
    let packet = prepare_object_frame(snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    prepare_object_paint_plan(&scene, &pins).unwrap()
}

fn output() -> ObjectOutputSpec {
    ObjectOutputSpec::new(64, 64, "white-srgb-normal-rgba8-area").unwrap()
}

/// A deliberate defect injected into the FIRST layer input of a stack.
enum Fault<'a> {
    None,
    /// Same contour, different colour bits (alpha 0.25 instead of 0.5).
    Recolour,
    /// Same colour, a contour the plan does not own.
    ForeignPath(&'a BezPath),
}

/// One plain layer per entry of `order` (an index into the plan's three runs,
/// repeated or dropped on purpose by the negative controls) plus a trailing
/// inert effect layer: the smallest real stack whose plan is layered while
/// every pixel still comes from the owner's commands.
fn stack_of<'a>(plan: &'a ObjectPaintPlan, order: &[usize], fault: Fault<'a>) -> GpuLayerStack<'a> {
    let runs = layer_runs_from_plan(plan).unwrap();
    assert_eq!(runs.len(), 3, "three source layers, one command each");
    let mut compositions = vec![LayerComposition::default(); order.len()];
    compositions.push(LayerComposition {
        is_effect_layer: Some(true),
        ..Default::default()
    });
    let mut inputs: Vec<GpuLayerInput<'a>> = order
        .iter()
        .map(|run| GpuLayerInput {
            paint: runs[*run].paint.clone(),
            ..Default::default()
        })
        .collect();
    if let Some(first) = inputs.first_mut() {
        for fill in &mut first.paint.fills {
            match &fault {
                Fault::None => {}
                Fault::Recolour => fill.rgba[3] = 0.25,
                Fault::ForeignPath(path) => fill.path = *path,
            }
        }
    }
    inputs.push(GpuLayerInput::default());
    GpuLayerStack::from_plan(&plan_composition(&compositions), inputs).unwrap()
}

fn new_owner(value: &Value) -> NativeObjectHistory {
    NativeObjectHistory::new(
        "l08-g-real-owner",
        decode_storage_project(&serde_json::to_vec(value).unwrap()).unwrap(),
    )
    .unwrap()
}

#[test]
fn gpu_layered_path_on_a_real_plan_matches_the_fast_path_and_keeps_the_witness_identity() {
    let owner = new_owner(&document(&[0, 5, 10], 0.5));
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 5);
    let prepared = prepare_object_composition(&input, &selector(&snapshot, 5)).unwrap();
    let stack = stack_of(&input, &[0, 1, 2], Fault::None);
    assert_eq!(stack.layer_fill_count(), input.commands().len());
    assert!(stack.covers_plan(&input));
    let mut gpu = Compositor::new().expect("L08-G needs a real headless Metal adapter");
    let fast = gpu.compose_object(&prepared, output()).unwrap();
    let fast_bytes = gpu.readback_object_rgba8(&fast).unwrap().bytes().to_vec();
    let layered = gpu
        .compose_object_layered(&prepared, output(), &stack)
        .unwrap();
    assert!(layered.identity().same_identity_as(&prepared));
    assert_eq!(
        layered.identity().document_snapshot_id(),
        snapshot.snapshot_id()
    );
    let layered_bytes = gpu
        .readback_object_rgba8(&layered)
        .unwrap()
        .bytes()
        .to_vec();
    assert_eq!(layered_bytes.len(), 64 * 64 * 4);
    // Non-vacuous: the owner's three translucent squares are really painted.
    assert!(fast_bytes
        .chunks_exact(4)
        .any(|pixel| pixel != [255, 255, 255, 255]));
    // Straight-alpha RGBA8 intermediates versus one float pass: a few steps.
    assert_eq!(worst_mismatch(&layered_bytes, &fast_bytes, 3), None);
}

#[test]
fn gpu_a_stack_that_does_not_cover_the_plan_is_refused_before_any_texture_exists() {
    let owner = new_owner(&document(&[0, 5, 10], 0.5));
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 5);
    let prepared = prepare_object_composition(&input, &selector(&snapshot, 5)).unwrap();
    let missing_one = stack_of(&input, &[0, 1], Fault::None);
    assert_eq!(missing_one.layer_fill_count(), 2);
    assert!(!missing_one.covers_plan(&input));
    let mut gpu = Compositor::new().expect("L08-G needs a real headless Metal adapter");
    complete(&gpu);
    let baseline = textures(&gpu);
    let error = gpu
        .compose_object_layered(&prepared, output(), &missing_one)
        .err()
        .expect("two fills cannot stand for three commands");
    assert!(error.to_string().contains("does not cover"));
    complete(&gpu);
    assert_eq!(textures(&gpu), baseline);
}

#[test]
fn gpu_a_stack_with_the_right_fill_count_but_the_wrong_fills_is_refused_before_any_texture_exists()
{
    let owner = new_owner(&document(&[0, 5, 10], 0.5));
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 5);
    let prepared = prepare_object_composition(&input, &selector(&snapshot, 5)).unwrap();
    let stray = BezPath::new();
    let mut gpu = Compositor::new().expect("L08-G needs a real headless Metal adapter");
    complete(&gpu);
    let baseline = textures(&gpu);
    // Each stack holds exactly three fills, as the plan has three commands.
    for (label, stack) in [
        (
            "a duplicated run in place of a dropped one",
            stack_of(&input, &[0, 0, 1], Fault::None),
        ),
        (
            "one recoloured run",
            stack_of(&input, &[0, 1, 2], Fault::Recolour),
        ),
        (
            "one run on a contour the plan does not own",
            stack_of(&input, &[0, 1, 2], Fault::ForeignPath(&stray)),
        ),
    ] {
        assert_eq!(stack.layer_fill_count(), input.commands().len(), "{label}");
        assert!(!stack.covers_plan(&input), "{label}");
        let error = gpu
            .compose_object_layered(&prepared, output(), &stack)
            .err()
            .unwrap_or_else(|| panic!("{label} must be refused"));
        assert!(error.to_string().contains("does not cover"), "{label}");
        complete(&gpu);
        assert_eq!(textures(&gpu), baseline, "{label}");
    }
    // The faithful stack, in any run order, still covers the plan.
    assert!(stack_of(&input, &[0, 1, 2], Fault::None).covers_plan(&input));
    assert!(stack_of(&input, &[2, 0, 1], Fault::None).covers_plan(&input));
}

fn schedule_pins(snapshot: &ObjectSnapshot, frame: u32) -> ObjectSchedulePins {
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

/// The scheduler-facing layered entry on the real scheduler. `check_live` runs
/// on the entry's own live permit, which borrows the scheduler, so a stale
/// permit cannot be passed in: the reachable half of the contract is that the
/// result keeps the work identity and that its texture and bytes are refused
/// the moment the work is no longer the live one.
#[test]
fn gpu_scheduled_layered_entry_keeps_the_work_identity_and_refuses_stale_pixels() {
    let mut gpu = Compositor::new().expect("L08-G needs a real headless Metal adapter");
    {
        // Warm the lazily built pools so the baseline below is steady state.
        let owner = object_test_owner("layered-warm");
        let snapshot = owner.acquire_snapshot(0).unwrap();
        let input = object_test_plan(&snapshot, 7);
        let prepared =
            prepare_object_composition(&input, &object_test_selector(&snapshot, 7)).unwrap();
        let stack = stack_of(&input, &[0, 1, 2], Fault::None);
        drop(gpu.compose_object(&prepared, output()).unwrap());
        drop(
            gpu.compose_object_layered(&prepared, output(), &stack)
                .unwrap(),
        );
    }
    complete(&gpu);
    let baseline = textures(&gpu);
    for terminal in 0..5 {
        let snapshot = object_test_owner("layered-terminal")
            .acquire_snapshot(0)
            .unwrap();
        let input = object_test_plan(&snapshot, 7);
        let stack = stack_of(&input, &[0, 1, 2], Fault::None);
        let mut scheduler = FrameScheduler::new();
        let old = schedule_test_object(&mut scheduler, &snapshot, 7);
        let (result, bytes) = {
            let live = prepare_scheduled_object_composition(
                &scheduler,
                &old,
                &input,
                &object_test_selector(&snapshot, 7),
            )
            .unwrap();
            let fast = gpu.compose_scheduled_object(&live).unwrap();
            let result = gpu.compose_layered_scheduled_object(&live, &stack).unwrap();
            assert_eq!(
                result.identity(),
                &ScheduledFrameIdentity::from_scheduled(&old)
            );
            let fast_bytes = gpu
                .readback_scheduled_object_rgba8(&fast, &live)
                .unwrap()
                .bytes(&live)
                .unwrap()
                .to_vec();
            assert!(fast_bytes
                .chunks_exact(4)
                .any(|pixel| pixel != [255, 255, 255, 255]));
            let bytes = gpu.readback_scheduled_object_rgba8(&result, &live).unwrap();
            assert_eq!(
                worst_mismatch(bytes.bytes(&live).unwrap(), &fast_bytes, 3),
                None
            );
            (result, bytes)
        };
        let replacement = object_test_owner("layered-replacement")
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
                    .replace_object_document(&replacement, &schedule_pins(&replacement, 0))
                    .unwrap();
            }
            4 => {
                scheduler.succeed(old.work_id()).unwrap();
            }
            _ => {}
        }
        let next_snapshot = if terminal == 3 {
            &replacement
        } else {
            &snapshot
        };
        let next = schedule_test_object(&mut scheduler, next_snapshot, 0);
        assert!(!scheduler.is_current(&old));
        let next_plan = object_test_plan(next_snapshot, 0);
        let live = prepare_scheduled_object_composition(
            &scheduler,
            &next,
            &next_plan,
            &object_test_selector(next_snapshot, 0),
        )
        .unwrap();
        assert!(gpu.readback_scheduled_object_rgba8(&result, &live).is_err());
        assert!(bytes.bytes(&live).is_err());
        drop(live);
        drop((result, bytes));
        // A frame superseded by a newer schedule is not terminal: without its own cancel (terminal 0 here, as in
        // `gpu_stale_cancel_fail_replace_and_success_refuse_old_texture_and_bytes`) it still holds its lease.
        let old_receipt = scheduler.cancel(old.work_id()).unwrap();
        assert_eq!(scheduler.succeed(old.work_id()).unwrap(), old_receipt);
        scheduler.cancel(next.work_id()).unwrap();
        assert_eq!(scheduler.lease_counters().live(), 0);
        complete(&gpu);
        assert_eq!(textures(&gpu), baseline, "terminal {terminal}");
    }
}
