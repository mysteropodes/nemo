//! One committed host replacement callback and its fail-closed executor fallback.

use crate::native_application::PreparedDesktopReplacement;
use crate::native_application_commands::{
    host_error, reconcile_export, viewport_host, work_label, DesktopNativeApplication, HostResult,
    NativeReplacementReceipt, NativeState,
};
use crate::native_dispatch::{ReplacementProgress, ReplacementStage};
use std::panic::{catch_unwind, AssertUnwindSafe};

pub(super) fn admission_error(
    native: &NativeState,
    message: String,
) -> nemo_mcp::contract::NativeApplicationError {
    let (code, message) = message
        .split_once(':')
        .unwrap_or(("unavailable", message.as_str()));
    let mut error = host_error(code, message);
    if let Ok(authority) = native.lock() {
        if let Some(progress) = authority.replacement_progress() {
            error.details = Some(serde_json::json!({
                "disposition": "notDispatched",
                "retryExecution": false,
                "replacement": progress,
            }));
        }
    }
    error
}

fn fenced_error(progress: ReplacementProgress) -> nemo_mcp::contract::NativeApplicationError {
    let failure = progress.failure.as_ref();
    let mut error = host_error(
        "replacement_indeterminate",
        failure.map_or("native replacement is indeterminate", |value| {
            value.message.as_str()
        }),
    );
    error.details = Some(serde_json::json!({
        "disposition": "indeterminate",
        "retryExecution": false,
        "replacement": progress,
    }));
    error
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<&str>()
        .copied()
        .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
        .unwrap_or("native replacement callback panicked")
        .to_owned()
}

pub(super) fn complete_replacement(
    native: &NativeState,
    generation: u64,
    instance: &str,
    prepared: PreparedDesktopReplacement,
) -> HostResult<NativeReplacementReceipt> {
    complete_replacement_with(native, generation, instance, prepared, |_| Ok(()))
}

