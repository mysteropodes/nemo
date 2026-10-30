//! Internal CPU scene inputs from one admitted immutable object snapshot.
//! Array positions are provenance only; composition policy belongs to consumers.
use crate::evaluation::object_vector_kernels::render_vector_eval::holders_fully_consumed;
// Detached geometry planning is a frame-source port; rendering never owns publication.
pub(crate) use crate::object_document::subselect::record as subselect_record;
pub(crate) use crate::object_document::{
    AuthoredOrder, BitmapBrushFields, FrameScopeKind, ObjectLayer, ObjectRecord, ObjectTarget,
    Presence, PropertyHolderTarget, SolidFill, VECTOR_FAMILY,
};
use crate::revision::ObjectSnapshot;
pub(crate) use property_sampling::frame_motion::MotionValues;
#[path = "object_expression_holder_source.rs"]
pub(crate) mod expression_sources;
#[path = "object_frame_motion_receipts.rs"]
pub(crate) mod motion_receipts;
#[path = "object_frame_projection.rs"]
pub(crate) mod projection;
#[path = "object_frame_text_selectors.rs"]
mod text_selectors;

#[path = "object_frame_context.rs"]
mod context;
#[path = "object_duplicator_snapshot_sources.rs"]
pub(crate) mod duplicator_sources;
#[path = "object_duplicator_frame.rs"]
pub(crate) mod duplicators;
#[path = "object_frame_sources.rs"]
mod sources;
use context::FrameContext;
#[path = "object_frame_prepare.rs"]
mod prepare;
use prepare::prepare_frame_with_order;

#[path = "object_order_evaluation.rs"]
mod order;
#[path = "object_text_frame.rs"]
pub(crate) mod text;
#[path = "object_frame_video.rs"]
pub(crate) mod video;
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
    context: FrameContext,
    scope_kind: FrameScopeKind,
    frame: u32,
    records: Vec<ObjectFrameIndices>,
    raster_records: Vec<ObjectFrameRasterIndex>,
    text_records: Vec<text::TextIndex>,
    video_records: Vec<video::VideoIndex>,
    duplicators: Vec<duplicators::LayerCopies>,
    source_frames: Vec<Option<u32>>,
    numeric_order: Option<NumericOrderPlan>,
    motion_receipts: Vec<motion_receipts::MotionReceipt>,
    selector_receipts: Vec<text_selectors::SelectorReceipt>,
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
        let offset = self.source_objects().len();
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
    /// Same stored camera provider for trusted export and locked-view consumers.
    /// The project's custom brush catalog (`state.customBrushPresets`), when the snapshot carries a project.
    pub(crate) fn custom_brush_presets(&self) -> Option<&crate::object_document::ProjectPresetCatalog> {
        self.snapshot.project().and_then(|p| p.project().custom_brush_presets())
    }
    /// `resolveBrushPreset(key)` (a built-in first, then the custom catalog) for an exporter that needs only the
    /// preset's nib size, scatter and spacing (the Rive brush wobble).
    pub(crate) fn brush_wobble_inputs(&self, key: &str) -> Option<(Option<f64>, Option<f64>, Option<f64>)> {
        use crate::evaluation::brush_dab::presets::{builtin, from_catalog};
        let preset = builtin(key).or_else(|| {
            self.custom_brush_presets()?.entries().iter().find(|(id, _)| id == key).and_then(|(_, p)| from_catalog(p))
        })?;
        Some((preset.nib_size, preset.scatter, preset.spacing))
    }
    pub(crate) fn stored_camera_matrix(&self) -> Result<Option<[f64; 6]>, ObjectFrameError> {
        self.snapshot
            .sampled_camera_matrix(f64::from(self.frame))
            .map_err(|_| ObjectFrameError::Unavailable)
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
        let values = self.sampled_motion_values(&target)?;
        Ok((values.opacity / 100.0).max(0.0))
    }
    /// Selected authored source for this layer; packet.frame() remains evaluation time.
    /// An unexposed layer (or invalid index) has no selected source.
    pub fn source_frame_for_layer(&self, index: usize) -> Option<u32> {
        self.source_frames.get(index).copied().flatten()
    }
    /// Original layer table, including layers with no matching records.
    pub fn source_layers(&self) -> &[ObjectLayer] {
        self.context.layers(self.snapshot.document())
    }
    pub(crate) fn source_objects(&self) -> &[ObjectRecord] {
        self.context.objects(self.snapshot.document())
    }
    /// Borrow one selected provenance position in constant time, not paint rank.
    pub(crate) fn record_at(&self, index: usize) -> Option<ObjectFrameRecord<'_>> {
        let indices = self.records.get(index)?;
        Some(ObjectFrameRecord {
            source_layer_index: indices.source_layer_index,
            source_object_index: indices.source_object_index,
            record: self.source_objects().get(indices.source_object_index)?,
        })
    }
    /// Source encounter order conveys provenance, never a paint-order promise.
    pub fn records(&self) -> impl ExactSizeIterator<Item = ObjectFrameRecord<'_>> + '_ {
        self.records.iter().map(|indices| ObjectFrameRecord {
            source_layer_index: indices.source_layer_index,
            source_object_index: indices.source_object_index,
            // Private indices were selected from this same immutable document.
            record: &self.source_objects()[indices.source_object_index],
        })
    }
}

