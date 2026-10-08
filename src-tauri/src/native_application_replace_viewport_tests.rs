//! Prechange attached-surface oracle for the N20R4 host replacement seam.

use crate::{
    native_application::{admit_release_request, complete_release, DesktopNativeApplication},
    native_application_commands::{
        replace_commands::{
            acceptance_tests::{desktop, geometry_b, preview_request, project, setup, snapshot_id},
            complete_replacement, complete_replacement_with,
        },
        replace_replay::replay_result,
    },
    native_application_contract::{
        admit_project, host_error, HostResult, NativeReleaseRequest, ViewportInput,
    },
    native_application_viewport::{
        self as viewport_host, FakeOutcome, FakeViewport, Presentation, SurfaceState,
        TestCompositionResult, TestFrameIdentity, TestViewportDriver, TestViewportMapping,
        TestWorkId,
    },
    native_dispatch::{NativePhase, NativeState, ReleaseAdmission, ReplacementAdmission},
};
use native_engine::compositor::Compositor;
use nemo_mcp::contract::NATIVE_API_VERSION;
use serde_json::json;
use std::sync::{Arc, Mutex};

impl TestViewportDriver for FakeViewport {
    fn present(
        &mut self,
        compositor: &Compositor,
        result: &TestCompositionResult,
        identity: TestFrameIdentity,
    ) -> HostResult<Presentation> {
        let (work_id, view_generation, receipt) = self
            .present(compositor, result, identity)
            .map_err(|error| host_error("internal", format!("fake viewport: {error:?}")))?;
        Ok(Presentation {
            work_id,
            view_generation,
            status: viewport_host::status_label(receipt.status()),
        })
    }

    fn retire_replacement(&mut self) -> Vec<TestWorkId> {
        FakeViewport::retire_replacement(self)
    }

    fn rebind_existing(&mut self, compositor: &Compositor) -> HostResult<()> {
        FakeViewport::rebind_existing(self, compositor)
            .map_err(|error| host_error("internal", error))
    }
}

fn mapping() -> TestViewportMapping {
    serde_json::from_value::<ViewportInput>(json!({
        "cssBounds":{"x":0.0,"y":0.0,"width":320.0,"height":180.0},
        "physicalExtent":{"width":320,"height":180},
        "compositionExtent":{"width":320,"height":180},
        "reportedDpr":1.0
    }))
    .unwrap()
    .admit()
    .unwrap()
}

struct TestSlot;
impl Drop for TestSlot {
    fn drop(&mut self) {
        let _ = viewport_host::remove_test_viewport();
    }
}

fn present_a(
    native: &NativeState,
    old_document: &str,
    state: &Arc<Mutex<SurfaceState>>,
) -> (String, Arc<Mutex<Compositor>>) {
    viewport_host::install_test_viewport(
        "native-fixture".into(),
        Box::new(FakeViewport::new(mapping(), Arc::clone(state))),
    );
    let (preview, snapshot, compositor) = {
        let mut authority = native.lock().unwrap();
        let generation = authority.active_generation().unwrap();
        let desktop = authority
            .active_mut(generation)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<DesktopNativeApplication>()
            .unwrap();
        let snapshot = snapshot_id(desktop, "viewport-snapshot-a");
        let preview = desktop
            .prepare_preview(&preview_request(
                &old_document,
                &snapshot,
                "geometry-a",
                "v1",
            ))
            .unwrap();
        (preview, snapshot, desktop.preview_compositor().inner())
    };
    let presented = {
        let compositor = compositor.lock().unwrap();
        viewport_host::present(
            "native-fixture",
            &compositor,
            &preview.result,
            preview.identity.clone(),
        )
        .unwrap()
    };
    assert_eq!(presented.status, "presented");
    {
        let mut authority = native.lock().unwrap();
        let generation = authority.active_generation().unwrap();
        let desktop = authority
            .active_mut(generation)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<DesktopNativeApplication>()
            .unwrap();
        desktop
            .finish_preview(presented.work_id, presented.status)
            .unwrap();
    }
    assert_eq!(
        state.lock().unwrap().attached_snapshot.as_deref(),
        Some(snapshot.as_str())
    );
    (snapshot, compositor)
}

