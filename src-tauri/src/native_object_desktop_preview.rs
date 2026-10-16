//! Private strict-byte desktop adapter; common SDK/MCP availability is unchanged.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{
    bounded_id, host_error, require_api_instance, require_main, work_label, HostResult,
    NativePreviewReceipt, OutputSpecInput, ViewportInput,
};
use crate::native_application_viewport::{self as viewport_host, NativeViewportConsumer};
use crate::native_dispatch::NativeObjectHost;
use native_engine::{
    compositor::Compositor,
    desktop_viewport::{DesktopViewportHost, PresentationReceipt, SurfacePort},
    object_document::FrameScopeKind,
    object_frame_packet::ObjectFrameSelector,
    scheduler::OutputSpec,
};
use serde::{Deserialize, Serialize};
use tauri::Manager;

const MAX_BYTES: usize = 8192;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
/// Exact read-side preview admission for the current external revision. This
/// is not an acknowledgment and never authorizes a document command.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PreviewRevisionProof {
    pub(crate) subscription_id: String,
    pub(crate) event: PreviewRevisionEvent,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PreviewRevisionEvent {
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) lifecycle_generation: u64,
    pub(crate) request_id: String,
    pub(crate) from_revision: u64,
    pub(crate) to_revision: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ObjectPreviewRequest {
    pub(crate) api_version: u32,
    pub(crate) request_id: String,
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) content_revision: u64,
    pub(crate) document_snapshot_id: String,
    pub(crate) lifecycle_generation: u64,
    pub(crate) context_id: String,
    pub(crate) scope_kind: FrameScopeKind,
    pub(crate) frame: u32,
    pub(crate) output_spec: OutputSpecInput,
    pub(crate) viewport: ViewportInput,
    pub(crate) revision_sync: Option<PreviewRevisionProof>,
    #[serde(default)]
    pub(crate) cancelled_before_dispatch: bool,
}
impl ObjectPreviewRequest {
    fn selector(&self) -> ObjectFrameSelector<'_> {
        ObjectFrameSelector {
            instance_id: &self.instance_id,
            document_id: &self.document_id,
            content_revision: self.content_revision,
            document_snapshot_id: &self.document_snapshot_id,
            context_id: &self.context_id,
            scope_kind: self.scope_kind.clone(),
            frame: self.frame,
        }
    }
}
pub(crate) fn admit_request(raw: &str, instance: &str) -> HostResult<ObjectPreviewRequest> {
    if raw.len() > MAX_BYTES || !raw.trim_start().starts_with('{') {
        return Err(host_error(
            "invalid_request",
            "object preview requires bounded original JSON bytes",
        ));
    }
    let shape: serde_json::Value = serde_json::from_str(raw)
        .map_err(|_| host_error("invalid_request", "invalid raw object preview request"))?;
    if let Some(proof) = shape.get("revisionSync") {
        if !proof.is_object() || !proof.get("event").is_some_and(serde_json::Value::is_object) {
            return Err(host_error(
                "invalid_request",
                "revision preview proof must be objects",
            ));
        }
    }
    let request: ObjectPreviewRequest = serde_json::from_str(raw)
        .map_err(|_| host_error("invalid_request", "invalid raw object preview request"))?;
    require_api_instance(request.api_version, &request.instance_id, instance)?;
    if [
        &request.request_id,
        &request.instance_id,
        &request.document_id,
        &request.document_snapshot_id,
        &request.context_id,
    ]
    .iter()
    .any(|id| !bounded_id(id))
        || request.content_revision > MAX_SAFE_INTEGER
        || request.lifecycle_generation == 0
        || request.lifecycle_generation > MAX_SAFE_INTEGER
    {
        return Err(host_error("invalid_request", "invalid object preview pins"));
    }
    if request.cancelled_before_dispatch {
        return Err(host_error(
            "cancelled_before_dispatch",
            "object preview was cancelled",
        ));
    }
    if let Some(proof) = &request.revision_sync {
        let event = &proof.event;
        if !bounded_id(&proof.subscription_id)
            || !bounded_id(&event.instance_id)
            || !bounded_id(&event.document_id)
            || !bounded_id(&event.request_id)
            || event.lifecycle_generation == 0
            || event.lifecycle_generation > MAX_SAFE_INTEGER
            || event.from_revision > MAX_SAFE_INTEGER
            || event.to_revision > MAX_SAFE_INTEGER
            || event.to_revision <= event.from_revision
        {
            return Err(host_error(
                "invalid_request",
                "invalid revision preview proof",
            ));
        }
    }
    let output = request.output_spec.admit_object()?;
    request.viewport.admit_object(&output)?;
    Ok(request)
}

