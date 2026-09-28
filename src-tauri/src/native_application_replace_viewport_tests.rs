//! Prechange attached-surface oracle for the N20R4 host replacement seam.

use super::*;
use crate::{
    native_application_commands::viewport_host::{self, Presentation, TestViewportDriver},
    native_dispatch::ReplacementAdmission,
};
use native_engine::{
    compositor::{CompositionResult, Compositor},
    desktop_viewport::{
        AcquiredSurfaceFrame, CssBounds, DesktopViewportHost, PhysicalExtent,
        SettledSurfaceFrame, SurfaceAttempt, SurfacePort, SurfacePresentation,
        SurfaceRecoveryError, ViewportError, ViewportMapping, ViewportStatus,
    },
    render_scene::ScheduledFrameIdentity,
    scheduler::WorkId,
};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct SurfaceState {
    attached_snapshot: Option<String>,
    disposals: usize,
}

struct FakeSurface {
    state: Arc<Mutex<SurfaceState>>,
}

struct FakeFrame {
    state: Arc<Mutex<SurfaceState>>,
    snapshot: String,
}

impl AcquiredSurfaceFrame for FakeFrame {
    fn settle(self) -> SettledSurfaceFrame {
        self.state.lock().unwrap().attached_snapshot = Some(self.snapshot);
        SettledSurfaceFrame::Presented
    }
}

impl SurfacePort for FakeSurface {
    type Frame<'port, 'source> = FakeFrame;

    fn acquire<'port, 'source>(
        &'port mut self,
        source: SurfacePresentation<'source>,
    ) -> SurfaceAttempt<Self::Frame<'port, 'source>> {
        SurfaceAttempt::Acquired(FakeFrame {
            state: Arc::clone(&self.state),
            snapshot: source.identity.evaluation_key().document_snapshot_id().to_owned(),
        })
    }

    fn recreate(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> { Ok(()) }
    fn reconfigure(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> { Ok(()) }
    fn dispose(&mut self) {
        let mut state = self.state.lock().unwrap();
        state.attached_snapshot = None;
        state.disposals += 1;
    }
}

struct FakeViewport {
    host: DesktopViewportHost<FakeSurface>,
}

impl TestViewportDriver for FakeViewport {
    fn present(
        &mut self,
        compositor: &Compositor,
        result: &CompositionResult,
        identity: ScheduledFrameIdentity,
    ) -> crate::native_application_contract::HostResult<Presentation> {
        self.host.register(identity.clone()).map_err(viewport_error)?;
        let receipt = self.host.present_composition(compositor, result).map_err(viewport_error)?;
        assert_eq!(receipt.status(), &ViewportStatus::Presented);
        Ok(Presentation {
            work_id: identity.work_id(),
            view_generation: identity.view_generation().value(),
            status: "presented",
        })
    }

    fn reconcile_replaced(&mut self, work_ids: &[WorkId]) -> Vec<WorkId> {
        self.host.reconcile_replaced(work_ids)
    }
}

fn viewport_error(error: ViewportError) -> nemo_mcp::contract::NativeApplicationError {
    crate::native_application_contract::host_error("internal", format!("fake viewport: {error:?}"))
}

fn mapping() -> ViewportMapping {
    ViewportMapping::new(
        CssBounds { x: 0.0, y: 0.0, width: 320.0, height: 180.0 },
        PhysicalExtent { width: 320, height: 180 },
        PhysicalExtent { width: 320, height: 180 },
        1.0,
    ).unwrap()
}

struct TestSlot;
impl Drop for TestSlot {
    fn drop(&mut self) { viewport_host::remove_test_viewport(); }
}

#[test]
fn attached_presented_a_without_pending_work_is_retired_before_b_activation() {
    let (_scratch, native, old_document, prepared_b) = setup();
    let state = Arc::new(Mutex::new(SurfaceState::default()));
    viewport_host::install_test_viewport(
        "native-fixture".into(),
        Box::new(FakeViewport {
            host: DesktopViewportHost::new(FakeSurface { state: Arc::clone(&state) }, mapping()),
        }),
    );
    let _slot = TestSlot;

    let (preview, snapshot, compositor) = {
        let mut authority = native.lock().unwrap();
        let generation = authority.active_generation().unwrap();
        let desktop = authority.active_mut(generation).unwrap()
            .as_any_mut().downcast_mut::<DesktopNativeApplication>().unwrap();
        let snapshot = snapshot_id(desktop, "viewport-snapshot-a");
        let preview = desktop.prepare_preview(
            &preview_request(&old_document, &snapshot, "geometry-a", "v1"),
        ).unwrap();
        (preview, snapshot, desktop.preview_compositor().inner())
    };
    let presented = {
        let compositor = compositor.lock().unwrap();
        viewport_host::present(
            "native-fixture", &compositor, &preview.result, preview.identity.clone(),
        ).unwrap()
    };
    assert_eq!(presented.status, "presented");
    {
        let mut authority = native.lock().unwrap();
        let generation = authority.active_generation().unwrap();
        let desktop = authority.active_mut(generation).unwrap()
            .as_any_mut().downcast_mut::<DesktopNativeApplication>().unwrap();
        desktop.finish_preview(presented.work_id, presented.status).unwrap();
    }
    assert_eq!(state.lock().unwrap().attached_snapshot.as_deref(), Some(snapshot.as_str()));
    let admission = native.lock().unwrap().admit_replace_request(
        "retire-a", b"typed-b", "native-fixture", &old_document, 0, |_| Ok(()),
    ).unwrap();
    let ReplacementAdmission::Execute(fresh) = admission else { panic!("fresh B must execute") };
    let receipt = complete_replacement(&native, fresh, "native-fixture", prepared_b).unwrap();
    assert_ne!(receipt.document_id, old_document);
    assert!(receipt.cancelled_preview_work_ids.is_empty());
    assert_eq!(native.lock().unwrap().active_generation().unwrap(), fresh);
    let surface = state.lock().unwrap();
    assert!(surface.attached_snapshot.is_none(), "A surface remains attached after B activation");
    assert_eq!(surface.disposals, 1, "old A surface must retire exactly once");
}
