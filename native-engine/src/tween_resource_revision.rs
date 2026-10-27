//! Only a sealed, fully baked tween candidate reaches the one native project revision store.
use crate::object_document::ProjectEnvelope;
use crate::revision::object_revision::{
    asset_admission, encoded_retention_budget, ObjectRevisionOwner, ProjectRevisionValue,
    MAX_CONTENT_REVISION,
};
use std::sync::Arc;
impl ObjectRevisionOwner {
    pub(crate) fn publish_tween_resources(
        &mut self,
        project: ProjectEnvelope,
    ) -> Result<u64, &'static str> {
        self.content_revision
            .checked_add(1)
            .filter(|r| *r <= MAX_CONTENT_REVISION)
            .ok_or("contentRevision safe-integer limit exhausted")?;
        let admissions =
            asset_admission::pending_charges(&project, encoded_retention_budget::shared())?;
        encoded_retention_budget::Admission::commit_all(admissions)
            .map_err(|_| "tween resource retention unavailable")?;
        // Revision exhaustion was checked above. No owner mutation intervenes,
        // so advance cannot refuse after the all-or-nothing retention commit.
        self.advance(Arc::new(ProjectRevisionValue::Project(project)))
    }
}
