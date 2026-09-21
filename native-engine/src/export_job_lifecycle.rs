//! Terminal lifecycle and cleanup reconciliation for native export jobs.

use crate::png_output::{ExportArtifact, StagedArtifactPort};

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

fn retain_cleanup_failure(receipt: &mut JobReceipt, message: String) {
    receipt.status = JobStatus::Failed;
    receipt.cleanup = CleanupReceipt {
        status: CleanupStatus::Failed,
        error: Some(JobError::new("cleanup_failed", message.clone())),
    };
    receipt.external_effect_disposition = ExternalEffectDisposition::Indeterminate;
    receipt.error = Some(JobError::new("cleanup_failed", message));
}