#[test]
fn attached_presented_a_without_pending_work_is_retired_before_b_activation() {
    let (_scratch, native, old_document, prepared_b) = setup();
    let state = Arc::new(Mutex::new(SurfaceState::default()));
    let (_snapshot, compositor) = present_a(&native, &old_document, &state);
    let _slot = TestSlot;
    let admission = native
        .lock()
        .unwrap()
        .admit_replace_request(
            "retire-a",
            b"typed-b",
            "native-fixture",
            &old_document,
            0,
            |_| Ok(()),
        )
        .unwrap();
    let ReplacementAdmission::Execute(fresh) = admission else {
        panic!("fresh B must execute")
    };
    let receipt = complete_replacement(&native, fresh, "native-fixture", prepared_b).unwrap();
    assert_ne!(receipt.document_id, old_document);
    assert!(receipt.cancelled_preview_work_ids.is_empty());
    assert_eq!(native.lock().unwrap().active_generation().unwrap(), fresh);
    let surface = state.lock().unwrap();
    assert!(
        surface.attached_snapshot.is_none(),
        "A surface remains attached after B activation"
    );
    assert_eq!(
        surface.disposals, 1,
        "old A surface must retire exactly once"
    );
    assert_eq!(surface.rebinds, 1, "one fresh blank host must be bound");
    drop(surface);

    let (preview_b, snapshot_b) = {
        let mut authority = native.lock().unwrap();
        let desktop = authority
            .active_mut(fresh)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<DesktopNativeApplication>()
            .unwrap();
        let snapshot = snapshot_id(desktop, "viewport-snapshot-b");
        let preview = desktop
            .prepare_preview(&preview_request(
                &receipt.document_id,
                &snapshot,
                "geometry-b",
                "v2",
            ))
            .unwrap();
        (preview, snapshot)
    };
    let presented_b = {
        let compositor = compositor.lock().unwrap();
        viewport_host::present(
            "native-fixture",
            &compositor,
            &preview_b.result,
            preview_b.identity,
        )
        .unwrap()
    };
    assert_eq!(presented_b.status, "presented");
    assert_eq!(
        state.lock().unwrap().attached_snapshot.as_deref(),
        Some(snapshot_b.as_str())
    );
    let replay = native
        .lock()
        .unwrap()
        .lookup_replace_replay("retire-a", b"typed-b")
        .unwrap()
        .unwrap();
    assert!(replay_result(replay).unwrap().retrieved);
    let surface = state.lock().unwrap();
    assert_eq!(
        surface.disposals, 1,
        "receipt replay must not re-quarantine B"
    );
    assert_eq!(surface.rebinds, 1);
    assert_eq!(
        surface.attached_snapshot.as_deref(),
        Some(snapshot_b.as_str())
    );
}