/// Reuse the scoped order producer with a caller-validated source selection.
/// Placed symbols intentionally select timing without entered-layer range gates.
pub(crate) fn prepare_scoped_order_from_sources(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    frame: u32,
    sources: &[Option<u32>],
) -> Result<NumericOrderPlan, ObjectFrameError> {
    order::prepare_context(snapshot, context_id, frame, sources, true)
}

/// Explicit exact-scoped native query, not a temporal desktop fallback.
/// A valid selection may contain zero records; no authored timing is inferred.
pub fn prepare_object_frame(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
) -> Result<ObjectFramePacket, ObjectFrameError> {
    prepare_frame_with_order(snapshot, selector, false, order::prepare_context, None)
}

/// Desktop drawing exposure requires explicit timing for every admitted layer.
/// Reference-scoped object data remains stored, never exposed for display here.
pub fn prepare_object_exposure_frame(
    snapshot: &ObjectSnapshot,
    selector: &ObjectFrameSelector<'_>,
) -> Result<ObjectFramePacket, ObjectFrameError> {
    prepare_frame_with_order(snapshot, selector, true, order::prepare_context, None)
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
        |snapshot, context_id, frame, sources, exposure| {
            order::prepare_context_with_resolver(
                snapshot,
                context_id,
                frame,
                sources,
                exposure,
                |_, layer, stroke| resolver(layer, stroke),
            )
        },
        None,
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
        |snapshot, context_id, frame, sources, exposure| {
            order::prepare_context_with_resolver(
                snapshot, context_id, frame, sources, exposure, resolver,
            )
        },
        None,
    )
}

#[cfg(all(test, feature = "codec"))]
#[path = "../tests/object_exposure.rs"]
mod exposure_tests;
#[cfg(test)]
#[path = "../tests/render_vector_eval_guard.rs"]
mod render_vector_eval_guard;
#[cfg(test)]
#[path = "../tests/render_vector_eval_contentless.rs"]
mod render_vector_eval_contentless;

#[cfg(all(test, feature = "history", feature = "compositor"))]
#[path = "../tests/object_layer_opacity_scene.rs"]
mod layer_opacity_tests;

#[cfg(all(test, feature = "application", feature = "project_media_codec"))]
#[path = "../tests/object_layer_opacity_raster.rs"]
mod layer_opacity_raster_tests;

#[cfg(all(test, feature = "codec"))]
#[path = "../tests/object_context_frame_packet.rs"]
mod context_frame_tests;
