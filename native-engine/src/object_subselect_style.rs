//! #2372 hit-test listing: the style a panel shows for a listed record, read from the stored record. `fill` and `stroke` are `#rrggbb`
//! or `#rrggbbaa`, `width` the stroke width (a ribbon's peak width, the size the baseline panel read), `opacity` the alpha of the solid
//! fill, else the item opacity. A member the record does not have is absent. A pressure ribbon paints with its fill, so that is its
//! `fill`; it has no `stroke`.
use crate::object_document::{ObjectRecord, SolidFill};
use serde_json::{json, Map, Value};

/// `#rrggbb`, with the alpha byte appended when the paint is not opaque.
pub(crate) fn hex8(paint: &SolidFill) -> Option<String> {
    let byte = |n: &serde_json::Number| n.as_f64().filter(|v| v.is_finite()).map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    let (r, g, b, a) = (byte(&paint.r)?, byte(&paint.g)?, byte(&paint.b)?, byte(&paint.a)?);
    Some(if a == 255 { format!("#{r:02x}{g:02x}{b:02x}") } else { format!("#{r:02x}{g:02x}{b:02x}{a:02x}") })
}

pub(crate) fn listed(record: &ObjectRecord) -> Option<Value> {
    let body = record.vector()?;
    let mut out = Map::new();
    if let Some(fill) = record.solid_fill().and_then(hex8) {
        out.insert("fill".into(), json!(fill));
    }
    let ext = record.vector_ext.value();
    let pressure = ext.and_then(|e| e.pressure.value());
    if pressure.is_none() && body.has_real_stroke {
        if let Some(stroke) = body.solid_stroke().and_then(hex8) {
            out.insert("stroke".into(), json!(stroke));
        }
    }
    let ribbon_width = pressure.and_then(|p| {
        let profile = p.width_profile.value().filter(|w| !w.is_empty()).map(|w| w.iter().filter_map(|s| s.width.as_f64()).fold(0.0, f64::max));
        profile.or_else(|| p.center_segments.value().map(|c| c.iter().filter_map(|s| s.width.as_f64()).fold(0.0, f64::max)))
    });
    if let Some(width) = ribbon_width.or_else(|| body.stroke_width.as_f64()).filter(|w| w.is_finite() && *w > 0.0) {
        out.insert("width".into(), json!(width));
    }
    let opacity = record.solid_fill().and_then(|f| f.a.as_f64()).or_else(|| body.opacity.as_f64());
    if let Some(opacity) = opacity.filter(|o| o.is_finite()) {
        out.insert("opacity".into(), json!(opacity));
    }
    (!out.is_empty()).then_some(Value::Object(out))
}

/// A raster image shows its opacity; a text block its colour and the colour's alpha.
pub(crate) fn raster(opacity: f64) -> Option<Value> {
    opacity.is_finite().then(|| json!({"opacity": opacity}))
}
pub(crate) fn text(color: &str) -> Option<Value> {
    let alpha = color.get(7..9).and_then(|a| u8::from_str_radix(a, 16).ok()).map_or(1.0, |a| f64::from(a) / 255.0);
    Some(json!({"fill": color.to_lowercase(), "opacity": alpha}))
}
