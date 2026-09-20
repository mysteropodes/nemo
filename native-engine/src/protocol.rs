//! Frozen native application v2 wire shapes.
//!
//! This module owns transport parsing/serialization only. Document, history,
//! evaluation, resource, and export behavior remain in their existing owners.

use crate::commands::{
    DispatchError, DispatchErrorCode, OpacityRequest, ResponseEnvelope,
    APPLICATION_API_VERSION,
};
use crate::export_job::{
    CleanupStatus, ExternalEffectDisposition, JobReceipt, JobStatus,
};
use serde::Deserialize;
use serde_json::{json, Value};

pub const OP_JOB_EXPORT_PNG_BEGIN: &str = "job.export.png.begin";
pub const OP_JOB_EXPORT_PNG_STATUS: &str = "job.export.png.status";
pub const OP_JOB_EXPORT_PNG_CANCEL: &str = "job.export.png.cancel";

pub const MAX_REQUEST_BYTES: usize = 4096;
pub const MAX_EXPORT_FRAMES: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct OpaqueResourceHandle {
    resource_id: String,
    resource_version: String,
}

impl OpaqueResourceHandle {
    pub fn resource_id(&self) -> &str {
        &self.resource_id
    }

    pub fn resource_version(&self) -> &str {
        &self.resource_version
    }

    fn is_valid(&self) -> bool {
        bounded_identifier(&self.resource_id) && bounded_identifier(&self.resource_version)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ExportFrameDescriptor {
    pub(crate) source_frame: u32,
    pub(crate) geometry_handle: OpaqueResourceHandle,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct ExportBeginPayload {
    pub(crate) context_id: String,
    pub(crate) quality: String,
    pub(crate) output_handle: String,
    pub(crate) frames: Vec<ExportFrameDescriptor>,
}

impl ExportBeginPayload {
    pub(crate) fn parse(value: Value) -> Result<Self, &'static str> {
        let payload: Self = serde_json::from_value(value).map_err(|_| "Invalid PNG job payload.")?;
        if !bounded_identifier(&payload.context_id)
            || payload.quality != "final"
            || !bounded_identifier(&payload.output_handle)
            || payload.frames.is_empty()
            || payload.frames.len() > MAX_EXPORT_FRAMES
            || payload
                .frames
                .windows(2)
                .any(|pair| pair[0].source_frame >= pair[1].source_frame)
            || payload.frames.iter().any(|frame| !frame.geometry_handle.is_valid())
        {
            return Err("Invalid bounded PNG job input.");
        }
        Ok(payload)
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct JobIdPayload {
    pub(crate) job_id: String,
}

impl JobIdPayload {
    pub(crate) fn parse(value: Value) -> Result<Self, &'static str> {
        let payload: Self = serde_json::from_value(value).map_err(|_| "Invalid job identity.")?;
        if !bounded_identifier(&payload.job_id) {
            return Err("Invalid job identity.");
        }
        Ok(payload)
    }
}

pub(crate) fn preflight_identity(
    request: &OpacityRequest,
    instance_id: &str,
    document_id: &str,
    content_revision: u64,
) -> Option<ResponseEnvelope> {
    if request.api_version != APPLICATION_API_VERSION
        || !bounded_identifier(&request.request_id)
        || !bounded_identifier(&request.instance_id)
        || !bounded_identifier(&request.document_id)
        || !request.payload.is_object()
        || !matches!(serde_json::to_vec(request).map(|bytes| bytes.len()), Ok(0..=MAX_REQUEST_BYTES))
    {
        return Some(failure(
            request,
            instance_id,
            document_id,
            content_revision,
            DispatchErrorCode::InvalidRequest,
            "Invalid native application request.",
            None,
        ));
    }
    if request.instance_id != instance_id {
        return Some(failure(
            request,
            instance_id,
            document_id,
            content_revision,
            DispatchErrorCode::WrongInstance,
            "Request targets a different native engine instance.",
            None,
        ));
    }
    if request.document_id != document_id {
        return Some(failure(
            request,
            instance_id,
            document_id,
            content_revision,
            DispatchErrorCode::WrongDocument,
            "Request targets a replaced document.",
            Some(json!({ "requestedDocumentId": request.document_id })),
        ));
    }
    None
}

pub(crate) fn job_response(
    request: &OpacityRequest,
    instance_id: &str,
    document_id: &str,
    content_revision: u64,
    receipt: &JobReceipt,
) -> ResponseEnvelope {
    ResponseEnvelope {
        api_version: APPLICATION_API_VERSION,
        request_id: request.request_id.clone(),
        instance_id: instance_id.to_owned(),
        document_id: document_id.to_owned(),
        content_revision,
        ok: true,
        result: Some(job_receipt(receipt)),
        error: None,
    }
}

pub(crate) fn failure(
    request: &OpacityRequest,
    instance_id: &str,
    document_id: &str,
    content_revision: u64,
    code: DispatchErrorCode,
    message: impl Into<String>,
    details: Option<Value>,
) -> ResponseEnvelope {
    ResponseEnvelope {
        api_version: APPLICATION_API_VERSION,
        request_id: request.request_id.clone(),
        instance_id: instance_id.to_owned(),
        document_id: document_id.to_owned(),
        content_revision,
        ok: false,
        result: None,
        error: Some(DispatchError {
            code,
            message: message.into(),
            details,
        }),
    }
}

fn job_receipt(receipt: &JobReceipt) -> Value {
    let mut value = json!({
        "jobId": receipt.job_id,
        "status": status(receipt.status),
        "pinnedRevision": receipt.pinned_revision,
        "documentSnapshotId": receipt.document_snapshot_id,
        "progress": receipt.progress,
        "artifact": receipt.artifact.as_ref().map(|artifact| json!({
            "target": artifact.target,
            "files": artifact.files
        })),
        "cleanup": { "status": cleanup_status(receipt.cleanup.status) },
        "externalEffectDisposition": effect(receipt.external_effect_disposition)
    });
    if let Some(error) = &receipt.cleanup.error {
        value["cleanup"]["error"] = job_error(error);
    }
    if let Some(error) = &receipt.error {
        value["error"] = job_error(error);
    }
    value
}

fn job_error(error: &crate::export_job::JobError) -> Value {
    let mut value = json!({ "code": error.code, "message": error.message });
    if let Some(details) = &error.details {
        value["details"] = details.clone();
    }
    value
}

fn status(value: JobStatus) -> &'static str {
    match value {
        JobStatus::Running => "running",
        JobStatus::Succeeded => "succeeded",
        JobStatus::Failed => "failed",
        JobStatus::Cancelled => "cancelled",
    }
}

fn cleanup_status(value: CleanupStatus) -> &'static str {
    match value {
        CleanupStatus::NotRequired => "not_required",
        CleanupStatus::Pending => "pending",
        CleanupStatus::Complete => "complete",
        CleanupStatus::Failed => "failed",
    }
}

fn effect(value: ExternalEffectDisposition) -> &'static str {
    match value {
        ExternalEffectDisposition::None => "none",
        ExternalEffectDisposition::Contained => "contained",
        ExternalEffectDisposition::Committed => "committed",
        ExternalEffectDisposition::Indeterminate => "indeterminate",
    }
}

fn bounded_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
}
