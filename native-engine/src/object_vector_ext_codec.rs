//! L01-E (G14/G22/G13): admission of the vector extension slot (pressure fields, `paramShape`, `boxAngle`)
//! and of the per-layer ordered `pathFx` stack. Pure checks over the decoded records; the single native
//! codec (object_codec.rs) calls them for every layer and record, so the legacy family stays byte-identical
//! and every refusal is a typed codec error. Nothing is clamped, defaulted or inferred.
//!
//! Rules the baseline states and this file keeps: `widthProfile.t` within 0..1 and not decreasing (what
//! `buildWidthProfile`/`denseProfile` guarantee), widths finite, a pressure ribbon never carries a real
//! stroke (`serP` forces `strokeColor:null, hasRealStroke:false` for `isVectorBrush`), only the kind's own
//! `paramShape` members, only the effect types of the baseline `TYPES` table. Rules the baseline does not
//! state are chosen conservatively and listed in HANDOFF.md (strokeProfile kinds, non-negative profileBase,
//! unknown effect types refused instead of silently skipped, an empty vectorExt refused).
#[path = "object_bitmap_brush_codec.rs"]
pub(crate) mod bitmap_brush;

#[path = "object_linked_fill_codec.rs"]
mod linked_fill;
pub(crate) use linked_fill::validate_linked_pairs;

use crate::object_codec::{ObjectCodecError, ObjectCodecErrorKind};
use crate::object_document::{
    BrushTexture,
    ObjectLayer, ObjectRecord, ParamShape, PressureFields, PARAM_SHAPE_KINDS, PATH_FX_TYPE_IDS,
    STROKE_PROFILE_KINDS, VECTOR_FAMILY,
};
use serde_json::Number;

fn invalid(message: &str) -> ObjectCodecError {
    ObjectCodecError::new(ObjectCodecErrorKind::Invalid, message)
}
fn unsupported(message: &str) -> ObjectCodecError {
    ObjectCodecError::new(ObjectCodecErrorKind::Unsupported, message)
}
fn finite(value: &Number) -> Option<f64> {
    value.as_f64().filter(|v| v.is_finite())
}

/// Admission of a record's extension slot; called for every record before the family checks.
pub(crate) fn validate_vector_ext(record: &ObjectRecord) -> Result<(), ObjectCodecError> {
    let ext = match record.vector_ext.value() {
        None => return Ok(()),
        Some(ext) => ext,
    };
    if record.family != VECTOR_FAMILY {
        return Err(unsupported(
            "vectorExt requires the editable-vector-path family",
        ));
    }
    if ext.is_empty() {
        return Err(invalid("an empty vectorExt must be omitted"));
    }
    // Keep authored null/default/unknown mode spelling under existing ingress limits.
    if ext
        .mask_feather
        .value()
        .and_then(Option::as_ref)
        .is_some_and(|feather| finite(feather).is_none())
    {
        return Err(invalid("maskFeather must be finite"));
    }
    if ext.fill_trace.value().is_some_and(|trace| !trace.valid()) {
        return Err(invalid("invalid fill trace provenance"));
    }
    if ext.texture.value().is_some_and(|t| !t.valid()) {
        return Err(invalid("invalid vector texture identity"));
    }
    linked_fill::validate_record(record)?;
    bitmap_brush::validate(record)?;
    if let Some(gradient) = ext.fill_gradient.value() {
        if !gradient.valid() {
            return Err(invalid("invalid stored two-stop gradient"));
        }
        if !ext.pressure.missing() {
            return Err(unsupported("pressure-gradient consumer is unavailable"));
        }
    }
    if let Some(pressure) = ext.pressure.value() {
        validate_pressure(pressure)?;
        let real_stroke = record
            .vector
            .value()
            .is_some_and(|body| body.has_real_stroke() || body.solid_stroke().is_some());
        if real_stroke {
            return Err(invalid(
                "a pressure stroke is painted through its fill and has no real stroke",
            ));
        }
    }
    if let Some(shape) = ext.param_shape.value() {
        validate_param_shape(shape)?;
    }
    if let Some(tags) = ext.brush_texture.value() {
        validate_brush_texture(tags)?;
    }
    if ext
        .box_angle
        .value()
        .is_some_and(|angle| finite(angle).is_none())
    {
        return Err(invalid("boxAngle must be finite"));
    }
    Ok(())
}

