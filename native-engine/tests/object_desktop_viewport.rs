//! Existing viewport test helpers plus scheduled-object surface controls.
use crate::desktop_viewport::{
    consume_attempt, DesktopViewportHost, PresentationReceipt, SurfacePort, ViewportError,
};
use crate::render_scene::ScheduledFrameIdentity;
use crate::scheduler::WorkId;

#[cfg(test)]
impl<P: SurfacePort> DesktopViewportHost<P> {
    pub(crate) fn simulate_port_completion(
        &mut self,
        identity: ScheduledFrameIdentity,
    ) -> Result<PresentationReceipt, ViewportError> {
        if let Some(receipt) = self.prepare_present(identity.clone())? {
            return Ok(receipt);
        }
        let first = consume_attempt(self.port.simulated_acquire(&identity));
        self.complete_event(identity.clone(), first, |port| {
            consume_attempt(port.simulated_acquire(&identity))
        })
    }
    pub(crate) fn test_port(&self) -> &P {
        &self.port
    }
    pub(crate) fn test_port_mut(&mut self) -> &mut P {
        &mut self.port
    }
    pub(crate) fn register_under_for_test(
        &mut self,
        work_id: WorkId,
        identity: ScheduledFrameIdentity,
    ) -> Result<(), ViewportError> {
        self.register_as(work_id, identity)
    }
}

#[cfg(feature = "history")]
mod actual_owner {
    use crate::compositor::{
        object_test_owner, object_test_plan as plan, object_test_selector as selector,
        schedule_test_object as schedule, Compositor, ScheduledObjectCompositionResult,
    };
    use crate::desktop_viewport::{
        AcquiredSurfaceFrame, CssBounds, DeferredAction, DesktopViewportHost, FatalAction,
        PhysicalExtent, SettledSurfaceFrame, SurfaceAttempt, SurfacePort, SurfacePresentation,
        SurfaceRecoveryError, SurfaceScalePolicy, ViewportError, ViewportMapping, ViewportStatus,
    };
    use crate::object_render_scene::{
        prepare_scheduled_object_composition as prepare, LiveObjectComposition,
    };
    use crate::render_scene::ScheduledFrameIdentity;
    use crate::revision::ObjectSnapshot;
    use crate::scheduler::FrameScheduler;
    use std::collections::VecDeque;

