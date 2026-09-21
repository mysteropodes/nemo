use crate::codec::decode_project;
use crate::commands::NativeOpacityApplication;
use crate::compositor::Compositor;
use crate::desktop_viewport::*;
use crate::render_scene::{
    prepare, GeometryPaintInput, LayerGeometry, OpaqueSrgbPaint, ScheduledFrameIdentity,
};
use crate::scheduler::{
    EvaluationKey, FrameFailure, FrameFailureKind, FrameScheduler, OutputSpec,
    PublicationDisposition,
};
use std::cell::RefCell;
use std::collections::VecDeque;

const PROJECT: &[u8] = include_bytes!("fixtures/opacity-v2/project.json");

#[derive(Clone, Copy)]
enum Step {
    Presented,
    Lost,
    Outdated,
    Timeout,
    Occluded,
    Validation,
    DeviceLost,
}
struct FakePort {
    steps: VecDeque<Step>,
    events: RefCell<Vec<&'static str>>,
    recreates: usize,
    reconfigures: usize,
    disposals: usize,
    settles: usize,
    recreate_error: Option<SurfaceRecoveryError>,
    reconfigure_error: Option<SurfaceRecoveryError>,
    presentations: Vec<(PhysicalExtent, PhysicalExtent, SurfaceScalePolicy)>,
    contexts: Vec<(wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue)>,
}
impl Default for FakePort {
    fn default() -> Self {
        Self {
            steps: VecDeque::new(),
            events: RefCell::new(Vec::new()),
            recreates: 0,
            reconfigures: 0,
            disposals: 0,
            settles: 0,
            recreate_error: None,
            reconfigure_error: None,
            presentations: Vec::new(),
            contexts: Vec::new(),
        }
    }
}
struct FakeFrame<'a> {
    port: Option<&'a mut FakePort>,
    outcome: SettledSurfaceFrame,
    consumed: bool,
}
impl AcquiredSurfaceFrame for FakeFrame<'_> {
    fn settle(mut self) -> SettledSurfaceFrame {
        let outcome = self.outcome;
        self.port
            .as_deref_mut()
            .unwrap()
            .events
            .borrow_mut()
            .push("settle");
        self.port.as_deref_mut().unwrap().settles += 1;
        self.consumed = true;
        outcome
    }
}
impl Drop for FakeFrame<'_> {
    fn drop(&mut self) {
        if let Some(port) = self.port.take() {
            port.events.borrow_mut().push(if self.consumed {
                "frame-dropped"
            } else {
                "frame-leaked"
            });
        }
    }
}
impl FakePort {
    fn queue(&mut self, steps: &[Step]) {
        self.steps.extend(steps.iter().copied());
    }
    fn attempt(&mut self) -> SurfaceAttempt<FakeFrame<'_>> {
        self.events.borrow_mut().push("acquire");
        match self.steps.pop_front().unwrap_or(Step::Presented) {
            Step::Presented => SurfaceAttempt::Acquired(FakeFrame {
                port: Some(self),
                outcome: SettledSurfaceFrame::Presented,
                consumed: false,
            }),
            Step::Validation => SurfaceAttempt::Acquired(FakeFrame {
                port: Some(self),
                outcome: SettledSurfaceFrame::ValidationFailure,
                consumed: false,
            }),
            Step::DeviceLost => SurfaceAttempt::Acquired(FakeFrame {
                port: Some(self),
                outcome: SettledSurfaceFrame::DeviceLost,
                consumed: false,
            }),
            Step::Lost => SurfaceAttempt::Lost,
            Step::Outdated => SurfaceAttempt::Outdated,
            Step::Timeout => SurfaceAttempt::Timeout,
            Step::Occluded => SurfaceAttempt::Occluded,
        }
    }
}
impl SurfacePort for FakePort {
    type Frame<'port, 'source> = FakeFrame<'port>;
    fn acquire<'port, 'source>(
        &'port mut self,
        source: SurfacePresentation<'source>,
    ) -> SurfaceAttempt<Self::Frame<'port, 'source>> {
        self.presentations.push((
            source.source_extent,
            source.surface_extent,
            source.scale_policy,
        ));
        self.contexts.push((
            source.instance.clone(),
            source.adapter.clone(),
            source.device.clone(),
            source.queue.clone(),
        ));
        self.attempt()
    }
    fn recreate(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        self.events.borrow_mut().push("recreate");
        self.recreates += 1;
        self.recreate_error.map_or(Ok(()), Err)
    }
    fn reconfigure(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        self.events.borrow_mut().push("reconfigure");
        self.reconfigures += 1;
        self.reconfigure_error.map_or(Ok(()), Err)
    }
    fn dispose(&mut self) {
        self.events.borrow_mut().push("dispose");
        self.disposals += 1;
    }
    fn simulated_acquire<'port>(
        &'port mut self,
        _: &ScheduledFrameIdentity,
    ) -> SurfaceAttempt<Self::Frame<'port, 'port>> {
        self.attempt()
    }
}

