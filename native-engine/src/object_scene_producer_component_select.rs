//! Input side of the component scene producer (L07-K; D32, I16a).
//!
//! Turns one component instance at one outer frame into the inputs of the L07-H
//! kernel, following `getEffectiveStrokes` (`src/js/app.js`, `symbolId` branch) at
//! baseline `fbe4b8a3`:
//! * `playback_input`: the held OUTER key (nearest authored timing key at or before
//!   the frame, scanning down) carries `componentFrame` / `blankOverride`; the rest
//!   of `resolveSymbolFrameIdx` reads the `sym*` fields of the instance record;
//! * `select_records`: for every visible inner layer, in the component's layer order,
//!   the records of the frame it exposes at the inner frame (a key or an interpolated
//!   sample shows its own frame, a hold shows the keyframe before it, an unexposed
//!   frame shows nothing); an inner layer whose features this producer cannot
//!   evaluate (active path effects) is a typed refusal, never skipped. A widget inner
//!   layer is NOT special-cased: the baseline `symbolId` branch never reads
//!   `isWidgetLayer` of `sym.layers`, it simply shows whatever records the layer holds;
//! * `evaluate_records`: the stored geometry as `ShapeData`, through the L07-H
//!   placement chain (component camera, then instance matrix), paired back with the
//!   stable target and the typed paint of the SOURCE record.
//!
//! Nothing here mutates a record: selection clones, the kernel returns detached
//! copies. The baseline steps that depend on providers that do not exist natively
//! (element and layer Motion, the inner duplicator, Time Remap sampling, component
//! camera keys) are ports of the join or typed unavailabilities, never defaults.

