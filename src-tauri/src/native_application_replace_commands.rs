//! One committed host replacement callback and its fail-closed executor fallback.

use crate::native_application::PreparedDesktopReplacement;
use crate::native_application_commands::{
    host_error, reconcile_export, viewport_host, work_label, DesktopNativeApplication, HostResult,
    NativeReplacementReceipt, NativeState,
};
use crate::native_dispatch::{ReplacementProgress, ReplacementStage};
use std::panic::{catch_unwind, AssertUnwindSafe};

#[cfg(test)]
#[path = "native_application_replace_commands_acceptance_tests.rs"]
mod acceptance_tests;

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

pub(super) fn retained_executor_error(
    native: &NativeState,
    generation: u64,
) -> nemo_mcp::contract::NativeApplicationError {
    let authority = native
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    match &authority.phase {
        crate::native_dispatch::NativePhase::Replacing {
            generation: current,
            progress,
            ..
        } if *current == generation && progress.failure.is_some() => fenced_error(progress.clone()),
        _ => host_error("unavailable", "native replacement completion is stale"),
    }
}

#[cfg(test)]
mod tests {
    use super::acceptance_tests::{geometry_b, project, setup};
    use super::*;
    use crate::{
        native_application::{admit_release_request, complete_release},
        native_application_commands::{
            active_generation, with_installed_instance, NATIVE_API_VERSION,
        },
        native_application_contract::{admit_project, NativeReleaseRequest},
        native_dispatch::{NativePhase, ReleaseAdmission},
    };
    use serde_json::json;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };

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
        assert!(admit_project(&invalid, &geometry_b()).is_err());
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
                        "geometryHandle": {"resourceId": "geometry-a", "resourceVersion": "v1"}}]
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
    fn waiter_drop_then_callback_drop_fences_exactly_once() {
        let (_scratch, native, old_document, _prepared) = setup();
        let (old, fresh) = reserve(&native, &old_document);
        let count = Arc::new(AtomicUsize::new(0));
        let hook_native = native.clone();
        let hook_count = count.clone();
        let mut queued = None;
        let waiter = viewport_host::submit_replacement(
            |callback| {
                queued = Some(callback);
                Ok(())
            },
            || -> HostResult<()> { panic!("discarded callback must not run") },
            move |failure| {
                hook_count.fetch_add(1, Ordering::SeqCst);
                let _ = fence_executor_failure(&hook_native, fresh, failure);
            },
        )
        .unwrap();
        drop(waiter);
        drop(queued.take());
        assert_eq!(count.load(Ordering::SeqCst), 1);
        let error = retained_executor_error(&native, fresh);
        assert_eq!(error.code, "replacement_indeterminate");
        assert_eq!(
            error.details.as_ref().unwrap()["replacement"]["failure"]["kind"],
            "callback_dropped"
        );
        assert_eq!(
            error.details.as_ref().unwrap()["replacement"]["core"],
            "pending"
        );
        assert!(native.lock().unwrap().active_mut(old).is_err());
    }

    #[test]
    fn schedule_rejection_fences_with_distinct_terminal_reason() {
        let (_scratch, native, old_document, _prepared) = setup();
        let (_, fresh) = reserve(&native, &old_document);
        let hook_native = native.clone();
        let result = viewport_host::submit_replacement(
            |_| Err(()),
            || -> HostResult<()> { panic!("unscheduled callback must not run") },
            move |failure| {
                let _ = fence_executor_failure(&hook_native, fresh, failure);
            },
        );
        assert!(matches!(
            result,
            Err(viewport_host::ReplacementCallbackFailure::SchedulingUnavailable)
        ));
        let error = retained_executor_error(&native, fresh);
        assert_eq!(
            error.details.as_ref().unwrap()["replacement"]["failure"]["kind"],
            "executor_unavailable"
        );
    }
}
