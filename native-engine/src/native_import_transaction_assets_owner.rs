//! Sealed import of drawing and retained resources through the existing owner.
use super::{derives_from, PreparedImportBatch};
use crate::commands::ObjectFillOwner;
use crate::object_document::{AssetInventory, ObjectDocument, Presence};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use std::sync::Arc;

fn invalid() -> DispatchErrorCode {
    DispatchErrorCode::InvalidRequest
}

fn order_preserved(after: &ObjectDocument, before: &ObjectDocument) -> bool {
    match (&after.composition_order, &before.composition_order) {
        (Presence::Missing, Presence::Missing) => true,
        (Presence::Present(after), Presence::Present(before)) => {
            after.format == before.format
                && after.format_version == before.format_version
                && after.document_format_version == before.document_format_version
                && after.migration == before.migration
                && after.reference_policy == before.reference_policy
                && after.contexts.len() == before.contexts.len()
                && after
                    .contexts
                    .iter()
                    .zip(&before.contexts)
                    .all(|(after, before)| {
                        after.context_id == before.context_id
                            && after.layers.starts_with(&before.layers)
                            && before.scopes.iter().all(|scope| {
                                after
                                    .scopes
                                    .iter()
                                    .find(|next| next.frame_scope == scope.frame_scope)
                                    .is_some_and(|next| next.elements.starts_with(&scope.elements))
                            })
                    })
        }
        _ => false,
    }
}

// Imports append actual items; they cannot erase or replace the origin's records.
fn items_preserved(after: &ObjectDocument, before: &ObjectDocument) -> bool {
    if after.image_meshes != before.image_meshes
        || after.component_tables != before.component_tables
    {
        return false;
    }
    match (&after.items, &before.items) {
        (_, Presence::Missing) => true,
        (Presence::Present(after), Presence::Present(before)) => {
            after.raster_entries().starts_with(before.raster_entries())
                && after.text_entries().starts_with(before.text_entries())
        }
        _ => false,
    }
}

impl ObjectFillOwner {
    pub(crate) fn seal_import_batch_with_assets(
        &self,
        origin: ObjectSnapshot,
        document: Option<ObjectDocument>,
        inventory: Option<AssetInventory>,
    ) -> Result<PreparedImportBatch, DispatchErrorCode> {
        if inventory.is_none()
            && document
                .as_ref()
                .is_none_or(|after| after.items == origin.document().items)
        {
            return self.seal_import_batch(origin, document);
        }
        #[cfg(not(feature = "project_media_codec"))]
        {
            let _ = (origin, document, inventory);
            Err(DispatchErrorCode::Unavailable)
        }
        #[cfg(feature = "project_media_codec")]
        {
            self.check_import_pins(&origin)?;
            let current = origin.project().ok_or(DispatchErrorCode::Unavailable)?;
            let document = document.ok_or_else(invalid)?;
            if !derives_from(&document, origin.document())
                || !items_preserved(&document, origin.document())
                || !order_preserved(&document, origin.document())
            {
                return Err(invalid());
            }
            let inventory = inventory.or_else(|| current.asset_inventory().cloned());
            if let Some(inventory) = &inventory {
                if let Some(before) = current.asset_inventory() {
                    if inventory.records().len() < before.records().len()
                        || !inventory
                            .records()
                            .iter()
                            .zip(before.records())
                            .all(|(after, before)| Arc::ptr_eq(after, before))
                        || inventory.placements() != before.placements()
                    {
                        return Err(invalid());
                    }
                } else if !inventory.placements().is_empty() {
                    return Err(invalid());
                }
            }
            ObjectSnapshot::validate_drawing(&document).map_err(|_| invalid())?;
            crate::object_codec::validate_item_assets(&document, inventory.as_ref())
                .map_err(|_| invalid())?;
            let project = current
                .with_drawing(document.clone())
                .with_asset_inventory(inventory);
            // Aggregate admission matters: individually saveable inputs may exceed
            // the existing envelope or asset-count limit when imported together.
            ObjectSnapshot::check_candidate_saveable(&project)?;
            // Resource admission and all quota tickets are obtained by the existing
            // revision owner at commit, before the single project publication.
            Ok(PreparedImportBatch {
                origin,
                document: Some(document),
                project: Some(project),
            })
        }
    }
}
