//! Dormant Tauri command adapter for the concrete native application host.
//!
//! These commands are registered for N20 cutover but are not invoked by the
//! current startup path. All mutable native work continues through the single
//! `ApplicationMcp`-owned application instance.

use crate::{
    application_mcp::ApplicationMcp,
    native_application::{self, DesktopNativeApplication},
    native_application_contract::*,
    native_application_ports::{DesktopArtifactPort, SharedCompositor},
    native_application_viewport as viewport_host,
    native_dispatch::{NativeAuthority, NativeState, ReleaseAdmission},
};
use native_engine::compositor::Compositor;
use nemo_mcp::contract::NATIVE_API_VERSION;
use std::path::PathBuf;
use tauri::Manager;

#[tauri::command]
pub(crate) async fn nemo_native_bootstrap(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, ApplicationMcp>,
    request: NativeBootstrapRequest,
) -> HostResult<NativeBootstrapReceipt> {
    require_main(&window)?;
    let _reservation = state
        .reserve_native_install()
        .map_err(|message| host_error("duplicate_bootstrap", message))?;
    let generation = _reservation.generation();
    bootstrap_reserved(app, &state, generation, request).await
}

async fn bootstrap_reserved(
    app: tauri::AppHandle,
    state: &ApplicationMcp,
    generation: u64,
    request: NativeBootstrapRequest,
) -> HostResult<NativeBootstrapReceipt> {
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    let admitted = admit_project(&request.projection, &request.resources)?;
    let root = app
        .path()
        .app_cache_dir()
        .map_err(|_| host_error("unavailable", "native staging root unavailable"))?
        .join("native-export-staging");
    let artifacts = DesktopArtifactPort::new(
        root,
        request
            .output_bindings
            .into_iter()
            .map(|binding| (binding.output_handle, PathBuf::from(binding.destination))),
    )
    .map_err(|message| host_error("invalid_request", message))?;
    let mapping = request.viewport.map(ViewportInput::admit).transpose()?;
    if let Some(mapping) = mapping {
        let app_for_main = app.clone();
        let instance = request.instance_id.clone();
        viewport_host::on_main_thread(&app, move || {
            let state = app_for_main.state::<ApplicationMcp>();
            let native = state.native_state();
            with_install_reservation(&native, generation, |authority| {
                let compositor = viewport_host::create(&app_for_main, instance.clone(), mapping)?;
                finish_bootstrap(
                    authority, generation, instance, admitted, artifacts, compositor, true,
                )
            })
        })
        .await
    } else {
        let compositor =
            Compositor::new().map_err(|error| host_error("unavailable", error.to_string()))?;
        let native = state.native_state();
        with_install_reservation(&native, generation, |authority| {
            finish_bootstrap(
                authority,
                generation,
                request.instance_id,
                admitted,
                artifacts,
                compositor,
                false,
            )
        })
    }
}

fn finish_bootstrap(
    authority: &mut NativeAuthority,
    generation: u64,
    instance_id: String,
    admitted: AdmittedProject,
    artifacts: DesktopArtifactPort,
    compositor: Compositor,
    viewport_retained: bool,
) -> HostResult<NativeBootstrapReceipt> {
    let resource_count = admitted.resources.len();
    let application = match DesktopNativeApplication::new(
        instance_id.clone(),
        admitted,
        artifacts,
        SharedCompositor::new(compositor),
    ) {
        Ok(application) => application,
        Err(error) => {
            if viewport_retained {
                let _ = viewport_host::remove(&instance_id);
            }
            return Err(error);
        }
    };
    let document_id = application.document_id().to_owned();
    let content_revision = application.content_revision();
    if let Err(message) = authority.install(generation, Box::new(application)) {
        if viewport_retained {
            let _ = viewport_host::remove(&instance_id);
        }
        return Err(host_error("duplicate_bootstrap", message));
    }
    Ok(NativeBootstrapReceipt {
        api_version: NATIVE_API_VERSION,
        instance_id,
        document_id,
        content_revision,
        resource_count,
        viewport_available: viewport_retained,
    })
}

#[tauri::command]
pub(crate) async fn nemo_native_replace(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, ApplicationMcp>,
    request: NativeReplacementRequest,
) -> HostResult<NativeReplacementReceipt> {
    require_main(&window)?;
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    let admitted = admit_project(&request.projection, &request.resources)?;
    let resource_count = admitted.resources.len();
    let native = state.native_state();
    let generation = active_generation(&native)?;
    let instance = request.instance_id.clone();
    viewport_host::on_main_thread(&app, move || {
        viewport_host::require_instance_or_absent(&instance)?;
        let mut guard = native
            .lock()
            .map_err(|_| host_error("unavailable", "native application lock unavailable"))?;
        let application = desktop_mut(&mut guard, generation)?;
        application.require_identity(
            &request.instance_id,
            &request.document_id,
            request.expected_revision,
        )?;
        let (exports, preview) = application.replace_project(admitted)?;
        viewport_host::reconcile_replaced(&instance, &preview)?;
        Ok(NativeReplacementReceipt {
            document_id: application.document_id().to_owned(),
            content_revision: application.content_revision(),
            resource_count,
            cancelled_preview_work_ids: preview.into_iter().map(work_label).collect(),
            reconciled_exports: exports.iter().map(reconcile_export).collect(),
        })
    })
    .await
}