fn complete_replacement_with(
    native: &NativeState,
    generation: u64,
    instance: &str,
    prepared: PreparedDesktopReplacement,
    mut checkpoint: impl FnMut(&'static str) -> HostResult<()>,
) -> HostResult<NativeReplacementReceipt> {
    let mut authority = native
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let resource_count = prepared.resource_count;
    let result = {
        let (application, progress) = authority
            .replacing_mut(generation)
            .map_err(|message| host_error("unavailable", message))?;
        progress.callback = ReplacementStage::Running;
        catch_unwind(AssertUnwindSafe(
            || -> HostResult<NativeReplacementReceipt> {
                viewport_host::require_instance_or_absent(instance)?;
                let desktop = application
                    .as_any_mut()
                    .downcast_mut::<DesktopNativeApplication>()
                    .ok_or_else(|| {
                        host_error("unavailable", "native desktop host is unavailable")
                    })?;
                let (exports, preview) =
                    desktop.replace_project_prepared(prepared, progress, &mut checkpoint)?;
                progress.viewport = ReplacementStage::Running;
                checkpoint("viewport")?;
                viewport_host::reconcile_replaced(instance, &preview)?;
                progress.viewport = ReplacementStage::Complete;
                Ok(NativeReplacementReceipt {
                    document_id: desktop.document_id().to_owned(),
                    content_revision: desktop.content_revision(),
                    resource_count,
                    cancelled_preview_work_ids: preview.into_iter().map(work_label).collect(),
                    reconciled_exports: exports.iter().map(reconcile_export).collect(),
                })
            },
        ))
    };
    match result {
        Ok(Ok(receipt)) => {
            let (_, progress) = authority
                .replacing_mut(generation)
                .map_err(|message| host_error("unavailable", message))?;
            progress.callback = ReplacementStage::Complete;
            authority
                .activate_replace(generation)
                .map_err(|message| host_error("unavailable", message))?;
            Ok(receipt)
        }
        Ok(Err(error)) => {
            let progress = authority
                .fence_replace(
                    generation,
                    "callback_failed",
                    format!("{}: {}", error.code, error.message),
                )
                .map_err(|message| host_error("unavailable", message))?;
            Err(fenced_error(progress))
        }
        Err(payload) => {
            let progress = authority
                .fence_replace(generation, "callback_panicked", panic_message(payload))
                .map_err(|message| host_error("unavailable", message))?;
            Err(fenced_error(progress))
        }
    }
}

pub(super) fn fence_executor_failure(
    native: &NativeState,
    generation: u64,
    failure: viewport_host::ReplacementCallbackFailure,
) -> nemo_mcp::contract::NativeApplicationError {
    let (kind, message) = match failure {
        viewport_host::ReplacementCallbackFailure::SchedulingUnavailable => (
            "executor_unavailable",
            "native main-thread replacement could not be scheduled",
        ),
        viewport_host::ReplacementCallbackFailure::CallbackDropped => (
            "callback_dropped",
            "native main-thread replacement callback was dropped",
        ),
    };
    let mut authority = native
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match authority.fence_replace(generation, kind, message) {
        Ok(progress) => fenced_error(progress),
        Err(stale) => host_error("unavailable", stale),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        native_application::{admit_release_request, complete_release},
        native_application_commands::{
            active_generation, with_installed_instance, ApplicationMcp, DesktopArtifactPort,
            NATIVE_API_VERSION,
        },
        native_application_contract::{admit_project, GeometryResourceInput, NativeReleaseRequest},
        native_application_ports::SharedCompositor,
        native_dispatch::{NativeAuthority, NativePhase, ReleaseAdmission},
    };
    use native_engine::compositor::Compositor;
    use serde_json::{json, Value};
    use std::{
        fs,
        path::PathBuf,
        sync::{Arc, Mutex},
    };

    const PROJECT: &[u8] =
        include_bytes!("../../native-engine/tests/fixtures/opacity-v2/project.json");

    struct Scratch(PathBuf);
    impl Scratch {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!("nemo-n20r2-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn geometry() -> Vec<GeometryResourceInput> {
        serde_json::from_value(json!([{
            "resourceId": "geometry", "resourceVersion": "v1",
            "layers": [{
                "layerUid": "r08_curve_layer",
                "bounds": [0.0, 0.0, 320.0, 180.0],
                "transform": [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                "paint": {"red": 12, "green": 34, "blue": 56}
            }]
        }]))
        .unwrap()
    }

    fn project(opacity: u64) -> Value {
        let mut value: Value = serde_json::from_slice(PROJECT).unwrap();
        value["layers"][0]["motionStatic"]["opacity"] = json!([opacity]);
        value
    }

    fn setup() -> (Scratch, NativeState, String, PreparedDesktopReplacement) {
        let scratch = Scratch::new();
        let first = admit_project(&project(25), &geometry()).unwrap();
        let second = admit_project(&project(35), &geometry()).unwrap();
        let prepared = DesktopNativeApplication::prepare_replacement(second).unwrap();
        let artifacts = DesktopArtifactPort::new(
            scratch.0.join("staging"),
            std::iter::empty::<(String, PathBuf)>(),
        )
        .unwrap();
        let compositor = SharedCompositor::new(Compositor::new().unwrap());
        let application =
            DesktopNativeApplication::new("native-fixture".into(), first, artifacts, compositor)
                .unwrap();
        let old_document = application.document_id().to_owned();
        let mut authority = NativeAuthority::default();
        let generation = authority.reserve_install().unwrap();
        authority
            .install(generation, Box::new(application))
            .unwrap();
        (
            scratch,
            Arc::new(Mutex::new(authority)),
            old_document,
            prepared,
        )
    }

    fn reserve(native: &NativeState, old_document: &str) -> (u64, u64) {
        let mut authority = native.lock().unwrap();
        let old = authority.active_generation().unwrap();
        let fresh = authority
            .admit_replace("native-fixture", old_document, 0, |application| {
                if application.as_any_mut().is::<DesktopNativeApplication>() {
                    Ok(())
                } else {
                    Err("unavailable:native desktop host is unavailable".into())
                }
            })
            .unwrap();
        (old, fresh)
    }

    #[test]
    fn invalid_b_fails_before_a_or_host_generation_changes() {
        let (_scratch, native, old_document, _prepared) = setup();
        let old = native.lock().unwrap().active_generation().unwrap();
        let mut invalid = project(35);
        invalid["formatVersion"] = json!(99);
        assert!(admit_project(&invalid, &geometry()).is_err());
        let authority = native.lock().unwrap();
        assert_eq!(authority.active_generation().unwrap(), old);
        assert_eq!(authority.active().unwrap().1.document_id(), old_document);
    }

    #[test]
    fn held_a_export_frame_cannot_finish_after_host_reservation() {
        let (scratch, native, old_document, _prepared) = setup();
        let (old, held) = {
            let mut authority = native.lock().unwrap();
            let old = authority.active_generation().unwrap();
            let application = authority.active_mut(old).unwrap();
            application
                .as_any_mut()
                .downcast_mut::<DesktopNativeApplication>()
                .unwrap()
                .bind_output("output-fixture".into(), scratch.0.join("render"))
                .unwrap();
            let request = serde_json::from_value(json!({
                "apiVersion": NATIVE_API_VERSION,
                "requestId": "begin-held",
                "instanceId": "native-fixture",
                "documentId": old_document,
                "expectedRevision": 0,
                "operation": "job.export.png.begin",
                "payload": {
                    "contextId": "scene-root", "quality": "final",
                    "outputHandle": "output-fixture",
                    "frames": [{"sourceFrame": 0,
                        "geometryHandle": {"resourceId": "geometry", "resourceVersion": "v1"}}]
                }
            }))
            .unwrap();
            let response = application.dispatch(request);
            assert!(response.is_ok());
            let job_id = response.result().unwrap()["jobId"].as_str().unwrap();
            let held = application
                .start_next_export_frame(job_id)
                .unwrap()
                .unwrap();
            (old, held)
        };
        let (_, fresh) = reserve(&native, &old_document);
        assert!(fresh > old);
        let mut authority = native.lock().unwrap();
        assert!(authority
            .active_mut(old)
            .and_then(|application| application.finish_export_frame(held))
            .is_err());
    }

    #[test]
    fn reserved_generation_blocks_a_and_clean_completion_activates_b_for_release() {
        let (_scratch, native, old_document, prepared) = setup();
        let (old, fresh) = reserve(&native, &old_document);
        {
            let mut authority = native.lock().unwrap();
            assert!(authority.active_mut(old).is_err());
            assert!(authority.active().is_none());
            assert!(authority
                .admit_release("release-a", "native-fixture", &old_document, 0, b"a", false)
                .is_err());
        }
        assert!(active_generation(&native).is_err());
        assert!(with_installed_instance(&native, old, "native-fixture", || Ok(())).is_err());
        let receipt = complete_replacement(&native, fresh, "native-fixture", prepared).unwrap();
        assert_ne!(receipt.document_id, old_document);
        assert_eq!(receipt.resource_count, 1);
        let authority = native.lock().unwrap();
        assert_eq!(authority.active_generation().unwrap(), fresh);
        assert_eq!(
            authority.active().unwrap().1.document_id(),
            receipt.document_id
        );
        drop(authority);
        let release = NativeReleaseRequest {
            api_version: NATIVE_API_VERSION,
            request_id: "release-b".into(),
            instance_id: "native-fixture".into(),
            document_id: receipt.document_id,
            expected_revision: receipt.content_revision,
            cancelled_before_dispatch: false,
        };
        let ReleaseAdmission::Execute { generation } =
            admit_release_request(&native, &release).unwrap()
        else {
            panic!("new B must admit release")
        };
        let released = complete_release(&native, generation, &release, || {
            Ok((Vec::new(), "already_absent"))
        })
        .unwrap();
        assert_eq!(released.status, "succeeded");
    }

    #[test]
    fn post_core_error_retains_b_behind_fence_with_exact_stage() {
        let (_scratch, native, old_document, prepared) = setup();
        let (old, fresh) = reserve(&native, &old_document);
        let error =
            complete_replacement_with(&native, fresh, "native-fixture", prepared, |stage| {
                if stage == "core" {
                    Err(host_error("injected", "after core"))
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        assert_eq!(error.code, "replacement_indeterminate");
        assert_eq!(
            error.details.as_ref().unwrap()["replacement"]["core"],
            "complete"
        );
        assert_eq!(
            error.details.as_ref().unwrap()["replacement"]["preview"],
            "pending"
        );
        let mut authority = native.lock().unwrap();
        assert!(authority.active_mut(old).is_err());
        assert!(authority.active_generation().is_err());
        let NativePhase::Replacing {
            application,
            progress,
            ..
        } = &authority.phase
        else {
            panic!("failed replacement must retain its application");
        };
        assert_ne!(application.document_id(), old_document);
        assert_eq!(progress.callback, ReplacementStage::Unknown);

        let (_scratch, native, old_document, prepared) = setup();
        let (_, fresh) = reserve(&native, &old_document);
        let error =
            complete_replacement_with(&native, fresh, "native-fixture", prepared, |stage| {
                if stage == "viewport" {
                    Err(host_error("injected", "viewport failed"))
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        let stages = &error.details.as_ref().unwrap()["replacement"];
        assert_eq!(stages["resources"], "complete");
        assert_eq!(stages["viewport"], "unknown");
        assert!(native.lock().unwrap().active().is_none());
    }

    #[test]
    fn post_preview_panic_retains_application_and_completed_stages() {
        let (_scratch, native, old_document, prepared) = setup();
        let (_, fresh) = reserve(&native, &old_document);
        let error =
            complete_replacement_with(&native, fresh, "native-fixture", prepared, |stage| {
                if stage == "preview" {
                    panic!("after preview")
                } else {
                    Ok(())
                }
            })
            .unwrap_err();
        let stages = &error.details.as_ref().unwrap()["replacement"];
        assert_eq!(stages["core"], "complete");
        assert_eq!(stages["preview"], "complete");
        assert_eq!(stages["resources"], "pending");
        assert_eq!(stages["failure"]["kind"], "callback_panicked");
        assert!(native.lock().unwrap().active().is_none());
    }

    #[test]
    fn scheduling_failure_and_callback_drop_keep_reserved_host_fenced() {
        for failure in [
            viewport_host::ReplacementCallbackFailure::SchedulingUnavailable,
            viewport_host::ReplacementCallbackFailure::CallbackDropped,
        ] {
            let (_scratch, native, old_document, _prepared) = setup();
            let (old, fresh) = reserve(&native, &old_document);
            let error = fence_executor_failure(&native, fresh, failure);
            assert_eq!(error.code, "replacement_indeterminate");
            assert_eq!(
                error.details.as_ref().unwrap()["replacement"]["core"],
                "pending"
            );
            let mut authority = native.lock().unwrap();
            assert!(authority.active_mut(old).is_err());
            assert!(matches!(authority.phase, NativePhase::Replacing { .. }));
        }
    }

    #[test]
    fn reservation_denies_mcp_dispatch_and_old_revision_ack() {
        let scratch = Scratch::new();
        let state = ApplicationMcp::default();
        let instance = state.instance_id().to_owned();
        let admitted = admit_project(&project(25), &geometry()).unwrap();
        let artifacts = DesktopArtifactPort::new(
            scratch.0.join("staging"),
            std::iter::empty::<(String, PathBuf)>(),
        )
        .unwrap();
        let compositor = SharedCompositor::new(Compositor::new().unwrap());
        let application =
            DesktopNativeApplication::new(instance.clone(), admitted, artifacts, compositor)
                .unwrap();
        let document = application.document_id().to_owned();
        let install = state.reserve_native_install().unwrap();
        state
            .install_dispatch(install.generation(), Box::new(application))
            .unwrap();

        let binding = state
            .control_revision_for_test(json!({"action":"binding"}))
            .unwrap();
        state
            .control_revision_for_test(json!({
                "action":"subscribe", "binding": binding
            }))
            .unwrap();
        let fresh = state
            .reserve_native_replacement(&instance, &document, 0, |application| {
                if application.as_any_mut().is::<DesktopNativeApplication>() {
                    Ok(())
                } else {
                    Err("unavailable:native desktop host is unavailable".into())
                }
            })
            .unwrap();
        assert!(fresh > install.generation());

        let response = state
            .dispatch_native(nemo_mcp::contract::NativeApplicationRequest {
                api_version: NATIVE_API_VERSION,
                request_id: "old-query".into(),
                instance_id: instance.clone(),
                document_id: document.clone(),
                expected_revision: None,
                operation: "query.document.revision".into(),
                payload: json!({}),
                cancelled_before_dispatch: false,
            })
            .unwrap();
        assert!(!response.ok);
        assert_eq!(response.error.unwrap().code, "unavailable");
        let stale_ack = state.control_revision_for_test(json!({
            "action":"acknowledge",
            "subscriptionId": binding["subscriptionId"],
            "event": {
                "instanceId": instance,
                "documentId": document,
                "lifecycleGeneration": install.generation(),
                "fromRevision": 0, "toRevision": 1, "requestId": "old-command"
            }
        }));
        assert!(stale_ack.unwrap_err().contains("unavailable"));
    }
}
