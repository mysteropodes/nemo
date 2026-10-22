//! Batch publication uses the existing revision store and encoded-retention ledger.
use crate::object_document::ProjectEnvelope;
use crate::revision::object_revision::{
    asset_admission, encoded_retention_budget, ObjectRevisionOwner, ProjectRevisionValue,
    MAX_CONTENT_REVISION,
};
use std::sync::Arc;

impl ObjectRevisionOwner {
    pub(crate) fn publish_import_project(
        &mut self,
        project: ProjectEnvelope,
    ) -> Result<u64, &'static str> {
        let current = self
            .acquire(self.content_revision)
            .ok_or("current revision absent")?;
        // A project/item import is not a brush rebake transaction. Preserve the
        // same compound refusal as every ordinary drawing publication, before
        // obtaining any retained-resource ticket or advancing the revision.
        asset_admission::validate_drawing_only(current.document(), project.drawing())?;
        // Fail before resource retention if this store cannot advance. The same
        // sole mutable owner performs the checked advance below.
        self.content_revision
            .checked_add(1)
            .filter(|next| *next <= MAX_CONTENT_REVISION)
            .ok_or("contentRevision safe-integer limit exhausted")?;
        // This is the ordinary project admission: references, PNG/font proof and
        // every encoded allocation share the same existing resource domain.
        // If any admission fails, earlier uncommitted tickets drop together.
        let admissions =
            asset_admission::pending_charges(&project, encoded_retention_budget::shared())?;
        for admission in admissions {
            admission
                .commit()
                .map_err(|_| "import retention unavailable")?;
        }
        self.advance(Arc::new(ProjectRevisionValue::Project(project)))
    }
}