pub(crate) struct ObjectPreviewConsumer<'a> {
    owner: &'a mut NativeObjectHost,
    selector: ObjectFrameSelector<'a>,
}
impl ObjectPreviewConsumer<'_> {
    pub(crate) fn present<P: SurfacePort>(
        self,
        compositor: &mut Compositor,
        viewport: &mut DesktopViewportHost<P>,
    ) -> Result<PresentationReceipt, String> {
        self.owner
            .present_prepared_object_preview(&self.selector, compositor, viewport)
    }
}
impl NativeViewportConsumer for ObjectPreviewConsumer<'_> {
    type Output = PresentationReceipt;
    fn consume<P: SurfacePort>(
        self,
        compositor: &mut Compositor,
        viewport: &mut DesktopViewportHost<P>,
    ) -> Result<Self::Output, String> {
        self.present(compositor, viewport)
    }
}

/// The target closure cannot run until full owner/CPU/barrier admission succeeds.
pub(crate) fn preview_with_target(
    state: &ApplicationMcp,
    request: &ObjectPreviewRequest,
    target: impl FnOnce(ObjectPreviewConsumer<'_>) -> HostResult<PresentationReceipt>,
) -> HostResult<NativePreviewReceipt> {
    let selector = request.selector();
    let output: OutputSpec = request.output_spec.admit_object()?;
    request.viewport.admit_object(&output)?;
    let revision_event = request
        .revision_sync
        .as_ref()
        .map(|proof| serde_json::to_value(&proof.event))
        .transpose()
        .map_err(|_| host_error("invalid_request", "invalid revision preview proof"))?;
    let revision_sync = request
        .revision_sync
        .as_ref()
        .zip(revision_event.as_ref())
        .map(|(proof, event)| (proof.subscription_id.as_str(), event));
    let receipt = state
        .with_object_preview_owner_for_preview(
            request.lifecycle_generation,
            &selector,
            revision_sync,
            |owner| {
                owner.require_project_output(output.dimensions())?;
                owner.with_prepared_object_preview(&selector, output, |owner| {
                    Ok(target(ObjectPreviewConsumer {
                        owner,
                        selector: selector.clone(),
                    }))
                })
            },
        )
        .map_err(|message| host_error("unavailable", message))??;
    Ok(NativePreviewReceipt {
        work_id: work_label(receipt.identity().work_id()),
        view_generation: receipt.identity().view_generation().value(),
        status: viewport_host::status_label(receipt.status()),
    })
}

#[tauri::command]
pub(crate) async fn nemo_native_object_preview(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, ApplicationMcp>,
    request_json: String,
) -> HostResult<NativePreviewReceipt> {
    require_main(&window)?;
    let request = admit_request(&request_json, state.instance_id())?;
    let app_for_main = app.clone();
    viewport_host::on_main_thread(&app, move || {
        let state = app_for_main.state::<ApplicationMcp>();
        preview_with_target(&state, &request, |consumer| {
            viewport_host::with_object_target(
                &app_for_main,
                &request.instance_id,
                request.lifecycle_generation,
                request
                    .viewport
                    .admit_object(&request.output_spec.admit_object()?)?,
                consumer,
            )
        })
    })
    .await
}

#[cfg(test)]
#[path = "native_object_desktop_preview_tests.rs"]
mod tests;
