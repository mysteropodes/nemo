//! Internal CPU scene inputs from one admitted immutable object snapshot.
//! Array positions are provenance only; composition policy belongs to consumers.
use crate::object_document::{FrameScopeKind, ObjectLayer, ObjectRecord};
use crate::revision::ObjectSnapshot;
use std::collections::HashMap;

/// All pins are mandatory, including the snapshot identity within a revision.
#[derive(Debug, Clone)]
pub struct ObjectFrameSelector<'a> {
    pub instance_id: &'a str,
    pub document_id: &'a str,
    pub content_revision: u64,
    pub document_snapshot_id: &'a str,
    pub context_id: &'a str,
    pub scope_kind: FrameScopeKind,
    pub frame: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectFrameError {
    WrongInstance,
    WrongDocument,
    WrongRevision,
    WrongSnapshot,
    InvalidContext,
    FrameOutOfRange,
    InvalidSource,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectFrameRecord {
    source_layer_index: usize,
    source_object_index: usize,
    record: ObjectRecord,
}

impl ObjectFrameRecord {
    pub fn source_layer_index(&self) -> usize {
        self.source_layer_index
    }
    pub fn source_object_index(&self) -> usize {
        self.source_object_index
    }
    pub fn record(&self) -> &ObjectRecord {
        &self.record
    }
}

/// No mutable fields, renderer resources, jobs or publication identities.
#[derive(Debug, Clone)]
pub struct ObjectFramePacket {
    snapshot: ObjectSnapshot,
    context_id: String,
    scope_kind: FrameScopeKind,
    frame: u32,
    records: Vec<ObjectFrameRecord>,
}

impl ObjectFramePacket {
    pub fn instance_id(&self) -> &str {
        self.snapshot.instance_id()
    }
    pub fn document_id(&self) -> &str {
        self.snapshot.document_id()
    }
    pub fn content_revision(&self) -> u64 {
        self.snapshot.content_revision()
    }
    pub fn document_snapshot_id(&self) -> &str {
        self.snapshot.snapshot_id()
    }
    pub fn context_id(&self) -> &str {
        &self.context_id
    }
    pub fn scope_kind(&self) -> &FrameScopeKind {
        &self.scope_kind
    }
    pub fn frame(&self) -> u32 {
        self.frame
    }
    /// Original layer table, including layers with no matching records.
    pub fn source_layers(&self) -> &[ObjectLayer] {
        self.snapshot.document().layers()
    }
    /// Source encounter order conveys provenance, never a paint-order promise.
    pub fn records(&self) -> &[ObjectFrameRecord] {
        &self.records
    }
}

/// Select exact scope/frame only. A valid selection may contain zero records.
pub fn prepare_object_frame(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
) -> Result<ObjectFramePacket, ObjectFrameError> {
    if selector.instance_id != snapshot.instance_id() {
        return Err(ObjectFrameError::WrongInstance);
    }
    if selector.document_id != snapshot.document_id() {
        return Err(ObjectFrameError::WrongDocument);
    }
    if selector.content_revision != snapshot.content_revision() {
        return Err(ObjectFrameError::WrongRevision);
    }
    if selector.document_snapshot_id != snapshot.snapshot_id() {
        return Err(ObjectFrameError::WrongSnapshot);
    }
    if selector.context_id != "scene-root" {
        return Err(ObjectFrameError::InvalidContext);
    }
    let document = snapshot.document();
    if selector.frame >= document.total_frames() {
        return Err(ObjectFrameError::FrameOutOfRange);
    }
    let layers: HashMap<_, _> = document
        .layers()
        .iter()
        .enumerate()
        .map(|(index, layer)| (layer.layer_uid(), index))
        .collect();
    let mut records = Vec::new();
    for (source_object_index, record) in document.objects().iter().enumerate() {
        let target = record.target();
        if target.context_id() == selector.context_id
            && target.scope_kind() == &selector.scope_kind
            && target.frame() == selector.frame
        {
            let source_layer_index = *layers
                .get(target.layer_uid())
                .ok_or(ObjectFrameError::InvalidSource)?;
            records.push(ObjectFrameRecord {
                source_layer_index,
                source_object_index,
                record: record.clone(),
            });
        }
    }
    Ok(ObjectFramePacket {
        snapshot: snapshot.clone(),
        context_id: selector.context_id.to_owned(),
        scope_kind: selector.scope_kind.clone(),
        frame: selector.frame,
        records,
    })
}
