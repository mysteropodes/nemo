//! Placement-only publication in the existing project revision store (G42).

use crate::object_document::{AssetInventory, AssetKind, ProjectEnvelope};
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

    /// T12 linked video: the prepared project keeps every current record allocation in order
    /// and appends at most one linked video record. A linked source owns no encoded bytes,
    /// so no retention ticket exists; one advance publishes record, placement and layer.
    pub(crate) fn publish_linked_video(
        &mut self,
        project: ProjectEnvelope,
    ) -> Result<u64, &'static str> {
        let current = self
            .snapshots
            .get(&self.content_revision)
            .ok_or("current revision absent")?;
        let ProjectRevisionValue::Project(before) = current.as_ref() else {
            return Err("bare project video link unavailable");
        };
        let before = before.asset_inventory().map_or(&[][..], |a| a.records());
        let after = project
            .asset_inventory()
            .ok_or("asset catalog absent")?
            .records();
        let kept = after.len() >= before.len()
            && after.len() <= before.len() + 1
            && before.iter().zip(after).all(|(a, b)| Arc::ptr_eq(a, b));
        let added_linked = after[before.len().min(after.len())..].iter().all(|record| {
            record.kind() == AssetKind::Video
                && record.source().locator().is_some()
                && record.source().encoded_allocation().is_none()
        });
        if !kept || !added_linked {
            return Err("video link can only append one linked video record");
        }
        self.advance(Arc::new(ProjectRevisionValue::Project(project)))
    }
}
