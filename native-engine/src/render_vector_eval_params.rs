//! Parametric side of the vector evaluation join (G22, #2107, leaf L01-H): the stored `paramShape` record read as the
//! L01-B kernel's `ParamShape`, the animated parameters of the stroke's element holder sampled with the D11 raw sampler,
//! and the predicate that says a holder table is fully consumed by this join.
//!
//! A parameter is animated exactly when its holder property has a non-empty track (`isAnimated` = `hasKeys`); a track with
//! zero keys, a static value or the catalog default never counts, and an unkeyed parameter falls back to the STORED
//! `paramShape` member (`cv(key, fallback)`). Only the keys of the shape's own kind are read (`hasParamShapeMotionFor`).
use crate::evaluation::object_vector_kernels::object_parametric_geometry::boxed::{
    AnimatedParams, ParamShape, ParamShapeKind,
};
use crate::evaluation::object_vector_kernels::render_vector_eval::{finite, VectorEvalError};
use crate::object_document::{
    FrameScopeKind, ObjectDocument, ObjectRecord, ParamShape as ParamShapeRecord, Presence,
    PropertyHolder, PropertyHolderTarget, VECTOR_FAMILY,
};
use crate::object_frame_packet::{sample_property_raw, PropertySampleError};
use crate::revision::ObjectSnapshot;
use serde_json::Number;

const RECT_KEYS: [&str; 4] = ["cornerTL", "cornerTR", "cornerBR", "cornerBL"];
const ELLIPSE_KEYS: [&str; 3] = ["arcStart", "arcSweep", "arcInner"];
const STAR_KEYS: [&str; 2] = ["starInner", "starCorner"];

/// The Motion properties the rebuild of a shape of this kind reads.
pub(crate) fn keys_of(kind: ParamShapeKind) -> &'static [&'static str] {
    match kind {
        ParamShapeKind::Rect => &RECT_KEYS,
        ParamShapeKind::Ellipse => &ELLIPSE_KEYS,
        ParamShapeKind::Star => &STAR_KEYS,
    }
}

fn member(slot: &Presence<Number>) -> Result<Option<f64>, VectorEvalError> {
    slot.value().map(finite).transpose()
}

/// The stored declared intent of a record as the kernel reads it; `None` for a record without a `paramShape`.
pub(crate) fn shape_of(record: &ObjectRecord) -> Result<Option<ParamShape>, VectorEvalError> {
    let Some(stored) = record
        .vector_ext
        .value()
        .and_then(|ext| ext.param_shape.value())
    else {
        return Ok(None);
    };
    kernel_shape(stored).map(Some)
}

fn kernel_shape(stored: &ParamShapeRecord) -> Result<ParamShape, VectorEvalError> {
    match stored.kind.as_str() {
        "rect" => Ok(ParamShape::Rect {
            tl: member(&stored.tl)?,
            tr: member(&stored.tr)?,
            br: member(&stored.br)?,
            bl: member(&stored.bl)?,
        }),
        "ellipse" => Ok(ParamShape::Ellipse {
            start_angle: member(&stored.start_angle)?,
            sweep: member(&stored.sweep)?,
            inner_radius: member(&stored.inner_radius)?,
        }),
        "star" => Ok(ParamShape::Star {
            point_count: member(&stored.point_count)?,
            inner_ratio: member(&stored.inner_ratio)?,
            corner_radius: member(&stored.corner_radius)?,
        }),
        _ => Err(VectorEvalError::UnknownShapeKind),
    }
}

