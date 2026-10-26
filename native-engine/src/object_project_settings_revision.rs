//! Project settings and Repeat's compound publication use the existing revision owner.
use crate::object_document::{ObjectDocument, ProjectMetadata};
use crate::revision::object_revision::{
    asset_admission, ObjectRevisionOwner, ProjectRevisionValue,
};
use std::sync::Arc;

// L09-H (#2115): placement-only publication (G42, I16b); nested here so the
// private revision store stays visible and object_revision.rs does not grow.
#[path = "object_media_link_revision.rs"]
#[allow(dead_code)]
mod object_media_link_revision;

impl ObjectRevisionOwner {
    /// Repeat owns one drawing edit and, only when the timeline grows, its work-area end.
    /// Publish both in the existing immutable envelope so one history entry restores both.
    pub(crate) fn publish_frame_repeat(
        &mut self,
        document: ObjectDocument,
        metadata: Option<ProjectMetadata>,
    ) -> Result<u64, &'static str> {
        let Some(metadata) = metadata else {
            return self.publish(document);
        };
        let current = self
            .acquire(self.content_revision)
            .ok_or("current revision absent")?;
        let project = current
            .project()
            .ok_or("bare project settings unavailable")?;
        // Keep the same resource/source guard as every drawing-only publication.
        asset_admission::validate_drawing_only(current.document(), &document)?;
        // The sealed command candidate already admitted this complete pair.
        let next = project.with_drawing(document).with_metadata(metadata);
        self.advance(Arc::new(ProjectRevisionValue::Project(next)))
    }

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