#[tauri::command]
pub(crate) fn nemo_native_bind_output(
    window: tauri::Window,
    state: tauri::State<ApplicationMcp>,
    request: NativeOutputBindingRequest,
) -> HostResult<NativeOutputBindingReceipt> {
    require_main(&window)?;
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    if !bounded_id(&request.output_handle) {
        return Err(host_error("invalid_request", "invalid output handle"));
    }
    let native = state.native_state();
    let generation = active_generation(&native)?;
    let mut guard = native
        .lock()
        .map_err(|_| host_error("unavailable", "native application lock unavailable"))?;
    let application = desktop_mut(&mut guard, generation)?;
    application.require_identity(
        &request.instance_id,
        &request.document_id,
        request.expected_revision,
    )?;
    application.bind_output(
        request.output_handle.clone(),
        PathBuf::from(request.destination),
    )?;
    Ok(NativeOutputBindingReceipt {
        output_handle: request.output_handle,
    })
}

#[tauri::command]
pub(crate) async fn nemo_native_preview(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, ApplicationMcp>,
    request: NativePreviewRequest,
) -> HostResult<NativePreviewReceipt> {
    require_main(&window)?;
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    let native = state.native_state();
    let generation = active_generation(&native)?;
    let instance = request.instance_id.clone();
    viewport_host::on_main_thread(&app, move || {
        let mut guard = native
            .lock()
            .map_err(|_| host_error("unavailable", "native application lock unavailable"))?;
        let application = desktop_mut(&mut guard, generation)?;
        let prepared = application.prepare_preview(&request)?;
        let work_id = prepared.identity.work_id();
        let shared = application.preview_compositor().inner();
        let compositor = shared
            .lock()
            .map_err(|_| host_error("internal", "compositor_lock_poisoned"))?;
        let presented =
            viewport_host::present(&instance, &compositor, &prepared.result, prepared.identity);
        let presented = match presented {
            Ok(receipt) => receipt,
            Err(error) => {
                let _ = application.finish_preview(work_id, "failed-validation");
                return Err(error);
            }
        };
        application.finish_preview(presented.work_id, presented.status)?;
        Ok(NativePreviewReceipt {
            work_id: work_label(presented.work_id),
            view_generation: presented.view_generation,
            status: presented.status,
        })
    })
    .await
}

#[tauri::command]
pub(crate) async fn nemo_native_viewport_resize(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, ApplicationMcp>,
    request: NativeViewportRequest,
) -> HostResult<NativeViewportReceipt> {
    require_main(&window)?;
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    let mapping = request.viewport.admit()?;
    let native = state.native_state();
    let generation = active_generation(&native)?;
    let instance = request.instance_id;
    viewport_host::on_main_thread(&app, move || {
        with_installed_instance(&native, generation, &instance, || {
            viewport_host::resize(&instance, mapping)?;
            Ok(NativeViewportReceipt {
                status: "resized",
                cancelled_work_ids: Vec::new(),
            })
        })
    })
    .await
}

#[tauri::command]
pub(crate) async fn nemo_native_viewport_dispose(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, ApplicationMcp>,
    request: NativeDisposeRequest,
) -> HostResult<NativeViewportReceipt> {
    require_main(&window)?;
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    let native = state.native_state();
    let generation = active_generation(&native)?;
    let instance = request.instance_id;
    viewport_host::on_main_thread(&app, move || {
        let mut guard = native
            .lock()
            .map_err(|_| host_error("unavailable", "native application lock unavailable"))?;
        let application = desktop_mut(&mut guard, generation)?;
        if application.instance_id() != instance {
            return Err(host_error(
                "wrong_instance",
                "native application instance mismatch",
            ));
        }
        let pending = viewport_host::dispose(&instance)?;
        application.cancel_preview(&pending)?;
        Ok(NativeViewportReceipt {
            status: "disposed",
            cancelled_work_ids: pending.into_iter().map(work_label).collect(),
        })
    })
    .await
}

