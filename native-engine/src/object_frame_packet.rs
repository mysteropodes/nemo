//! Internal CPU scene inputs from one admitted immutable object snapshot.
//! Array positions are provenance only; composition policy belongs to consumers.
use crate::evaluation::object_vector_kernels::render_vector_eval::holders_fully_consumed;
use crate::object_document::{
    AuthoredOrder, FrameScopeKind, ObjectLayer, ObjectRecord, Presence, PropertyHolderTarget,
};
use crate::revision::ObjectSnapshot;
use std::collections::HashMap;

#[path = "object_frame_sources.rs"]
mod sources;

#[path = "object_order_evaluation.rs"]
mod order;
#[path = "object_text_frame.rs"]
pub(crate) mod text;
pub use order::{NumericElementOrder, NumericLayerOrder, NumericOrderPlan};
#[path = "object_property_sampling.rs"]
mod property_sampling;
pub use property_sampling::{sample_property_raw, PropertySampleError};

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
            let matching_item = document.item_records().is_some_and(|items| {
                items.targets().any(|target| {
                    target.context_id() == context_id
                        && target.scope_kind() == scope_kind
                        && target.frame() == source_frame
                        && target.layer_uid() == layer_uid
                        && target.stroke_id() == stroke_id
                })
            });
            if !matching_record && !matching_item {
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
#[derive(Debug, Clone, Copy)]
struct ObjectFrameRasterIndex {
    layer: usize,
    entry: usize,
}
#[derive(Debug, Clone, Copy)]
pub(crate) struct ObjectFrameRasterRecord<'a> {
    layer: usize,
    global_index: usize,
    entry: &'a crate::object_document::RasterEntry,
}
impl<'a> ObjectFrameRasterRecord<'a> {
    pub(crate) fn layer_index(&self) -> usize {
        self.layer
    }
    pub(crate) fn source_index(&self) -> usize {
        self.global_index
    }
    pub(crate) fn entry(&self) -> &'a crate::object_document::RasterEntry {
        self.entry
    }
}

#[derive(Debug, Clone)]
pub struct ObjectFramePacket {
    snapshot: ObjectSnapshot,
    context_id: String,
    scope_kind: FrameScopeKind,
    frame: u32,
    records: Vec<ObjectFrameIndices>,
    raster_records: Vec<ObjectFrameRasterIndex>,
    text_records: Vec<text::TextIndex>,
    source_frames: Vec<Option<u32>>,
    numeric_order: Option<NumericOrderPlan>,
}

