//! Selection-only publication of a lower-document-layer sealed plan.
//! Ordinary drawing/import publishers retain their compound-brush guards.
use crate::object_document::object_group_kernels::object_selection_compound::validate_document;
use crate::object_document::object_group_kernels::object_selection_edit_plan::SelectionPublication;
use crate::object_document::item_asset_issue;
use crate::revision::object_revision::{ObjectRevisionOwner, ObjectSnapshot, ProjectRevisionValue};
use std::sync::Arc;

impl ObjectRevisionOwner {
    pub(crate) fn publish_selection(
        &mut self,
        origin: &ObjectSnapshot,
        proof: SelectionPublication,
    ) -> Result<u64, &'static str> {
        let current = self.acquire(self.content_revision).ok_or("current revision absent")?;
        if origin.instance_id() != current.instance_id()
            || origin.document_id() != current.document_id()
            || origin.snapshot_id() != current.snapshot_id()
            || origin.content_revision() != current.content_revision()
            || !Arc::ptr_eq(&origin.value, &current.value)
            || !proof.matches(current.document(), proof.after()) {
            return Err("selection origin changed");
        }
        validate_document(current.document()).map_err(|_| "invalid original companion")?;
        validate_document(proof.after()).map_err(|_| "invalid selection companion")?;
        let inventory = current.project().and_then(|project| project.asset_inventory());
        if item_asset_issue(proof.after(), inventory).is_some() {
            return Err("selection asset reference unavailable");
        }
        // No new asset, decode, retention ticket or resource allocation. This retains
        // the exact project envelope resources/metadata and replaces only its drawing.
        let document = proof.into_document();
        let value = match current.value.as_ref() {
            ProjectRevisionValue::BareDrawing(_) => ProjectRevisionValue::BareDrawing(document),
            ProjectRevisionValue::Project(project) => ProjectRevisionValue::Project(project.with_drawing(document)),
        };
        self.advance(Arc::new(value))
    }
}