fn mapping(physical: PhysicalExtent, composition: PhysicalExtent, dpr: f64) -> ViewportMapping {
    ViewportMapping::new(
        CssBounds {
            x: 100.0,
            y: 50.0,
            width: 640.0,
            height: 360.0,
        },
        physical,
        composition,
        dpr,
    )
    .unwrap()
}
fn standard() -> ViewportMapping {
    mapping(
        PhysicalExtent {
            width: 1280,
            height: 720,
        },
        PhysicalExtent {
            width: 320,
            height: 180,
        },
        1.25,
    )
}
fn app() -> NativeOpacityApplication {
    NativeOpacityApplication::new("viewport", decode_project(PROJECT).unwrap()).unwrap()
}
fn identity(
    app: &NativeOpacityApplication,
    scheduler: &mut FrameScheduler,
    frame: u32,
) -> ScheduledFrameIdentity {
    let snapshot = app.acquire_snapshot(0).unwrap();
    let key = EvaluationKey::new(
        snapshot.id(),
        "scene-root",
        frame,
        "final",
        OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap(),
        Vec::<(String, String)>::new(),
    )
    .unwrap();
    ScheduledFrameIdentity::from_scheduled(&scheduler.schedule(snapshot, key).unwrap())
}
fn queue(host: &mut DesktopViewportHost<FakePort>, steps: &[Step]) {
    host.test_port_mut().queue(steps);
}
fn fail(scheduler: &mut FrameScheduler, id: &ScheduledFrameIdentity) {
    assert_eq!(
        scheduler
            .fail(
                id.work_id(),
                FrameFailure::new(FrameFailureKind::Worker, "viewport failure")
            )
            .unwrap()
            .publication(),
        &PublicationDisposition::NotPublished
    );
}
fn assert_leases(scheduler: &FrameScheduler, exact: u64) {
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (exact, exact, 0)
    );
}

#[rustfmt::skip]
#[test]
fn mapping_freezes_edges_extent_dpr_and_finite_pointer_boundary() {
    let map = standard(); let point = CssPoint { x: 420.0, y: 230.0 };
    assert_eq!(map.pointer_to_local_css(point), CssPoint { x: 320.0, y: 180.0 }); assert_eq!(map.pointer_to_physical(point), CssPoint { x: 640.0, y: 360.0 }); assert_eq!(map.pointer_to_composition(point), CssPoint { x: 160.0, y: 90.0 });
    let edges = CompositionEdges { x0: 20.0, y0: 60.0, x1: 40.0, y1: 80.0 };
    assert_eq!(map.selection_to_local_css(edges).unwrap(), CssEdges { x0: 40.0, y0: 120.0, x1: 80.0, y1: 160.0 }); assert_eq!(map.selection_to_client(edges).unwrap(), CssEdges { x0: 140.0, y0: 170.0, x1: 180.0, y1: 210.0 });
    assert_eq!(mapping(map.physical_extent(), map.composition_extent(), 2.0).pointer_to_physical(point), CssPoint { x: 640.0, y: 360.0 });
    for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] { assert!(ViewportMapping::new(CssBounds { x: 0.0, y: 0.0, width: 1.0, height: 1.0 }, PhysicalExtent { width: 1, height: 1 }, PhysicalExtent { width: 1, height: 1 }, bad).is_err()); }
    for invalid in [CompositionEdges { x0: 2.0, y0: 0.0, x1: 1.0, y1: 1.0 }, CompositionEdges { x0: f64::NAN, y0: 0.0, x1: 1.0, y1: 1.0 }] { assert_eq!(map.selection_to_client(invalid), Err(ViewportError::InvalidSelection)); }
    let host = DesktopViewportHost::new(FakePort::default(), map); assert_eq!(host.surface_scale_policy(), SurfaceScalePolicy::StretchToSurface); assert_eq!(host.pointer_intent(CssPoint { x: f64::NAN, y: 0.0 }), Err(ViewportError::InvalidPointer));
}

