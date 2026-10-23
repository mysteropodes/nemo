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
#[path = "native_object_desktop_sequence.rs"]
mod sequence;

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
    #[serde(default, deserialize_with = "sequence::non_null_range")]
    frame_range: Option<sequence::FrameRange>,
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
        || !sequence::valid_range(&request)
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
                self.staging_root.clone(),
                OUTPUT_HANDLE.into(),
                self.destination.clone(),
            )
        } else {
            LazyDesktopArtifactPort::new(
                self.staging_root.clone(),
                OUTPUT_HANDLE.into(),
                self.destination.clone(),
            )
        };
        self.run_with_port(compositor, &mut port)
    }
    fn run_with_port<P: native_engine::export_job::StagedArtifactPort>(
        self,
        compositor: &mut Compositor,
        port: &mut P,
    ) -> Result<ObjectHostExportOutcome, String> {
        if self.format == ObjectExportFormat::Svg {
            return self.owner.export_pinned_object_svg_frames(
                &self.selector,
                &self.frames,
                self.request_id,
                OUTPUT_HANDLE.into(),
                self.output,
                self.maximum_encoded_bytes,
                port,
            );
        }
        self.owner.export_pinned_object_png_frames(
            &self.selector,
            &self.frames,
            self.request_id,
            OUTPUT_HANDLE.into(),
            self.output,
            self.maximum_encoded_bytes,
            port,
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

/// Each caller retains exactly one original-owner/GPU borrow for one bounded child.
fn with_export_consumer<T>(
    state: &ApplicationMcp,
    admitted: &AdmittedExport,
    staging_root: PathBuf,
    frame: u32,
    frames: Vec<u32>,
    request_id: String,
    target: impl FnOnce(ObjectExportConsumer<'_>) -> HostResult<T>,
) -> HostResult<T> {
    let mut selector = admitted.request.selector();
    selector.frame = frame;
    state
        .with_object_preview_owner(admitted.request.lifecycle_generation, &selector, |owner| {
            if !owner.matches_project_snapshot(&admitted.request.document_snapshot_id) {
                return Err("native object export snapshot is stale".into());
            }
            owner.require_project_output(admitted.output.dimensions())?;
            if let Some(range) = admitted.request.frame_range {
                let info = owner.project_file_info()?;
                if info
                    .get("totalFrames")
                    .and_then(serde_json::Value::as_u64)
                    .is_none_or(|total| u64::from(range.end) >= total)
                {
                    return Err("native object sequence range exceeds the pinned document".into());
                }
            }
            Ok(target(ObjectExportConsumer {
                owner,
                selector: selector.clone(),
                frames,
                request_id,
                output: admitted.output.clone(),
                maximum_encoded_bytes: admitted.request.maximum_encoded_bytes,
                format: admitted.request.format,
                destination: admitted.destination.clone(),
                staging_root,
            }))
        })
        .map_err(|message| host_error("unavailable", message))?
}
fn export_with_target(
    state: &ApplicationMcp,
    admitted: &AdmittedExport,
    staging_root: PathBuf,
    target: impl FnOnce(ObjectExportConsumer<'_>) -> HostResult<ObjectHostExportOutcome>,
) -> HostResult<ObjectExportReceipt> {
    let frames = admitted
        .request
        .frames
        .clone()
        .unwrap_or_else(|| vec![admitted.request.frame]);
    let outcome = with_export_consumer(
        state,
        admitted,
        staging_root,
        admitted.request.frame,
        frames,
        admitted.request.request_id.clone(),
        target,
    )?;
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
    let admitted =
        std::sync::Arc::new(admit_request(&request_json, state.instance_id(), |path| {
            window
                .try_fs_scope()
                .is_some_and(|scope| scope.is_allowed(path))
        })?);
    let staging_root = app
        .path()
        .app_cache_dir()
        .map_err(|_| host_error("unavailable", "native staging root unavailable"))?
        .join("native-export-staging");
    if admitted.request.frame_range.is_some() {
        return sequence::run(&app, admitted, staging_root).await;
    }
    for attempt in 0..2 {
        let app_for_main = app.clone();
        let for_main = std::sync::Arc::clone(&admitted);
        let staging = staging_root.clone();
        let (result, completion, diagnostic_ready) =
            viewport_host::on_main_thread(&app, move || {
                let state = app_for_main.state::<ApplicationMcp>();
                let result = export_with_target(&state, &for_main, staging, |consumer| {
                    viewport_host::with_object_target(
                        &app_for_main,
                        &for_main.request.instance_id,
                        for_main.request.lifecycle_generation,
                        for_main.mapping,
                        consumer,
                    )
                });
                // Ordered CPU effect preflight finished before artifact staging.
                // Native/revision/GPU borrows have returned before this callback.
                let diagnostic_ready = state
                    .seal_object_motion_diagnostics()
                    .map_err(|error| host_error("unavailable", error))?;
                let completion = state.spawn_object_motion_completion(&app_for_main);
                Ok((result, completion, diagnostic_ready))
            })
            .await?;
        if result.is_err() && diagnostic_ready == Some(true) && attempt == 0 {
            continue;
        }
        if let Some((receiver, waits_for_prior)) = completion {
            if waits_for_prior {
                return result;
            }
            let finalized = receiver
                .await
                .map_err(|_| host_error("unavailable", "native Motion completion was cancelled"))?
                .map_err(|error| host_error("unavailable", error))?;
            if result.is_err() && finalized && attempt == 0 {
                continue;
            }
        }
        return result;
    }
    unreachable!("bounded cached fallback retry")
}

#[cfg(test)]
#[path = "native_object_desktop_export_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "native_object_svg_desktop_export_tests.rs"]
mod svg_tests;
