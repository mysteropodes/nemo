//! History side of the atomic import transaction (L10-H): `NativeObjectHistory` prepares a batch
//! from its own current snapshot, commits it through the owner and records exactly ONE
//! `ObjectHistoryEntry` for the whole batch (none for the no-op, which also keeps the redo stack),
//! and the staged dispatcher routes the command here so cancellation, `expectedRevision`, owner
//! pins and retained request replay apply unchanged.
//!
//! The existing scalar operation retains its staged dispatcher. The closed source-bearing
//! branch adds a request/receipt entry for trusted host resolution; neither prepared candidates
//! nor the revision owner are exposed. The dispatcher bounds the whole request and the whole response at 4096 bytes,
//! so a receipt that would not fit is refused BEFORE commit (`ReceiptTooLarge`): a published batch
//! whose reply failed would be the worst outcome.
use crate::commands::{ImportBatchResult, PreparedImportBatch};
use crate::history::object_history::object_shot_kernels::native_import_transaction::wire::parse_batch_at;
use crate::history::object_history::object_shot_kernels::native_import_transaction::{
    plan_import_batch, ImportBatch, ImportError,
};
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;
use serde_json::Value;

/// Existing declared batch operation, shared by scalar and source-aware request entries.
pub(crate) const OP_IMPORT_BATCH: &str = "command.document.object.import.batch";
/// Largest serialized `result` value the dispatcher can echo inside its 4096-byte response.
pub(crate) const MAX_RECEIPT_BYTES: usize = 3072;

#[cfg(feature = "project_media_codec")]
#[path = "native_import_transaction_sources.rs"]
mod sources;

/// Trusted native input: binary bytes stay outside the bounded scalar wire.
/// The normal PNG/font providers validate and retain them before batch sealing.
#[cfg(feature = "project_media_codec")]
pub struct ImportAssetSource<'a> {
    pub source_id: &'a str,
    pub name: &'a str,
    pub media_type: &'a str,
    pub bytes: &'a [u8],
}

/// A sealed batch with the receipt its commit will report.
#[derive(Debug, Clone)]
pub(crate) struct PreparedImport {
    sealed: PreparedImportBatch,
    result: ImportBatchResult,
}

impl PreparedImport {
    pub(crate) fn result(&self) -> &ImportBatchResult {
        &self.result
    }
    /// False for the no-op batch.
    pub(crate) fn changed(&self) -> bool {
        self.sealed.changed()
    }
}