#[rustfmt::skip]
#[test]
fn identity_deferred_and_terminal_receipts_are_exactly_owned() {
    let app = app(); let mut scheduler = FrameScheduler::new(); let first = identity(&app, &mut scheduler, 1); let rejected = identity(&app, &mut scheduler, 2);
    let mut host = DesktopViewportHost::new(FakePort::default(), standard()); assert!(matches!(host.simulate_port_completion(first.clone()), Err(ViewportError::Unregistered(_)))); host.register(first.clone()).unwrap(); assert_eq!(host.register_under_for_test(first.work_id(), rejected.clone()), Err(ViewportError::IdentityMismatch(first.work_id())));
    queue(&mut host, &[Step::Timeout, Step::Presented]); assert_eq!(host.simulate_port_completion(first.clone()).unwrap().status(), &ViewportStatus::Deferred(DeferredAction::Timeout)); assert_eq!(host.pending_work_ids(), vec![first.work_id()]); assert_eq!(host.simulate_port_completion(first.clone()).unwrap().status(), &ViewportStatus::Presented); let events = host.test_port().events.borrow().len(); assert_eq!(host.simulate_port_completion(first.clone()).unwrap().status(), &ViewportStatus::Presented); assert_eq!(host.test_port().events.borrow().len(), events); scheduler.succeed(first.work_id()).unwrap(); scheduler.cancel(rejected.work_id()).unwrap();
    let occluded = identity(&app, &mut scheduler, 3); let mut host = DesktopViewportHost::new(FakePort::default(), standard()); host.register(occluded.clone()).unwrap(); queue(&mut host, &[Step::Occluded, Step::Presented]); assert_eq!(host.simulate_port_completion(occluded.clone()).unwrap().status(), &ViewportStatus::Deferred(DeferredAction::Occluded)); assert_eq!(host.simulate_port_completion(occluded.clone()).unwrap().status(), &ViewportStatus::Presented); scheduler.succeed(occluded.work_id()).unwrap(); assert_leases(&scheduler, 3);
}

#[rustfmt::skip]
#[test]
fn recovery_consumes_owned_frames_before_recreate_or_reconfigure() {
    let app = app(); let mut scheduler = FrameScheduler::new();
    for (initial, recovery) in [(Step::Lost, "recreate"), (Step::Outdated, "reconfigure")] {
        let id = identity(&app, &mut scheduler, if matches!(initial, Step::Lost) { 1 } else { 2 }); let mut host = DesktopViewportHost::new(FakePort::default(), standard()); host.register(id.clone()).unwrap(); queue(&mut host, &[initial, Step::Presented]); assert_eq!(host.simulate_port_completion(id.clone()).unwrap().status(), &ViewportStatus::Presented); assert_eq!(host.test_port().events.borrow().as_slice(), ["acquire", recovery, "acquire", "settle", "frame-dropped"]); scheduler.succeed(id.work_id()).unwrap();
    }
    assert_leases(&scheduler, 2);
}

#[rustfmt::skip]
#[test]
fn fatal_receipt_is_retained_before_latch_but_disposal_has_precedence() {
    let app = app(); let mut scheduler = FrameScheduler::new(); let id = identity(&app, &mut scheduler, 1); let rejected = identity(&app, &mut scheduler, 2);
    let mut host = DesktopViewportHost::new(FakePort::default(), standard()); host.register(id.clone()).unwrap(); queue(&mut host, &[Step::Validation]); let failed = host.simulate_port_completion(id.clone()).unwrap(); assert_eq!(failed.status(), &ViewportStatus::Failed(FatalAction::ValidationFailure)); let events = host.test_port().events.borrow().len();
    assert_eq!(host.simulate_port_completion(id.clone()).unwrap(), failed); assert_eq!(host.simulate_port_completion(rejected.clone()), Err(ViewportError::Fatal(FatalAction::ValidationFailure))); assert_eq!(host.register(rejected.clone()), Err(ViewportError::Fatal(FatalAction::ValidationFailure))); assert_eq!(host.resize(mapping(PhysicalExtent { width: 640, height: 360 }, PhysicalExtent { width: 320, height: 180 }, 1.0)), Err(ViewportError::Fatal(FatalAction::ValidationFailure))); assert_eq!(host.pointer_intent(CssPoint { x: 0.0, y: 0.0 }), Err(ViewportError::Fatal(FatalAction::ValidationFailure))); assert_eq!(host.test_port().events.borrow().len(), events); fail(&mut scheduler, &id); scheduler.cancel(rejected.work_id()).unwrap();
    host.dispose(); assert_eq!(host.simulate_port_completion(id), Err(ViewportError::Disposed)); assert_eq!(host.test_port().disposals, 1); assert_leases(&scheduler, 2);
}