#[test]
fn rejected_b_keeps_a_attached_and_post_retirement_failures_leave_no_surface() {
    for (stage, panics) in [
        ("viewport_retired", false),
        ("core", false),
        ("viewport_rebound", false),
        ("viewport_rebound", true),
    ] {
        let (_scratch, native, old_document, prepared_b) = setup();
        let state = Arc::new(Mutex::new(SurfaceState::default()));
        let (snapshot, _) = present_a(&native, &old_document, &state);
        let _slot = TestSlot;
        let old = native.lock().unwrap().active_generation().unwrap();
        let mut invalid = project(35);
        invalid["formatVersion"] = json!(99);
        assert!(admit_project(&invalid, &geometry_b()).is_err());
        assert_eq!(native.lock().unwrap().active_generation().unwrap(), old);
        assert_eq!(
            state.lock().unwrap().attached_snapshot.as_deref(),
            Some(snapshot.as_str())
        );

        let admission = native
            .lock()
            .unwrap()
            .admit_replace_request(
                stage,
                b"typed-b",
                "native-fixture",
                &old_document,
                0,
                |_| Ok(()),
            )
            .unwrap();
        let ReplacementAdmission::Execute(fresh) = admission else {
            panic!("B must execute")
        };
        let checked = Arc::clone(&state);
        let mut core_completed = false;
        let error = complete_replacement_with(&native, fresh, "native-fixture", prepared_b, |at| {
            if at == "viewport_retired" {
                assert!(!core_completed, "A must retire before core mutation");
                let surface = checked.lock().unwrap();
                assert!(
                    surface.attached_snapshot.is_none(),
                    "A must detach before core mutation"
                );
                assert_eq!(surface.disposals, 1);
                assert_eq!(surface.rebinds, 0);
            }
            if at == "core" {
                core_completed = true;
            }
            if at == "viewport_rebound" {
                assert!(core_completed, "B core must be staged before rebind");
                let surface = checked.lock().unwrap();
                assert!(
                    surface.attached_snapshot.is_none(),
                    "fresh host must be blank before B activation"
                );
                assert_eq!(surface.disposals, 1);
                assert_eq!(surface.rebinds, 1);
            }
            if at == stage {
                if panics {
                    panic!("injected post-rebind panic");
                }
                Err(host_error("injected", at))
            } else {
                Ok(())
            }
        })
        .unwrap_err();
        assert_eq!(error.code, "replacement_indeterminate");
        let progress = &error.details.as_ref().unwrap()["replacement"];
        assert_eq!(
            progress["failure"]["kind"],
            if panics {
                "callback_panicked"
            } else {
                "callback_failed"
            }
        );
        assert_eq!(progress["viewport"], "unknown");
        let mut authority = native.lock().unwrap();
        assert!(authority.active().is_none());
        assert!(authority.active_mut(old).is_err());
        let NativePhase::Replacing { application, .. } = &authority.phase else {
            panic!("failed B must remain fenced")
        };
        assert_eq!(
            application.document_id() == old_document,
            stage == "viewport_retired"
        );
        drop(authority);
        let surface = state.lock().unwrap();
        assert_eq!(
            surface.disposals,
            if stage == "viewport_rebound" { 2 } else { 1 }
        );
        assert!(
            surface.attached_snapshot.is_none(),
            "failed B must leave no A or B surface"
        );
        assert_eq!(
            surface.rebinds,
            if stage == "viewport_rebound" { 1 } else { 0 }
        );
    }
}

#[test]
fn rebound_b_surface_keeps_deferred_recovery_and_fatal_outcomes_distinct() {
    for (outcome, expected, recreates, reconfigures) in [
        (FakeOutcome::Timeout, "deferred-timeout", 0, 0),
        (FakeOutcome::Occluded, "deferred-occluded", 0, 0),
        (FakeOutcome::Lost, "presented", 1, 0),
        (FakeOutcome::Outdated, "presented", 0, 1),
        (FakeOutcome::ValidationFailure, "failed-validation", 0, 0),
        (FakeOutcome::DeviceLost, "failed-device-lost", 0, 0),
    ] {
        #[cfg(target_os = "macos")]
        let diagnostics = viewport_host::test_observations();
        let (_scratch, native, old_document, prepared_b) = setup();
        let state = Arc::new(Mutex::new(SurfaceState::default()));
        let (_snapshot_a, compositor) = present_a(&native, &old_document, &state);
        let _slot = TestSlot;
        let request = format!("rebound-{outcome:?}");
        let admission = native
            .lock()
            .unwrap()
            .admit_replace_request(
                &request,
                b"typed-b",
                "native-fixture",
                &old_document,
                0,
                |_| Ok(()),
            )
            .unwrap();
        let ReplacementAdmission::Execute(fresh) = admission else {
            panic!("B must execute")
        };
        let receipt = complete_replacement(&native, fresh, "native-fixture", prepared_b).unwrap();
        assert!(state.lock().unwrap().attached_snapshot.is_none());
        let (preview, snapshot_b) = {
            let mut authority = native.lock().unwrap();
            let desktop = authority
                .active_mut(fresh)
                .unwrap()
                .as_any_mut()
                .downcast_mut::<DesktopNativeApplication>()
                .unwrap();
            let snapshot = snapshot_id(desktop, &format!("snapshot-{outcome:?}"));
            let preview = desktop
                .prepare_preview(&preview_request(
                    &receipt.document_id,
                    &snapshot,
                    "geometry-b",
                    "v2",
                ))
                .unwrap();
            (preview, snapshot)
        };
        state.lock().unwrap().next_outcome = Some(outcome);
        let presented = viewport_host::present(
            "native-fixture",
            &compositor.lock().unwrap(),
            &preview.result,
            preview.identity,
        )
        .unwrap();
        assert_eq!(presented.status, expected, "{outcome:?}");
        #[cfg(target_os = "macos")]
        diagnostics.validate_last(expected);
        let surface = state.lock().unwrap();
        assert_eq!(surface.recreates, recreates, "{outcome:?}");
        assert_eq!(surface.reconfigures, reconfigures, "{outcome:?}");
        assert_eq!(
            surface.attached_snapshot.as_deref(),
            if expected == "presented" {
                Some(snapshot_b.as_str())
            } else {
                None
            },
            "{outcome:?} must not resurrect A"
        );
        assert_eq!(surface.disposals, 1);
        assert_eq!(surface.rebinds, 1);
    }
}

