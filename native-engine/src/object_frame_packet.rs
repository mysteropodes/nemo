//! Internal CPU scene inputs from one admitted immutable object snapshot.
//! Array positions are provenance only; composition policy belongs to consumers.
use crate::object_document::{AuthoredOrder, FrameScopeKind, ObjectLayer, ObjectRecord};
use crate::revision::ObjectSnapshot;
use std::collections::HashMap;

#[path = "object_order_evaluation.rs"]
mod order;
pub use order::{NumericElementOrder, NumericLayerOrder, NumericOrderPlan};

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
    Unavailable,
}

/// Private raw Order lookup. Unlike prepare_object_frame, this does not ask C1
/// to compose an active expression; callers may only inspect its authored data.
pub(crate) fn raw_expression_order<'a>(
    snapshot: &'a ObjectSnapshot,
    context_id: &str,
    scope_kind: &FrameScopeKind,
    source_frame: u32,
    layer_uid: &str,
    stroke_id: Option<&str>,
) -> Result<&'a AuthoredOrder, ObjectFrameError> {
    let document = snapshot.document();
    if document.format_version() != 2 || *scope_kind != FrameScopeKind::Authored {
        return Err(ObjectFrameError::Unavailable);
    }
    if context_id != "scene-root" {
        return Err(ObjectFrameError::InvalidContext);
    }
    if source_frame >= document.total_frames() {
        return Err(ObjectFrameError::FrameOutOfRange);
    }
    let plan = document
        .composition_order
        .value()
        .ok_or(ObjectFrameError::Unavailable)?;
    let context = plan
        .contexts
        .iter()
        .find(|context| context.context_id == context_id)
        .ok_or(ObjectFrameError::InvalidSource)?;
    if !document
        .layers()
        .iter()
        .any(|layer| layer.layer_uid() == layer_uid)
    {
        return Err(ObjectFrameError::InvalidSource);
    }
    match stroke_id {
        None => context
            .layers
            .iter()
            .find(|layer| layer.layer_uid == layer_uid)
            .map(|layer| &layer.order)
            .ok_or(ObjectFrameError::InvalidSource),
        Some(stroke_id) => {
            let matching_record = document.objects().iter().any(|record| {
                let target = record.target();
                target.context_id() == context_id
                    && target.scope_kind() == scope_kind
                    && target.frame() == source_frame
                    && target.layer_uid() == layer_uid
                    && target.stroke_id() == stroke_id
            });
            if !matching_record {
                return Err(ObjectFrameError::InvalidSource);
            }
            context
                .scopes
                .iter()
                .find(|scope| {
                    scope.frame_scope.kind == *scope_kind && scope.frame_scope.frame == source_frame
                })
                .and_then(|scope| {
                    scope.elements.iter().find(|element| {
                        element.layer_uid == layer_uid && element.stroke_id == stroke_id
                    })
                })
                .map(|element| &element.order)
                .ok_or(ObjectFrameError::InvalidSource)
        }
    }
}

pub(crate) fn raw_expression_numeric(
    order: &AuthoredOrder,
    evaluated_frame: u32,
) -> Result<f64, ObjectFrameError> {
    order::numeric(order, evaluated_frame)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectFrameRecord<'a> {
    source_layer_index: usize,
    source_object_index: usize,
    record: &'a ObjectRecord,
}

impl<'a> ObjectFrameRecord<'a> {
    pub fn source_layer_index(&self) -> usize {
        self.source_layer_index
    }
    pub fn source_object_index(&self) -> usize {
        self.source_object_index
    }
    pub fn record(&self) -> &'a ObjectRecord {
        self.record
    }
}

#[derive(Debug, Clone, Copy)]
struct ObjectFrameIndices {
    source_layer_index: usize,
    source_object_index: usize,
}

/// No mutable fields, renderer resources, jobs or publication identities.
#[derive(Debug, Clone)]
pub struct ObjectFramePacket {
    snapshot: ObjectSnapshot,
    context_id: String,
    scope_kind: FrameScopeKind,
    frame: u32,
    records: Vec<ObjectFrameIndices>,
    numeric_order: Option<NumericOrderPlan>,
}

impl ObjectFramePacket {
    /// Explicit partial numeric composition; records() remains source provenance.
    pub fn numeric_order(&self) -> Option<&NumericOrderPlan> {
        self.numeric_order.as_ref()
    }
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
    /// Borrow one selected provenance position in constant time, not paint rank.
    pub(crate) fn record_at(&self, index: usize) -> Option<ObjectFrameRecord<'_>> {
        let indices = self.records.get(index)?;
        Some(ObjectFrameRecord {
            source_layer_index: indices.source_layer_index,
            source_object_index: indices.source_object_index,
            record: self
                .snapshot
                .document()
                .objects()
                .get(indices.source_object_index)?,
        })
    }
    /// Source encounter order conveys provenance, never a paint-order promise.
    pub fn records(&self) -> impl ExactSizeIterator<Item = ObjectFrameRecord<'_>> + '_ {
        self.records.iter().map(|indices| ObjectFrameRecord {
            source_layer_index: indices.source_layer_index,
            source_object_index: indices.source_object_index,
            // Private indices were selected from this same immutable document.
            record: &self.snapshot.document().objects()[indices.source_object_index],
        })
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
    let v2 = snapshot.document().format_version() == 2;
    if v2
        && (selector.context_id != "scene-root" || selector.scope_kind != FrameScopeKind::Authored)
    {
        return Err(ObjectFrameError::Unavailable);
    }
    if selector.context_id != "scene-root" {
        return Err(ObjectFrameError::InvalidContext);
    }
    let document = snapshot.document();
    if selector.frame >= document.total_frames() {
        return Err(ObjectFrameError::FrameOutOfRange);
    }
    let numeric_order = if v2 {
        Some(order::prepare(snapshot, selector.frame)?)
    } else {
        None
    };
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
            records.push(ObjectFrameIndices {
                source_layer_index,
                source_object_index,
            });
        }
    }
    Ok(ObjectFramePacket {
        snapshot: snapshot.clone(),
        context_id: selector.context_id.to_owned(),
        scope_kind: selector.scope_kind.clone(),
        frame: selector.frame,
        records,
        numeric_order,
    })
}