#[rustfmt::skip]
#[test]
fn retry_configuration_and_resize_failures_latch_without_port_reentry() {
    let app = app(); let mut scheduler = FrameScheduler::new();
    for (first, retry, action) in [(Step::Lost, Step::Lost, FatalAction::RetryLost), (Step::Outdated, Step::Outdated, FatalAction::RetryOutdated)] {
        let id = identity(&app, &mut scheduler, 10 + action as u32); let mut host = DesktopViewportHost::new(FakePort::default(), standard()); host.register(id.clone()).unwrap(); queue(&mut host, &[first, retry]); assert_eq!(host.simulate_port_completion(id.clone()).unwrap().status(), &ViewportStatus::Failed(action)); fail(&mut scheduler, &id);
    }
    let recreate = identity(&app, &mut scheduler, 20); let mut host = DesktopViewportHost::new(FakePort::default(), standard()); host.register(recreate.clone()).unwrap(); host.test_port_mut().recreate_error = Some(SurfaceRecoveryError::Failed); queue(&mut host, &[Step::Lost]); assert_eq!(host.simulate_port_completion(recreate.clone()).unwrap().status(), &ViewportStatus::Failed(FatalAction::RecreateFailed)); fail(&mut scheduler, &recreate);
    let reconfigure = identity(&app, &mut scheduler, 21); let mut host = DesktopViewportHost::new(FakePort::default(), standard()); host.register(reconfigure.clone()).unwrap(); host.test_port_mut().reconfigure_error = Some(SurfaceRecoveryError::Failed); queue(&mut host, &[Step::Outdated]); assert_eq!(host.simulate_port_completion(reconfigure.clone()).unwrap().status(), &ViewportStatus::Failed(FatalAction::ReconfigureFailed)); fail(&mut scheduler, &reconfigure);
    let lost = identity(&app, &mut scheduler, 22); let mut host = DesktopViewportHost::new(FakePort::default(), standard()); host.register(lost.clone()).unwrap(); host.test_port_mut().recreate_error = Some(SurfaceRecoveryError::DeviceLost); queue(&mut host, &[Step::Lost, Step::Presented]); assert_eq!(host.simulate_port_completion(lost.clone()).unwrap().status(), &ViewportStatus::Failed(FatalAction::DeviceLost)); assert_eq!(host.test_port().events.borrow().as_slice(), ["acquire", "recreate"]); fail(&mut scheduler, &lost);
    let mut resize = DesktopViewportHost::new(FakePort::default(), standard()); let same = mapping(PhysicalExtent { width: 1280, height: 720 }, PhysicalExtent { width: 640, height: 360 }, 2.0); resize.resize(same).unwrap(); assert_eq!(resize.test_port().reconfigures, 0); let moved = ViewportMapping::new(CssBounds { x: 120.0, y: 75.0, width: 640.0, height: 360.0 }, same.physical_extent(), same.composition_extent(), same.reported_dpr()).unwrap(); resize.resize(moved).unwrap(); assert_eq!(resize.test_port().reconfigures, 1); let old = resize.mapping(); resize.test_port_mut().reconfigure_error = Some(SurfaceRecoveryError::Failed); let changed = mapping(PhysicalExtent { width: 640, height: 360 }, PhysicalExtent { width: 320, height: 180 }, 2.0); assert_eq!(resize.resize(changed), Err(ViewportError::Fatal(FatalAction::ReconfigureFailed))); assert_eq!(resize.mapping(), old); assert_leases(&scheduler, 5);
}

