//! Placement-only publication in the existing project revision store (G42).

use crate::object_document::AssetInventory;
use crate::revision::object_revision::{ObjectRevisionOwner, ProjectRevisionValue};
use std::sync::Arc;

impl ObjectRevisionOwner {
    /// The command owner has checked the sealed candidate's full origin pins.
    /// The catalog RECORDS must be the very same allocations in the same order:
    /// a placement edit cannot add, drop or swap an asset, so no encoded
    /// retention ticket is created or released here.
    pub(crate) fn publish_asset_placements(
        &mut self,
        inventory: AssetInventory,
    ) -> Result<u64, &'static str> {
        let current = self
            .snapshots
            .get(&self.content_revision)
            .ok_or("current revision absent")?;
        let ProjectRevisionValue::Project(project) = current.as_ref() else {
            return Err("bare project placements unavailable");
        };
        let before = project.asset_inventory().ok_or("asset catalog absent")?;
        let same_records = before.records().len() == inventory.records().len()
            && before
                .records()
                .iter()
                .zip(inventory.records())
                .all(|(a, b)| Arc::ptr_eq(a, b));
        if !same_records {
            return Err("placement edit cannot change the catalog records");
        }
        let next = project.with_asset_inventory(Some(inventory));
        self.advance(Arc::new(ProjectRevisionValue::Project(next)))
    }
}
