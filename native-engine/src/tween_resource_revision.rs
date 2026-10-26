//! Only a sealed, fully baked tween candidate reaches the one native project revision store.
use super::{
    asset_admission, encoded_retention_budget, ObjectRevisionOwner, ProjectRevisionValue,
    MAX_CONTENT_REVISION,
};
use crate::tween_kernels::PreparedTweenResources;
use std::sync::Arc;
impl ObjectRevisionOwner {
    pub(crate) fn publish_tween_resources(
        &mut self,
        prepared: PreparedTweenResources,
    ) -> Result<u64, &'static str> {
        self.content_revision
            .checked_add(1)
            .filter(|r| *r <= MAX_CONTENT_REVISION)
            .ok_or("contentRevision safe-integer limit exhausted")?;
        let project = prepared.into_project();
        let admissions =
            asset_admission::pending_charges(&project, encoded_retention_budget::shared())?;
        for admission in admissions {
            admission
                .commit()
                .map_err(|_| "tween resource retention unavailable")?;
        }
        self.advance(Arc::new(ProjectRevisionValue::Project(project)))
    }
}
