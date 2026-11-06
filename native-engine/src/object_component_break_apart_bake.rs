//! The evaluation half of breaking a component apart: the instance of one scene layer, evaluated by the one component join at
//! every frame of the scene (held outer key, playback, Time Remap, exposed overrides, component camera, `symMatrix`),
//! with identical consecutive frames collapsed into one key plus holds, exactly as `convertComponentToLayer` collapses the
//! JSON of `getEffectiveStrokes`. The records come back with the placement chain applied to their geometry (points through the
//! matrix, handles through its linear part, the way `applyMatrixToStrokeData` does), the inner layer's opacity Motion folded
//! into their opacity (`sd2.opacity * layerMat.op`) and everything else untouched; images take the same chain as the baseline
//! raster dict (centre mapped, size scaled, rotation dropped). Vector shapes and images come back in the inner paint order
//! (the component's scoped order plan, the order the viewport paints them in). The plan half
//! (`object_component_break_apart.rs`) writes them into the layer. Nothing here mutates the snapshot.
use crate::evaluation::object_component_kernels::object_component_camera_ports::RecordPorts;
use crate::evaluation::object_component_kernels::object_component_override_port::PacketOverrides;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component::{
    evaluate_component_shapes, ResolvedComponentPlacement,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_shape::{
    RasterRect, ShapeData,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_join::{
    evaluate_instance, ComponentInputs,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::{
    ComponentOutcome, PlacedComponent,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::{
    component_holder_consumed, ComponentUnavailable,
};
use crate::object_document::object_component_record_kernels::object_component_records::{Baked, SCENE_CONTEXT};
use crate::object_document::{
    FrameScopeKind, ObjectPoint, ObjectRecord, ObjectTarget, Presence, RasterEntry,
};
use crate::object_frame_packet::{
    prepare_object_exposure_frame, prepare_scoped_order_from_sources, sample_property_raw,
    ObjectFrameSelector, PropertyHolderTarget,
};
use crate::revision::ObjectSnapshot;
use serde_json::Number;
use std::collections::HashMap;

/// Why the instance could not be evaluated into records: a stable, user-facing sentence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct BakeError(pub(crate) &'static str);

const NOT_EVALUABLE: BakeError =
    BakeError("component-break-apart: the instance could not be evaluated at every frame.");

fn why(reason: &ComponentUnavailable) -> BakeError {
    match reason {
        ComponentUnavailable::TimeRemap(_) => BakeError(
            "component-break-apart: the Time Remap of the instance has no finite value on every frame.",
        ),
        ComponentUnavailable::Camera(_) => {
            BakeError("component-break-apart: the component camera needs the project canvas.")
        }
        ComponentUnavailable::InnerFeature { .. } => BakeError(
            "component-break-apart: an inner layer has a feature a flat layer cannot hold yet.",
        ),
        _ => NOT_EVALUABLE,
    }
}

fn through(chain: &[[f64; 6]], x: f64, y: f64) -> [f64; 2] {
    chain.iter().fold([x, y], |[x, y], [a, b, c, d, tx, ty]| {
        [x * a + y * c + tx, x * b + y * d + ty]
    })
}

fn number(value: f64) -> Result<Number, BakeError> {
    Number::from_f64(value).ok_or(NOT_EVALUABLE)
}

fn pair(point: &ObjectPoint) -> Result<[f64; 2], BakeError> {
    Ok([
        point.x.as_f64().ok_or(NOT_EVALUABLE)?,
        point.y.as_f64().ok_or(NOT_EVALUABLE)?,
    ])
}

/// `applyMatrixToStrokeData` over the stored geometry: points through every matrix of the chain, handles through its linear part.
fn place(record: &ObjectRecord, chain: &[[f64; 6]]) -> Result<ObjectRecord, BakeError> {
    let mut placed = record.clone();
    if chain.is_empty() {
        return Ok(placed);
    }
    let origin = through(chain, 0.0, 0.0);
    for segment in &mut placed.geometry.segments {
        let [x, y] = pair(&segment.point)?;
        let moved = through(chain, x, y);
        segment.point = ObjectPoint {
            x: number(moved[0])?,
            y: number(moved[1])?,
        };
        for handle in [&mut segment.handle_in, &mut segment.handle_out] {
            let [hx, hy] = pair(handle)?;
            let moved = through(chain, hx, hy);
            *handle = ObjectPoint {
                x: number(moved[0] - origin[0])?,
                y: number(moved[1] - origin[1])?,
            };
        }
    }
    Ok(placed)
}

fn chain_of(placement: &ResolvedComponentPlacement) -> Vec<[f64; 6]> {
    placement.chain().into_iter().map(|m| m.0).collect()
}

const OPACITY_UNBAKEABLE: BakeError = BakeError(
    "component-break-apart: opacity Motion on an inner layer of a record without a solid fill cannot be baked into the layer yet.",
);

/// The raw opacity (`0..`) of one inner layer at the inner frame, as the viewport paint reads it.
fn raw_opacity(
    snapshot: &ObjectSnapshot,
    symbol: &str,
    layer_uid: &str,
    frame: u32,
) -> Result<f64, BakeError> {
    let target = PropertyHolderTarget {
        context_id: symbol.to_owned(),
        layer_uid: layer_uid.to_owned(),
        stroke_id: Presence::Missing,
    };
    match sample_property_raw(snapshot, &target, "opacity", f64::from(frame))
        .map_err(|_| NOT_EVALUABLE)?
        .as_slice()
    {
        [value] if value.is_finite() => Ok((value / 100.0).max(0.0)),
        _ => Err(NOT_EVALUABLE),
    }
}

/// The vector record as the baseline dict `getEffectiveStrokes` returns: the chain on its geometry, the inner layer's opacity
/// folded into the record's own (the host's stays on the host layer).
fn vector(
    source: &ObjectRecord,
    chain: &[[f64; 6]],
    inner_opacity: f64,
) -> Result<Baked, BakeError> {
    let mut record = place(source, chain)?;
    if inner_opacity != 1.0 {
        if let Presence::Present(body) = &mut record.vector {
            body.opacity = body
                .opacity
                .as_f64()
                .and_then(|opacity| Number::from_f64(opacity * inner_opacity))
                .ok_or(NOT_EVALUABLE)?;
        } else if let Some(Some(fill)) = record.fill.value_mut() {
            // A plain fill record has no opacity of its own: the layer opacity is its fill alpha, as the paint folds it.
            fill.a = fill
                .a
                .as_f64()
                .and_then(|alpha| Number::from_f64((alpha * inner_opacity).min(1.0)))
                .ok_or(NOT_EVALUABLE)?;
        } else {
            return Err(OPACITY_UNBAKEABLE);
        }
    }
    Ok(Baked::Vector(record))
}

/// The image as the baseline raster dict after `applyMatrixToStrokeData`: with a chain the centre is mapped, the size scaled by
/// the column scales and the rotation dropped; without one the stored rect stays. Opacity takes the inner layer's.
fn image(
    entry: &RasterEntry,
    placement: &ResolvedComponentPlacement,
    inner_opacity: f64,
) -> Result<Baked, BakeError> {
    let item = &entry.item;
    let rect = if placement.chain().is_empty() {
        None
    } else {
        let (x, y) = item.center();
        let (width, height) = item.size();
        let stored = ShapeData::Raster(RasterRect {
            flag: serde_json::Value::Bool(true),
            src: None,
            x,
            y,
            width,
            height,
            opacity: None,
            rest: serde_json::Map::new(),
        });
        let placed = evaluate_component_shapes(std::slice::from_ref(&stored), placement)
            .map_err(|_| NOT_EVALUABLE)?;
        let Some(ShapeData::Raster(moved)) = placed.shapes.first().map(|shape| shape.data()) else {
            return Err(NOT_EVALUABLE);
        };
        Some(((moved.x, moved.y), (moved.width, moved.height)))
    };
    let baked = item
        .baked(rect, item.opacity() * inner_opacity)
        .map_err(|_| NOT_EVALUABLE)?;
    Ok(Baked::Image(RasterEntry {
        target: entry.target.clone(),
        item: baked,
    }))
}

impl ObjectSnapshot {
    /// What the placed component shows on one frame, in paint order: `(source order, baked items)`.
    fn bake_frame(
        &self,
        symbol_id: &str,
        placed: &PlacedComponent,
    ) -> Result<Vec<Baked>, BakeError> {
        let document = self.document();
        let component = document
            .component_set()
            .and_then(|set| set.component(symbol_id))
            .ok_or(NOT_EVALUABLE)?;
        let Some(inner) = placed.inner_frame.index(component.total_frames() as usize) else {
            return Ok(Vec::new());
        };
        let inner = inner as u32;
        let placement = placed.placement.as_ref().ok_or(NOT_EVALUABLE)?;
        let chain = chain_of(placement);
        let mut layers: HashMap<&str, usize> = HashMap::new();
        let mut source_frames = Vec::with_capacity(component.layers().len());
        for (index, layer) in component.layers().iter().enumerate() {
            layers.insert(layer.layer_uid(), index);
            source_frames.push(if layer.visible() {
                layer
                    .timing
                    .value()
                    .ok_or(NOT_EVALUABLE)?
                    .exposure(inner)
                    .1
            } else {
                None
            });
        }
        let order = prepare_scoped_order_from_sources(self, symbol_id, inner, &source_frames)
            .map_err(|_| NOT_EVALUABLE)?;
        // The order plan names its sources by position: the component's own records, then the document's images.
        let local = component.objects().len();
        let mut sources: HashMap<usize, &ObjectRecord> = HashMap::new();
        for (index, record) in component.objects().iter().enumerate() {
            let target = record.target();
            let at = *layers.get(target.layer_uid()).ok_or(NOT_EVALUABLE)?;
            if target.scope_kind() == &FrameScopeKind::Authored
                && source_frames[at] == Some(target.frame())
            {
                sources.insert(index, record);
            }
        }
        let mut images: HashMap<usize, &RasterEntry> = HashMap::new();
        if let Some(items) = document.item_records() {
            for (entry, raster) in items.raster_entries().iter().enumerate() {
                let target = &raster.target;
                if target.context_id() != symbol_id
                    || target.scope_kind() != &FrameScopeKind::Authored
                {
                    continue;
                }
                let at = *layers.get(target.layer_uid()).ok_or(NOT_EVALUABLE)?;
                if source_frames[at] == Some(target.frame()) {
                    images.insert(local + entry, raster);
                }
            }
        }
        let key = |target: &ObjectTarget| {
            (
                target.layer_uid().to_owned(),
                target.stroke_id().to_owned(),
                target.frame(),
            )
        };
        let shapes: HashMap<_, _> = placed
            .shapes
            .iter()
            .map(|shape| (key(&shape.target), shape))
            .collect();
        let mut baked = Vec::new();
        for layer in order.layers() {
            for element in layer.elements() {
                let index = element.source_object_index();
                if let Some(entry) = images.get(&index) {
                    let opacity = raw_opacity(self, symbol_id, entry.target.layer_uid(), inner)?;
                    baked.push(image(entry, placement, opacity)?);
                    continue;
                }
                let source = sources.get(&index).ok_or(NOT_EVALUABLE)?;
                // Only a source the override evaluator reported hidden is absent from the shapes.
                let Some(shape) = shapes.get(&key(source.target())) else {
                    continue;
                };
                let layer_raw = raw_opacity(self, symbol_id, source.target().layer_uid(), inner)?;
                baked.push(vector(&shape.source, &chain, layer_raw)?);
            }
        }
        Ok(baked)
    }

    /// The key frames of the layer's instance, each with the items it shows (`convertComponentToLayer`'s `frames`).
    pub(crate) fn bake_component_instance(
        &self,
        layer_uid: &str,
    ) -> Result<Vec<(u32, Vec<Baked>)>, BakeError> {
        let document = self.document();
        let set = document.component_set().ok_or(NOT_EVALUABLE)?;
        let instance = set
            .instance(SCENE_CONTEXT, layer_uid)
            .ok_or(NOT_EVALUABLE)?;
        let host = document
            .layers()
            .iter()
            .find(|layer| layer.layer_uid() == layer_uid)
            .ok_or(NOT_EVALUABLE)?;
        // The viewport applies the inner layers' opacity track and nothing else of their Motion; anything more would bake a
        // value the instance does not show.
        if document.property_holders().is_some_and(|holders| {
            holders
                .iter()
                .filter(|holder| holder.target().context_id() == instance.symbol_id())
                .any(|holder| component_holder_consumed(document, holder) != Some(true))
        }) {
            return Err(BakeError(
                "component-break-apart: Motion other than the layer opacity on the inner layers cannot be baked into the layer yet.",
            ));
        }
        let canvas = self.project().map(|p| {
            let (w, h) = p.project().canvas_dimensions();
            (f64::from(w), f64::from(h))
        });
        let has_camera = set
            .component(instance.symbol_id())
            .and_then(|c| c.camera_keys())
            .is_some_and(|keys| !keys.is_empty());
        if has_camera && canvas.is_none() {
            return Err(why(&ComponentUnavailable::Camera(
                "canvas size is not supplied",
            )));
        }
        let inputs = ComponentInputs {
            components: set,
            canvas: canvas.unwrap_or((0.0, 0.0)),
        };
        let mut ports = RecordPorts::new(set);
        let mut frames: Vec<(u32, Vec<Baked>)> = Vec::new();
        for frame in 0..document.total_frames {
            let selector = ObjectFrameSelector {
                instance_id: self.instance_id(),
                document_id: self.document_id(),
                content_revision: self.content_revision(),
                document_snapshot_id: self.snapshot_id(),
                context_id: SCENE_CONTEXT,
                scope_kind: FrameScopeKind::Authored,
                frame,
            };
            let packet =
                prepare_object_exposure_frame(self, &selector).map_err(|_| NOT_EVALUABLE)?;
            let outcome = evaluate_instance(
                &packet,
                &inputs,
                &mut ports,
                &mut PacketOverrides::new(&packet),
                instance,
                host,
            )
            .map_err(|reason| why(&reason))?;
            let items = match outcome {
                ComponentOutcome::Shapes(placed) => {
                    self.bake_frame(instance.symbol_id(), &placed)?
                }
                ComponentOutcome::Blank => Vec::new(),
                ComponentOutcome::Unavailable(reason) => return Err(why(&reason)),
            };
            if frames.last().is_none_or(|last| last.1 != items) {
                frames.push((frame, items));
            }
        }
        Ok(frames)
    }
}
