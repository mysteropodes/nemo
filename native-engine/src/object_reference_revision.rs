//! Publish an already-admitted reference in the existing project revision store.
use crate::object_document::ProjectReferenceState;
use crate::revision::object_revision::{
    encoded_retention_budget, ObjectRevisionOwner, ProjectRevisionValue,
};
use std::sync::Arc;

impl ObjectRevisionOwner {
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
