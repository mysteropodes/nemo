//! Sealed placement edits through the existing revision owner (G42, I16b).
//!
//! The same shape as the project-settings transaction: [`prepare_placement_edit`]
//! reads the CURRENT immutable snapshot, runs the pure edit kernel against the
//! document that owns the targets and seals the next inventory to that exact
//! origin (instance, document, snapshot id and content revision);
//! [`commit_placement_edit`] re-checks the origin against the live owner and
//! publishes one revision. Nothing is published by preparing, so dropping a
//! candidate is a complete cancel. A no-op edit prepares a candidate that
//! commits to the current revision without a history entry, and committing the
//! same candidate twice is refused the second time (its origin is stale).
//!
//! There is no JSON command here: the public `object.project.*` command and its
//! request receipts belong to the application dispatch, which is not changed.
//!
//! [`prepare_placement_edit`]: ObjectFillOwner::prepare_placement_edit
//! [`commit_placement_edit`]: ObjectFillOwner::commit_placement_edit

use crate::commands::ObjectFillOwner;
use crate::object_document::media_timing::object_media_link_edit::{
    apply_edit, PlacementEdit, PlacementEditError, PlacementOutcome,
};
use crate::object_document::AssetInventory;
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

/// Only this module can create the candidate or replace its origin.
#[derive(Debug, Clone)]
pub(crate) struct PreparedPlacementEdit {
    origin: ObjectSnapshot,
    /// `None` is the no-op edit.
    next: Option<AssetInventory>,
}

fn code_of(error: PlacementEditError) -> DispatchErrorCode {
    match error {
        PlacementEditError::Refused(refusal) if refusal.unresolved() => DispatchErrorCode::NotFound,
        PlacementEditError::UnknownPlacement | PlacementEditError::UnknownAsset => {
            DispatchErrorCode::NotFound
        }
        PlacementEditError::Refused(_) | PlacementEditError::Limit => {
            DispatchErrorCode::Unavailable
        }
        PlacementEditError::Invalid
        | PlacementEditError::KindMismatch
        | PlacementEditError::Conflict
        | PlacementEditError::Overflow => DispatchErrorCode::InvalidRequest,
    }
}

impl ObjectFillOwner {
    pub(crate) fn prepare_placement_edit(
        &self,
        edit: PlacementEdit,
    ) -> Result<PreparedPlacementEdit, DispatchErrorCode> {
        let origin = self.current_snapshot();
        let project = origin.project().ok_or(DispatchErrorCode::Unavailable)?;
        let inventory = project
            .asset_inventory()
            .ok_or(DispatchErrorCode::Unavailable)?;
        let next = match apply_edit(inventory, project.drawing(), edit).map_err(code_of)? {
            PlacementOutcome::Unchanged => None,
            PlacementOutcome::Changed(next) => Some(next),
        };
        Ok(PreparedPlacementEdit { origin, next })
    }

    pub(crate) fn commit_placement_edit(
        &mut self,
        candidate: PreparedPlacementEdit,
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
        let Some(inventory) = candidate.next else {
            return Ok(current.content_revision());
        };
        self.revisions
            .publish_asset_placements(inventory)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}
