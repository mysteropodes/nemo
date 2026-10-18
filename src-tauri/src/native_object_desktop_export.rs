//! Private main-window object PNG/SVG route; no SDK/MCP capability activation.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{
    bounded_id, host_error, reconcile_export, require_api_instance, require_main, HostResult,
    OutputSpecInput, ViewportInput, HOST_API_VERSION,
};
use crate::native_application_viewport::{self as viewport_host, NativeViewportConsumer};
use crate::native_dispatch::NativeObjectHost;
use native_engine::{
    compositor::Compositor,
    desktop_viewport::{DesktopViewportHost, SurfacePort, ViewportMapping},
    export_job::{
        ExternalEffectDisposition, JobStatus, ObjectHostExportOutcome, MAX_OBJECT_EXPORT_FRAMES,
    },
    object_document::FrameScopeKind,
    object_frame_packet::ObjectFrameSelector,
    scheduler::OutputSpec,
};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Component, Path, PathBuf},
};
use tauri::Manager;
use tauri_plugin_fs::FsExt;

#[path = "native_object_desktop_export_port.rs"]
mod port;
use port::LazyDesktopArtifactPort;

const MAX_REQUEST_BYTES: usize = 8192;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;
const MAX_PNG_BYTES: usize = 4 * 1024 * 1024;
const OUTPUT_HANDLE: &str = "object-output";