    fn snapshot(instance: &str) -> ObjectSnapshot {
        object_test_owner(instance).acquire_snapshot(0).unwrap()
    }
    fn extent(size: u32) -> PhysicalExtent {
        PhysicalExtent {
            width: size,
            height: size,
        }
    }
    fn mapping(physical: u32, composition: u32) -> ViewportMapping {
        ViewportMapping::new(
            CssBounds {
                x: 10.,
                y: 20.,
                width: physical as f64 / 2.,
                height: physical as f64 / 2.,
            },
            extent(physical),
            extent(composition),
            2.,
        )
        .unwrap()
    }
    #[derive(Clone, Copy)]
    enum Step {
        Present,
        Lost,
        Outdated,
        Timeout,
        Occluded,
    }
    struct Port {
        context: (wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue),
        expected: (wgpu::Texture, wgpu::TextureView, ScheduledFrameIdentity),
        mapping: ViewportMapping,
        steps: VecDeque<Step>,
        events: Vec<&'static str>,
        calls: usize,
        frames: usize,
        settles: usize,
        disposals: usize,
        extents: Vec<(PhysicalExtent, PhysicalExtent)>,
    }
    impl Port {
        fn new(
            gpu: &Compositor,
            result: &ScheduledObjectCompositionResult,
            live: &LiveObjectComposition<'_>,
        ) -> Self {
            let view = gpu.scheduled_object_surface(result, live).unwrap();
            Self {
                context: (
                    view.instance.clone(),
                    view.adapter.clone(),
                    view.device.clone(),
                    view.queue.clone(),
                ),
                expected: (
                    view.texture.clone(),
                    view.texture_view.clone(),
                    view.identity.clone(),
                ),
                mapping: mapping(128, 64),
                steps: VecDeque::new(),
                events: vec![],
                calls: 0,
                frames: 0,
                settles: 0,
                disposals: 0,
                extents: vec![],
            }
        }
        fn expect(
            &mut self,
            gpu: &Compositor,
            result: &ScheduledObjectCompositionResult,
            live: &LiveObjectComposition<'_>,
        ) {
            let view = gpu.scheduled_object_surface(result, live).unwrap();
            self.expected = (
                view.texture.clone(),
                view.texture_view.clone(),
                view.identity.clone(),
            );
        }
    }
    struct Frame<'p, 's> {
        port: &'p mut Port,
        source: Option<SurfacePresentation<'s>>,
        consumed: bool,
    }
    impl AcquiredSurfaceFrame for Frame<'_, '_> {
        fn settle(mut self) -> SettledSurfaceFrame {
            self.consumed = true;
            if self.source.is_some() {
                self.port.settles += 1;
                self.port.events.push("settle");
                SettledSurfaceFrame::Presented
            } else {
                SettledSurfaceFrame::ValidationFailure
            }
        }
    }
    impl Drop for Frame<'_, '_> {
        fn drop(&mut self) {
            self.port.events.push(if self.consumed {
                "drop"
            } else {
                "unsettled-drop"
            });
        }
    }
    impl SurfacePort for Port {
        type Frame<'p, 's> = Frame<'p, 's>;
        fn acquire<'p, 's>(
            &'p mut self,
            source: SurfacePresentation<'s>,
        ) -> SurfaceAttempt<Self::Frame<'p, 's>> {
            self.calls += 1;
            self.events.push("invoke");
            // Same target-owned comparison as the existing MacOsSurfacePort:
            // method invocation is distinct from acquiring a real frame token.
            if source.instance != &self.context.0
                || source.adapter != &self.context.1
                || source.device != &self.context.2
                || source.queue != &self.context.3
                || source.surface_extent != self.mapping.physical_extent()
            {
                self.events.push("target-refused");
                return SurfaceAttempt::Acquired(Frame {
                    port: self,
                    source: None,
                    consumed: false,
                });
            }
            assert_eq!(source.texture, &self.expected.0);
            assert_eq!(source.texture_view, &self.expected.1);
            assert_eq!(source.identity, &self.expected.2);
            assert_eq!(source.scale_policy, SurfaceScalePolicy::StretchToSurface);
            self.extents
                .push((source.source_extent, source.surface_extent));
            match self.steps.pop_front().unwrap_or(Step::Present) {
                Step::Lost => SurfaceAttempt::Lost,
                Step::Outdated => SurfaceAttempt::Outdated,
                Step::Timeout => SurfaceAttempt::Timeout,
                Step::Occluded => SurfaceAttempt::Occluded,
                Step::Present => {
                    self.frames += 1;
                    self.events.push("acquired");
                    SurfaceAttempt::Acquired(Frame {
                        port: self,
                        source: Some(source),
                        consumed: false,
                    })
                }
            }
        }
        fn recreate(&mut self, mapping: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
            self.events.push("recreate");
            self.mapping = mapping;
            Ok(())
        }
        fn reconfigure(&mut self, mapping: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
            self.events.push("reconfigure");
            self.mapping = mapping;
            Ok(())
        }
        fn dispose(&mut self) {
            self.disposals += 1;
            self.events.push("dispose");
        }
        fn simulated_acquire<'p>(
            &'p mut self,
            _: &ScheduledFrameIdentity,
        ) -> SurfaceAttempt<Self::Frame<'p, 'p>> {
            panic!("this port requires actual compositor resources")
        }
    }

    #[test]
    fn gpu_real_object_texture_identity_pixels_resize_and_disposal_share_surface_path() {
        let mut gpu = Compositor::new().expect("real headless Metal");
        let snapshot = snapshot("viewport-pixels");
        let input = plan(&snapshot, 7);
        let mut scheduler = FrameScheduler::new();
        let work = schedule(&mut scheduler, &snapshot, 7);
        let duplicate = schedule(&mut scheduler, &snapshot, 7);
        let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
        let result = gpu.compose_scheduled_object(&live).unwrap();
        let mut host = DesktopViewportHost::new(Port::new(&gpu, &result, &live), mapping(128, 64));
        host.register(result.identity().clone()).unwrap();
        host.register(ScheduledFrameIdentity::from_scheduled(&duplicate))
            .unwrap();
        let receipt = host.present_scheduled_object(&gpu, &result, &live).unwrap();
        assert_eq!(receipt.status(), &ViewportStatus::Presented);
        assert_eq!(receipt.identity(), result.identity());
        assert_eq!(
            host.present_scheduled_object(&gpu, &result, &live).unwrap(),
            receipt
        );
        assert_eq!(
            (
                host.test_port().calls,
                host.test_port().frames,
                host.test_port().settles
            ),
            (1, 1, 1)
        );
        let pixels = gpu.readback_scheduled_object_rgba8(&result, &live).unwrap();
        for (x, y, expected) in [(8, 8, [64u8, 96, 159, 255]), (40, 40, [255, 255, 255, 255])] {
            let actual = &pixels.bytes(&live).unwrap()[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
            assert!(
                actual
                    .iter()
                    .zip(expected)
                    .all(|(&a, b)| a.abs_diff(b) <= 1),
                "{actual:?}"
            );
        }
        host.resize(mapping(128, 32)).unwrap();
        assert!(matches!(
            host.present_scheduled_object(&gpu, &result, &live),
            Err(ViewportError::CompositionExtentMismatch { .. })
        ));
        assert_eq!(host.test_port().calls, 1);
        drop(live);
        scheduler.succeed(work.work_id()).unwrap();
        host.resize(mapping(160, 64)).unwrap();
        let next = schedule(&mut scheduler, &snapshot, 0);
        let next_plan = plan(&snapshot, 0);
        let live = prepare(&scheduler, &next, &next_plan, &selector(&snapshot, 0)).unwrap();
        let result = gpu.compose_scheduled_object(&live).unwrap();
        host.test_port_mut().expect(&gpu, &result, &live);
        host.register(result.identity().clone()).unwrap();
        assert_eq!(
            host.present_scheduled_object(&gpu, &result, &live)
                .unwrap()
                .status(),
            &ViewportStatus::Presented
        );
        assert_eq!(
            host.test_port().extents,
            vec![(extent(64), extent(128)), (extent(64), extent(160))]
        );
        assert!(host
            .test_port()
            .events
            .windows(2)
            .any(|p| p == ["drop", "reconfigure"]));
        assert!(host.dispose().is_empty());
        assert!(host.dispose().is_empty());
        assert_eq!(host.test_port().disposals, 1);
        assert_eq!(
            host.present_scheduled_object(&gpu, &result, &live),
            Err(ViewportError::Disposed)
        );
        drop(live);
        scheduler.succeed(next.work_id()).unwrap();
        assert_eq!(scheduler.lease_counters().live(), 0);
    }

    #[test]
    fn gpu_foreign_work_gpu_and_extent_never_invoke_surface_and_receipts_keep_provenance() {
        let mut gpu = Compositor::new().expect("real headless Metal");
        let other_gpu = Compositor::new().unwrap();
        let snapshot = snapshot("foreign-gates");
        let input = plan(&snapshot, 7);
        let mut a = FrameScheduler::new();
        let mut b = FrameScheduler::new();
        let wa = schedule(&mut a, &snapshot, 7);
        let wb = schedule(&mut b, &snapshot, 7);
        let live = prepare(&a, &wa, &input, &selector(&snapshot, 7)).unwrap();
        let foreign = prepare(&b, &wb, &input, &selector(&snapshot, 7)).unwrap();
        let result = gpu.compose_scheduled_object(&live).unwrap();
        let mut host = DesktopViewportHost::new(Port::new(&gpu, &result, &live), mapping(128, 64));
        assert!(matches!(
            host.present_scheduled_object(&gpu, &result, &live),
            Err(ViewportError::Unregistered(_))
        ));
        host.register(result.identity().clone()).unwrap();
        assert_eq!(wa.work_id(), wb.work_id());
        assert_eq!(wa.view_generation(), wb.view_generation());
        assert_eq!(wa.key(), wb.key());
        assert!(matches!(
            host.register(ScheduledFrameIdentity::from_scheduled(&wb)),
            Err(ViewportError::IdentityMismatch(_))
        ));
        assert!(host
            .present_scheduled_object(&other_gpu, &result, &live)
            .is_err());
        assert!(host
            .present_scheduled_object(&gpu, &result, &foreign)
            .is_err());
        host.resize(mapping(128, 32)).unwrap();
        assert!(host.present_scheduled_object(&gpu, &result, &live).is_err());
        assert_eq!(host.test_port().calls, 0);
        host.resize(mapping(128, 64)).unwrap();
        host.present_scheduled_object(&gpu, &result, &live).unwrap();
        assert!(matches!(
            host.register(ScheduledFrameIdentity::from_scheduled(&wb)),
            Err(ViewportError::IdentityMismatch(_))
        ));
        assert_eq!(host.test_port().calls, 1);
        drop(live);
        drop(foreign);
        a.cancel(wa.work_id()).unwrap();
        b.cancel(wb.work_id()).unwrap();
    }

    #[test]
    fn gpu_retry_defer_and_target_refusal_preserve_owned_frame_settlement() {
        let mut gpu = Compositor::new().expect("real headless Metal");
        let wrong_gpu = Compositor::new().unwrap();
        for step in [Step::Lost, Step::Outdated, Step::Timeout, Step::Occluded] {
            let snapshot = snapshot("surface-events");
            let input = plan(&snapshot, 7);
            let mut scheduler = FrameScheduler::new();
            let work = schedule(&mut scheduler, &snapshot, 7);
            let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
            let result = gpu.compose_scheduled_object(&live).unwrap();
            let mut port = Port::new(&gpu, &result, &live);
            port.steps.extend([step, Step::Present]);
            let mut host = DesktopViewportHost::new(port, mapping(128, 64));
            host.register(result.identity().clone()).unwrap();
            let first = host.present_scheduled_object(&gpu, &result, &live).unwrap();
            match step {
                Step::Timeout | Step::Occluded => {
                    let action = if matches!(step, Step::Timeout) {
                        DeferredAction::Timeout
                    } else {
                        DeferredAction::Occluded
                    };
                    assert_eq!(first.status(), &ViewportStatus::Deferred(action));
                    assert_eq!(host.test_port().frames, 0);
                    assert_eq!(host.pending_work_ids(), vec![work.work_id()]);
                    assert_eq!(
                        host.present_scheduled_object(&gpu, &result, &live)
                            .unwrap()
                            .status(),
                        &ViewportStatus::Presented
                    );
                }
                _ => assert_eq!(first.status(), &ViewportStatus::Presented),
            }
            assert_eq!(
                (
                    host.test_port().calls,
                    host.test_port().frames,
                    host.test_port().settles
                ),
                (2, 1, 1)
            );
            assert!(!host.test_port().events.contains(&"unsettled-drop"));
            if matches!(step, Step::Lost) {
                assert!(host.test_port().events.contains(&"recreate"));
            }
            if matches!(step, Step::Outdated) {
                assert!(host.test_port().events.contains(&"reconfigure"));
            }
            drop(live);
            scheduler.succeed(work.work_id()).unwrap();
        }
        let snapshot = snapshot("wrong-target");
        let input = plan(&snapshot, 7);
        let mut scheduler = FrameScheduler::new();
        let work = schedule(&mut scheduler, &snapshot, 7);
        let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
        let result = gpu.compose_scheduled_object(&live).unwrap();
        let mut port = Port::new(&gpu, &result, &live);
        port.context = (
            wrong_gpu.instance().clone(),
            wrong_gpu.adapter().clone(),
            wrong_gpu.device().clone(),
            wrong_gpu.queue().clone(),
        );
        let mut host = DesktopViewportHost::new(port, mapping(128, 64));
        host.register(result.identity().clone()).unwrap();
        assert_eq!(
            host.present_scheduled_object(&gpu, &result, &live)
                .unwrap()
                .status(),
            &ViewportStatus::Failed(FatalAction::ValidationFailure)
        );
        assert_eq!(
            (
                host.test_port().calls,
                host.test_port().frames,
                host.test_port().settles
            ),
            (1, 0, 0)
        );
        drop(live);
        scheduler.cancel(work.work_id()).unwrap();
    }

    #[test]
    fn gpu_deferred_then_stale_terminal_and_replaced_work_cannot_acquire() {
        let mut gpu = Compositor::new().expect("real headless Metal");
        let snapshot = snapshot("retire-work");
        let input = plan(&snapshot, 7);
        let mut scheduler = FrameScheduler::new();
        let work = schedule(&mut scheduler, &snapshot, 7);
        let (result, mut host) = {
            let live = prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).unwrap();
            let result = gpu.compose_scheduled_object(&live).unwrap();
            let mut port = Port::new(&gpu, &result, &live);
            port.steps.push_back(Step::Timeout);
            let mut host = DesktopViewportHost::new(port, mapping(128, 64));
            host.register(result.identity().clone()).unwrap();
            assert_eq!(
                host.present_scheduled_object(&gpu, &result, &live)
                    .unwrap()
                    .status(),
                &ViewportStatus::Deferred(DeferredAction::Timeout)
            );
            (result, host)
        };
        let next = schedule(&mut scheduler, &snapshot, 0);
        let next_plan = plan(&snapshot, 0);
        let live = prepare(&scheduler, &next, &next_plan, &selector(&snapshot, 0)).unwrap();
        assert!(prepare(&scheduler, &work, &input, &selector(&snapshot, 7)).is_err());
        assert!(host.present_scheduled_object(&gpu, &result, &live).is_err());
        drop(live);
        // Cancel the current generation before any newer generation masks it.
        scheduler.cancel(next.work_id()).unwrap();
        assert!(prepare(&scheduler, &next, &next_plan, &selector(&snapshot, 0)).is_err());
        scheduler.cancel(work.work_id()).unwrap();
        let replaced = schedule(&mut scheduler, &snapshot, 7);
        assert!(prepare(&scheduler, &replaced, &input, &selector(&snapshot, 7)).is_ok());
        scheduler.replace_document("retired-to-opacity").unwrap();
        assert!(prepare(&scheduler, &replaced, &input, &selector(&snapshot, 7)).is_err());
        assert_eq!(
            host.reconcile_replaced(&[work.work_id(), next.work_id(), replaced.work_id()]),
            vec![work.work_id()]
        );
        assert_eq!(
            (
                host.test_port().calls,
                host.test_port().frames,
                host.test_port().settles
            ),
            (1, 0, 0)
        );
        host.dispose();
        assert_eq!(scheduler.lease_counters().live(), 0);
    }
}
