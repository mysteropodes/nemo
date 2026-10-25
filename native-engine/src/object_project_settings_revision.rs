//! Project settings and Repeat's compound publication use the existing revision owner.
use crate::object_document::{ObjectDocument, ProjectMetadata};
use crate::request_receipts::DispatchErrorCode;
use crate::revision::object_revision::{ObjectRevisionOwner, ProjectRevisionValue};
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
    ) -> Result<u64, DispatchErrorCode> {
        let unavailable = |_| DispatchErrorCode::Unavailable;
        let current = self
            .acquire(self.content_revision)
            .ok_or(DispatchErrorCode::Unavailable)?;
        let Some(project) = current.project() else {
            return self.publish(document).map_err(unavailable);
        };
        if document.total_frames <= current.document().total_frames {
            return self.publish(document).map_err(unavailable);
        }
        // Keep the same resource/source guard as every drawing-only publication.
        super::asset_admission::validate_drawing_only(current.document(), &document)
            .map_err(unavailable)?;
        let last = f64::from(document.total_frames - 1);
        let (wa_in, wa_out) = project.project().work_area();
        let effective_out = wa_out.unwrap_or(f64::from(current.document().total_frames - 1));
        let metadata = if effective_out < last {
            project
                .project()
                .clone()
                .with_work_area(wa_in, Some(last))
                .map_err(|_| DispatchErrorCode::InvalidRequest)?
        } else {
            project.project().clone()
        };
        let next = project.with_drawing(document).with_metadata(metadata);
        #[cfg(feature = "project_media_codec")]
        crate::object_snapshot::ObjectSnapshot::check_candidate_saveable(&next)?;
        self.advance(Arc::new(ProjectRevisionValue::Project(next)))
            .map_err(unavailable)
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
