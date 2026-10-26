//! Immutable object revision storage. Admission and command policy live above it.
use crate::object_document::{ObjectDocument, ProjectEnvelope, ProjectReferenceState};
#[cfg(feature = "history")]
#[path = "project_asset_admission.rs"]
mod asset_admission;
#[cfg(feature = "history")]
#[path = "encoded_retention_budget.rs"]
mod encoded_retention_budget;
#[cfg(feature = "history")]
pub(crate) use encoded_retention_budget::Reservation as EncodedReservation;
#[cfg(feature = "history")]
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};
#[cfg(feature = "history")]
#[path = "object_project_settings_revision.rs"]
mod project_settings;
#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[path = "object_reference_revision.rs"]
mod reference_attachment;

#[cfg(all(feature = "history", feature = "project_media_codec"))]
#[path = "object_raster_image_import_revision.rs"]
mod image_import;

#[cfg(feature = "history")]
#[path = "native_import_transaction_assets_revision.rs"]
mod import_assets;

#[cfg(feature = "history")]
#[path = "object_selection_revision.rs"]
mod selection;

#[cfg(feature = "application")]
#[path = "tween_resource_revision.rs"]
mod tween_resources;

static NEXT_OBJECT_DOCUMENT: AtomicU64 = AtomicU64::new(1);
#[cfg(feature = "history")]
const MAX_CONTENT_REVISION: u64 = 9_007_199_254_740_991;

#[derive(Debug, PartialEq)]
enum ProjectRevisionValue {
    BareDrawing(ObjectDocument),
    Project(ProjectEnvelope),
}

#[derive(Debug, Clone)]
pub struct ObjectSnapshot {
    instance_id: String,
    document_id: String,
    snapshot_id: String,
    content_revision: u64,
    value: Arc<ProjectRevisionValue>,
}

impl ObjectSnapshot {
    #[cfg(all(test, feature = "history"))]
    pub(crate) fn encoded_test_budget(
        limit: usize,
    ) -> encoded_retention_budget::EncodedRetentionBudget {
        encoded_retention_budget::EncodedRetentionBudget::new(limit).unwrap()
    }
    #[cfg(feature = "history")]
    pub(crate) fn reserve_encoded(cost: usize) -> Result<EncodedReservation, &'static str> {
        encoded_retention_budget::shared()
            .reserve(cost)
            .map_err(|_| "encoded retention unavailable")
    }
    /// Called only after semantic admission by the snapshot layer.
    pub(crate) fn from_admitted(
        instance_id: String,
        document: ObjectDocument,
    ) -> Result<Self, &'static str> {
        Self::from_value(instance_id, ProjectRevisionValue::BareDrawing(document))
    }
    /// Storage seam only: the existing snapshot admission layer checked drawing.
    #[cfg(feature = "history")]
    pub(crate) fn from_admitted_project(
        instance_id: String,
        project: ProjectEnvelope,
    ) -> Result<Self, &'static str> {
        Self::project_entry(
            instance_id,
            project,
            encoded_retention_budget::shared(),
            Self::from_value,
        )
    }
    #[cfg(feature = "history")]
    fn project_entry(
        instance_id: String,
        project: ProjectEnvelope,
        budget: &encoded_retention_budget::EncodedRetentionBudget,
        identity: impl FnOnce(String, ProjectRevisionValue) -> Result<Self, &'static str>,
    ) -> Result<Self, &'static str> {
        if instance_id.is_empty() {
            return Err("instanceId must be nonempty");
        }
        let admissions = asset_admission::pending_charges(&project, budget)?;
        let snapshot = identity(instance_id, ProjectRevisionValue::Project(project))?;
        for admission in admissions {
            admission
                .commit()
                .map_err(|_| "encoded retention unavailable")?;
        }
        Ok(snapshot)
    }
    fn from_value(instance_id: String, value: ProjectRevisionValue) -> Result<Self, &'static str> {
        if instance_id.is_empty() {
            return Err("instanceId must be nonempty");
        }
        let sequence = NEXT_OBJECT_DOCUMENT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| "object document identity sequence exhausted")?;
        let document_id = format!("native-object-document-{sequence}");
        Ok(Self {
            instance_id,
            snapshot_id: format!("native-object:{document_id}:0"),
            document_id,
            content_revision: 0,
            value: Arc::new(value),
        })
    }
    pub fn instance_id(&self) -> &str {
        &self.instance_id
    }
    pub fn document_id(&self) -> &str {
        &self.document_id
    }
    pub fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }
    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }
    pub fn document(&self) -> &ObjectDocument {
        match self.value.as_ref() {
            ProjectRevisionValue::BareDrawing(drawing) => drawing,
            ProjectRevisionValue::Project(project) => project.drawing(),
        }
    }
    pub(crate) fn project(&self) -> Option<&ProjectEnvelope> {
        match self.value.as_ref() {
            ProjectRevisionValue::BareDrawing(_) => None,
            ProjectRevisionValue::Project(project) => Some(project),
        }
    }
}