impl NativeObjectHistory {
    #[cfg(feature = "project_media_codec")]
    pub(crate) fn prepare_import_batch_sources(
        &self,
        request_id: &str,
        batch: &ImportBatch,
        sources: &[ImportAssetSource<'_>],
    ) -> Result<PreparedImport, ImportError> {
        use crate::history::object_history::object_shot_kernels::native_import_transaction::ImportAsset;
        use crate::object_document::AssetKind;
        use crate::object_snapshot::AssetEdit;
        if request_id.trim().is_empty() {
            return Err(ImportError::Payload);
        }
        if sources.len() > 1024 {
            return Err(ImportError::Overflow);
        }
        let origin = self
            .acquire_snapshot(self.content_revision())
            .ok_or(ImportError::Internal)?;
        let project = origin
            .project()
            .ok_or(ImportError::Dispatch(DispatchErrorCode::Unavailable))?;
        let mut joined = batch.clone();
        for (index, source) in sources.iter().enumerate() {
            let id = format!("{request_id}-{index}");
            let record = match source.media_type {
                "image/png" => {
                    let prepared = origin
                        .prepare_asset_attachment(&id, source.name, source.media_type, source.bytes)
                        .map_err(|error| ImportError::Dispatch(error.code()))?;
                    match prepared.into_edit() {
                        AssetEdit::Add(record) => record,
                        AssetEdit::Unchanged => project
                            .asset_inventory()
                            .and_then(|catalog| {
                                catalog.records().iter().find(|record| {
                                    record.kind() == AssetKind::Image
                                        && record.source().media_type() == Some("image/png")
                                        && record
                                            .source()
                                            .encoded_allocation()
                                            .is_some_and(|bytes| bytes.as_slice() == source.bytes)
                                })
                            })
                            .cloned()
                            .ok_or(ImportError::Internal)?,
                        AssetEdit::Remove(_) => return Err(ImportError::Internal),
                    }
                }
                "font/ttf" => {
                    #[cfg(feature = "font_asset_wire")]
                    {
                        origin
                            .admit_font_asset(
                                format!("asset-{id}"),
                                source.name.into(),
                                source.bytes,
                            )
                            .map_err(ImportError::Dispatch)?
                    }
                    #[cfg(not(feature = "font_asset_wire"))]
                    {
                        return Err(ImportError::Dispatch(DispatchErrorCode::Unavailable));
                    }
                }
                _ => return Err(ImportError::Dispatch(DispatchErrorCode::Unavailable)),
            };
            joined.assets.push(ImportAsset {
                source_id: source.source_id.into(),
                record,
            });
        }
        // No asset was attached first: one sealed candidate contains ALL new
        // resources and authored items, with one later publish/history entry.
        let prepared = self.prepare_import_batch(&joined)?;
        let receipt =
            serde_json::to_vec(&prepared.result().result()).map_err(|_| ImportError::Internal)?;
        if receipt.len() > MAX_RECEIPT_BYTES {
            return Err(ImportError::ReceiptTooLarge);
        }
        Ok(prepared)
    }

    /// Plans the batch against the current immutable snapshot and seals it; nothing is published.
    pub(crate) fn prepare_import_batch(
        &self,
        batch: &ImportBatch,
    ) -> Result<PreparedImport, ImportError> {
        let origin = self
            .acquire_snapshot(self.content_revision())
            .ok_or(ImportError::Internal)?;
        let plan = plan_import_batch(&origin, batch)?;
        let sealed = self
            .owner
            .seal_import_batch_with_assets(origin, plan.document, plan.asset_inventory)
            .map_err(ImportError::Dispatch)?;
        Ok(PreparedImport {
            sealed,
            result: plan.result,
        })
    }

    /// Strict original-byte entry.
    pub(crate) fn prepare_import_batch_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedImport, ImportError> {
        let origin = self
            .acquire_snapshot(self.content_revision())
            .ok_or(ImportError::Internal)?;
        self.prepare_import_batch(&parse_batch_at(&origin, bytes)?)
    }

    /// One owner publish and one history entry for the whole batch; the no-op adds neither and
    /// leaves the redo stack alone. Returns the head revision and the receipt.
    pub(crate) fn commit_import_batch(
        &mut self,
        prepared: PreparedImport,
    ) -> Result<(u64, ImportBatchResult), DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_import_batch(prepared.sealed)?;
        if after_revision != before_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok((after_revision, prepared.result))
    }

    /// Operations the staged dispatcher routes here.
    pub(crate) fn import_batch_operation(operation: &str) -> bool {
        operation == OP_IMPORT_BATCH
    }

    /// Prepare and commit for the dispatcher, which has already checked the envelope, owner pins,
    /// cancellation, revision and request replay.
    pub(crate) fn dispatch_import_batch(
        &mut self,
        payload: &Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let refuse = |error: ImportError| (error.code(), error.message());
        let bytes = serde_json::to_vec(payload).map_err(|_| refuse(ImportError::Payload))?;
        let prepared = self.prepare_import_batch_json(&bytes).map_err(refuse)?;
        let receipt = serde_json::to_vec(&prepared.result().result())
            .map_err(|_| refuse(ImportError::Internal))?;
        if receipt.len() > MAX_RECEIPT_BYTES {
            return Err(refuse(ImportError::ReceiptTooLarge));
        }
        let (_, result) = self
            .commit_import_batch(prepared)
            .map_err(|code| (code, "Import batch could not commit."))?;
        Ok(CompactResult::ImportBatch(result))
    }
}
