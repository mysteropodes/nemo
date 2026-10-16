//! Metadata publication preserves the current drawing and every resource allocation.
use crate::object_document::ProjectMetadata;
use crate::revision::object_revision::{ObjectRevisionOwner, ProjectRevisionValue};
use std::sync::Arc;

impl ObjectRevisionOwner {
    pub(crate) fn publish_project_settings(
        &mut self,
        metadata: ProjectMetadata,
    ) -> Result<u64, &'static str> {
        let current = self
            .snapshots
            .get(&self.content_revision)
            .ok_or("current revision absent")?;
        let ProjectRevisionValue::Project(project) = current.as_ref() else {
            return Err("bare project settings unavailable");
        };
        self.advance(Arc::new(ProjectRevisionValue::Project(
            project.with_metadata(metadata),
        )))
    }
}
