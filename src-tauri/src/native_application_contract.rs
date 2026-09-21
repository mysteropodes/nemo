//! Strict versioned DTOs and admission for the dormant native desktop host.

use crate::native_application_ports::DesktopResourceResolver;
use native_engine::{
    codec::decode_project,
    desktop_viewport::{CssBounds, PhysicalExtent, ViewportMapping},
    document::OpacityDocument,
    export_job::{CleanupStatus, ExternalEffectDisposition, JobReceipt, JobStatus},
    render_scene::{GeometryPaintInput, LayerGeometry, OpaqueSrgbPaint},
    resource_leases::WorkId,
    scheduler::OutputSpec,
};
use nemo_mcp::contract::{NativeApplicationError, NATIVE_API_VERSION};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeSet;

pub(crate) const HOST_API_VERSION: u32 = NATIVE_API_VERSION;
const MAX_PROJECTION_BYTES: usize = 1_048_576;
const MAX_RESOURCES: usize = 64;
const MAX_LAYERS_PER_RESOURCE: usize = 256;

pub(crate) type HostResult<T> = Result<T, NativeApplicationError>;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeBootstrapRequest {
    pub(crate) api_version: u32,
    pub(crate) instance_id: String,
    pub(crate) projection: Value,
    pub(crate) resources: Vec<GeometryResourceInput>,
    #[serde(default)]
    pub(crate) output_bindings: Vec<OutputBindingInput>,
    pub(crate) viewport: Option<ViewportInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeReplacementRequest {
    pub(crate) api_version: u32,
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) projection: Value,
    pub(crate) resources: Vec<GeometryResourceInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeReleaseRequest {
    pub(crate) api_version: u32,
    pub(crate) request_id: String,
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) expected_revision: u64,
    #[serde(default)]
    pub(crate) cancelled_before_dispatch: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeOutputBindingRequest {
    pub(crate) api_version: u32,
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) expected_revision: u64,
    pub(crate) output_handle: String,
    pub(crate) destination: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativePreviewRequest {
    pub(crate) api_version: u32,
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) content_revision: u64,
    pub(crate) document_snapshot_id: String,
    pub(crate) context_id: String,
    pub(crate) frame: u32,
    pub(crate) quality: String,
    pub(crate) output_spec: OutputSpecInput,
    pub(crate) geometry_handle: ResourceIdentityInput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeViewportRequest {
    pub(crate) api_version: u32,
    pub(crate) instance_id: String,
    pub(crate) viewport: ViewportInput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct NativeDisposeRequest {
    pub(crate) api_version: u32,
    pub(crate) instance_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct GeometryResourceInput {
    resource_id: String,
    resource_version: String,
    layers: Vec<GeometryLayerInput>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct GeometryLayerInput {
    layer_uid: String,
    bounds: [f64; 4],
    transform: [f64; 6],
    paint: PaintInput,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PaintInput {
    red: u8,
    green: u8,
    blue: u8,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ResourceIdentityInput {
    pub(crate) resource_id: String,
    pub(crate) resource_version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OutputBindingInput {
    pub(crate) output_handle: String,
    pub(crate) destination: String,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ViewportInput {
    css_bounds: CssBoundsInput,
    physical_extent: ExtentInput,
    composition_extent: ExtentInput,
    reported_dpr: f64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CssBoundsInput {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ExtentInput {
    width: u32,
    height: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct OutputSpecInput {
    kind: String,
    format: String,
    width: u32,
    height: u32,
    color_interpretation: String,
    alpha_mode: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeBootstrapReceipt {
    pub(crate) api_version: u32,
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) content_revision: u64,
    pub(crate) resource_count: usize,
    pub(crate) viewport_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeReplacementReceipt {
    pub(crate) document_id: String,
    pub(crate) content_revision: u64,
    pub(crate) resource_count: usize,
    pub(crate) cancelled_preview_work_ids: Vec<String>,
    pub(crate) reconciled_exports: Vec<ExportReconciliation>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ExportReconciliation {
    pub(crate) job_id: String,
    pub(crate) status: String,
    pub(crate) cleanup_status: String,
    pub(crate) external_effect_disposition: String,
    pub(crate) error_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeReleaseReceipt {
    pub(crate) api_version: u32,
    pub(crate) request_id: String,
    pub(crate) instance_id: String,
    pub(crate) document_id: String,
    pub(crate) content_revision: u64,
    pub(crate) lifecycle_generation: u64,
    pub(crate) status: String,
    pub(crate) retrieved: bool,
    pub(crate) authority_removal_completed: bool,
    pub(crate) cancelled_transaction_id: Option<String>,
    pub(crate) cancelled_transaction: Option<Value>,
    pub(crate) undo_depth: usize,
    pub(crate) redo_depth: usize,
    pub(crate) reconciled_exports: Vec<ExportReconciliation>,
    pub(crate) cancelled_preview_work_ids: Vec<String>,
    pub(crate) viewport_status: String,
    pub(crate) reentry_available: bool,
    pub(crate) error: Option<NativeApplicationError>,
}

impl NativeReleaseReceipt {
    pub(crate) fn indeterminate(
        request_id: String,
        identity: (String, String, u64),
        generation: u64,
        message: &str,
    ) -> Self {
        Self {
            api_version: HOST_API_VERSION,
            request_id,
            instance_id: identity.0,
            document_id: identity.1,
            content_revision: identity.2,
            lifecycle_generation: generation,
            status: "indeterminate".into(),
            retrieved: false,
            authority_removal_completed: true,
            cancelled_transaction_id: None,
            cancelled_transaction: None,
            undo_depth: 0,
            redo_depth: 0,
            reconciled_exports: Vec::new(),
            cancelled_preview_work_ids: Vec::new(),
            viewport_status: "cleanup_failed".into(),
            reentry_available: false,
            error: Some(host_error("cleanup_failed", message)),
        }
    }

    pub(crate) fn retained_value(&self) -> Value {
        serde_json::to_value(self).unwrap_or_else(|_| serde_json::json!({
            "apiVersion": self.api_version, "requestId": self.request_id,
            "instanceId": self.instance_id, "documentId": self.document_id,
            "contentRevision": self.content_revision,
            "lifecycleGeneration": self.lifecycle_generation, "status": "indeterminate",
            "retrieved": false, "authorityRemovalCompleted": true,
            "cancelledTransactionId": self.cancelled_transaction_id,
            "cancelledTransaction": self.cancelled_transaction,
            "undoDepth": self.undo_depth, "redoDepth": self.redo_depth,
            "reconciledExports": [], "cancelledPreviewWorkIds": [],
            "viewportStatus": "cleanup_failed", "reentryAvailable": false,
            "error": {"code": "cleanup_failed", "message": "native release receipt serialization failed", "details": null}
        }))
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeOutputBindingReceipt {
    pub(crate) output_handle: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativePreviewReceipt {
    pub(crate) work_id: String,
    pub(crate) view_generation: u64,
    pub(crate) status: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct NativeViewportReceipt {
    pub(crate) status: &'static str,
    pub(crate) cancelled_work_ids: Vec<String>,
}

pub(crate) struct AdmittedProject {
    pub(crate) document: OpacityDocument,
    pub(crate) resources: Vec<GeometryPaintInput>,
}

impl OutputSpecInput {
    pub(crate) fn admit(&self) -> HostResult<OutputSpec> {
        if self.kind != "frame"
            || self.format != "rgba8"
            || self.width != 320
            || self.height != 180
            || self.color_interpretation != "srgb"
            || self.alpha_mode != "straight"
        {
            return Err(host_error(
                "invalid_request",
                "unsupported preview output contract",
            ));
        }
        OutputSpec::new(
            &self.kind,
            &self.format,
            self.width,
            self.height,
            &self.color_interpretation,
            &self.alpha_mode,
        )
        .map_err(|error| host_error("invalid_request", error.to_string()))
    }
}

impl ViewportInput {
    pub(crate) fn admit(self) -> HostResult<ViewportMapping> {
        if self.composition_extent.width != 320 || self.composition_extent.height != 180 {
            return Err(host_error(
                "invalid_request",
                "unsupported viewport composition extent",
            ));
        }
        ViewportMapping::new(
            CssBounds {
                x: self.css_bounds.x,
                y: self.css_bounds.y,
                width: self.css_bounds.width,
                height: self.css_bounds.height,
            },
            PhysicalExtent {
                width: self.physical_extent.width,
                height: self.physical_extent.height,
            },
            PhysicalExtent {
                width: self.composition_extent.width,
                height: self.composition_extent.height,
            },
            self.reported_dpr,
        )
        .map_err(|error| {
            host_error(
                "invalid_request",
                format!("invalid viewport mapping: {error:?}"),
            )
        })
    }
}

pub(crate) fn admit_project(
    projection: &Value,
    inputs: &[GeometryResourceInput],
) -> HostResult<AdmittedProject> {
    let bytes = serde_json::to_vec(projection)
        .map_err(|_| host_error("invalid_request", "projection is not serializable"))?;
    if bytes.len() > MAX_PROJECTION_BYTES || inputs.is_empty() || inputs.len() > MAX_RESOURCES {
        return Err(host_error(
            "invalid_request",
            "native project exceeds bounded input limits",
        ));
    }
    let document =
        decode_project(&bytes).map_err(|error| host_error("invalid_request", error.to_string()))?;
    let expected: BTreeSet<_> = document
        .layers()
        .iter()
        .map(|layer| layer.layer_uid())
        .collect();
    let mut resources = Vec::with_capacity(inputs.len());
    for input in inputs {
        if !bounded_id(&input.resource_id)
            || !bounded_id(&input.resource_version)
            || input.layers.is_empty()
            || input.layers.len() > MAX_LAYERS_PER_RESOURCE
        {
            return Err(host_error(
                "invalid_request",
                "invalid geometry resource identity or size",
            ));
        }
        let actual: BTreeSet<_> = input
            .layers
            .iter()
            .map(|layer| layer.layer_uid.as_str())
            .collect();
        if actual.len() != input.layers.len() || actual != expected {
            return Err(host_error(
                "invalid_request",
                "geometry and projection layerUid sets differ",
            ));
        }
        let layers = input
            .layers
            .iter()
            .map(|layer| {
                LayerGeometry::new(
                    &layer.layer_uid,
                    layer.bounds,
                    layer.transform,
                    OpaqueSrgbPaint::new(layer.paint.red, layer.paint.green, layer.paint.blue),
                )
                .map_err(|error| host_error("invalid_request", error.to_string()))
            })
            .collect::<HostResult<Vec<_>>>()?;
        resources.push(
            GeometryPaintInput::new(&input.resource_id, &input.resource_version, layers)
                .map_err(|error| host_error("invalid_request", error.to_string()))?,
        );
    }
    DesktopResourceResolver::new(resources.clone())
        .map_err(|message| host_error("invalid_request", message))?;
    Ok(AdmittedProject {
        document,
        resources,
    })
}

pub(crate) fn require_api_instance(version: u32, requested: &str, actual: &str) -> HostResult<()> {
    if version != NATIVE_API_VERSION || !bounded_id(requested) {
        return Err(host_error(
            "invalid_request",
            "invalid native host request identity",
        ));
    }
    if requested != actual {
        return Err(host_error(
            "wrong_instance",
            "native application instance mismatch",
        ));
    }
    Ok(())
}

pub(crate) fn release_fingerprint(request: &NativeReleaseRequest) -> HostResult<Vec<u8>> {
    if request.api_version != NATIVE_API_VERSION
        || !bounded_id(&request.request_id)
        || !bounded_id(&request.instance_id)
        || !bounded_id(&request.document_id)
        || request.expected_revision > 9_007_199_254_740_991
    {
        return Err(host_error(
            "invalid_request",
            "invalid native release request identity",
        ));
    }
    serde_json::to_vec(request).map_err(|_| {
        host_error(
            "invalid_request",
            "native release request is not serializable",
        )
    })
}

pub(crate) fn require_main(window: &tauri::Window) -> HostResult<()> {
    if window.label() == "main" {
        Ok(())
    } else {
        Err(host_error(
            "wrong_instance",
            "native application requires the main window",
        ))
    }
}

pub(crate) fn bounded_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    matches!(bytes.first(), Some(byte) if byte.is_ascii_alphanumeric())
        && bytes.len() <= 128
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(byte))
}

pub(crate) fn host_error(
    code: impl Into<String>,
    message: impl Into<String>,
) -> NativeApplicationError {
    NativeApplicationError {
        code: code.into(),
        message: message.into(),
        details: None,
    }
}

pub(crate) fn catch_unwind_message<T>(operation: impl FnOnce() -> T) -> Result<T, String> {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(operation)).map_err(|payload| {
        payload
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| payload.downcast_ref::<String>().map(String::as_str))
            .unwrap_or("native release cleanup panicked")
            .to_string()
    })
}

pub(crate) fn work_label(work_id: WorkId) -> String {
    format!("native-work-{}", work_id.value())
}

pub(crate) fn reconcile_export(receipt: &JobReceipt) -> ExportReconciliation {
    ExportReconciliation {
        job_id: receipt.job_id.clone(),
        status: match receipt.status {
            JobStatus::Running => "running",
            JobStatus::Succeeded => "succeeded",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        }
        .into(),
        cleanup_status: match receipt.cleanup.status {
            CleanupStatus::NotRequired => "not_required",
            CleanupStatus::Pending => "pending",
            CleanupStatus::Complete => "complete",
            CleanupStatus::Failed => "failed",
        }
        .into(),
        external_effect_disposition: match receipt.external_effect_disposition {
            ExternalEffectDisposition::None => "none",
            ExternalEffectDisposition::Contained => "contained",
            ExternalEffectDisposition::Committed => "committed",
            ExternalEffectDisposition::Indeterminate => "indeterminate",
        }
        .into(),
        error_code: receipt.error.as_ref().map(|error| error.code.to_owned()),
    }
}