/// The animated tracks of the record's element holder sampled at `frame`; `None` for an unkeyed parameter.
pub(crate) fn animated_params(
    snapshot: &ObjectSnapshot,
    record: &ObjectRecord,
    kind: ParamShapeKind,
    frame: f64,
) -> Result<AnimatedParams, VectorEvalError> {
    let target = PropertyHolderTarget {
        context_id: record.target.context_id.clone(),
        layer_uid: record.target.layer_uid.clone(),
        stroke_id: Presence::Present(record.target.stroke_id.clone()),
    };
    let holder = snapshot
        .document()
        .property_holders()
        .and_then(|holders| holders.iter().find(|holder| holder.target() == &target));
    let sample = |property: &'static str| track_value(snapshot, &target, holder, property, frame);
    let mut animated = AnimatedParams::default();
    match kind {
        ParamShapeKind::Rect => {
            animated.corner_tl = sample("cornerTL")?;
            animated.corner_tr = sample("cornerTR")?;
            animated.corner_br = sample("cornerBR")?;
            animated.corner_bl = sample("cornerBL")?;
        }
        ParamShapeKind::Ellipse => {
            animated.arc_start = sample("arcStart")?;
            animated.arc_sweep = sample("arcSweep")?;
            animated.arc_inner = sample("arcInner")?;
        }
        ParamShapeKind::Star => {
            animated.star_inner = sample("starInner")?;
            animated.star_corner = sample("starCorner")?;
        }
    }
    Ok(animated)
}

/// `isAnimated(holder, key) ? valueAtFrame(holder, key, frame)[0] : unkeyed`.
fn track_value(
    snapshot: &ObjectSnapshot,
    target: &PropertyHolderTarget,
    holder: Option<&PropertyHolder>,
    property: &'static str,
    frame: f64,
) -> Result<Option<f64>, VectorEvalError> {
    let record = holder.and_then(|holder| {
        holder
            .properties()
            .iter()
            .find(|record| record.property() == property)
    });
    let keyed = record.is_some_and(|record| {
        record
            .track
            .value()
            .is_some_and(|track| !track.keys.is_empty())
    });
    if !keyed {
        return Ok(None);
    }
    if record.is_some_and(|record| record.expression.value().is_some_and(|code| code.enabled)) {
        return Err(VectorEvalError::ExpressionUnavailable { property });
    }
    let values = sample_property_raw(snapshot, target, property, frame)
        .map_err(|error| VectorEvalError::Sample { property, error })?;
    values
        .first()
        .copied()
        .map(Some)
        .ok_or(VectorEvalError::Sample {
            property,
            error: PropertySampleError::Unavailable,
        })
}

/// Content-free static Nulls may supply an ancestor pose, never a paint item.
/// Match the existing parent consumer's unsupported-context checks.
pub(crate) fn null_layer_motion_target(
    document: &ObjectDocument,
    target: &PropertyHolderTarget,
) -> bool {
    if document.format_version() != 2
        || target.context_id() != "scene-root"
        || target.stroke_id().is_some()
    {
        return false;
    }
    let Some(layer) = document
        .layers()
        .iter()
        .find(|l| l.layer_uid() == target.layer_uid())
    else {
        return false;
    };
    layer.is_null_layer()
        && layer.three_d.value() != Some(&true)
        && !layer.is_effect_layer()
        && !layer.is_folder_layer()
        && layer.widget.value().is_none()
        && layer.groups.value().is_none()
        && layer.rig.value().is_none()
        && !layer.folder_id.value().is_some_and(Option::is_some)
        && !layer.follow_path.value().is_some_and(Option::is_some)
        && !layer.blend_mode().is_some_and(|v| v != "normal")
        && !layer.matte_mode().is_some_and(|v| v != "none")
        && layer.matte_source_layer_uid.value().is_none()
        && !layer.mattes_more.value().is_some_and(|v| !v.is_empty())
        && !layer.effects.value().is_some_and(|v| !v.is_empty())
        && layer.effects_from.value().is_none()
        && !layer
            .path_fx()
            .is_some_and(|fx| fx.iter().any(|f| f.enabled_flag() != Some(false)))
        && !document.duplicator_set().is_some_and(|set| {
            set.duplicator(target.context_id(), target.layer_uid())
                .is_some()
        })
        && !crate::evaluation::object_vector_kernels::render_vector_eval::has_unjoined_parent_blend(
            document, target,
        )
        && !document.objects().iter().any(|r| {
            r.target.context_id == target.context_id() && r.target.layer_uid == target.layer_uid()
        })
        && document.item_records().is_none_or(|items| {
            !items.targets().any(|t| {
                t.context_id() == target.context_id() && t.layer_uid() == target.layer_uid()
            })
        })
}