use crate::evaluation::object_component_kernels::object_component_exposed::ExposedError;
use crate::evaluation::object_component_kernels::object_component_playback::{
    OuterKey, PlayMode, PlaybackInput,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component::{
    evaluate_component_shapes, EvaluatedShape, ResolvedComponentPlacement,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_shape::{
    PathSegment, ShapeData, ShapeError,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::SceneSolid;
use crate::object_document::object_component_record_kernels::object_component_records::{
    ComponentInstance, ComponentRecord,
};
use crate::object_document::{
    FrameScopeKind, LayerTiming, ObjectLayer, ObjectPoint, ObjectRecord, ObjectTarget,
    RangeEvaluator,
};
use serde_json::{Map, Number, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// The dense L04-A outer-key row costs one `OuterKey` per frame up to the held key, so a
/// key beyond this frame is a typed unavailable slot until the kernel takes a sparse row.
pub(crate) const MAX_HELD_FRAME: u32 = 100_000;

pub(crate) const VECTOR_BODY: &str =
    "vector body stroke and gradient drawing belongs to the style consumer";
pub(crate) const NO_FILL: &str = "record carries no solid fill";

/// Why an instance (not the whole frame) cannot be evaluated. The slot of the
/// placing layer stays in the scene with this reason.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ComponentUnavailable {
    /// A stored outer/entered range requires a provider the immutable reader lacks.
    RangeProvider { layer_uid: String },
    /// The instance names a component the table does not hold.
    UnresolvedComponent { symbol_id: String },
    /// Frame keys exist but the placing layer has no authored timing, so a key
    /// cannot be told from a hold.
    HostTimingMissing,
    /// The held outer key that carries a field lies beyond `MAX_HELD_FRAME`.
    HostTimingTooLong { held: u32 },
    /// The instance has Time Remap and the sampler port reported.
    TimeRemap(&'static str),
    /// `symMatrix` is not six finite numbers.
    InvalidSymMatrix,
    /// The component camera port reported.
    Camera(&'static str),
    /// An inner layer has no authored timing: its exposure is unknown.
    InnerTimingMissing { layer_uid: String },
    /// An inner layer or selected record carries a feature this producer does not evaluate.
    InnerFeature {
        layer_uid: String,
        feature: &'static str,
    },
    /// A stored record is not a relative-handle cubic path or a coordinate is not finite.
    Geometry {
        stroke_id: String,
        reason: &'static str,
    },
    /// Exposed properties are declared and the property runtime port is unavailable.
    OverridesUnavailable,
    /// An exposed property override refused.
    Overrides(ExposedError),
    /// The shape kernel refused one shape; nothing of the instance is published.
    Shape {
        stroke_id: String,
        cause: ShapeError,
    },
}

/// The paint of the source record, typed; geometry survives either way.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ShapePaint {
    Solid(SceneSolid),
    /// No typed paint yet (vector body, missing or unrepresentable fill): the
    /// consumer decides, the shape is not dropped.
    Pending(&'static str),
}

/// One evaluated shape of a component instance.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ComponentShape {
    /// Stable identity of the source record: component context, inner layer,
    /// stroke id and the authored frame it was read from.
    pub(crate) target: ObjectTarget,
    /// Detached transformed copy (L07-H); the stored record is untouched.
    pub(crate) shape: EvaluatedShape,
    pub(crate) paint: ShapePaint,
    /// Immutable post-override source style; never a second document authority.
    pub(crate) source: Arc<ObjectRecord>,
}

/// `held` is the greatest timing key at or before `main_frame`; interpolated
/// samples are not keys (the baseline scans for `isKeyframe`).
fn held_key_frame(timing: &LayerTiming, main_frame: u32) -> Option<u32> {
    let mut frame = main_frame;
    loop {
        let (kind, source, _) = timing.exposure(frame);
        match kind {
            "key" => return Some(frame),
            "held" => return source,
            "interpolated" if frame > 0 => frame -= 1,
            _ => return None,
        }
    }
}

/// The `PlaybackInput` of `resolveSymbolFrameIdx` for one instance. The kernel scans
/// the outer keys down from the frame, so only the held key matters: a row of
/// `held + 1` entries whose last entry is that key reproduces the scan exactly. A held
/// key that carries neither `componentFrame` nor `blankOverride` decides nothing (the
/// scan stops on it and plays by mode, as with no key), so it costs no row at all; a key
/// that does carry a field beyond `MAX_HELD_FRAME` is `HostTimingTooLong`, never an
/// allocation sized by an unbounded document frame.
pub(crate) fn playback_input(
    instance: &ComponentInstance,
    host: &ObjectLayer,
    main_frame: u32,
) -> Result<PlaybackInput, ComponentUnavailable> {
    let timing = host.timing.value();
    if timing.is_none() && !instance.frame_keys().is_empty() {
        return Err(ComponentUnavailable::HostTimingMissing);
    }
    let outer_keys = match timing.and_then(|t| held_key_frame(t, main_frame)) {
        None => Vec::new(),
        Some(held) => match instance.frame_keys().iter().find(|k| k.frame() == held) {
            None => Vec::new(),
            Some(_) if held > MAX_HELD_FRAME => {
                return Err(ComponentUnavailable::HostTimingTooLong { held });
            }
            Some(key) => {
                let mut keys = vec![OuterKey::default(); held as usize + 1];
                let last = &mut keys[held as usize];
                last.is_keyframe = true;
                last.component_frame = key.component_frame();
                last.blank_override = key.blank_override();
                keys
            }
        },
    };
    Ok(PlaybackInput {
        play_mode: PlayMode::from_js(instance.play_mode()),
        speed: instance.speed(),
        placed_at: instance.placed_at(),
        single_frame: instance.single_frame(),
        trim_in: instance.trim_in(),
        trim_out: instance.trim_out(),
        outer_keys,
    })
}

/// `symMatrixOf`: any present array applies, even the identity; six finite numbers.
pub(crate) fn sym_matrix(
    instance: &ComponentInstance,
) -> Result<Option<[f64; 6]>, ComponentUnavailable> {
    let Some(numbers) = instance.matrix() else {
        return Ok(None);
    };
    if numbers.len() != 6 {
        return Err(ComponentUnavailable::InvalidSymMatrix);
    }
    let mut matrix = [0.0; 6];
    for (slot, number) in matrix.iter_mut().zip(numbers) {
        *slot = number
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or(ComponentUnavailable::InvalidSymMatrix)?;
    }
    Ok(Some(matrix))
}

/// The records every visible inner layer shows at `inner_frame`, layer by layer in
/// the component's layer order, record order inside a layer as stored.
pub(crate) fn select_records(
    component: &ComponentRecord,
    inner_frame: u32,
) -> Result<Vec<ObjectRecord>, ComponentUnavailable> {
    select_records_with_ranges(component, inner_frame, None)
}

/// Entered-component reader only. Placed instances keep the baseline's separate
/// symbol branch, which does not apply inner layer ranges.
pub(crate) fn select_records_with_ranges(
    component: &ComponentRecord,
    inner_frame: u32,
    ranges: Option<&RangeEvaluator<'_>>,
) -> Result<Vec<ObjectRecord>, ComponentUnavailable> {
    let layers = component.layers();
    let mut sources: Vec<Option<u32>> = Vec::with_capacity(layers.len());
    for layer in layers {
        if !layer.visible() {
            sources.push(None);
            continue;
        }
        let has_active_path_fx = layer
            .path_fx()
            .is_some_and(|stack| stack.iter().any(|fx| fx.enabled_flag() != Some(false)));
        if has_active_path_fx {
            return Err(ComponentUnavailable::InnerFeature {
                layer_uid: layer.layer_uid().to_owned(),
                feature: "path effects",
            });
        }
        let Some(timing) = layer.timing.value() else {
            return Err(ComponentUnavailable::InnerTimingMissing {
                layer_uid: layer.layer_uid().to_owned(),
            });
        };
        let included = match ranges {
            None => true,
            Some(ranges) => {
                let unavailable = |_| ComponentUnavailable::InnerFeature {
                    layer_uid: layer.layer_uid().to_owned(),
                    feature: "layer range provider",
                };
                !ranges.gates(layer.layer_uid()).map_err(unavailable)?
                    || ranges
                        .resolve(layer.layer_uid(), true)
                        .map_err(unavailable)?
                        .contains(inner_frame)
            }
        };
        sources.push(if included {
            timing.exposure(inner_frame).1
        } else {
            None
        });
    }
    let index: HashMap<&str, usize> = layers
        .iter()
        .enumerate()
        .map(|(at, layer)| (layer.layer_uid(), at))
        .collect();
    let mut grouped: Vec<Vec<&ObjectRecord>> = vec![Vec::new(); layers.len()];
    for record in component.objects() {
        let target = record.target();
        let Some(&at) = index.get(target.layer_uid()) else {
            continue;
        };
        if sources[at] == Some(target.frame()) && target.scope_kind() == &FrameScopeKind::Authored {
            grouped[at].push(record);
        }
    }
    Ok(grouped.into_iter().flatten().cloned().collect())
}

fn finite(number: &Number) -> Option<f64> {
    number.as_f64().filter(|v| v.is_finite())
}

fn pair(point: &ObjectPoint) -> Option<[f64; 2]> {
    Some([finite(&point.x)?, finite(&point.y)?])
}

/// The stored cubic path as the kernel's shape: identity and topology only
/// (`strokeId`, `closed`); paint stays with the source record.
pub(crate) fn shape_data(record: &ObjectRecord) -> Result<ShapeData, ComponentUnavailable> {
    let stroke_id = record.target().stroke_id();
    let geometry = &record.geometry;
    let refuse = |reason: &'static str| ComponentUnavailable::Geometry {
        stroke_id: stroke_id.to_owned(),
        reason,
    };
    if geometry.kind != "cubic-path" || geometry.handle_space != "relative" {
        return Err(refuse("geometry is not a relative-handle cubic path"));
    }
    let mut segments = Vec::with_capacity(geometry.segments.len());
    for segment in &geometry.segments {
        let (Some(point), Some(handle_in), Some(handle_out)) = (
            pair(&segment.point),
            pair(&segment.handle_in),
            pair(&segment.handle_out),
        ) else {
            return Err(refuse("coordinates must be finite f64 values"));
        };
        segments.push(PathSegment {
            point,
            handle_in,
            handle_out,
        });
    }
    let mut rest = Map::new();
    rest.insert("strokeId".to_owned(), Value::String(stroke_id.to_owned()));
    rest.insert("closed".to_owned(), Value::Bool(geometry.closed));
    Ok(ShapeData::Path { segments, rest })
}

/// The typed paint of the source record (never rewritten).
pub(crate) fn shape_paint(record: &ObjectRecord) -> ShapePaint {
    if record.vector().is_some() {
        return ShapePaint::Pending(VECTOR_BODY);
    }
    match record.solid_fill() {
        None => ShapePaint::Pending(NO_FILL),
        Some(fill) => match SceneSolid::from_fill(fill) {
            Ok(solid) => ShapePaint::Solid(solid),
            Err(error) => ShapePaint::Pending(error.message()),
        },
    }
}

/// Every record through the placement chain, in order; one failing shape publishes
/// none of the instance.
pub(crate) fn evaluate_records(
    records: &[ObjectRecord],
    placement: &ResolvedComponentPlacement,
) -> Result<Vec<ComponentShape>, ComponentUnavailable> {
    evaluate_owned_records(records.to_vec(), placement)
}

/// Production transfers its detached post-override records into immutable
/// source handles, preserving full typed style without another geometry copy.
pub(crate) fn evaluate_owned_records(
    records: Vec<ObjectRecord>,
    placement: &ResolvedComponentPlacement,
) -> Result<Vec<ComponentShape>, ComponentUnavailable> {
    if let Some(record) = records.iter().find(|record| record.has_enabled_effects()) {
        return Err(ComponentUnavailable::InnerFeature {
            layer_uid: record.target().layer_uid().to_owned(),
            feature: "per-element effects",
        });
    }
    let sources = records
        .iter()
        .map(shape_data)
        .collect::<Result<Vec<_>, _>>()?;
    let evaluated = evaluate_component_shapes(&sources, placement).map_err(|error| {
        ComponentUnavailable::Shape {
            stroke_id: records
                .get(error.shape_index)
                .map(|record| record.target().stroke_id().to_owned())
                .unwrap_or_default(),
            cause: error.cause,
        }
    })?;
    Ok(records
        .into_iter()
        .zip(evaluated.shapes)
        .map(|(record, shape)| {
            let target = record.target().clone();
            let paint = shape_paint(&record);
            ComponentShape {
                target,
                shape,
                paint,
                source: Arc::new(record),
            }
        })
        .collect())
}

/// Shared admission/consumer declaration for declared component holders only.
/// None preserves the caller's existing root/order-only policy. Element values
/// stay unavailable until BOTH entered and placed paints consume them.
pub(crate) fn component_holder_consumed(
    document: &crate::object_document::ObjectDocument,
    holder: &crate::object_document::PropertyHolder,
) -> Option<bool> {
    let target = holder.target();
    let component = document.component_set()?.component(target.context_id())?;
    let local = component
        .layers()
        .iter()
        .any(|layer| layer.layer_uid() == target.layer_uid())
        && target.stroke_id().is_none_or(|stroke| {
            component.objects().iter().any(|record| {
                let item = record.target();
                item.context_id() == target.context_id()
                    && item.layer_uid() == target.layer_uid()
                    && item.stroke_id() == stroke
                    && item.scope_kind() == &FrameScopeKind::Authored
            })
        });
    Some(
        local
            && !holder
                .controls
                .value()
                .is_some_and(|controls| !controls.is_empty())
            && holder.properties().iter().all(|record| {
                target.stroke_id().is_none()
                    && record.property() == "opacity"
                    && !record
                        .expression
                        .value()
                        .is_some_and(|expression| expression.enabled)
            }),
    )
}
