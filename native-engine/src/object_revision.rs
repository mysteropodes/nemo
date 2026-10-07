//! Immutable object revision storage. Admission and command policy live above it.
use crate::object_document::ObjectDocument;
#[cfg(feature = "history")]
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

static NEXT_OBJECT_DOCUMENT: AtomicU64 = AtomicU64::new(1);
#[cfg(feature = "history")]
const MAX_CONTENT_REVISION: u64 = 9_007_199_254_740_991;

#[derive(Debug, Clone)]
pub struct ObjectSnapshot {
    instance_id: String,
    document_id: String,
    snapshot_id: String,
    content_revision: u64,
    document: Arc<ObjectDocument>,
}

impl ObjectSnapshot {
    /// Called only after semantic admission by the snapshot layer.
    pub(crate) fn from_admitted(
        instance_id: String,
        document: ObjectDocument,
    ) -> Result<Self, &'static str> {
        let sequence = NEXT_OBJECT_DOCUMENT
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| n.checked_add(1))
            .map_err(|_| "object document identity sequence exhausted")?;
        let document_id = format!("native-object-document-{sequence}");
        Ok(Self {
            instance_id,
            snapshot_id: format!("native-object:{document_id}:0"),
            document_id,
            content_revision: 0,
            document: Arc::new(document),
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
        &self.document
    }
}

#[derive(Debug)]
#[cfg(feature = "history")]
pub(crate) struct ObjectRevisionOwner {
    instance_id: String,
    document_id: String,
    content_revision: u64,
    snapshots: BTreeMap<u64, Arc<ObjectDocument>>,
}

#[cfg(feature = "history")]
impl ObjectRevisionOwner {
    pub(crate) fn from_admitted(snapshot: ObjectSnapshot) -> Self {
        Self {
            instance_id: snapshot.instance_id,
            document_id: snapshot.document_id,
            content_revision: 0,
            snapshots: BTreeMap::from([(0, snapshot.document)]),
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
        self.snapshots
            .get(&revision)
            .map(|document| ObjectSnapshot {
                instance_id: self.instance_id.clone(),
                document_id: self.document_id.clone(),
                snapshot_id: format!("native-object:{}:{revision}", self.document_id),
                content_revision: revision,
                document: Arc::clone(document),
            })
    }
    pub(crate) fn publish(&mut self, document: ObjectDocument) -> Result<u64, &'static str> {
        self.advance(Arc::new(document))
    }
    pub(crate) fn restore(&mut self, revision: u64) -> Result<u64, &'static str> {
        let document = self
            .snapshots
            .get(&revision)
            .ok_or("retained revision absent")?;
        self.advance(Arc::clone(document))
    }
    fn advance(&mut self, document: Arc<ObjectDocument>) -> Result<u64, &'static str> {
        let next = self
            .content_revision
            .checked_add(1)
            .filter(|next| *next <= MAX_CONTENT_REVISION)
            .ok_or("contentRevision safe-integer limit exhausted")?;
        self.snapshots.insert(next, document);
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
        };
        let snapshot =
            ObjectSnapshot::from_admitted("limit-test".into(), document.clone()).unwrap();
        let mut owner = ObjectRevisionOwner::from_admitted(snapshot.clone());
        owner
            .snapshots
            .insert(MAX_CONTENT_REVISION - 1, Arc::new(document.clone()));
        owner.content_revision = MAX_CONTENT_REVISION - 1;
        assert_eq!(owner.publish(document.clone()), Ok(MAX_CONTENT_REVISION));
        let before = owner.snapshots.clone();
        assert!(owner.publish(document).is_err());
        assert!(owner.restore(0).is_err());
        assert!(owner.restore(MAX_CONTENT_REVISION + 1).is_err());
        assert_eq!(owner.content_revision(), MAX_CONTENT_REVISION);
        assert_eq!(owner.snapshots, before);
        assert_eq!(owner.acquire(0).unwrap().document(), snapshot.document());
        assert!(owner.acquire(MAX_CONTENT_REVISION + 1).is_none());
    }
}