/// True when every property holder of the document is one this join consumes: an element holder of a stroke whose every
/// authored record is a vector record with a `paramShape`, holding only keys of that shape's kind, no enabled expression
/// and no expression controls. Ordinary root layer holders admit raw opacity; Position is additionally
/// consumed only where every content record is a vector reaching the Motion join (no raster/text siblings).
/// Text/raster layer poses and their actual ancestors use the scene parent consumer.
/// Visible vector parents, unsupported properties and expressions remain unavailable.
pub(crate) fn holders_fully_consumed(document: &ObjectDocument) -> bool {
    holders_consumed(document, false)
}

/// The owner frame route supplies the real evaluated duplicator/effector port.
/// All other holders, expression families and paint boundaries stay strict.
pub(crate) fn holders_fully_consumed_with_motion(document: &ObjectDocument) -> bool {
    holders_consumed(document, true)
}
fn holders_consumed(document: &ObjectDocument, evaluated: bool) -> bool {
    if !crate::evaluation::object_vector_kernels::render_vector_eval::motion::visible_vector_parents_supported(document) {
        return false;
    }
    document.property_holders().is_none_or(|holders| {
        holders.iter().all(|holder| {
            let remaining = if evaluated {
                crate::object_frame_packet::duplicators::remaining_holder_with_motion(
                    document, holder, true,
                )
            } else {
                crate::object_frame_packet::duplicators::remaining_holder(document, holder)
            };
            let remaining = crate::object_frame_packet::motion_receipts::remaining_holder(
                document,
                remaining.as_ref().unwrap_or(holder),
                evaluated,
            )
            .or(remaining);
            match remaining {
                Some(ref remaining) if remaining.properties().is_empty() => remaining
                    .controls
                    .value()
                    .is_none_or(|controls| controls.is_empty()),
                Some(ref remaining) => holder_consumed(document, remaining),
                None => holder_consumed(document, holder),
            }
        })
    })
}

fn holder_consumed(document: &ObjectDocument, holder: &PropertyHolder) -> bool {
    if crate::object_document::bitmap_brush::trim_holder_consumed(document, holder) {
        return true;
    }
    // Exposed component keys on an instance layer are read by the component override port.
    if let Some(rest) = crate::object_document::without_exposed(document.component_set(), holder) {
        return rest
            .controls
            .value()
            .is_none_or(|controls| controls.is_empty())
            && (rest.properties().is_empty() || holder_consumed(document, &rest));
    }
    if let Some(consumed) = crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::component_holder_consumed(document, holder) {
        return consumed;
    }
    let target = holder.target();
    if holder
        .controls
        .value()
        .is_some_and(|controls| !controls.is_empty())
    {
        return false;
    }
    let Some(stroke_id) = target.stroke_id() else {
        return crate::evaluation::object_vector_kernels::render_vector_eval::motion::layer_holder_consumed(document, holder);
    };
    if crate::object_frame_packet::motion_receipts::text_element(document, target) {
        return holder.properties().iter().all(|p| {
            crate::object_frame_packet::motion_receipts::KEYS.contains(&p.property())
                && !p.expression.value().is_some_and(|e| e.enabled)
        });
    }
    let mut matched = false;
    for record in document.objects() {
        let owner = &record.target;
        if owner.context_id != target.context_id()
            || owner.layer_uid != target.layer_uid()
            || owner.stroke_id != stroke_id
            || owner.frame_scope.kind != FrameScopeKind::Authored
        {
            continue;
        }
        matched = true;
        if record.family != VECTOR_FAMILY {
            return false;
        }
        let keys = match shape_of(record) {
            Ok(Some(shape)) => keys_of(shape.kind()),
            Ok(None) => &[],
            Err(_) => return false,
        };
        let consumed = holder.properties().iter().all(|property| {
            (keys.iter().any(|key| *key == property.property())
                || matches!(
                    property.property(),
                    "position" | "anchor" | "rotation" | "scale"
                ))
                && !property.expression.value().is_some_and(|code| code.enabled)
        });
        if !consumed {
            return false;
        }
    }
    matched
}