#[rustfmt::skip]
#[test]
fn sequential_and_out_of_order_bursts_present_only_current_generation() {
    let app = app(); let mut scheduler = FrameScheduler::new(); let mut sequential = DesktopViewportHost::new(FakePort::default(), standard());
    for frame in 1..=120 { let id = identity(&app, &mut scheduler, frame); sequential.register(id.clone()).unwrap(); assert_eq!(sequential.simulate_port_completion(id.clone()).unwrap().status(), &ViewportStatus::Presented); scheduler.succeed(id.work_id()).unwrap(); }
    assert_eq!(sequential.test_port().settles, 120); assert!(sequential.pending_work_ids().is_empty()); assert_leases(&scheduler, 120);
    let ids: Vec<_> = (121..=240).map(|frame| identity(&app, &mut scheduler, frame)).collect(); let mut burst = DesktopViewportHost::new(FakePort::default(), standard()); for id in &ids { burst.register(id.clone()).unwrap(); }
    assert_eq!(burst.simulate_port_completion(ids[119].clone()).unwrap().status(), &ViewportStatus::Presented); for id in ids[..119].iter().rev() { assert_eq!(burst.simulate_port_completion(id.clone()).unwrap().status(), &ViewportStatus::StaleDiscarded); }
    assert_eq!(burst.test_port().settles, 1); assert!(burst.pending_work_ids().is_empty()); for id in &ids { scheduler.succeed(id.work_id()).unwrap(); } assert_leases(&scheduler, 240);
}

#[rustfmt::skip]
#[test]
fn scheduler_terminalization_is_caller_owned_and_releases_every_lease() {
    let app = app();
    let mut published = FrameScheduler::new(); let id = identity(&app, &mut published, 1); let mut host = DesktopViewportHost::new(FakePort::default(), standard()); host.register(id.clone()).unwrap(); assert_eq!(host.simulate_port_completion(id.clone()).unwrap().status(), &ViewportStatus::Presented); assert!(published.receipt(id.work_id()).is_none()); assert_eq!(published.succeed(id.work_id()).unwrap().publication(), &PublicationDisposition::Published); assert_leases(&published, 1);
    let mut stale = FrameScheduler::new(); let old = identity(&app, &mut stale, 2); let new = identity(&app, &mut stale, 3); let mut host = DesktopViewportHost::new(FakePort::default(), standard()); host.register(old.clone()).unwrap(); host.register(new.clone()).unwrap(); assert_eq!(host.simulate_port_completion(old.clone()).unwrap().status(), &ViewportStatus::StaleDiscarded); assert_eq!(stale.succeed(old.work_id()).unwrap().publication(), &PublicationDisposition::SuppressedStale { newest_generation: new.view_generation() }); stale.succeed(new.work_id()).unwrap(); assert_leases(&stale, 2);
    let mut failed = FrameScheduler::new(); let id = identity(&app, &mut failed, 4); let mut host = DesktopViewportHost::new(FakePort::default(), standard()); host.register(id.clone()).unwrap(); queue(&mut host, &[Step::DeviceLost]); assert!(matches!(host.simulate_port_completion(id.clone()).unwrap().status(), ViewportStatus::Failed(_))); fail(&mut failed, &id); assert_leases(&failed, 1);
}

#[rustfmt::skip]
#[test]
fn n12_result_dimensions_bind_to_mapping_before_port_activity() {
    let app = app(); let mut scheduler = FrameScheduler::new(); let snapshot = app.acquire_snapshot(0).unwrap();
    let key = EvaluationKey::new(snapshot.id(), "scene-root", 1, "final", OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap(), [("geometry/r08".to_owned(), "v1".to_owned())]).unwrap(); let scheduled = scheduler.schedule(snapshot, key).unwrap(); let geometry = GeometryPaintInput::new("geometry/r08", "v1", vec![LayerGeometry::new("r08_curve_layer", [20.0, 60.0, 40.0, 80.0], [1.0, 0.0, 0.0, 1.0, 0.0, 0.0], OpaqueSrgbPaint::new(255, 0, 0)).unwrap()]).unwrap(); let scene = prepare(&scheduler, &scheduled, &geometry).unwrap(); let mut compositor = Compositor::new().expect("N12 GPU context"); let result = compositor.compose(&scene).unwrap(); let id = result.scheduled_identity().clone();
    let mut host = DesktopViewportHost::new(FakePort::default(), mapping(PhysicalExtent { width: 1280, height: 720 }, PhysicalExtent { width: 319, height: 180 }, 1.0)); host.register(id.clone()).unwrap(); assert_eq!(host.present_composition(&compositor, &result), Err(ViewportError::CompositionExtentMismatch { expected: PhysicalExtent { width: 319, height: 180 }, actual: PhysicalExtent { width: 320, height: 180 } })); assert!(host.test_port().events.borrow().is_empty()); host.resize(mapping(PhysicalExtent { width: 1280, height: 720 }, PhysicalExtent { width: 320, height: 180 }, 1.0)).unwrap(); assert_eq!(host.test_port().reconfigures, 0); assert_eq!(host.present_composition(&compositor, &result).unwrap().status(), &ViewportStatus::Presented); assert_eq!(host.test_port().presentations, vec![(PhysicalExtent { width: 320, height: 180 }, PhysicalExtent { width: 1280, height: 720 }, SurfaceScalePolicy::StretchToSurface)]); scheduler.succeed(id.work_id()).unwrap(); assert_leases(&scheduler, 2);
    let context = &host.test_port().contexts[0]; assert_eq!(&context.0, compositor.instance()); assert_eq!(&context.1, compositor.adapter()); assert_eq!(&context.2, compositor.device()); assert_eq!(&context.3, compositor.queue());
}

