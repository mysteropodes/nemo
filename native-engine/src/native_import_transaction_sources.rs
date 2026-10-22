//! Source-aware request entry on the existing history/replay owner. Bytes resolve
//! outside the owner; neither a Prepared value nor mutable authority escapes.
use super::{ImportAssetSource, OP_IMPORT_BATCH};
use crate::commands::ImportBatchResult;
use crate::history::object_history::NativeObjectHistory;
use crate::request_receipts::{
    DispatchErrorCode, OpacityRequest, RequestFingerprint, ResponseEnvelope,
};
use crate::transaction::{CompactFailure, CompactResult, ReceiptLookup, RetainedDisposition};
#[path = "native_import_transaction_source_types.rs"]
mod types;
use types::{identifier, SourceArgs};

impl NativeObjectHistory {
    fn import_source_args(
        &self,
        request: &OpacityRequest,
    ) -> Result<SourceArgs, DispatchErrorCode> {
        if request.api_version != 2
            || request.operation != OP_IMPORT_BATCH
            || request
                .expected_revision
                .is_none_or(|n| n > 9_007_199_254_740_991)
            || [
                &request.request_id,
                &request.instance_id,
                &request.document_id,
            ]
            .iter()
            .any(|id| !identifier(id))
            || serde_json::to_vec(request).map_or(true, |bytes| bytes.len() > 4096)
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if request.instance_id != self.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if request.document_id != self.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        SourceArgs::parse(&request.payload)
    }
    fn import_source_origin(
        &self,
        request: &OpacityRequest,
        args: &SourceArgs,
    ) -> Result<(), DispatchErrorCode> {
        if request.cancelled_before_dispatch {
            return Err(DispatchErrorCode::CancelledBeforeDispatch);
        }
        if request.expected_revision != Some(self.content_revision()) {
            return Err(DispatchErrorCode::StaleRevision);
        }
        let origin = self
            .acquire_snapshot(self.content_revision())
            .ok_or(DispatchErrorCode::Unavailable)?;
        if origin.snapshot_id() != args.snapshot_id {
            return Err(DispatchErrorCode::StaleRevision);
        }
        // This strict retained-record parse occurs AFTER receipt lookup: a replay
        // after undo must not depend on today's catalog or read the source again.
        args.batch(&origin)?;
        Ok(())
    }
    fn import_source_unretained(
        &self,
        request: &OpacityRequest,
        code: DispatchErrorCode,
    ) -> Result<Option<ResponseEnvelope>, DispatchErrorCode> {
        if !identifier(&request.request_id) {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        RetainedDisposition::Exact {
            content_revision: self.content_revision(),
            outcome: Err(CompactFailure {
                code,
                message: "Native batch source import refused.".into(),
                details: None,
            }),
        }
        .response(request, self.instance_id(), self.document_id())
        .filter(|response| serde_json::to_vec(response).is_ok_and(|bytes| bytes.len() <= 4096))
        .map(Some)
        .ok_or(DispatchErrorCode::InvalidRequest)
    }
    fn retain_import_source_outcome(
        &mut self,
        request: &OpacityRequest,
        outcome: Result<ImportBatchResult, DispatchErrorCode>,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        let fingerprint =
            RequestFingerprint::new(request).ok_or(DispatchErrorCode::InvalidRequest)?;
        let disposition = RetainedDisposition::Exact {
            content_revision: self.content_revision(),
            outcome: outcome
                .map(CompactResult::ImportBatch)
                .map_err(|code| CompactFailure {
                    code,
                    message: "Native batch source import refused.".into(),
                    details: None,
                }),
        };
        self.receipts
            .retain(request.request_id.clone(), fingerprint, disposition.clone());
        disposition
            .response(request, self.instance_id(), self.document_id())
            .ok_or(DispatchErrorCode::Internal)
    }
    /// None authorizes ONLY source resolution, never a publication. The host must
    /// repeat its actual lifecycle, close, synchronization and scoped I/O fences.
    pub fn import_batch_sources_status(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<Option<ResponseEnvelope>, DispatchErrorCode> {
        let args = match self.import_source_args(request) {
            Ok(args) => args,
            Err(code) => return self.import_source_unretained(request, code),
        };
        let fingerprint =
            RequestFingerprint::new(request).ok_or(DispatchErrorCode::InvalidRequest)?;
        match self.receipts.lookup(&request.request_id, &fingerprint) {
            ReceiptLookup::Retained(value) => {
                return value
                    .response(request, self.instance_id(), self.document_id())
                    .map(Some)
                    .ok_or(DispatchErrorCode::Internal)
            }
            ReceiptLookup::ChangedBody => {
                return self.import_source_unretained(request, DispatchErrorCode::InvalidRequest)
            }
            ReceiptLookup::Missing => {}
        }
        if let Err(code) = self.import_source_origin(request, &args) {
            return self
                .retain_import_source_outcome(request, Err(code))
                .map(Some);
        }
        Ok(None)
    }
    pub fn dispatch_import_batch_sources(
        &mut self,
        request: &OpacityRequest,
        sources: &[ImportAssetSource<'_>],
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        self.dispatch_import_batch_sources_cancellable(request, sources, &|| false)
    }
    pub fn dispatch_import_batch_sources_cancellable(
        &mut self,
        request: &OpacityRequest,
        sources: &[ImportAssetSource<'_>],
        cancel: &dyn Fn() -> bool,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        if let Some(response) = self.import_batch_sources_status(request)? {
            return Ok(response);
        }
        let args = self.import_source_args(request)?;
        let outcome = (|| {
            self.import_source_origin(request, &args)?;
            if cancel() {
                return Err(DispatchErrorCode::CancelledBeforeDispatch);
            }
            if !args.matches(sources) {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            let origin = self
                .acquire_snapshot(self.content_revision())
                .ok_or(DispatchErrorCode::Unavailable)?;
            let batch = args.batch(&origin)?;
            let prepared = self
                .prepare_import_batch_sources(&request.request_id, &batch, sources)
                .map_err(|error| error.code())?;
            if cancel() {
                return Err(DispatchErrorCode::CancelledBeforeDispatch);
            }
            // prepare_import_batch_sources checked the complete3072-byte receipt
            // before a commit; it and the original owner retain one history entry.
            self.commit_import_batch(prepared).map(|(_, result)| result)
        })();
        self.retain_import_source_outcome(request, outcome)
    }
}

#[cfg(test)]
#[path = "../tests/native_import_transaction_sources.rs"]
mod tests;