#[test]
fn rebound_b_close_disposes_its_surface_and_reentry_uses_fresh_host() {
    #[cfg(target_os = "macos")]
    let diagnostics = viewport_host::test_observations();
    let (scratch, native, old_document, prepared_b) = setup();
    let state_b = Arc::new(Mutex::new(SurfaceState::default()));
    let (_snapshot_a, _compositor_a) = present_a(&native, &old_document, &state_b);
    let _slot = TestSlot;
    let admission = native
        .lock()
        .unwrap()
        .admit_replace_request(
            "replace-then-close",
            b"typed-b",
            "native-fixture",
            &old_document,
            0,
            |_| Ok(()),
        )
        .unwrap();
    let ReplacementAdmission::Execute(fresh) = admission else {
        panic!("B must execute")
    };
    let receipt = complete_replacement(&native, fresh, "native-fixture", prepared_b).unwrap();
    let release = NativeReleaseRequest {
        api_version: NATIVE_API_VERSION,
        request_id: "close-b".into(),
        instance_id: "native-fixture".into(),
        document_id: receipt.document_id,
        expected_revision: receipt.content_revision,
        cancelled_before_dispatch: false,
    };
    let ReleaseAdmission::Execute { generation } =
        admit_release_request(&native, &release).unwrap()
    else {
        panic!("B close must execute")
    };
    let closed = complete_release(&native, generation, &release, || {
        Ok((viewport_host::remove_test_viewport(), "disposed"))
    })
    .unwrap();
    assert_eq!(closed.status, "succeeded");
    assert!(closed.reentry_available);
    assert_eq!(state_b.lock().unwrap().disposals, 2);
    assert!(state_b.lock().unwrap().attached_snapshot.is_none());

    let next = desktop("native-fixture", &scratch);
    let next_document = next.document_id().to_owned();
    let next_generation = {
        let mut authority = native.lock().unwrap();
        let generation = authority.reserve_install().unwrap();
        authority.install(generation, Box::new(next)).unwrap();
        generation
    };
    let state_c = Arc::new(Mutex::new(SurfaceState::default()));
    viewport_host::install_test_viewport(
        "native-fixture".into(),
        Box::new(FakeViewport::new(mapping(), Arc::clone(&state_c))),
    );
    let (preview, snapshot_c, compositor_c) = {
        let mut authority = native.lock().unwrap();
        let desktop = authority
            .active_mut(next_generation)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<DesktopNativeApplication>()
            .unwrap();
        let snapshot = snapshot_id(desktop, "reentry-c-snapshot");
        let preview = desktop
            .prepare_preview(&preview_request(
                &next_document,
                &snapshot,
                "geometry-a",
                "v1",
            ))
            .unwrap();
        (preview, snapshot, desktop.preview_compositor().inner())
    };
    let presented = viewport_host::present(
        "native-fixture",
        &compositor_c.lock().unwrap(),
        &preview.result,
        preview.identity,
    )
    .unwrap();
    assert_eq!(presented.status, "presented");
    #[cfg(target_os = "macos")]
    diagnostics.validate_last("presented");
    assert_eq!(
        state_c.lock().unwrap().attached_snapshot.as_deref(),
        Some(snapshot_c.as_str())
    );
    assert_eq!(state_b.lock().unwrap().disposals, 2);
}
