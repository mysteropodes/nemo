//! Owner side of the atomic import transaction (L10-H): the sealed batch candidate and its single
//! publication through the existing `ObjectRevisionOwner`. Declared as a private child of
//! `object_fill_owner.rs`, next to the layer-create / vector-edit preparation it mirrors.
//!
//! This layer cannot name the importers' plan types (they live in the history layer), so the
//! after-document is built there by the pure planner and handed in whole. Sealing re-reads the
//! owner's CURRENT immutable snapshot and refuses an origin that is not it, then runs the same
//! storage admission layer-create runs on the complete after-document, after checking it is the origin
//! plus appended layers and records (a forged or stale-derived document cannot be sealed). Commit re-checks instance,
//! document, snapshot and content revision exactly like `commit_layer_create`, and publishes the
//! whole batch with ONE call to `ObjectRevisionOwner::publish`. A candidate with no document is
//! the no-op: it publishes nothing and reports the unchanged head. No history, receipt or id state
//! lives here.
use crate::commands::ObjectFillOwner;
use crate::object_document::ObjectDocument;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::{json, Value};

/// One element the importer skipped, as the receipt reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportSkipReport {
    pub(crate) kind: &'static str,
    pub(crate) source_id: String,
    pub(crate) reason: String,
}

/// The compact receipt of one import: what was created (source id, native id) and what was skipped.
/// `layers + objects + skipped` always equals the number of entries the batch declared.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImportBatchResult {
    pub(crate) applied: bool,
    pub(crate) layers: Vec<(String, String)>,
    pub(crate) objects: Vec<(String, String)>,
    pub(crate) skipped: Vec<ImportSkipReport>,
}

impl ImportBatchResult {
    /// The retained result value echoed in the response envelope.
    pub(crate) fn result(&self) -> Value {
        let layers: Vec<Value> = self
            .layers
            .iter()
            .map(|(source, native)| json!({"sourceId": source, "layerUid": native}))
            .collect();
        let objects: Vec<Value> = self
            .objects
            .iter()
            .map(|(source, native)| json!({"sourceId": source, "strokeId": native}))
            .collect();
        let skipped: Vec<Value> = self
            .skipped
            .iter()
            .map(|skip| json!({"kind": skip.kind, "sourceId": skip.source_id, "reason": skip.reason}))
            .collect();
        json!({
            "applied": self.applied,
            "historyEntriesAdded": u64::from(self.applied),
            "layers": layers,
            "objects": objects,
            "skipped": skipped,
        })
    }
}

/// A detached batch sealed to the snapshot it was planned from. Only this module can create one
/// or replace its origin or after-state; it is not an authoritative snapshot.
#[derive(Debug, Clone)]
pub(crate) struct PreparedImportBatch {
    origin: ObjectSnapshot,
    document: Option<ObjectDocument>,
}

impl PreparedImportBatch {
    /// False for the no-op batch (nothing to publish).
    pub(crate) fn changed(&self) -> bool {
        self.document.is_some()
    }
}

/// True when `candidate` is `origin` plus appended layers/records: same format, version, length and
/// untouched folders/holders, every origin layer and record preserved in order as a prefix.
fn derives_from(candidate: &ObjectDocument, origin: &ObjectDocument) -> bool {
    candidate.format == origin.format
        && candidate.format_version == origin.format_version
        && candidate.total_frames == origin.total_frames
        && candidate.property_holders == origin.property_holders
        && candidate.layer_folders == origin.layer_folders
        && candidate.layers.starts_with(&origin.layers)
        && candidate.objects.starts_with(&origin.objects)
}

impl ObjectFillOwner {
    /// Pins of a candidate against the owner's current head; the same order and codes as every
    /// other structural commit (`commit_layer_create`).
    fn check_import_pins(&self, origin: &ObjectSnapshot) -> Result<(), DispatchErrorCode> {
        let current = self.current_snapshot();
        if origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if origin.snapshot_id() != current.snapshot_id()
            || origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        Ok(())
    }

    /// Seals a planned after-document to the owner's current snapshot. `origin` must BE that
    /// snapshot; the complete after-document must be derived from it (an import only appends layers
    /// and records, so the origin is an unchanged prefix) and passes storage admission before
    /// anything can publish.
    pub(crate) fn seal_import_batch(
        &self,
        origin: ObjectSnapshot,
        document: Option<ObjectDocument>,
    ) -> Result<PreparedImportBatch, DispatchErrorCode> {
        self.check_import_pins(&origin)?;
        if let Some(candidate) = &document {
            if !derives_from(candidate, origin.document()) {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            ObjectSnapshot::validate_drawing(candidate)
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        }
        Ok(PreparedImportBatch { origin, document })
    }

    /// Publishes the whole batch if and only if the origin pins still match the current head.
    /// The no-op returns the unchanged head revision without publishing.
    pub(crate) fn commit_import_batch(
        &mut self,
        candidate: PreparedImportBatch,
    ) -> Result<u64, DispatchErrorCode> {
        self.check_import_pins(&candidate.origin)?;
        match candidate.document {
            None => Ok(self.content_revision()),
            Some(document) => self
                .revisions
                .publish(document)
                .map_err(|_| DispatchErrorCode::Unavailable),
        }
    }
}
