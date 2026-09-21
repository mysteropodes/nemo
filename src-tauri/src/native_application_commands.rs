//! Dormant Tauri command adapter for the concrete native application host.
//!
//! These commands are registered for N20 cutover but are not invoked by the
//! current startup path. All mutable native work continues through the single
//! `ApplicationMcp`-owned application instance.

use crate::{
    application_mcp::ApplicationMcp,
    native_application::DesktopNativeApplication,
    native_application_contract::*,
    native_application_ports::{DesktopArtifactPort, SharedCompositor},
    native_application_viewport as viewport_host,
    native_dispatch::{NativeDispatch, NativeState},
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
    bootstrap_reserved(app, &state, request).await
}

async fn bootstrap_reserved(
    app: tauri::AppHandle,
    state: &ApplicationMcp,
    request: NativeBootstrapRequest,
) -> HostResult<NativeBootstrapReceipt> {
    require_api_instance(
        request.api_version,
        &request.instance_id,
        state.instance_id(),
    )?;
    let admitted = admit_project(&request.projection, &request.resources)?;
    let resource_count = admitted.resources.len();
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
    let viewport_available = mapping.is_some();
    let compositor = if let Some(mapping) = mapping {
        let app_for_main = app.clone();
        let instance = request.instance_id.clone();
        viewport_host::on_main_thread(&app, move || {
            viewport_host::create(&app_for_main, instance, mapping)
        })
        .await?
    } else {
        Compositor::new().map_err(|error| host_error("unavailable", error.to_string()))?
    };
    let application = match DesktopNativeApplication::new(
        request.instance_id.clone(),
        admitted,
        artifacts,
        SharedCompositor::new(compositor),
    ) {
        Ok(application) => application,
        Err(error) => {
            if viewport_available {
                let instance = request.instance_id.clone();
                let _ =
                    viewport_host::on_main_thread(&app, move || viewport_host::remove(&instance))
                        .await;
            }
            return Err(error);
        }
    };
    let document_id = application.document_id().to_owned();
    let content_revision = application.content_revision();
    if let Err(message) = state.install_dispatch(Box::new(application)) {
        if viewport_available {
            let instance = request.instance_id.clone();
            let _ =
                viewport_host::on_main_thread(&app, move || viewport_host::remove(&instance)).await;
        }
        return Err(host_error("duplicate_bootstrap", message));
    }
    Ok(NativeBootstrapReceipt {
        api_version: NATIVE_API_VERSION,
        instance_id: request.instance_id,
        document_id,
        content_revision,
        resource_count,
        viewport_available,
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
    let instance = request.instance_id.clone();
    viewport_host::on_main_thread(&app, move || {
        viewport_host::require_instance_or_absent(&instance)?;
        let mut guard = native
            .lock()
            .map_err(|_| host_error("unavailable", "native application lock unavailable"))?;
        let application = desktop_mut(&mut guard)?;
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
    let mut guard = native
        .lock()
        .map_err(|_| host_error("unavailable", "native application lock unavailable"))?;
    let application = desktop_mut(&mut guard)?;
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
    let instance = request.instance_id.clone();
    viewport_host::on_main_thread(&app, move || {
        let mut guard = native
            .lock()
            .map_err(|_| host_error("unavailable", "native application lock unavailable"))?;
        let application = desktop_mut(&mut guard)?;
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
    let instance = request.instance_id;
    viewport_host::on_main_thread(&app, move || {
        require_installed_instance(&native, &instance)?;
        viewport_host::resize(&instance, mapping)?;
        Ok(NativeViewportReceipt {
            status: "resized",
            cancelled_work_ids: Vec::new(),
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
    let instance = request.instance_id;
    viewport_host::on_main_thread(&app, move || {
        let mut guard = native
            .lock()
            .map_err(|_| host_error("unavailable", "native application lock unavailable"))?;
        let application = desktop_mut(&mut guard)?;
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

fn desktop_mut(
    guard: &mut Option<Box<dyn NativeDispatch>>,
) -> HostResult<&mut DesktopNativeApplication> {
    guard
        .as_mut()
        .ok_or_else(|| host_error("unavailable", "native application is unavailable"))?
        .as_any_mut()
        .downcast_mut::<DesktopNativeApplication>()
        .ok_or_else(|| host_error("unavailable", "native desktop host is unavailable"))
}

fn require_installed_instance(native: &NativeState, instance: &str) -> HostResult<()> {
    let guard = native
        .lock()
        .map_err(|_| host_error("unavailable", "native application lock unavailable"))?;
    let application = guard
        .as_ref()
        .ok_or_else(|| host_error("unavailable", "native application is unavailable"))?;
    if application.instance_id() != instance {
        return Err(host_error(
            "wrong_instance",
            "native application instance mismatch",
        ));
    }
    Ok(())
}