#[derive(Clone, Copy, Default, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum ObjectExportFormat {
    #[default]
    Png,
    Svg,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ObjectExportRequest {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    content_revision: u64,
    document_snapshot_id: String,
    lifecycle_generation: u64,
    context_id: String,
    scope_kind: FrameScopeKind,
    frame: u32,
    #[serde(default, deserialize_with = "non_null_frames")]
    frames: Option<Vec<u32>>,
    output_spec: OutputSpecInput,
    viewport: ViewportInput,
    destination_directory: String,
    maximum_encoded_bytes: usize,
    #[serde(default)]
    format: ObjectExportFormat,
    #[serde(default)]
    cancelled_before_dispatch: bool,
}
fn non_null_frames<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<Vec<u32>>, D::Error> {
    Vec::<u32>::deserialize(d).map(Some)
}
impl ObjectExportRequest {
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

struct AdmittedExport {
    request: ObjectExportRequest,
    destination: PathBuf,
    output: OutputSpec,
    mapping: ViewportMapping,
}

/// The destination is a new directory containing the exact ordinal frame manifest.
fn admit_request(
    raw: &str,
    instance: &str,
    allowed: impl Fn(&Path) -> bool,
) -> HostResult<AdmittedExport> {
    let invalid = || host_error("invalid_request", "invalid original object export request");
    if raw.len() > MAX_REQUEST_BYTES || !raw.trim_start().starts_with('{') {
        return Err(invalid());
    }
    let request: ObjectExportRequest = serde_json::from_str(raw).map_err(|_| invalid())?;
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
        || request.scope_kind != FrameScopeKind::Authored
        || request.maximum_encoded_bytes == 0
        || request.maximum_encoded_bytes > MAX_PNG_BYTES
        || request.frames.as_ref().is_some_and(|frames| {
            frames.is_empty()
                || frames.len() > MAX_OBJECT_EXPORT_FRAMES
                || frames.first() != Some(&request.frame)
                || frames.windows(2).any(|pair| pair[0] >= pair[1])
        })
    {
        return Err(invalid());
    }
    if request.cancelled_before_dispatch {
        return Err(host_error(
            "cancelled_before_dispatch",
            "object export was cancelled",
        ));
    }
    let output = request.output_spec.admit_object()?;
    let mapping = request.viewport.admit_object(&output)?;
    let raw_destination = Path::new(&request.destination_directory);
    if !raw_destination.is_absolute()
        || !matches!(
            raw_destination.components().next_back(),
            Some(Component::Normal(_))
        )
        || !allowed(raw_destination)
    {
        return Err(invalid());
    }
    let parent = raw_destination
        .parent()
        .ok_or_else(invalid)?
        .canonicalize()
        .map_err(|_| host_error("unavailable", "object export parent unavailable"))?;
    if !parent.is_dir() {
        return Err(invalid());
    }
    let destination = parent.join(raw_destination.file_name().ok_or_else(invalid)?);
    if !allowed(&destination) {
        return Err(host_error(
            "unavailable",
            "object export destination outside existing filesystem scope",
        ));
    }
    match fs::symlink_metadata(&destination) {
        Ok(_) => {
            return Err(host_error(
                "unavailable",
                "object export destination exists",
            ))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => {
            return Err(host_error(
                "unavailable",
                "object export destination unavailable",
            ))
        }
    }
    Ok(AdmittedExport {
        request,
        destination,
        output,
        mapping,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObjectExportReceipt {
    api_version: u32,
    request_id: String,
    instance_id: String,
    document_id: String,
    content_revision: u64,
    document_snapshot_id: String,
    status: String,
    cleanup_status: String,
    external_effect_disposition: String,
    error_code: Option<String>,
    artifact_files: Vec<String>,
    published: bool,
    cleanup_complete: bool,
}

fn receipt(
    request: &ObjectExportRequest,
    outcome: ObjectHostExportOutcome,
) -> HostResult<ObjectExportReceipt> {
    let job = outcome.receipt.map_err(|error| {
        let mut refused = host_error(
            "unavailable",
            format!("object export refused: {}", error.message),
        );
        refused.details =
            Some(serde_json::json!({"cleanupComplete":outcome.release.cleanup_complete}));
        refused
    })?;
    let disposition = reconcile_export(&job);
    Ok(ObjectExportReceipt {
        api_version: HOST_API_VERSION,
        request_id: request.request_id.clone(),
        instance_id: request.instance_id.clone(),
        document_id: request.document_id.clone(),
        content_revision: job.pinned_revision,
        document_snapshot_id: job.document_snapshot_id,
        status: disposition.status,
        cleanup_status: disposition.cleanup_status,
        external_effect_disposition: disposition.external_effect_disposition,
        error_code: disposition.error_code,
        artifact_files: job
            .artifact
            .as_ref()
            .map_or_else(Vec::new, |artifact| artifact.files.clone()),
        published: job.status == JobStatus::Succeeded
            && job.external_effect_disposition == ExternalEffectDisposition::Committed,
        cleanup_complete: outcome.release.cleanup_complete,
    })
}

struct ObjectExportConsumer<'a> {
    owner: &'a mut NativeObjectHost,
    selector: ObjectFrameSelector<'a>,
    frames: Vec<u32>,
    request_id: String,
    output: OutputSpec,
    maximum_encoded_bytes: usize,
    format: ObjectExportFormat,
    destination: PathBuf,
    staging_root: PathBuf,
}
impl ObjectExportConsumer<'_> {
    fn run(self, compositor: &mut Compositor) -> Result<ObjectHostExportOutcome, String> {
        let mut port = if self.format == ObjectExportFormat::Svg {
            LazyDesktopArtifactPort::new_svg(
                self.staging_root,
                OUTPUT_HANDLE.into(),
                self.destination,
            )
        } else {
            LazyDesktopArtifactPort::new(self.staging_root, OUTPUT_HANDLE.into(), self.destination)
        };
        if self.format == ObjectExportFormat::Svg {
            return self.owner.export_pinned_object_svg_frames(
                &self.selector,
                &self.frames,
                self.request_id,
                OUTPUT_HANDLE.into(),
                self.output,
                self.maximum_encoded_bytes,
                &mut port,
            );
        }
        self.owner.export_pinned_object_png_frames(
            &self.selector,
            &self.frames,
            self.request_id,
            OUTPUT_HANDLE.into(),
            self.output,
            self.maximum_encoded_bytes,
            &mut port,
            compositor,
        )
    }
}
impl NativeViewportConsumer for ObjectExportConsumer<'_> {
    type Output = ObjectHostExportOutcome;
    fn consume<P: SurfacePort>(
        self,
        compositor: &mut Compositor,
        _: &mut DesktopViewportHost<P>,
    ) -> Result<Self::Output, String> {
        self.run(compositor)
    }
}

/// Authority and revision remain held through the original viewport/GPU borrow.
fn export_with_target(
    state: &ApplicationMcp,
    admitted: &AdmittedExport,
    staging_root: PathBuf,
    target: impl FnOnce(ObjectExportConsumer<'_>) -> HostResult<ObjectHostExportOutcome>,
) -> HostResult<ObjectExportReceipt> {
    let selector = admitted.request.selector();
    let outcome = state
        .with_object_preview_owner(admitted.request.lifecycle_generation, &selector, |owner| {
            owner.require_project_output(admitted.output.dimensions())?;
            Ok(target(ObjectExportConsumer {
                owner,
                selector: selector.clone(),
                frames: admitted
                    .request
                    .frames
                    .clone()
                    .unwrap_or_else(|| vec![selector.frame]),
                request_id: admitted.request.request_id.clone(),
                output: admitted.output.clone(),
                maximum_encoded_bytes: admitted.request.maximum_encoded_bytes,
                format: admitted.request.format,
                destination: admitted.destination.clone(),
                staging_root,
            }))
        })
        .map_err(|message| host_error("unavailable", message))??;
    receipt(&admitted.request, outcome)
}

#[tauri::command]
pub(crate) async fn nemo_native_object_export(
    app: tauri::AppHandle,
    window: tauri::Window,
    state: tauri::State<'_, ApplicationMcp>,
    request_json: String,
) -> HostResult<ObjectExportReceipt> {
    require_main(&window)?;
    let admitted = admit_request(&request_json, state.instance_id(), |path| {
        window
            .try_fs_scope()
            .is_some_and(|scope| scope.is_allowed(path))
    })?;
    let staging_root = app
        .path()
        .app_cache_dir()
        .map_err(|_| host_error("unavailable", "native staging root unavailable"))?
        .join("native-export-staging");
    let app_for_main = app.clone();
    viewport_host::on_main_thread(&app, move || {
        let state = app_for_main.state::<ApplicationMcp>();
        export_with_target(&state, &admitted, staging_root, |consumer| {
            viewport_host::with_object_target(
                &app_for_main,
                &admitted.request.instance_id,
                admitted.request.lifecycle_generation,
                admitted.mapping,
                consumer,
            )
        })
    })
    .await
}

#[cfg(test)]
#[path = "native_object_desktop_export_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "native_object_svg_desktop_export_tests.rs"]
mod svg_tests;
