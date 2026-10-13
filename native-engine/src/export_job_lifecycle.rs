//! Terminal lifecycle and cleanup reconciliation for native export jobs.

use crate::export_job::ObjectExportBegin;
use crate::history::NativeObjectHistory;
use crate::png_output::{ExportArtifact, ExportBegin, StagedArtifactPort};
use crate::revision::ObjectSnapshot;
use crate::scheduler::FrameScheduler;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReconciliationStage {
    Pending,
    Complete,
    Unknown,
}

impl ReconciliationStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Complete => "complete",
            Self::Unknown => "unknown",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JobStatus {
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CleanupStatus {
    NotRequired,
    Pending,
    Complete,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CleanupReceipt {
    pub status: CleanupStatus,
    pub error: Option<JobError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExternalEffectDisposition {
    None,
    Contained,
    Committed,
    Indeterminate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobError {
    pub code: &'static str,
    pub message: String,
    pub details: Option<serde_json::Value>,
}

impl JobError {
    pub(crate) fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct JobReceipt {
    pub job_id: String,
    pub status: JobStatus,
    pub pinned_revision: u64,
    pub document_snapshot_id: String,
    pub progress: f64,
    pub artifact: Option<ExportArtifact>,
    pub cleanup: CleanupReceipt,
    pub external_effect_disposition: ExternalEffectDisposition,
    pub error: Option<JobError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportJobErrorKind {
    InvalidRequest,
    ChangedRequestId,
    Busy,
    NotFound,
    WrongDocument,
    Released,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportJobError {
    pub kind: ExportJobErrorKind,
    pub message: String,
}

impl ExportJobError {
    pub(crate) fn new(kind: ExportJobErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportReleaseReconciliation {
    pub receipts: Vec<JobReceipt>,
    pub cleanup_complete: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ApplicationReleaseReceipt {
    pub instance_id: String,
    pub document_id: String,
    pub content_revision: u64,
    pub cancelled_transaction_id: Option<String>,
    pub cancelled_transaction: Option<serde_json::Value>,
    pub undo_depth: usize,
    pub redo_depth: usize,
    pub transaction_stage: ReconciliationStage,
    pub export_stage: ReconciliationStage,
    pub exports: ExportReleaseReconciliation,
}

impl ApplicationReleaseReceipt {
    pub fn cleanup_complete(&self) -> bool {
        self.export_stage == ReconciliationStage::Complete && self.exports.cleanup_complete
    }
}

pub(crate) fn release_snapshot(
    receipts: impl Iterator<Item = JobReceipt>,
) -> ExportReleaseReconciliation {
    ExportReleaseReconciliation {
        receipts: receipts.collect(),
        cleanup_complete: false,
    }
}

pub(crate) fn cleanup_allows_release(receipt: &JobReceipt) -> bool {
    matches!(
        receipt.cleanup.status,
        CleanupStatus::NotRequired | CleanupStatus::Complete
    ) && receipt.external_effect_disposition != ExternalEffectDisposition::Indeterminate
}

pub(crate) fn terminalize<P: StagedArtifactPort>(
    port: &mut P,
    receipt: &mut JobReceipt,
    status: JobStatus,
    error: Option<JobError>,
) {
    receipt.artifact = None;
    match port.cleanup(&receipt.job_id) {
        Ok(()) => {
            receipt.status = status;
            receipt.cleanup = CleanupReceipt {
                status: CleanupStatus::Complete,
                error: None,
            };
            receipt.external_effect_disposition = ExternalEffectDisposition::None;
            receipt.error = error;
        }
        Err(cleanup_error) => retain_cleanup_failure(receipt, cleanup_error),
    }
}

/// Retry one previously indeterminate cleanup before the application authority
/// is retired. A successful retry contains the external effect but preserves
/// the terminal failure as evidence that the job did not succeed.
pub(crate) fn retry_failed_cleanup<P: StagedArtifactPort>(port: &mut P, receipt: &mut JobReceipt) {
    if receipt.cleanup.status != CleanupStatus::Failed {
        return;
    }
    match port.cleanup(&receipt.job_id) {
        Ok(()) => {
            receipt.cleanup = CleanupReceipt {
                status: CleanupStatus::Complete,
                error: None,
            };
            receipt.external_effect_disposition = ExternalEffectDisposition::None;
        }
        Err(error) => retain_cleanup_failure(receipt, error),
    }
}

pub(crate) fn retain_cleanup_failure(receipt: &mut JobReceipt, message: String) {
    receipt.status = JobStatus::Failed;
    receipt.cleanup = CleanupReceipt {
        status: CleanupStatus::Failed,
        error: Some(JobError::new("cleanup_failed", message.clone())),
    };
    receipt.external_effect_disposition = ExternalEffectDisposition::Indeterminate;
    receipt.error = Some(JobError::new("cleanup_failed", message));
}

pub(crate) fn validate_begin(request: &ExportBegin) -> Result<(), ExportJobError> {
    if request.request_id.is_empty()
        || request.context_id.is_empty()
        || request.quality != "final"
        || request.target.is_empty()
        || request.frames.is_empty()
    {
        return Err(ExportJobError::new(
            ExportJobErrorKind::InvalidRequest,
            "export request identity, target, final quality and frames are required",
        ));
    }
    if request.frames.windows(2).any(|pair| pair[0].0 >= pair[1].0) {
        return Err(ExportJobError::new(
            ExportJobErrorKind::InvalidRequest,
            "source frames must be strictly ascending",
        ));
    }
    Ok(())
}

pub(crate) fn invoke<T>(call: impl FnOnce() -> Result<T, String>) -> Result<T, String> {
    catch_unwind(AssertUnwindSafe(call)).unwrap_or_else(|_| Err("export callback panicked".into()))
}

pub(crate) fn validate_object_begin(
    history: &NativeObjectHistory,
    request: &ObjectExportBegin,
    scheduler: &FrameScheduler,
) -> Result<ObjectSnapshot, ExportJobError> {
    let invalid = |message| ExportJobError::new(ExportJobErrorKind::InvalidRequest, message);
    if scheduler.lease_counters().live() != 0 {
        return Err(ExportJobError::new(
            ExportJobErrorKind::Busy,
            "caller scheduler already owns live work",
        ));
    }
    if request.request_id.is_empty()
        || request.request_id.len() > 128
        || request.target.is_empty()
        || request.target.len() > 4096
        || request.target.chars().any(char::is_control)
        || request.maximum_encoded_bytes == 0
    {
        return Err(invalid(
            "bounded request, target and positive PNG budget required",
        ));
    }
    let pins = &request.pins;
    if pins.instance_id != history.instance_id()
        || pins.document_id != history.document_id()
        || pins.content_revision != history.content_revision()
    {
        return Err(invalid(
            "object export must begin on the original current owner",
        ));
    }
    history
        .acquire_snapshot(pins.content_revision)
        .ok_or_else(|| invalid("snapshot missing"))
}

pub(crate) fn replay_object_begin(
    original: &ObjectExportBegin,
    requested: &ObjectExportBegin,
    same_witness: bool,
    receipt: &JobReceipt,
) -> Result<JobReceipt, ExportJobError> {
    if original.request_id != requested.request_id {
        return Err(ExportJobError::new(
            ExportJobErrorKind::Busy,
            "one request is admitted per object export session",
        ));
    }
    if original != requested || !same_witness {
        return Err(ExportJobError::new(
            ExportJobErrorKind::ChangedRequestId,
            "changed object export replay",
        ));
    }
    Ok(receipt.clone())
}
