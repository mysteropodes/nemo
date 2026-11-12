//! D17 (#2072): `self.inPoint`/`self.outPoint`, the alias-layer `inPoint`/`outPoint`
//! and `contentBox()` over one pinned `ObjectSnapshot`.
//!
//! In/out points mirror `src/js/motion.js`: `SELF_VIEW` answers FRAMES from
//! `layerInPoint(ld)`/`layerOutPoint(ld)` (`app.js`), and `0`/`state.totalFrames - 1`
//! when the holder resolves no layer; `layerSnapshot` answers the same frames for any
//! layer; `buildAliasLayer` divides them by `_exprFps()` and is `null` without a
//! resolved holder. The resolution order (time link, stored edge, blank-keyframe
//! default, timeline clamp, folder intersection) is the shared D27 `RangeEvaluator`,
//! never a second copy. Inside a component `enterSymbol` swaps `state.layers`,
//! `state.totalFrames` and `state.fps` to the symbol's, so the context's own layers,
//! frame count and fps apply. Frames need no project metadata; seconds do, so a bare
//! drawing refuses them instead of inventing Motion's `|| 24`.
//!
//! `contentBox()` is the Paper `bounds` of the holder's live item, else of its whole
//! layer: tight cubic bounds BEFORE the layer's own Motion. Motion answers the loaded
//! frame; here the frame is explicit. Motion maps "no layer" and "no geometry" to the
//! same zero box; this kernel keeps [`ContentBox::Missing`] distinct from a real
//! zero-size box (a one-anchor path), and [`ContentBox::motion_value`] restores the
//! Motion mapping. Raster/text items, rigged layers and legacy contours are refused:
//! their Paper bounds need providers this evaluation-layer kernel does not own.

use crate::evaluation::object_component_kernels::object_context_resolver::{
    ContextSource, ResolvedContext,
};
use crate::evaluation::object_vector_kernels::render_vector_eval::{
    evaluate_record_rest_with_pose, evaluate_vector_rest, is_vector_record,
};
use crate::evaluation::property_expression_lookup::marker_binding::{
    snapshot_fps, MarkerBindingError,
};
use crate::evaluation::property_expression_lookup::LookupRefusal;
use crate::evaluation::property_expression_views::ExpressionContextView;
use crate::object_document::{RangeError, RangeEvaluator};
use crate::object_frame_packet::projection::layer_bounds;
use crate::object_frame_packet::{
    prepare_object_rest_geometry_frame, FrameScopeKind, ObjectFrameSelector,
};
use crate::revision::ObjectSnapshot;

/// Why an extent or a content box cannot be answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelfExtentError {
    /// The view was bound to a different revision than the snapshot supplied.
    StaleView,
    /// Seconds were asked of a bare drawing: no fps exists.
    NoProject,
    Refused(LookupRefusal),
    /// The context id names no context of the pinned document.
    UnknownContext,
    /// No layer of that context has this uid.
    UnknownLayer,
    /// The shared range reader refused (timing provider, offset expression, frame).
    Range(RangeError),
    /// The holder's geometry needs a provider this kernel does not own.
    Geometry(&'static str),
}

impl From<MarkerBindingError> for SelfExtentError {
    fn from(e: MarkerBindingError) -> Self {
        match e {
            MarkerBindingError::Refused(r) => Self::Refused(r),
            MarkerBindingError::NoProject => Self::NoProject,
            MarkerBindingError::UnknownContext => Self::UnknownContext,
            MarkerBindingError::UnknownLayer => Self::UnknownLayer,
        }
    }
}

/// One layer's resolved in/out points, in frames of its context's clock.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LayerExtent {
    in_point: f64,
    out_point: f64,
    /// `_exprFps()` of the context; `None` for a bare drawing.
    fps: Option<f64>,
    /// False only for Motion's unresolved holder (fallback edges, alias `null`).
    resolved: bool,
}

