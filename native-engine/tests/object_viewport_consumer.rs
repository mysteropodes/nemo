//! Public-facade controls using retained real owners and instrumented Metal.
use super::ObjectViewportFrame;
use crate::compositor::{
    object_test_owner as owner, object_test_plan as plan, object_test_selector as selector,
    schedule_test_object as schedule, Compositor,
};
use crate::desktop_viewport::*;
use crate::object_document::FrameScopeKind;
use crate::object_render_scene::{
    prepare_scheduled_object_composition as prepare, LiveObjectComposition,
};
use crate::revision::ObjectSnapshot;
use crate::scheduler::{
    EvaluationKey, FrameFailure, FrameFailureKind, FrameScheduler, ObjectSchedulePins,
    ObjectScheduleScope, OutputSpec,
};

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
fn mapping(size: u32) -> ViewportMapping {
    let extent = PhysicalExtent {
        width: size,
        height: size,
    };
    ViewportMapping::new(
        CssBounds {
            x: 0.,
            y: 0.,
            width: 64.,
            height: 64.,
        },
        extent,
        extent,
        1.,
    )
    .unwrap()
}
struct Port {
    context: (wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue),
    expected: (wgpu::Texture, wgpu::TextureView),
    calls: usize,
    frames: usize,
    settlements: usize,
    defer_once: bool,
}
impl Port {
    fn counts(&self) -> (usize, usize, usize) {
        (self.calls, self.frames, self.settlements)
    }
    fn new(
        gpu: &Compositor,
        frame: &ObjectViewportFrame,
        live: &LiveObjectComposition<'_>,
    ) -> Self {
        let source = gpu.scheduled_object_surface(&frame.result, live).unwrap();
        Self {
            context: (
                gpu.instance().clone(),
                gpu.adapter().clone(),
                gpu.device().clone(),
                gpu.queue().clone(),
            ),
            expected: (source.texture.clone(), source.texture_view.clone()),
            calls: 0,
            frames: 0,
            settlements: 0,
            defer_once: false,
        }
    }
}
struct Frame<'a> {
    settlements: &'a mut usize,
    accepted: bool,
}
impl AcquiredSurfaceFrame for Frame<'_> {
    fn settle(self) -> SettledSurfaceFrame {
        if self.accepted {
            *self.settlements += 1;
            SettledSurfaceFrame::Presented
        } else {
            SettledSurfaceFrame::ValidationFailure
        }
    }
}
impl SurfacePort for Port {
    type Frame<'p, 's> = Frame<'p>;
    fn acquire<'p, 's>(
        &'p mut self,
        source: SurfacePresentation<'s>,
    ) -> SurfaceAttempt<Self::Frame<'p, 's>> {
        self.calls += 1;
        if source.instance != &self.context.0
            || source.adapter != &self.context.1
            || source.device != &self.context.2
            || source.queue != &self.context.3
        {
            return SurfaceAttempt::Acquired(Frame {
                settlements: &mut self.settlements,
                accepted: false,
            });
        }
        assert_eq!(source.texture, &self.expected.0);
        assert_eq!(source.texture_view, &self.expected.1);
        assert_eq!(source.source_extent, mapping(64).composition_extent());
        if std::mem::take(&mut self.defer_once) {
            return SurfaceAttempt::Timeout;
        }
        self.frames += 1;
        SurfaceAttempt::Acquired(Frame {
            settlements: &mut self.settlements,
            accepted: true,
        })
    }
    fn recreate(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn reconfigure(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn dispose(&mut self) {}
    fn simulated_acquire<'p>(
        &'p mut self,
        _: &ScheduledFrameIdentity,
    ) -> SurfaceAttempt<Self::Frame<'p, 'p>> {
        unreachable!("facade tests must consume the actual texture")
    }
}

#[test]
fn cpu_public_permit_rejects_every_selector_pin_and_unrelated_retained_plan() {
    let snapshot = owner("public-pins").acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, 7);
    for field in 0..7 {
        let mut wrong = selector(&snapshot, 7);
        match field {
            0 => wrong.instance_id = "foreign",
            1 => wrong.document_id = "foreign",
            2 => wrong.content_revision += 1,
            3 => wrong.document_snapshot_id = "foreign",
            4 => wrong.context_id = "foreign",
            5 => wrong.scope_kind = FrameScopeKind::Reference,
            _ => wrong.frame = 0,
        }
        assert!(prepare(&scheduler, &work, &input, &wrong).is_err());
    }
    let other = owner("public-other").acquire_snapshot(0).unwrap();
    assert!(prepare(&scheduler, &work, &plan(&other, 7), &selector(&other, 7)).is_err());
    let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
    assert_eq!(
        live.identity(),
        &ScheduledFrameIdentity::from_scheduled(&work)
    );
    assert_eq!(live.dimensions(), (64, 64));
    drop(live);
    scheduler.cancel(work.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn cpu_public_permit_refuses_unsupported_output_quality_and_resources() {
    let snapshot = owner("public-output").acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    for field in 0..6 {
        let output = OutputSpec::new(
            if field == 0 { "tile" } else { "frame" },
            if field == 1 { "rgba16f" } else { "rgba8" },
            64,
            64,
            if field == 2 { "linear" } else { "srgb" },
            if field == 3 {
                "premultiplied"
            } else {
                "straight"
            },
        )
        .unwrap();
        let resources = if field == 5 {
            vec![("unbound".into(), "v1".into())]
        } else {
            vec![]
        };
        let key = EvaluationKey::new(
            snapshot.snapshot_id(),
            "scene-root",
            7,
            if field == 4 { "draft" } else { "final" },
            output,
            resources,
        )
        .unwrap();
        let work = scheduler
            .schedule_object(snapshot.clone(), pins(&snapshot, 7), key)
            .unwrap();
        assert!(prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).is_err());
        scheduler.cancel(work.work_id()).unwrap();
    }
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn gpu_public_facade_preserves_pixels_texture_defer_cache_and_refusal_boundaries() {
    let mut gpu = Compositor::new().expect("D3B2A requires real headless Metal");
    let other_gpu = Compositor::new().unwrap();
    let snapshot = owner("public-frame").acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, 7);
    let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
    let frame = ObjectViewportFrame::compose(&mut gpu, &live).unwrap();
    assert_eq!(frame.identity(), live.identity());
    let pixels = gpu
        .readback_scheduled_object_rgba8(&frame.result, &live)
        .unwrap();
    for (x, y, expected) in [(8, 8, [64u8, 96, 159, 255]), (40, 40, [255, 255, 255, 255])] {
        let pixel = &pixels.bytes(&live).unwrap()[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
        assert!(pixel.iter().zip(expected).all(|(&a, b)| a.abs_diff(b) <= 1));
    }
    let mut port = Port::new(&gpu, &frame, &live);
    port.defer_once = true;
    let mut viewport = DesktopViewportHost::new(port, mapping(64));
    viewport.register(frame.identity().clone()).unwrap();
    assert_eq!(
        frame.present(&mut viewport, &gpu, &live).unwrap().status(),
        &ViewportStatus::Deferred(DeferredAction::Timeout)
    );
    let receipt = frame.present(&mut viewport, &gpu, &live).unwrap();
    assert_eq!(receipt.status(), &ViewportStatus::Presented);
    assert_eq!(frame.present(&mut viewport, &gpu, &live).unwrap(), receipt);
    assert_eq!(viewport.test_port().counts(), (2, 1, 1));
    assert!(frame.present(&mut viewport, &other_gpu, &live).is_err());
    let mut foreign_scheduler = FrameScheduler::new();
    let foreign_work = schedule(&mut foreign_scheduler, &snapshot, 7);
    assert_eq!(foreign_work.work_id(), work.work_id());
    assert_eq!(foreign_work.view_generation(), work.view_generation());
    assert_eq!(foreign_work.key(), work.key());
    let foreign = prepare(
        &foreign_scheduler,
        &foreign_work,
        &input,
        &selector(&snapshot, 7),
    )
    .unwrap();
    assert!(frame.present(&mut viewport, &gpu, &foreign).is_err());
    assert_eq!(viewport.test_port().calls, 2);
    let mut wrong_extent = DesktopViewportHost::new(Port::new(&gpu, &frame, &live), mapping(32));
    wrong_extent.register(frame.identity().clone()).unwrap();
    assert!(frame.present(&mut wrong_extent, &gpu, &live).is_err());
    assert_eq!(wrong_extent.test_port().calls, 0);
    let mut wrong_target = Port::new(&gpu, &frame, &live);
    wrong_target.context = (
        other_gpu.instance().clone(),
        other_gpu.adapter().clone(),
        other_gpu.device().clone(),
        other_gpu.queue().clone(),
    );
    let mut wrong_target = DesktopViewportHost::new(wrong_target, mapping(64));
    wrong_target.register(frame.identity().clone()).unwrap();
    assert!(matches!(
        frame
            .present(&mut wrong_target, &gpu, &live)
            .unwrap()
            .status(),
        ViewportStatus::Failed(_)
    ));
    assert_eq!(wrong_target.test_port().counts(), (1, 0, 0));
    drop(live);
    drop(foreign);
    scheduler.succeed(work.work_id()).unwrap();
    foreign_scheduler.cancel(foreign_work.work_id()).unwrap();
}

#[test]
fn gpu_public_retained_frame_cannot_present_after_stale_or_terminal_work() {
    let mut gpu = Compositor::new().expect("D3B2A requires real headless Metal");
    for terminal in 0..5 {
        let snapshot = owner("public-old").acquire_snapshot(0).unwrap();
        let replacement = owner("public-replacement").acquire_snapshot(0).unwrap();
        let input = plan(&snapshot, 7);
        let mut scheduler = FrameScheduler::new();
        let old = schedule(&mut scheduler, &snapshot, 7);
        let (frame, mut viewport) = {
            let live = prepare(&scheduler, &old, &input, &selector(&snapshot, 7)).unwrap();
            let frame = ObjectViewportFrame::compose(&mut gpu, &live).unwrap();
            let mut viewport =
                DesktopViewportHost::new(Port::new(&gpu, &frame, &live), mapping(64));
            viewport.register(frame.identity().clone()).unwrap();
            (frame, viewport)
        };
        match terminal {
            1 => {
                scheduler.cancel(old.work_id()).unwrap();
            }
            2 => {
                scheduler
                    .fail(
                        old.work_id(),
                        FrameFailure::new(FrameFailureKind::Worker, "refused"),
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
            assert!(prepare(&scheduler, &old, &input, &selector(&snapshot, 7)).is_err());
        }
        let current = if terminal == 3 {
            &replacement
        } else {
            &snapshot
        };
        let next = schedule(&mut scheduler, current, 0);
        let current_plan = plan(current, 0);
        let live = prepare(&scheduler, &next, &current_plan, &selector(current, 0)).unwrap();
        assert!(prepare(&scheduler, &old, &input, &selector(&snapshot, 7)).is_err());
        assert!(frame.present(&mut viewport, &gpu, &live).is_err());
        assert_eq!(viewport.test_port().calls, 0);
        drop(live);
        scheduler.cancel(old.work_id()).unwrap();
        scheduler.cancel(next.work_id()).unwrap();
        assert_eq!(scheduler.lease_counters().live(), 0);
    }
}