impl ObjectFramePacket {
    pub(crate) fn raster_records(
        &self,
    ) -> impl ExactSizeIterator<Item = ObjectFrameRasterRecord<'_>> + '_ {
        let entries = self
            .snapshot
            .document()
            .item_records()
            .map(|items| items.raster_entries())
            .unwrap_or(&[]);
        let offset = self.snapshot.document().objects().len();
        self.raster_records
            .iter()
            .map(move |index| ObjectFrameRasterRecord {
                layer: index.layer,
                global_index: offset + index.entry,
                entry: &entries[index.entry],
            })
    }
    pub(crate) fn selected_source_indices(&self) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.records()
            .map(|record| (record.source_object_index(), record.source_layer_index()))
            .chain(
                self.raster_records()
                    .map(|record| (record.source_index(), record.layer_index())),
            )
            .chain(
                self.text_records()
                    .map(|record| (record.source_index(), record.layer_index())),
            )
    }

    /// Read-side consumers borrow the same retained source that passed selection.
    pub(crate) fn snapshot(&self) -> &ObjectSnapshot {
        &self.snapshot
    }
    /// Renderer fallback belongs here; absent stored metadata stays absent.
    pub(crate) fn canvas_background_rgba8(&self) -> [u8; 4] {
        self.snapshot
            .project()
            .and_then(|project| project.project().canvas_background_rgba8())
            .unwrap_or([255; 4])
    }
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
    /// Raw layer opacity at evaluation time, never the held drawing's source frame.
    /// Packet admission has already refused unconsumed holders and enabled expressions.
    pub(crate) fn layer_opacity(&self, index: usize) -> Result<f64, ObjectFrameError> {
        let layer = self
            .source_layers()
            .get(index)
            .ok_or(ObjectFrameError::InvalidSource)?;
        let target = PropertyHolderTarget {
            context_id: self.context_id.clone(),
            layer_uid: layer.layer_uid().to_owned(),
            stroke_id: Presence::Missing,
        };
        let values = sample_property_raw(&self.snapshot, &target, "opacity", self.frame as f64)
            .map_err(|_| ObjectFrameError::Unavailable)?;
        match values.as_slice() {
            [value] if value.is_finite() => Ok((value / 100.0).max(0.0)),
            _ => Err(ObjectFrameError::Unavailable),
        }
    }
    /// Selected authored source for this layer; packet.frame() remains evaluation time.
    /// An unexposed layer (or invalid index) has no selected source.
    pub fn source_frame_for_layer(&self, index: usize) -> Option<u32> {
        self.source_frames.get(index).copied().flatten()
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

/// Explicit exact-scoped native query, not a temporal desktop fallback.
/// A valid selection may contain zero records; no authored timing is inferred.
pub fn prepare_object_frame(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
) -> Result<ObjectFramePacket, ObjectFrameError> {
    prepare_frame_with_order(snapshot, selector, false, order::prepare)
}

/// Desktop drawing exposure requires explicit timing for every admitted layer.
/// Reference-scoped object data remains stored, never exposed for display here.
pub fn prepare_object_exposure_frame(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
) -> Result<ObjectFramePacket, ObjectFrameError> {
    prepare_frame_with_order(snapshot, selector, true, order::prepare)
}

/// Explicit exact-scoped resolver; its authored source equals the selector frame.
pub(crate) fn prepare_object_frame_with_resolver<E, F>(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    mut resolver: F,
) -> Result<ObjectFramePacket, E>
where
    E: From<ObjectFrameError>,
    F: FnMut(&str, Option<&str>) -> Result<f64, E>,
{
    prepare_frame_with_order(
        snapshot,
        selector,
        false,
        |snapshot, frame, sources, exposure| {
            order::prepare_with_resolver(snapshot, frame, sources, exposure, |_, layer, stroke| {
                resolver(layer, stroke)
            })
        },
    )
}

/// The same native ordering kernel, explicitly requiring authored exposure.
pub(crate) fn prepare_object_exposure_frame_with_resolver<E, F>(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    resolver: F,
) -> Result<ObjectFramePacket, E>
where
    E: From<ObjectFrameError>,
    F: FnMut(u32, &str, Option<&str>) -> Result<f64, E>,
{
    prepare_frame_with_order(
        snapshot,
        selector,
        true,
        |snapshot, frame, sources, exposure| {
            order::prepare_with_resolver(snapshot, frame, sources, exposure, resolver)
        },
    )
}

fn prepare_frame_with_order<E, F>(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
    exposure: bool,
    prepare_order: F,
) -> Result<ObjectFramePacket, E>
where
    E: From<ObjectFrameError>,
    F: FnOnce(&ObjectSnapshot, u32, &[Option<u32>], bool) -> Result<NumericOrderPlan, E>,
{
    if selector.instance_id != snapshot.instance_id() {
        return Err(ObjectFrameError::WrongInstance.into());
    }
    if selector.document_id != snapshot.document_id() {
        return Err(ObjectFrameError::WrongDocument.into());
    }
    if selector.content_revision != snapshot.content_revision() {
        return Err(ObjectFrameError::WrongRevision.into());
    }
    if selector.document_snapshot_id != snapshot.snapshot_id() {
        return Err(ObjectFrameError::WrongSnapshot.into());
    }
    let v2 = snapshot.document().format_version() == 2;
    if (exposure || v2)
        && (selector.context_id != "scene-root" || selector.scope_kind != FrameScopeKind::Authored)
    {
        return Err(ObjectFrameError::Unavailable.into());
    }
    if selector.context_id != "scene-root" {
        return Err(ObjectFrameError::InvalidContext.into());
    }
    let document = snapshot.document();
    if selector.frame >= document.total_frames() {
        return Err(ObjectFrameError::FrameOutOfRange.into());
    }
    // The typed holder table is admitted for persistence and native reads.
    // Until its sampled values reach this producer, never paint unmodified
    // geometry while silently ignoring authored Motion properties. L01-H consumes exactly
    // parametric-shape, root-vector Position and the actual Text/raster parent consumer.
    // Other holders and visible vector parenting remain unavailable (holders_fully_consumed).
    if !holders_fully_consumed(document) {
        return Err(ObjectFrameError::Unavailable.into());
    }
    let source_frames = sources::select(snapshot, selector, exposure)?;
    let numeric_order = if v2 {
        Some(prepare_order(
            snapshot,
            selector.frame,
            &source_frames,
            exposure,
        )?)
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
        if target.context_id() == selector.context_id && target.scope_kind() == &selector.scope_kind
        {
            let source_layer_index = *layers
                .get(target.layer_uid())
                .ok_or(ObjectFrameError::InvalidSource)?;
            if source_frames[source_layer_index] != Some(target.frame())
                || !document.layers()[source_layer_index].visible()
            {
                continue;
            }
            records.push(ObjectFrameIndices {
                source_layer_index,
                source_object_index,
            });
        }
    }
    let mut raster_records = Vec::new();
    if let Some(items) = document.item_records() {
        for (entry, record) in items.raster_entries().iter().enumerate() {
            let target = &record.target;
            if target.context_id() == selector.context_id
                && target.scope_kind() == &selector.scope_kind
            {
                let layer = *layers
                    .get(target.layer_uid())
                    .ok_or(ObjectFrameError::InvalidSource)?;
                if source_frames[layer] == Some(target.frame())
                    && document.layers()[layer].visible()
                {
                    raster_records.push(ObjectFrameRasterIndex { layer, entry });
                }
            }
        }
    }
    let text_records = text::select(snapshot, selector, &source_frames)?;
    Ok(ObjectFramePacket {
        text_records,
        raster_records,
        snapshot: snapshot.clone(),
        context_id: selector.context_id.to_owned(),
        scope_kind: selector.scope_kind.clone(),
        frame: selector.frame,
        records,
        source_frames,
        numeric_order,
    })
}

#[cfg(all(test, feature = "codec"))]
#[path = "../tests/object_exposure.rs"]
mod exposure_tests;
#[cfg(test)]
#[path = "../tests/render_vector_eval_guard.rs"]
mod render_vector_eval_guard;

#[cfg(all(test, feature = "history", feature = "compositor"))]
#[path = "../tests/object_layer_opacity_scene.rs"]
mod layer_opacity_tests;

#[cfg(all(test, feature = "application", feature = "project_media_codec"))]
#[path = "../tests/object_layer_opacity_raster.rs"]
mod layer_opacity_raster_tests;
