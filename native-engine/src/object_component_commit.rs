//! Sealed replacement of the lane-4 tables through the existing revision owner
//! (L04-I, I14/I35). Declared as a private child of `object_fill_owner.rs`,
//! next to the property-edit and layer-lifecycle preparations it mirrors.
//!
//! This is the persistence primitive the later setup / convert transactions
//! (L04-M) build their tables for; it is not a public operation: no wire
//! command string, request receipt or host route is added here. Preparation
//! reads the owner's current immutable snapshot, applies the typed edit to a
//! detached clone, runs the same storage admission every other edit runs
//! (including the component codec checks) and resolves StoryBoard sounds in
//! the project inventory, then seals the result to that snapshot. Commit
//! re-checks instance, document and revision exactly like `commit_property_edit`
//! and publishes nothing for an edit that changes nothing, so a no-op keeps the
//! presence, the serialized bytes, the history depth and the redo stack.
use crate::commands::ObjectFillOwner;
use crate::object_document::{
    resolve_sound_assets, ComponentTables, ObjectDocument, Presence, SoundAssetError,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

/// What a caller may do to the tables member; whole-member granularity only.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ComponentTablesEdit {
    /// Author exactly these tables (v2 documents only).
    Replace(ComponentTables),
    /// Make the member absent again.
    Remove,
}

/// A detached candidate sealed to the snapshot it was prepared from. Only this
/// module can create one or replace its origin or after-state.
#[derive(Debug, Clone)]
pub(crate) struct PreparedComponentTables {
    origin: ObjectSnapshot,
    document: ObjectDocument,
    changed: bool,
}

impl PreparedComponentTables {
    /// False when the candidate equals its origin; committing it publishes nothing.
    pub(crate) fn changed(&self) -> bool {
        self.changed
    }
    /// The admitted after-state (read only).
    pub(crate) fn document(&self) -> &ObjectDocument {
        &self.document
    }
}

impl ObjectFillOwner {
    pub(crate) fn prepare_component_tables(
        &self,
        edit: ComponentTablesEdit,
    ) -> Result<PreparedComponentTables, DispatchErrorCode> {
        let origin = self.current_snapshot();
        let mut document = origin.document().clone();
        document.component_tables = match edit {
            ComponentTablesEdit::Remove => Presence::Missing,
            ComponentTablesEdit::Replace(tables) => {
                // A v1 document cannot name a component context at all.
                if document.format_version() != 2 {
                    return Err(DispatchErrorCode::Unavailable);
                }
                Presence::Present(tables)
            }
        };
        ObjectSnapshot::validate_drawing(&document)
            .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let inventory = origin
            .project()
            .and_then(|project| project.asset_inventory());
        resolve_sound_assets(&document, inventory).map_err(|error| match error {
            SoundAssetError::Unresolved => DispatchErrorCode::NotFound,
            SoundAssetError::NotAudio => DispatchErrorCode::InvalidRequest,
        })?;
        let changed = &document != origin.document();
        Ok(PreparedComponentTables {
            origin,
            document,
            changed,
        })
    }
    pub(crate) fn commit_component_tables(
        &mut self,
        candidate: PreparedComponentTables,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        if !candidate.changed {
            return Ok(current.content_revision());
        }
        self.revisions
            .publish(candidate.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}