#[derive(Debug)]
#[cfg(feature = "history")]
pub(crate) struct ObjectRevisionOwner {
    instance_id: String,
    document_id: String,
    content_revision: u64,
    snapshots: BTreeMap<u64, Arc<ProjectRevisionValue>>,
}

#[cfg(feature = "history")]
impl ObjectRevisionOwner {
    pub(crate) fn from_admitted(snapshot: ObjectSnapshot) -> Self {
        Self {
            instance_id: snapshot.instance_id,
            document_id: snapshot.document_id,
            content_revision: 0,
            snapshots: BTreeMap::from([(0, snapshot.value)]),
        }
    }
    pub(crate) fn instance_id(&self) -> &str {
        &self.instance_id
    }
    pub(crate) fn document_id(&self) -> &str {
        &self.document_id
    }
    pub(crate) fn content_revision(&self) -> u64 {
        self.content_revision
    }
    pub(crate) fn acquire(&self, revision: u64) -> Option<ObjectSnapshot> {
        self.snapshots.get(&revision).map(|value| ObjectSnapshot {
            instance_id: self.instance_id.clone(),
            document_id: self.document_id.clone(),
            snapshot_id: format!("native-object:{}:{revision}", self.document_id),
            content_revision: revision,
            value: Arc::clone(value),
        })
    }
    pub(crate) fn publish(&mut self, document: ObjectDocument) -> Result<u64, &'static str> {
        let current = self
            .acquire(self.content_revision)
            .ok_or("current revision absent")?;
        asset_admission::validate_drawing_only(current.document(), &document)?;
        let value = match current.value.as_ref() {
            ProjectRevisionValue::BareDrawing(_) => ProjectRevisionValue::BareDrawing(document),
            ProjectRevisionValue::Project(project) => {
                ProjectRevisionValue::Project(project.with_drawing(document))
            }
        };
        self.advance(Arc::new(value))
    }
    /// Existing allocation only. No caller can attach/replace through this seam.
    pub(crate) fn publish_reference(
        &mut self,
        reference: ProjectReferenceState,
    ) -> Result<u64, &'static str> {
        let current = self
            .snapshots
            .get(&self.content_revision)
            .ok_or("current revision absent")?;
        let ProjectRevisionValue::Project(project) = current.as_ref() else {
            return Err("bare reference unavailable");
        };
        if let ProjectReferenceState::StillImage(after) = &reference {
            let ProjectReferenceState::StillImage(before) = project.reference_state() else {
                return Err("attach resource admission unavailable");
            };
            if after.id() != before.id() || !std::ptr::eq(after.encoded_png(), before.encoded_png())
            {
                return Err("replacement resource admission unavailable");
            }
        }
        let next = project.with_reference(reference);
        self.advance(Arc::new(ProjectRevisionValue::Project(next)))
    }
    pub(crate) fn restore(&mut self, revision: u64) -> Result<u64, &'static str> {
        let document = self
            .snapshots
            .get(&revision)
            .ok_or("retained revision absent")?;
        self.advance(Arc::clone(document))
    }
    fn advance(&mut self, value: Arc<ProjectRevisionValue>) -> Result<u64, &'static str> {
        let next = self
            .content_revision
            .checked_add(1)
            .filter(|next| *next <= MAX_CONTENT_REVISION)
            .ok_or("contentRevision safe-integer limit exhausted")?;
        self.snapshots.insert(next, value);
        self.content_revision = next;
        Ok(next)
    }
}

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_revision_owner.rs"]
mod tests;