#[rustfmt::skip]
#[test]
fn dispose_returns_only_pending_ids_once_and_then_closes_all_activity() {
    let app = app(); let mut scheduler = FrameScheduler::new(); let terminal = identity(&app, &mut scheduler, 1); let mut host = DesktopViewportHost::new(FakePort::default(), standard()); host.register(terminal.clone()).unwrap(); assert_eq!(host.simulate_port_completion(terminal.clone()).unwrap().status(), &ViewportStatus::Presented);
    let first = identity(&app, &mut scheduler, 2); let second = identity(&app, &mut scheduler, 3); host.register(first.clone()).unwrap(); host.register(second.clone()).unwrap(); let events = host.test_port().events.borrow().len(); assert_eq!(host.dispose(), vec![first.work_id(), second.work_id()]); assert!(host.dispose().is_empty()); assert_eq!(host.test_port().disposals, 1); assert_eq!(host.register(first.clone()), Err(ViewportError::Disposed)); assert_eq!(host.resize(mapping(PhysicalExtent { width: 640, height: 360 }, PhysicalExtent { width: 320, height: 180 }, 1.0)), Err(ViewportError::Disposed)); assert_eq!(host.pointer_intent(CssPoint { x: 1.0, y: 1.0 }), Err(ViewportError::Disposed)); assert_eq!(host.selection_intent(CompositionEdges { x0: 0.0, y0: 0.0, x1: 1.0, y1: 1.0 }), Err(ViewportError::Disposed)); assert_eq!(host.simulate_port_completion(second.clone()), Err(ViewportError::Disposed)); assert_eq!(host.test_port().events.borrow().len(), events + 1);
    assert_eq!(scheduler.succeed(terminal.work_id()).unwrap().publication(), &PublicationDisposition::SuppressedStale { newest_generation: second.view_generation() }); scheduler.cancel(first.work_id()).unwrap(); scheduler.cancel(second.work_id()).unwrap(); assert_leases(&scheduler, 3);
}

#[rustfmt::skip]
#[test]
fn replacement_reconciles_deferred_viewport_work_before_later_disposal() {
    let app = app(); let mut scheduler = FrameScheduler::new(); let pending = identity(&app, &mut scheduler, 1); let deferred = identity(&app, &mut scheduler, 2);
    let mut host = DesktopViewportHost::new(FakePort::default(), standard()); host.register(pending.clone()).unwrap(); host.register(deferred.clone()).unwrap(); queue(&mut host, &[Step::Timeout]); assert_eq!(host.simulate_port_completion(deferred.clone()).unwrap().status(), &ViewportStatus::Deferred(DeferredAction::Timeout));
    let replaced = scheduler.replace_document("replacement-document").unwrap(); let replaced_ids: Vec<_> = replaced.iter().map(|receipt| receipt.work_id()).collect(); assert_eq!(replaced_ids, vec![pending.work_id(), deferred.work_id()]);
    assert_eq!(host.reconcile_replaced(&replaced_ids), replaced_ids); assert!(host.pending_work_ids().is_empty()); assert!(host.reconcile_replaced(&replaced_ids).is_empty()); assert!(host.dispose().is_empty()); assert_eq!(host.test_port().disposals, 1); assert_leases(&scheduler, 2);
}