/// serP writes these members only when truthy (`preTexture*` whenever defined); an anchor and a dab are
/// distinct roles, so one record cannot be both.
fn validate_brush_texture(tags: &BrushTexture) -> Result<(), ObjectCodecError> {
    let text = |v: &String| !v.is_empty() && v.len() <= 255 && !v.contains('\0');
    let colour = |v: &String| {
        [4, 7, 9].contains(&v.len()) && v.starts_with('#') && v.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
    };
    if tags.brush_group_id.missing()
        && tags.is_brush_texture_copy.missing()
        && tags.brush_texture_preset.missing()
        && tags.pre_texture_opacity.missing()
        && tags.pre_texture_stroke.missing()
        && tags.stroke_seed.missing()
    {
        return Err(invalid("an empty brushTexture must be omitted"));
    }
    if tags.brush_group_id.value().is_some_and(|v| !text(v))
        || tags.brush_texture_preset.value().is_some_and(|v| !text(v))
        || tags.is_brush_texture_copy.value() == Some(&false)
        || tags.pre_texture_opacity.value().is_some_and(|n| finite(n).is_none())
        || tags.pre_texture_stroke.value().and_then(Option::as_ref).is_some_and(|v| !colour(v))
    {
        return Err(invalid("invalid brushTexture member"));
    }
    if tags.is_brush_texture_copy.value().is_some() && tags.brush_texture_preset.value().is_some() {
        return Err(invalid("a brush-texture dab is not also an anchor"));
    }
    // The stamping seed belongs to an anchor (it names its preset) and is an unsigned 32-bit integer.
    if let Some(seed) = tags.stroke_seed.value() {
        if seed.as_u64().is_none_or(|s| s > u64::from(u32::MAX)) || tags.brush_texture_preset.missing() {
            return Err(invalid("strokeSeed is a uint32 of a brush-texture anchor"));
        }
    }
    Ok(())
}

/// `t` is only required to be within 0..1 and non-decreasing, which is all `buildWidthProfile` (equal `t`
/// for a stationary pen) and `denseProfile` guarantee; widths only have to be finite, and the centreline is
/// stored exactly as authored (no minimum anchor count, no bound).
fn validate_pressure(fields: &PressureFields) -> Result<(), ObjectCodecError> {
    if let Some(anchors) = fields.center_segments.value() {
        for anchor in anchors {
            for point in [&anchor.point, &anchor.handle_in, &anchor.handle_out] {
                if finite(&point.x).is_none() || finite(&point.y).is_none() {
                    return Err(invalid("centerSegments coordinates must be finite"));
                }
            }
            if finite(&anchor.width).is_none() {
                return Err(invalid("centerSegments width must be finite"));
            }
        }
    }
    if let Some(stops) = fields.width_profile.value() {
        let mut previous = 0.0;
        for stop in stops {
            match finite(&stop.t).filter(|t| (0.0..=1.0).contains(t)) {
                Some(t) if t >= previous => previous = t,
                _ => {
                    return Err(invalid(
                        "widthProfile t must be within 0..1 and not decrease",
                    ))
                }
            }
            if finite(&stop.width).is_none() {
                return Err(invalid("widthProfile width must be finite"));
            }
        }
    }
    if let Some(kind) = fields.stroke_profile.value() {
        if !STROKE_PROFILE_KINDS.contains(&kind.as_str()) {
            return Err(unsupported(
                "strokeProfile must be taper-in, taper-out, taper-both, bulge or even",
            ));
        }
    }
    if let Some(base) = fields.profile_base.value() {
        if !finite(base).is_some_and(|v| v >= 0.0) {
            return Err(invalid("profileBase must be finite and not negative"));
        }
    }
    Ok(())
}

/// A known kind, only that kind's members, finite numbers, a finite frame. No range is imposed: the
/// baseline builders clamp (`max(3, round(pointCount||5))`, radius to half the short side).
fn validate_param_shape(shape: &ParamShape) -> Result<(), ObjectCodecError> {
    if !PARAM_SHAPE_KINDS.contains(&shape.kind.as_str()) {
        return Err(unsupported("paramShape kind must be rect, ellipse or star"));
    }
    let members = [
        ("rect", &shape.tl),
        ("rect", &shape.tr),
        ("rect", &shape.br),
        ("rect", &shape.bl),
        ("ellipse", &shape.start_angle),
        ("ellipse", &shape.sweep),
        ("ellipse", &shape.inner_radius),
        ("star", &shape.point_count),
        ("star", &shape.inner_ratio),
        ("star", &shape.corner_radius),
    ];
    for (owner, member) in members {
        if let Some(number) = member.value() {
            if owner != shape.kind.as_str() {
                return Err(invalid("paramShape member does not belong to its kind"));
            }
            if finite(number).is_none() {
                return Err(invalid("paramShape numbers must be finite"));
            }
        }
    }
    if let Some(frame) = shape.frame.value() {
        if [&frame.x1, &frame.y1, &frame.x2, &frame.y2]
            .into_iter()
            .any(|n| finite(n).is_none())
        {
            return Err(invalid("paramShape box coordinates must be finite"));
        }
    }
    Ok(())
}

/// Admission of one layer's stack (absent is fine; an empty stack is an authored empty stack).
pub(crate) fn validate_layer_path_fx(layer: &ObjectLayer) -> Result<(), ObjectCodecError> {
    let stack = match layer.path_fx.value() {
        None => return Ok(()),
        Some(stack) => stack,
    };
    for entry in stack {
        if !PATH_FX_TYPE_IDS.contains(&entry.fx_type.as_str()) {
            return Err(unsupported("pathFx type must be zigzag or roughen"));
        }
        for slot in [&entry.p1, &entry.p2, &entry.p3] {
            let number = slot.value().and_then(Option::as_ref);
            if number.is_some_and(|n| finite(n).is_none()) {
                return Err(invalid("pathFx parameters must be finite numbers or null"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/object_vector_extensions.rs"]
mod object_vector_extensions_tests;
