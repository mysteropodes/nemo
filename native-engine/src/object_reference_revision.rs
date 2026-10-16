//! Publish an already-admitted reference in the existing project revision store.
use crate::object_document::ProjectReferenceState;
use crate::revision::object_revision::{
    encoded_retention_budget, ObjectRevisionOwner, ProjectRevisionValue,
};
use std::sync::Arc;

impl ObjectRevisionOwner {
    #[cfg(feature = "font_asset_wire")]
    pub(crate) fn publish_admitted_font(
        &mut self,
        record: Arc<crate::object_document::AssetRecord>,
    ) -> Result<u64, &'static str> {
        use crate::object_document::{AssetInventory, AssetKind};
        if record.kind() != AssetKind::Font
            || !record.source().has_font_admission()
            || record.source().media_type() != Some("font/ttf")
            || record
                .source()
                .encoded_allocation()
                .is_none_or(|a| a.is_empty() || a.capacity() > 16 * 1024 * 1024)
        {
            return Err("font asset admission unavailable");
        }
        let current = self
            .snapshots
            .get(&self.content_revision)
            .ok_or("current revision absent")?;
        let ProjectRevisionValue::Project(project) = current.as_ref() else {
            return Err("font import requires a project");
        };
        let old = project.asset_inventory();
        if old.is_some_and(|a| {
            !a.placements().is_empty()
                || a.records().len() >= 1024
                || a.records().iter().any(|r| r.id() == record.id())
        }) {
            return Err("font catalog unavailable");
        }
        let mut records = old.map(|a| a.records().to_vec()).unwrap_or_default();
        records
            .try_reserve_exact(1)
            .map_err(|_| "font catalog allocation unavailable")?;
        records.push(record);
        let inventory = AssetInventory::new(records, Vec::new(), 1024, 0)
            .map_err(|_| "font catalog unavailable")?;
        let next = project.with_asset_inventory(Some(inventory));
        let tickets =
            super::asset_admission::pending_charges(&next, &encoded_retention_budget::shared())?;
        for ticket in tickets {
            ticket
                .commit()
                .map_err(|_| "encoded retention unavailable")?;
        }
        self.advance(Arc::new(ProjectRevisionValue::Project(next)))
    }
    /// The command owner has checked the sealed admission's full origin pins.
    /// Existing settings/remove still use their narrower storage port.
    pub(crate) fn publish_admitted_reference(
        &mut self,
        reference: ProjectReferenceState,
    ) -> Result<u64, &'static str> {
        let ProjectReferenceState::StillImage(data) = &reference else {
            return Err("attachment must contain admitted media");
        };
        let current = self
            .snapshots
            .get(&self.content_revision)
            .ok_or("current revision absent")?;
        let ProjectRevisionValue::Project(project) = current.as_ref() else {
            return Err("bare reference unavailable");
        };
        // An already-bound producer allocation shares its existing charge.
        // Commit the retention ticket before publishing any document effect.
        let admission = encoded_retention_budget::shared()
            .admit(data.encoded_allocation())
            .map_err(|_| "encoded retention unavailable")?;
        let next = project.with_reference(reference);
        admission
            .commit()
            .map_err(|_| "encoded retention unavailable")?;
        self.advance(Arc::new(ProjectRevisionValue::Project(next)))
    }
}
