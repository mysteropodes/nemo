//! Authored bitmap-anchor data. The companion remains the existing RasterItem;
//! its asset, rectangle and opacity are never copied into this extension.
use crate::object_document::{object_deserialize, ObjectRecord, Presence, VECTOR_FAMILY};
use serde::{Deserialize, Serialize};
use serde_json::Number;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BitmapBrushSpec {
    pub(crate) tip: String,
    pub(crate) size: Number,
    pub(crate) spacing: Number,
    pub(crate) scatter: Number,
    pub(crate) opacity: Number,
    pub(crate) color: String,
    pub(crate) pressure: bool,
    pub(crate) seed: Number,
}
object_deserialize!(BitmapBrushSpec {
    tip: String,
    size: Number,
    spacing: Number,
    scatter: Number,
    opacity: Number,
    color: String,
    pressure: bool,
    seed: Number
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BitmapBrushFields {
    /// Native provenance for interpStroke output, which never runs applyBitmapBrushTexture's camouflage.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) tween_generated: Presence<bool>,
    pub(crate) bitmap_brush_spec: BitmapBrushSpec,
    pub(crate) brush_group_id: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) pre_texture_stroke: Presence<Option<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) pre_texture_opacity: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) bitmap_pressure_profile: Presence<Vec<[Number; 3]>>,
}
object_deserialize!(BitmapBrushFields {
    #[serde(default)] tween_generated: Presence<bool>,
    bitmap_brush_spec: BitmapBrushSpec,
    brush_group_id: String,
    #[serde(default)] pre_texture_stroke: Presence<Option<String>>,
    #[serde(default)] pre_texture_opacity: Presence<Number>,
    #[serde(default)] bitmap_pressure_profile: Presence<Vec<[Number; 3]>>
});