#[tauri::command]
pub(crate) async fn nemo_native_release(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, ApplicationMcp>,
    request: NativeReleaseRequest,
) -> HostResult<NativeReleaseReceipt> {
    require_main(&window)?;
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    let native = state.native_state();
    let admission = native_application::admit_release_request(&native, &request)?;
    let generation = match admission {
        ReleaseAdmission::Retry {
            generation,
            receipt,
        } => {
            let receipt: NativeReleaseReceipt = serde_json::from_value(receipt)
                .map_err(|_| host_error("internal", "retained release receipt is invalid"))?;
            if receipt.lifecycle_generation != generation {
                return Err(host_error(
                    "internal",
                    "retained release generation is invalid",
                ));
            }
            return Ok(receipt);
        }
        ReleaseAdmission::Execute { generation } => generation,
    };
    state.invalidate_native_subscriber(&app);
    let scheduled_native = native.clone();
    let fallback_native = native;
    let scheduled_request = request.clone();
    let fallback_request = request;
    let scheduled = viewport_host::on_main_thread_committed(&app, move || {
        let instance_id = scheduled_request.instance_id.clone();
        native_application::complete_release(
            &scheduled_native,
            generation,
            &scheduled_request,
            move || {
                viewport_host::release(&instance_id)
                    .map(|released| (released.work_ids, released.status))
            },
        )
    })
    .await;
    match scheduled {
        Ok(receipt) => Ok(receipt),
        Err(error) => native_application::complete_release(
            &fallback_native,
            generation,
            &fallback_request,
            || {
                Err(host_error(
                    "cleanup_failed",
                    format!(
                        "native main-thread release completion was indeterminate: {}",
                        error.message
                    ),
                ))
            },
        )
        .or_else(|_| {
            match native_application::admit_release_request(&fallback_native, &fallback_request)? {
                ReleaseAdmission::Retry {
                    generation: retained_generation,
                    receipt,
                } if retained_generation == generation => serde_json::from_value(receipt)
                    .map_err(|_| host_error("internal", "retained release receipt is invalid")),
                _ => Err(host_error(
                    "unavailable",
                    "native release completion could not be reconciled",
                )),
            }
        }),
    }
}

fn desktop_mut(
    guard: &mut NativeAuthority,
    generation: u64,
) -> HostResult<&mut DesktopNativeApplication> {
    guard
        .active_mut(generation)
        .map_err(|message| host_error("unavailable", message))?
        .as_any_mut()
        .downcast_mut::<DesktopNativeApplication>()
        .ok_or_else(|| host_error("unavailable", "native desktop host is unavailable"))
}

fn active_generation(native: &NativeState) -> HostResult<u64> {
    native
        .lock()
        .map_err(|_| host_error("unavailable", "native application lock unavailable"))?
        .active_generation()
        .map_err(|message| host_error("unavailable", message))
}

fn with_install_reservation<T>(
    native: &NativeState,
    generation: u64,
    operation: impl FnOnce(&mut NativeAuthority) -> HostResult<T>,
) -> HostResult<T> {
    let mut authority = native
        .lock()
        .map_err(|_| host_error("unavailable", "native application lock unavailable"))?;
    authority
        .require_installing_generation(generation)
        .map_err(|message| host_error("duplicate_bootstrap", message))?;
    operation(&mut authority)
}

fn with_installed_instance<T>(
    native: &NativeState,
    generation: u64,
    instance: &str,
    operation: impl FnOnce() -> HostResult<T>,
) -> HostResult<T> {
    let guard = native
        .lock()
        .map_err(|_| host_error("unavailable", "native application lock unavailable"))?;
    let (_, application) = guard
        .active()
        .filter(|(active, _)| *active == generation)
        .ok_or_else(|| host_error("unavailable", "native lifecycle generation is stale"))?;
    if application.instance_id() != instance {
        return Err(host_error(
            "wrong_instance",
            "native application instance mismatch",
        ));
    }
    operation()
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::native_dispatch::tests::TerminalPump;

    #[test]
    fn stale_install_cannot_begin_viewport_work_and_resize_holds_authority() {
        let native = std::sync::Arc::new(std::sync::Mutex::new(NativeAuthority::default()));
        let (stale, current) = {
            let mut authority = native.lock().unwrap();
            let stale = authority.reserve_install().unwrap();
            authority.rollback_install(stale);
            (stale, authority.reserve_install().unwrap())
        };
        let mut touched = false;
        assert!(with_install_reservation(&native, stale, |_| {
            touched = true;
            Ok(())
        })
        .is_err());
        assert!(!touched);
        with_install_reservation(&native, current, |authority| {
            touched = true;
            authority
                .install(
                    current,
                    Box::new(TerminalPump(std::sync::Arc::new(
                        std::sync::atomic::AtomicUsize::new(0),
                    ))),
                )
                .map_err(|error| host_error("internal", error))
        })
        .unwrap();
        assert!(touched);
        with_installed_instance(&native, current, "pump-fixture", || {
            assert!(matches!(
                native.try_lock(),
                Err(std::sync::TryLockError::WouldBlock)
            ));
            Ok(())
        })
        .unwrap();
    }
}
