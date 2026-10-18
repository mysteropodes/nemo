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
            .snapshots
            .get(&self.content_revision)
            .ok_or("current revision absent")?;
        let value = match current.as_ref() {
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
mod tests {
    use super::*;
    fn quota_project(bytes: Vec<u8>) -> ProjectEnvelope {
        let drawing: ObjectDocument = serde_json::from_value(serde_json::json!({
            "format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,
            "layers":[{"layerUid":"L"}],"objects":[]}))
        .unwrap();
        ObjectSnapshot::validate_drawing(&drawing).unwrap();
        let project = ProjectEnvelope::no_media(
            crate::object_document::ProjectMetadata::new(320, 180, 24.).unwrap(),
            drawing,
        );
        // Opaque already-admitted retention fixture; no PNG decoding claim.
        project.with_reference(
            ProjectReferenceState::admitted_still(
                "media-1".into(),
                "plate.png".into(),
                true,
                1.,
                0,
                bytes,
                1024,
            )
            .unwrap(),
        )
    }
    fn allocation(project: &ProjectEnvelope) -> Arc<Vec<u8>> {
        let ProjectReferenceState::StillImage(data) = project.reference_state() else {
            panic!()
        };
        Arc::clone(data.encoded_allocation())
    }

    #[test]
    fn real_owner_entry_counts_aliases_pins_and_escaped_models_until_blob_dies() {
        let project = quota_project(vec![1, 2, 3]);
        let other = quota_project(vec![1, 2, 3]);
        let cost = allocation(&project).capacity();
        let other_cost = allocation(&other).capacity();
        let budget =
            encoded_retention_budget::EncodedRetentionBudget::new(cost + other_cost - 1).unwrap();
        let first = ObjectSnapshot::project_entry(
            "first".into(),
            project.clone(),
            &budget,
            ObjectSnapshot::from_value,
        )
        .unwrap();
        let second = ObjectSnapshot::project_entry(
            "second".into(),
            project.clone(),
            &budget,
            ObjectSnapshot::from_value,
        )
        .unwrap();
        assert_ne!(first.document_id(), second.document_id());
        assert_eq!(budget.used(), Ok(cost));
        let mut owner = ObjectRevisionOwner::from_admitted(first);
        let pinned = owner.acquire(0).unwrap();
        let escaped = pinned.project().unwrap().clone();
        let reference = escaped.reference_state();
        owner.publish(project.drawing().clone()).unwrap();
        let changed = project
            .reference_state()
            .with_settings("after.png".into(), false, 0.5, 2)
            .unwrap();
        owner.publish_reference(changed).unwrap();
        owner.restore(0).unwrap();
        assert_eq!(budget.used(), Ok(cost));
        owner
            .publish_reference(ProjectReferenceState::NoMedia)
            .unwrap();
        assert_eq!(budget.used(), Ok(cost));
        assert!(ObjectSnapshot::project_entry(
            "over".into(),
            other.clone(),
            &budget,
            ObjectSnapshot::from_value
        )
        .is_err());
        assert_eq!(owner.content_revision(), 4);
        assert!(owner.acquire(5).is_none());
        drop(owner);
        drop(project);
        drop(second);
        assert_eq!(budget.used(), Ok(cost));
        drop(pinned);
        assert_eq!(budget.used(), Ok(cost));
        drop(escaped);
        assert_eq!(budget.used(), Ok(cost));
        drop(reference);
        assert_eq!(budget.used(), Ok(0));
        let admitted = ObjectSnapshot::project_entry(
            "reaped".into(),
            other,
            &budget,
            ObjectSnapshot::from_value,
        )
        .unwrap();
        assert_eq!(budget.used(), Ok(other_cost));
        drop(admitted);
        assert_eq!(budget.used(), Ok(0));
    }

    #[test]
    fn owner_identity_failure_rolls_back_only_uncommitted_adoption() {
        let project = quota_project(vec![1, 2, 3]);
        let cost = allocation(&project).capacity();
        let budget = encoded_retention_budget::EncodedRetentionBudget::new(cost).unwrap();
        let no_media = project.with_reference(ProjectReferenceState::NoMedia);
        let plain = ObjectSnapshot::project_entry(
            "no-media".into(),
            no_media,
            &budget,
            ObjectSnapshot::from_value,
        )
        .unwrap();
        assert!(plain.project().is_some());
        assert_eq!(budget.used(), Ok(0));
        let bare = ObjectSnapshot::from_admitted("bare".into(), project.drawing().clone()).unwrap();
        assert!(bare.project().is_none());
        assert_eq!(budget.used(), Ok(0));
        assert!(ObjectSnapshot::project_entry(
            "fail".into(),
            project.clone(),
            &budget,
            |_, _| Err("injected identity failure")
        )
        .is_err());
        assert_eq!(budget.used(), Ok(0));
        assert!(ObjectSnapshot::project_entry(
            "".into(),
            project.clone(),
            &budget,
            ObjectSnapshot::from_value
        )
        .is_err());
        assert_eq!(budget.used(), Ok(0));
        let admitted = ObjectSnapshot::project_entry(
            "live".into(),
            project.clone(),
            &budget,
            ObjectSnapshot::from_value,
        )
        .unwrap();
        assert!(ObjectSnapshot::project_entry(
            "fail".into(),
            project.clone(),
            &budget,
            |_, _| Err("injected identity failure")
        )
        .is_err());
        assert_eq!(budget.used(), Ok(cost));
        assert_eq!(admitted.content_revision(), 0);
        let mut owned = Vec::with_capacity(cost + 1);
        owned.extend_from_slice(&[1, 2, 3]);
        let oversized = quota_project(owned);
        assert!(
            ObjectSnapshot::project_entry("large".into(), oversized, &budget, |_, _| panic!(
                "quota refusal must precede identity allocation"
            ))
            .is_err()
        );
        assert_eq!(budget.used(), Ok(cost));
        drop(project);
        drop(admitted);
        assert_eq!(budget.used(), Ok(0));
    }

    #[test]
    fn safe_integer_exhaustion_preserves_all_retained_snapshots_and_head() {
        let document: ObjectDocument = serde_json::from_value(serde_json::json!({
            "format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,
            "layers":[{"layerUid":"limit-layer"}],"objects":[]}))
        .unwrap();
        let bare = ObjectSnapshot::from_admitted("limit-test".into(), document.clone()).unwrap();
        let project = ProjectEnvelope::no_media(
            crate::object_document::ProjectMetadata::new(320, 180, 24.).unwrap(),
            document.clone(),
        );
        let composite =
            ObjectSnapshot::from_admitted_project("project-limit".into(), project).unwrap();
        for snapshot in [bare, composite] {
            let mut owner = ObjectRevisionOwner::from_admitted(snapshot.clone());
            owner
                .snapshots
                .insert(MAX_CONTENT_REVISION - 1, Arc::clone(&snapshot.value));
            owner.content_revision = MAX_CONTENT_REVISION - 1;
            assert_eq!(owner.publish(document.clone()), Ok(MAX_CONTENT_REVISION));
            let before = owner.snapshots.clone();
            assert!(owner.publish(document.clone()).is_err());
            assert!(owner.restore(0).is_err());
            assert!(owner.restore(MAX_CONTENT_REVISION + 1).is_err());
            assert_eq!(owner.content_revision(), MAX_CONTENT_REVISION);
            assert_eq!(owner.snapshots, before);
            assert_eq!(owner.acquire(0).unwrap().document(), snapshot.document());
            assert_eq!(
                owner.acquire(MAX_CONTENT_REVISION).unwrap().project(),
                snapshot.project()
            );
            assert!(owner.acquire(MAX_CONTENT_REVISION + 1).is_none());
        }
    }
    #[test]
    fn reference_storage_refuses_new_allocations_ids_and_attach_before_publication() {
        let document: ObjectDocument = serde_json::from_value(serde_json::json!({
            "format":"nemo.native-object-document","formatVersion":1,"totalFrames":1,
            "layers":[{"layerUid":"L"}],"objects":[]}))
        .unwrap();
        ObjectSnapshot::validate_drawing(&document).unwrap();
        let project = ProjectEnvelope::no_media(
            crate::object_document::ProjectMetadata::new(320, 180, 24.).unwrap(),
            document.clone(),
        );
        // Already-admitted bytes; this test checks allocation/identity barriers,
        // not PNG validity or resource-manager admission.
        let encoded=b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x02\0\0\0\x02\x08\x06\0\0\0\x72\xb6\x0d\x24\0\0\0\x1aIDAT\x78\x9c\x63\x10\x54\x32\xfe\xef\x12\x9a\xd6\xc0\x50\xde\x31\x93\x61\xd5\xee\x33\xff\x01\x37\x8a\x07\xad\x4f\xf3\x86\x4d\0\0\0\0IEND\xae\x42\x60\x82";
        let state = ProjectReferenceState::admitted_still(
            "media-1".into(),
            "plate.png".into(),
            true,
            0.5,
            0,
            encoded.to_vec(),
            1024,
        )
        .unwrap();
        let mut owner = ObjectRevisionOwner::from_admitted(
            ObjectSnapshot::from_admitted_project(
                "guard".into(),
                project.with_reference(state.clone()),
            )
            .unwrap(),
        );
        let before = owner.snapshots.clone();
        for id in ["media-1", "foreign"] {
            let other = ProjectReferenceState::admitted_still(
                id.into(),
                "plate.png".into(),
                true,
                0.5,
                0,
                encoded.to_vec(),
                1024,
            )
            .unwrap();
            assert!(owner.publish_reference(other).is_err());
            assert_eq!(owner.content_revision(), 0);
            assert_eq!(owner.snapshots, before);
        }
        assert_eq!(
            owner.publish_reference(ProjectReferenceState::NoMedia),
            Ok(1)
        );
        assert!(owner.publish_reference(state.clone()).is_err());
        assert_eq!(owner.content_revision(), 1);
        let bare = ObjectSnapshot::from_admitted("bare".into(), document).unwrap();
        assert!(ObjectRevisionOwner::from_admitted(bare)
            .publish_reference(state.clone())
            .is_err());
        let full = Arc::clone(owner.snapshots.get(&0).unwrap());
        owner.snapshots.insert(MAX_CONTENT_REVISION, full);
        owner.content_revision = MAX_CONTENT_REVISION;
        let before = owner.snapshots.clone();
        let same_blob = state
            .with_settings("changed.png".into(), false, 0.25, 7)
            .unwrap();
        assert!(owner.publish_reference(same_blob).is_err());
        assert_eq!(owner.snapshots, before);
        assert_eq!(owner.content_revision(), MAX_CONTENT_REVISION);
    }
}