/// Pure document-value admission. Transport/codec errors are mapped by the
/// existing codec adapter; this domain owns the one set of value rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BitmapBrushValueError {
    Invalid(&'static str),
    Unsupported(&'static str),
}

pub(crate) const PROCEDURAL_TIPS: [&str; 10] = [
    "soft",
    "hard",
    "grain",
    "charcoal-rough",
    "dry-brush",
    "watercolor-edge",
    "splatter",
    "ink-splatter-fine",
    "stipple",
    "marker-flat",
];
fn invalid(message: &'static str) -> BitmapBrushValueError {
    BitmapBrushValueError::Invalid(message)
}
/// Same hex forms as the existing native stored-colour boundary.
pub(crate) fn rgba(text: &str) -> Option<[u8; 4]> {
    let bytes = text.strip_prefix('#')?;
    if ![3, 6, 8].contains(&bytes.len()) || !bytes.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    if bytes.len() == 3 {
        let mut out = [255; 4];
        for (index, digit) in bytes.bytes().enumerate() {
            out[index] = (digit as char).to_digit(16)? as u8 * 17;
        }
        return Some(out);
    }
    let mut out = [255; 4];
    for (index, pair) in bytes.as_bytes().chunks_exact(2).enumerate() {
        out[index] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(out)
}
pub(crate) fn validate_fields(fields: &BitmapBrushFields) -> Result<(), BitmapBrushValueError> {
    let spec = &fields.bitmap_brush_spec;
    if fields.brush_group_id.is_empty()
        || fields.brush_group_id.len() > 255
        || fields.brush_group_id.contains('\0')
    {
        return Err(invalid("invalid bitmap brush group"));
    }
    // Unknown/custom names remain persisted. The concrete baker must report
    // unavailable instead of replacing an absent imported tip with a soft tip.
    if spec.tip.is_empty() || spec.tip.len() > 255 || spec.tip.contains('\0') {
        return Err(invalid("invalid bitmap brush tip identity"));
    }
    if [
        &spec.size,
        &spec.spacing,
        &spec.scatter,
        &spec.opacity,
        &spec.seed,
    ]
    .iter()
    .any(|n| !n.as_f64().is_some_and(f64::is_finite))
    {
        return Err(invalid("bitmap brush spec numbers must be finite"));
    }
    if rgba(&spec.color).is_none() {
        return Err(invalid("bitmap brush colour is invalid"));
    }
    if let Some(Some(color)) = fields.pre_texture_stroke.value() {
        if rgba(color).is_none() {
            return Err(invalid("bitmap brush remembered stroke is invalid"));
        }
    }
    if fields
        .pre_texture_opacity
        .value()
        .is_some_and(|n| !n.as_f64().is_some_and(f64::is_finite))
    {
        return Err(invalid("bitmap brush remembered opacity must be finite"));
    }
    if let Some(profile) = fields.bitmap_pressure_profile.value() {
        if profile.len() > 4096
            || profile
                .iter()
                .flatten()
                .any(|n| !n.as_f64().is_some_and(f64::is_finite))
        {
            return Err(invalid(
                "bitmap brush pressure profile is invalid or over budget",
            ));
        }
    }
    Ok(())
}
pub(crate) fn validate_record(record: &ObjectRecord) -> Result<(), BitmapBrushValueError> {
    let Some(fields) = record
        .vector_ext
        .value()
        .and_then(|ext| ext.bitmap_brush.value())
    else {
        return Ok(());
    };
    if record.family != VECTOR_FAMILY {
        return Err(BitmapBrushValueError::Unsupported(
            "bitmap brush requires editable vector",
        ));
    }
    validate_fields(fields)?;
    if record.target().context_id() != "scene-root" {
        return Err(BitmapBrushValueError::Unsupported(
            "bitmap brushes inside component contexts are unavailable",
        ));
    }
    let Some(vector) = record.vector.value() else {
        return Err(invalid("bitmap brush anchor has no vector body"));
    };
    // Generated samples retain the interpolated plain anchor plus separately baked ink.
    // Their provenance is authored by the sealed tween command, not inferred from missing values.
    if fields.tween_generated.value() == Some(&true) {
        return Ok(());
    }
    if record.solid_fill().is_some() {
        if vector.solid_stroke().is_some() || fields.pre_texture_stroke.missing() {
            return Err(invalid(
                "filled bitmap brush anchor camouflage is inconsistent",
            ));
        }
    } else if vector.opacity.as_f64() != Some(0.0) || fields.pre_texture_opacity.missing() {
        return Err(invalid(
            "unfilled bitmap brush anchor camouflage is inconsistent",
        ));
    }
    Ok(())
}
/// Read-side frame admission may use this only together with the normal brush
/// consumer. Do not broaden all element holders or ignore active expressions.
pub(crate) fn trim_holder_consumed(
    document: &crate::object_document::ObjectDocument,
    holder: &crate::object_document::PropertyHolder,
) -> bool {
    let target = &holder.target;
    let Some(stroke) = target.stroke_id.value() else {
        return false;
    };
    if holder.controls.value().is_some_and(|v| !v.is_empty())
        || holder.properties.is_empty()
        || !holder.properties.iter().all(|p| {
            ["trimStart", "trimEnd", "trimOffset"].contains(&p.property.as_str())
                && !p.expression.value().is_some_and(|e| e.enabled)
        })
    {
        return false;
    }
    let mut matched = false;
    for record in document.objects().iter().filter(|r| {
        r.target.context_id == target.context_id
            && r.target.layer_uid == target.layer_uid
            && r.target.stroke_id == *stroke
    }) {
        matched = true;
        if record.target.frame_scope.kind != crate::object_document::FrameScopeKind::Authored
            || record
                .vector_ext
                .value()
                .and_then(|v| v.bitmap_brush.value())
                .is_none()
            || validate_record(record).is_err()
        {
            return false;
        }
    }
    matched
}
#[cfg(test)]
#[path = "../tests/object_bitmap_brush_record.rs"]
mod tests;