impl LayerExtent {
    /// `self.inPoint` / `layer(name).inPoint`, in frames.
    pub(crate) fn in_point(&self) -> f64 {
        self.in_point
    }
    /// `self.outPoint` / `layer(name).outPoint`, in frames.
    pub(crate) fn out_point(&self) -> f64 {
        self.out_point
    }
    pub(crate) fn is_resolved(&self) -> bool {
        self.resolved
    }
    /// Alias `thisLayer.inPoint` in seconds; `None` is the alias layer's `null`.
    pub(crate) fn alias_in_point(&self) -> Result<Option<f64>, SelfExtentError> {
        self.seconds(self.in_point)
    }
    /// Alias `thisLayer.outPoint` in seconds; `None` is the alias layer's `null`.
    pub(crate) fn alias_out_point(&self) -> Result<Option<f64>, SelfExtentError> {
        self.seconds(self.out_point)
    }
    fn seconds(&self, frames: f64) -> Result<Option<f64>, SelfExtentError> {
        if !self.resolved {
            return Ok(None);
        }
        let fps = self.fps.ok_or(SelfExtentError::NoProject)?;
        Ok(Some(frames / fps))
    }
}

/// The layer's own bounds in canvas coordinates, or the absence of any geometry.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ContentBox {
    /// Real bounds; width and height may be zero (a one-anchor path).
    Bounds {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    /// No resolved holder, or nothing drawn at the frame.
    Missing,
}

impl ContentBox {
    /// Motion's `{x, y, width, height}` (`top`/`left` repeat `y`/`x`): zeros when missing.
    pub(crate) fn motion_value(&self) -> [f64; 4] {
        match *self {
            Self::Bounds {
                x,
                y,
                width,
                height,
            } => [x, y, width, height],
            Self::Missing => [0.0; 4],
        }
    }
}

fn resolve<'a>(
    snapshot: &'a ObjectSnapshot,
    context_id: &str,
) -> Result<ResolvedContext<'a>, SelfExtentError> {
    ContextSource::of_document(snapshot.document())
        .context(context_id)
        .map_err(|_| SelfExtentError::UnknownContext)
}

