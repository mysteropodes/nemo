//! Read-only evidence for a core document replacement that cannot finish cleanly.
//! A fenced application keeps A installed; export cleanup may already have effects.

use crate::application::{ExportResourceResolver, NativeApplication};
use crate::document::OpacityDocument;
use crate::export_job::{
    CleanupStatus, ExportReleaseReconciliation, ExternalEffectDisposition, JobReceipt, JobStatus,
};
use crate::export_job::{
    ExportCompositor, ExportJobError, ExportJobErrorKind, PendingFrame, StagedArtifactPort,
};
use crate::history::NativeOpacityHistory;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementFailureKind {
    ReconciliationError,
    UnresolvedCleanup,
    Unwind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplacementPhase {
    Reconciling,
    Fenced(ReplacementFailureKind),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplacementProgress {
    pub phase: ReplacementPhase,
    pub old_document_id: String,
    pub old_revision: u64,
    pub old_undo_depth: usize,
    pub old_redo_depth: usize,
    pub prepared_document_id: String,
    pub export_receipts: Vec<JobReceipt>,
    pub live_leases: u64,
    pub diagnostic: Option<String>,
}

impl ReplacementProgress {
    pub(super) fn new(
        old_document_id: String,
        old_revision: u64,
        history_depths: (usize, usize),
        prepared_document_id: String,
    ) -> Self {
        Self {
            phase: ReplacementPhase::Reconciling,
            old_document_id,
            old_revision,
            old_undo_depth: history_depths.0,
            old_redo_depth: history_depths.1,
            prepared_document_id,
            export_receipts: Vec::new(),
            live_leases: 0,
            diagnostic: None,
        }
    }

    pub(super) fn fence(
        &mut self,
        kind: ReplacementFailureKind,
        snapshot: ExportReleaseReconciliation,
        live_leases: u64,
        diagnostic: String,
    ) {
        self.phase = ReplacementPhase::Fenced(kind);
        self.export_receipts = snapshot.receipts;
        self.live_leases = live_leases;
        self.diagnostic = Some(diagnostic);
    }
}

/// A release snapshot's aggregate flag is not a replacement oracle. Inspect every
/// retained job, including terminal jobs omitted by replace_document's return.
pub(super) fn unresolved(snapshot: &ExportReleaseReconciliation, live_leases: u64) -> bool {
    live_leases != 0
        || snapshot.receipts.iter().any(|receipt| {
            receipt.status == JobStatus::Running
                || !matches!(
                    receipt.cleanup.status,
                    CleanupStatus::NotRequired | CleanupStatus::Complete
                )
                || receipt.external_effect_disposition == ExternalEffectDisposition::Indeterminate
        })
}

impl<P: StagedArtifactPort, C: ExportCompositor, R: ExportResourceResolver>
    NativeApplication<P, C, R>
{
    pub fn replacement_progress(&self) -> Option<&ReplacementProgress> {
        self.replacement.as_ref()
    }

    /// A held A frame may still arrive after replacement cleanup has become
    /// indeterminate. Deny it before it can reach the export manager.
    pub fn finish_export_frame(
        &mut self,
        pending: PendingFrame,
    ) -> Result<JobReceipt, ExportJobError> {
        if self.replacement.is_some() {
            return Err(ExportJobError {
                kind: ExportJobErrorKind::Released,
                message: "native document replacement is fenced".into(),
            });
        }
        self.exports.finish_frame(pending)
    }

    pub fn replace_document(
        &mut self,
        document: OpacityDocument,
    ) -> Result<Vec<JobReceipt>, String> {
        if self.release.is_some() {
            return Err("native application authority has been released".into());
        }
        if self.replacement.is_some() {
            return Err("native document replacement remains fenced".into());
        }
        let prepared = NativeOpacityHistory::new(self.instance_id().to_owned(), document)
            .map_err(str::to_owned)?;
        let document_id = prepared.document_id().to_owned();
        self.replacement = Some(ReplacementProgress::new(
            self.document_id().to_owned(),
            self.content_revision(),
            self.history.history_depths(),
            document_id.clone(),
        ));
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            self.exports.replace_document(&document_id)
        }));
        let snapshot = self.exports.release_snapshot();
        let live_leases = self.exports.lease_counters().live();
        let (kind, message) = match &outcome {
            Ok(Ok(_)) if unresolved(&snapshot, live_leases) => (
                Some(ReplacementFailureKind::UnresolvedCleanup),
                "native replacement export cleanup is unresolved".to_owned(),
            ),
            Ok(Err(error)) => (
                Some(ReplacementFailureKind::ReconciliationError),
                error.message.clone(),
            ),
            Err(_) => (
                Some(ReplacementFailureKind::Unwind),
                "native replacement export reconciliation unwound".to_owned(),
            ),
            Ok(Ok(_)) => (None, String::new()),
        };
        if let Some(kind) = kind {
            self.replacement
                .as_mut()
                .unwrap()
                .fence(kind, snapshot, live_leases, message.clone());
            return Err(message);
        }
        let reconciled = outcome.unwrap().unwrap();
        self.history = prepared;
        self.diagnostics = Default::default();
        self.requests.clear();
        self.replacement = None;
        Ok(reconciled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export_job::CleanupReceipt;

    fn terminal() -> JobReceipt {
        JobReceipt {
            job_id: "retained".into(),
            status: JobStatus::Cancelled,
            pinned_revision: 0,
            document_snapshot_id: "snapshot".into(),
            progress: 0.0,
            artifact: None,
            cleanup: CleanupReceipt {
                status: CleanupStatus::Complete,
                error: None,
            },
            external_effect_disposition: ExternalEffectDisposition::None,
            error: None,
        }
    }

    #[test]
    fn every_retained_receipt_and_live_lease_is_checked_independently_of_aggregate_flag() {
        let mut snapshot = ExportReleaseReconciliation {
            receipts: vec![terminal()],
            cleanup_complete: true,
        };
        assert!(!unresolved(&snapshot, 0));
        assert!(unresolved(&snapshot, 1));
        snapshot.receipts.push(terminal());
        snapshot.receipts[1].external_effect_disposition = ExternalEffectDisposition::Indeterminate;
        assert!(unresolved(&snapshot, 0));
        snapshot.receipts[1].external_effect_disposition = ExternalEffectDisposition::None;
        snapshot.receipts[1].status = JobStatus::Running;
        assert!(unresolved(&snapshot, 0));
        snapshot.receipts[1].status = JobStatus::Failed;
        snapshot.receipts[1].cleanup.status = CleanupStatus::Failed;
        assert!(unresolved(&snapshot, 0));
    }
}
