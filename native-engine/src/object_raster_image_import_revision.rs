//! The one existing revision store publishes the prepared image resource and drawing together.
use crate::object_document::{AssetRecord, ProjectEnvelope};
use crate::revision::object_revision::{
    encoded_retention_budget, ObjectRevisionOwner, ProjectRevisionValue,
};
use std::sync::Arc;

impl ObjectRevisionOwner {
    pub(crate) fn publish_image_import(
        &mut self,
        project: ProjectEnvelope,
        added: Option<Arc<AssetRecord>>,
    ) -> Result<u64, &'static str> {
        let ticket = if let Some(record) = &added {
            let bytes = record
                .source()
                .encoded_allocation()
                .ok_or("image resource absent")?;
            Some(
                encoded_retention_budget::shared()
                    .admit(bytes)
                    .map_err(|_| "image retention unavailable")?,
            )
        } else {
            None
        };
        if let Some(ticket) = ticket {
            ticket.commit().map_err(|_| "image retention unavailable")?;
        }
        self.advance(Arc::new(ProjectRevisionValue::Project(project)))
    }
}