fn context_fps(
    snapshot: &ObjectSnapshot,
    context_id: &str,
) -> Result<Option<f64>, SelfExtentError> {
    match snapshot_fps(snapshot, context_id) {
        Ok(fps) => Ok(Some(fps.get())),
        Err(MarkerBindingError::NoProject) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// `layerInPoint(ld)`/`layerOutPoint(ld)` for layer `layer_uid` of `context_id`,
/// with `frame` the evaluation frame (the range reader's explicit sample frame).
pub(crate) fn layer_extent(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    layer_uid: &str,
    frame: u32,
) -> Result<LayerExtent, SelfExtentError> {
    let context = resolve(snapshot, context_id)?;
    context
        .layer(layer_uid)
        .ok_or(SelfExtentError::UnknownLayer)?;
    let range = RangeEvaluator::for_document(snapshot.document(), context.id(), frame)
        .and_then(|ranges| ranges.resolve(layer_uid, true))
        .map_err(SelfExtentError::Range)?;
    Ok(LayerExtent {
        in_point: range.in_point,
        out_point: range.out_point,
        fps: context_fps(snapshot, context_id)?,
        resolved: true,
    })
}

/// `SELF_VIEW.inPoint`/`outPoint` and `buildAliasLayer` for a bound view. An
/// unresolved holder reads `0` and `state.totalFrames - 1` of its context.
pub(crate) fn self_extent(
    view: &ExpressionContextView,
    snapshot: &ObjectSnapshot,
    frame: u32,
) -> Result<LayerExtent, SelfExtentError> {
    if !view.is_bound_to(snapshot) {
        return Err(SelfExtentError::StaleView);
    }
    let context_id = view.target().context_id();
    match view.self_view() {
        Some(this) => layer_extent(snapshot, context_id, this.layer_uid(), frame),
        None => Ok(LayerExtent {
            in_point: 0.0,
            out_point: f64::from(resolve(snapshot, context_id)?.total_frames()) - 1.0,
            fps: context_fps(snapshot, context_id)?,
            resolved: false,
        }),
    }
}

/// `exprContentBox()` for a bound view at `frame`: the holder's stroke when it is
/// drawn there, else its whole layer, as Motion falls back from `liveItemByStrokeId`.
pub(crate) fn content_box(
    view: &ExpressionContextView,
    snapshot: &ObjectSnapshot,
    frame: u32,
) -> Result<ContentBox, SelfExtentError> {
    if !view.is_bound_to(snapshot) {
        return Err(SelfExtentError::StaleView);
    }
    let Some(this) = view.self_view() else {
        return Ok(ContentBox::Missing);
    };
    let context_id = view.target().context_id();
    let stroke = this.stroke_id();
    if stroke.is_some() {
        let own = layer_box(snapshot, context_id, this.layer_uid(), stroke, frame)?;
        if own != ContentBox::Missing {
            return Ok(own);
        }
    }
    layer_box(snapshot, context_id, this.layer_uid(), None, frame)
}

/// Tight rest bounds of the vector records `layer_uid` shows at `frame`, optionally
/// only the record of `stroke`. Every record comes from the context's own table.
pub(crate) fn layer_box(
    snapshot: &ObjectSnapshot,
    context_id: &str,
    layer_uid: &str,
    stroke: Option<&str>,
    frame: u32,
) -> Result<ContentBox, SelfExtentError> {
    use SelfExtentError::Geometry;
    let context = resolve(snapshot, context_id)?;
    let selector = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: context.id(),
        scope_kind: FrameScopeKind::Authored,
        frame,
    };
    // Rest geometry only: no property, expression or control is evaluated here, which
    // is what keeps this read free of the expression it answers.
    let packet = prepare_object_rest_geometry_frame(snapshot, &selector)
        .map_err(|_| Geometry("the frame packet cannot be prepared"))?;
    let index = packet
        .source_layers()
        .iter()
        .position(|l| l.layer_uid() == layer_uid)
        .ok_or(SelfExtentError::UnknownLayer)?;
    if packet.source_layers()[index].rig().is_some() {
        return Err(Geometry("a rigged layer is boxed on its posed strokes"));
    }
    // Out of range or blank: the live Paper layer is empty.
    let Some(held) = packet.source_frame_for_layer(index) else {
        return Ok(ContentBox::Missing);
    };
    let at_held = |t: &crate::object_document::ObjectTarget| {
        t.context_id() == context.id()
            && t.layer_uid() == layer_uid
            && t.scope_kind() == packet.scope_kind()
            && t.frame() == held
            && stroke.map_or(true, |s| t.stroke_id() == s)
    };
    let items = snapshot.document().item_records();
    if items.is_some_and(|items| items.targets().any(at_held)) {
        return Err(Geometry("raster and text bounds need their item provider"));
    }
    let mut vectors = Vec::new();
    for (at, record) in context.objects().iter().enumerate() {
        if !at_held(record.target()) {
            continue;
        }
        if !is_vector_record(record) {
            return Err(Geometry("a legacy contour record has no evaluated bounds"));
        }
        let vector = match packet.records().find(|r| r.source_object_index() == at) {
            Some(source) => evaluate_record_rest_with_pose(&packet, source, None),
            None if context.id() != "scene-root" => {
                return Err(Geometry("a component record is not in its frame packet"));
            }
            None => evaluate_vector_rest(snapshot, at, packet.frame()),
        }
        .map_err(|_| Geometry("the record geometry cannot be evaluated"))?;
        vectors.push(vector);
    }
    let bounds = layer_bounds(&vectors).map_err(|_| Geometry("the bounds are not finite"))?;
    Ok(bounds.map_or(ContentBox::Missing, |b| ContentBox::Bounds {
        x: b.x,
        y: b.y,
        width: b.width,
        height: b.height,
    }))
}

/// D17: in/out points and contentBox over actual pinned native records.
#[cfg(all(test, feature = "history"))]
#[path = "../tests/property_expression_self_extent.rs"]
mod tests;
