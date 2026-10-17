//! Publish an admitted embedded image asset, or remove one, in the existing
//! project revision store (L06-H, G01 / G02 / G27).
//!
//! This is the revision-layer half of the asset attach/remove transaction. The
//! command owner has already checked the sealed preparation's full origin pins
//! (instance, document, snapshot id, content revision); this file only turns an
//! admitted value into the next immutable revision, in the order that makes every
//! failure leave revision, budget and inventory untouched:
//! encoded-retention ticket (`admit`) -> next inventory -> `commit` -> `advance`.
//!
//! Nothing here decodes, hashes, names or deduplicates: admission policy is the
//! document-layer kernel (`object_raster_asset_admission`), the byte reservation is
//! the preparation (`object_raster_asset_prepare`). The existing records keep their
//! `Arc` identity across the new revision, so undo/redo restore them untouched.
//! The `_in` entry takes the budget explicitly so a test can measure a private
//! ledger; the production entry always passes the shared one.
use crate::object_document::object_raster_asset_kernels::object_raster_asset_use::asset_in_use;
use crate::object_document::{AssetId, AssetInventory, AssetKind, AssetRecord};
use crate::revision::object_revision::{
    encoded_retention_budget, ObjectRevisionOwner, ProjectRevisionValue,
};
use std::sync::Arc;

const MAX_ASSETS: usize = 1024;
const MAX_ENCODED_BYTES: usize = 16 * 1024 * 1024;

impl ObjectRevisionOwner {
    /// Append `record` to the project's asset catalog in the shared ledger.
    pub(crate) fn publish_admitted_asset(
        &mut self,
        record: Arc<AssetRecord>,
    ) -> Result<u64, &'static str> {
        self.publish_admitted_asset_in(record, encoded_retention_budget::shared())
    }

    pub(crate) fn publish_admitted_asset_in(
        &mut self,
        record: Arc<AssetRecord>,
        budget: &encoded_retention_budget::EncodedRetentionBudget,
    ) -> Result<u64, &'static str> {
        let allocation = record
            .source()
            .encoded_allocation()
            .filter(|bytes| !bytes.is_empty() && bytes.capacity() <= MAX_ENCODED_BYTES)
            .ok_or("asset admission unavailable")?;
        if record.kind() != AssetKind::Image || record.source().media_type() != Some("image/png") {
            return Err("asset admission unavailable");
        }
        let current = self
            .snapshots
            .get(&self.content_revision)
            .ok_or("current revision absent")?;
        let ProjectRevisionValue::Project(project) = current.as_ref() else {
            return Err("asset attach requires a project");
        };
        let old = project.asset_inventory();
        if old.is_some_and(|assets| {
            assets.records().len() >= MAX_ASSETS || assets.get(record.id()).is_some()
        }) {
            return Err("asset catalog unavailable");
        }
        // An allocation bound by the preparation shares its existing charge.
        // Commit the ticket only after the next value exists, before any effect.
        let admission = budget
            .admit(allocation)
            .map_err(|_| "encoded retention unavailable")?;
        let mut records = old.map(|a| a.records().to_vec()).unwrap_or_default();
        records
            .try_reserve_exact(1)
            .map_err(|_| "asset catalog allocation unavailable")?;
        records.push(Arc::clone(&record));
        let placements = old.map(|a| a.placements().to_vec()).unwrap_or_default();
        let placement_limit = placements.len();
        let inventory = AssetInventory::new(records, placements, MAX_ASSETS, placement_limit)
            .map_err(|_| "asset catalog unavailable")?;
        let next = project.with_asset_inventory(Some(inventory));
        admission
            .commit()
            .map_err(|_| "encoded retention unavailable")?;
        self.advance(Arc::new(ProjectRevisionValue::Project(next)))
    }

    /// Drop the asset `id` from the catalog. The removed record's bytes stay
    /// charged while any retained revision or snapshot still pins them.
    pub(crate) fn publish_asset_removal(&mut self, id: &AssetId) -> Result<u64, &'static str> {
        let current = self
            .snapshots
            .get(&self.content_revision)
            .ok_or("current revision absent")?;
        let ProjectRevisionValue::Project(project) = current.as_ref() else {
            return Err("asset removal requires a project");
        };
        let old = project.asset_inventory().ok_or("asset absent")?;
        let position = old
            .records()
            .iter()
            .position(|record| record.id() == id)
            .ok_or("asset absent")?;
        if old.records()[position].kind() != AssetKind::Image {
            return Err("asset kind unavailable");
        }
        if asset_in_use(project, id) {
            return Err("asset in use");
        }
        let mut records = old.records().to_vec();
        records.remove(position);
        let placements = old.placements().to_vec();
        let placement_limit = placements.len();
        let inventory = AssetInventory::new(records, placements, MAX_ASSETS, placement_limit)
            .map_err(|_| "asset catalog unavailable")?;
        let next = project.with_asset_inventory(Some(inventory));
        self.advance(Arc::new(ProjectRevisionValue::Project(next)))
    }
}

#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/object_raster_asset_attach.rs"]
mod object_raster_asset_attach;
#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/object_raster_asset_attach_controls.rs"]
mod object_raster_asset_attach_controls;
