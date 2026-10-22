//! Sealed import of drawing and retained resources through the existing owner.
use crate::commands::object_fill_owner::import_batch::{derives_from, PreparedImportBatch};
use crate::commands::ObjectFillOwner;
use crate::object_document::{AssetInventory, AssetKind, AssetRecord, ObjectDocument, Presence};
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
    /// Validate retained provider identity before the history planner remaps it.
    pub(crate) fn validate_import_asset(
        origin: &ObjectSnapshot,
        record: &AssetRecord,
    ) -> Result<(), DispatchErrorCode> {
        match record.kind() {
            AssetKind::Image => {
                #[cfg(feature = "project_media_codec")]
                {
                    let source = record.source();
                    let bytes = source.encoded_allocation().ok_or_else(invalid)?;
                    if source.media_type() != Some("image/png") {
                        return Err(invalid());
                    }
                    origin.validate_import_png(bytes)
                }
                #[cfg(not(feature = "project_media_codec"))]
                {
                    let _ = origin;
                    Err(DispatchErrorCode::Unavailable)
                }
            }
            AssetKind::Font => {
                if record.source().has_font_admission() {
                    Ok(())
                } else {
                    Err(DispatchErrorCode::Unavailable)
                }
            }
            _ => Err(invalid()),
        }
    }

    /// Prepare one real provider record without attaching or publishing it.
    #[cfg(feature = "project_media_codec")]
    pub(crate) fn prepare_import_asset_source(
        origin: &ObjectSnapshot,
        id: &str,
        name: &str,
        media_type: &str,
        bytes: &[u8],
    ) -> Result<Arc<AssetRecord>, DispatchErrorCode> {
        let project = origin.project().ok_or(DispatchErrorCode::Unavailable)?;
        match media_type {
            "image/png" => {
                let prepared = origin
                    .prepare_asset_attachment(id, name, media_type, bytes)
                    .map_err(|error| error.code())?;
                match prepared.into_edit() {
                    crate::object_snapshot::AssetEdit::Add(record) => Ok(record),
                    crate::object_snapshot::AssetEdit::Unchanged => project
                        .asset_inventory()
                        .and_then(|catalog| {
                            catalog.records().iter().find(|record| {
                                record.kind() == AssetKind::Image
                                    && record.source().media_type() == Some("image/png")
                                    && record
                                        .source()
                                        .encoded_allocation()
                                        .is_some_and(|held| held.as_slice() == bytes)
                            })
                        })
                        .cloned()
                        .ok_or(DispatchErrorCode::Internal),
                    crate::object_snapshot::AssetEdit::Remove(_) => {
                        Err(DispatchErrorCode::Internal)
                    }
                }
            }
            "font/ttf" => {
                #[cfg(feature = "font_asset_wire")]
                {
                    origin.admit_font_asset(format!("asset-{id}"), name.into(), bytes)
                }
                #[cfg(not(feature = "font_asset_wire"))]
                {
                    Err(DispatchErrorCode::Unavailable)
                }
            }
            _ => Err(DispatchErrorCode::Unavailable),
        }
    }

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
            if crate::object_document::item_asset_issue(&document, inventory.as_ref()).is_some() {
                return Err(invalid());
            }
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
