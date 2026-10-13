//! Immutable object revision storage. Admission and command policy live above it.
use crate::object_document::{ObjectDocument, ProjectEnvelope};
#[cfg(feature = "history")]
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

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
        Self::from_value(instance_id, ProjectRevisionValue::Project(project))
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

    #[test]
    fn safe_integer_exhaustion_preserves_all_retained_snapshots_and_head() {
        let document = ObjectDocument {
            format: "nemo.native-object-document".into(),
            format_version: 1,
            total_frames: 1,
            layers: vec![crate::object_document::ObjectLayer {
                layer_uid: "limit-layer".into(),
            }],
            objects: vec![],
            composition_order: Default::default(),
        };
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
}
