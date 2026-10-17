//! Metadata publication preserves the current drawing and every resource allocation.
use crate::object_document::ProjectMetadata;
use crate::revision::object_revision::{ObjectRevisionOwner, ProjectRevisionValue};
use std::sync::Arc;

// L09-H (#2115): placement-only publication (G42, I16b); nested here so the
// private revision store stays visible and object_revision.rs does not grow.
#[path = "object_media_link_revision.rs"]
#[allow(dead_code)]
mod object_media_link_revision;

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
