//! Native document identity, monotonic content revisions, and immutable snapshots.

use crate::document::OpacityDocument;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

static NEXT_DOCUMENT_ID: AtomicU64 = AtomicU64::new(1);

fn native_document_id() -> Result<String, &'static str> {
    let sequence = NEXT_DOCUMENT_ID
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
            value.checked_add(1)
        })
        .map_err(|_| "native documentId sequence exhausted")?;
    Ok(format!("native-document-{sequence}"))
}

#[derive(Debug, Clone)]
pub struct DocumentSnapshot {
    id: String,
    document_id: String,
    content_revision: u64,
    document: Arc<OpacityDocument>,
}

impl DocumentSnapshot {
    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn document_id(&self) -> &str {
        &self.document_id
    }

    pub fn content_revision(&self) -> u64 {
        self.content_revision
    }

    pub fn document(&self) -> &OpacityDocument {
        &self.document
    }

    pub fn static_opacity(&self, layer_uid: &str) -> Option<f64> {
        self.document
            .layers()
            .iter()
            .find(|layer| layer.layer_uid() == layer_uid)
            .and_then(|layer| layer.static_opacity())
            .and_then(|value| value.as_f64())
    }
}

#[derive(Debug)]
pub(crate) struct RevisionOwner {
    instance_id: String,
    document_id: String,
    content_revision: u64,
    snapshots: BTreeMap<u64, Arc<OpacityDocument>>,
}

impl RevisionOwner {
    pub(crate) fn new(
        instance_id: impl Into<String>,
        document: OpacityDocument,
    ) -> Result<Self, &'static str> {
        let instance_id = instance_id.into();
        if instance_id.is_empty() {
            return Err("instanceId must be non-empty");
        }
        let mut snapshots = BTreeMap::new();
        snapshots.insert(0, Arc::new(document));
        Ok(Self {
            instance_id,
            document_id: native_document_id()?,
            content_revision: 0,
            snapshots,
        })
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

    pub(crate) fn current_document(&self) -> Arc<OpacityDocument> {
        Arc::clone(
            self.snapshots
                .get(&self.content_revision)
                .expect("the current native revision always has a snapshot"),
        )
    }

    pub(crate) fn commit(&mut self, document: OpacityDocument) -> Result<u64, &'static str> {
        let next = self
            .content_revision
            .checked_add(1)
            .ok_or("contentRevision overflow")?;
        self.snapshots.insert(next, Arc::new(document));
        self.content_revision = next;
        Ok(next)
    }

    pub(crate) fn acquire(&self, revision: u64) -> Option<DocumentSnapshot> {
        self.snapshots
            .get(&revision)
            .map(|document| DocumentSnapshot {
                id: format!("native-opacity:{}:{revision}", self.document_id),
                document_id: self.document_id.clone(),
                content_revision: revision,
                document: Arc::clone(document),
            })
    }

    pub(crate) fn replace(&mut self, document: OpacityDocument) -> Result<(), &'static str> {
        self.document_id = native_document_id()?;
        self.content_revision = 0;
        self.snapshots.clear();
        self.snapshots.insert(0, Arc::new(document));
        Ok(())
    }
}
