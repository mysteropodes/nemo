//! Sealed commit of a prepared asset attach / remove (L06-H, G01 / G02 / G27): the
//! command-owner half of the transaction.
//!
//! A `PreparedAssetEdit` pins the immutable snapshot it was built from. Here the
//! owner compares those pins with its own current snapshot - instance, document,
//! snapshot id and content revision - and only then asks the revision store to
//! publish. A prepared edit from another instance, another document incarnation or
//! an older revision therefore cannot publish, and an `Unchanged` edit (exact
//! duplicate, removal of an absent id) returns the current revision without
//! touching the store.
//!
//! The pin comparison is a separate pure function so it has one definition that
//! the tests and the mutation controls drive directly.
use crate::commands::ObjectFillOwner;
use crate::object_snapshot::{AssetEdit, PreparedAssetEdit};
use crate::request_receipts::DispatchErrorCode;
use crate::revision::ObjectSnapshot;

/// `None` when `origin` is exactly the owner's `current` snapshot.
pub(crate) fn origin_refusal(
    origin: &ObjectSnapshot,
    current: &ObjectSnapshot,
) -> Option<DispatchErrorCode> {
    if origin.instance_id() != current.instance_id() {
        return Some(DispatchErrorCode::WrongInstance);
    }
    if origin.document_id() != current.document_id() {
        return Some(DispatchErrorCode::WrongDocument);
    }
    if origin.snapshot_id() != current.snapshot_id()
        || origin.content_revision() != current.content_revision()
    {
        return Some(DispatchErrorCode::StaleRevision);
    }
    None
}

impl ObjectFillOwner {
    pub(crate) fn commit_asset_edit(
        &mut self,
        candidate: PreparedAssetEdit,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if let Some(code) = origin_refusal(candidate.origin(), &current) {
            return Err(code);
        }
        match candidate.into_edit() {
            AssetEdit::Unchanged => Ok(self.content_revision()),
            AssetEdit::Add(record) => self
                .revisions
                .publish_admitted_asset(record)
                .map_err(|_| DispatchErrorCode::Unavailable),
            AssetEdit::Remove(id) => self
                .revisions
                .publish_asset_removal(&id)
                .map_err(|_| DispatchErrorCode::Unavailable),
        }
    }
}

#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/object_raster_asset_attach_owner.rs"]
mod object_raster_asset_attach_owner;
