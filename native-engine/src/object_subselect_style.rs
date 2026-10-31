//! #2372 hit-test listing: the style a panel shows for a listed record, read from the stored record. `fill` and `stroke` are `#rrggbb`
//! or `#rrggbbaa`, `width` the stroke width (a ribbon's peak width, the size the baseline panel read), `opacity` the alpha of the solid
//! fill, else the item opacity. A member the record does not have is absent. A pressure ribbon paints with its fill, so that is its
//! `fill`; it has no `stroke`.
use crate::object_document::{ObjectRecord, SolidFill, VectorBody};
use serde_json::{json, Map, Value};

/// `#rrggbb`, with the alpha byte appended when the paint is not opaque.
pub(crate) fn hex8(paint: &SolidFill) -> Option<String> {
    let byte = |n: &serde_json::Number| {
        n.as_f64()
            .filter(|v| v.is_finite())
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
    };
    let (r, g, b, a) = (
        byte(&paint.r)?,
        byte(&paint.g)?,
        byte(&paint.b)?,
        byte(&paint.a)?,
    );
    Some(if a == 255 {
        format!("#{r:02x}{g:02x}{b:02x}")
    } else {
        format!("#{r:02x}{g:02x}{b:02x}{a:02x}")
    })
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
        let profile = p.width_profile.value().filter(|w| !w.is_empty()).map(|w| {
            w.iter()
                .filter_map(|s| s.width.as_f64())
                .fold(0.0, f64::max)
        });
        profile.or_else(|| {
            p.center_segments.value().map(|c| {
                c.iter()
                    .filter_map(|s| s.width.as_f64())
                    .fold(0.0, f64::max)
            })
        })
    });
    if let Some(width) = ribbon_width
        .or_else(|| body.stroke_width.as_f64())
        .filter(|w| w.is_finite() && *w > 0.0)
    {
        out.insert("width".into(), json!(width));
    }
    let opacity = record
        .solid_fill()
        .and_then(|f| f.a.as_f64())
        .or_else(|| body.opacity.as_f64());
    if let Some(opacity) = opacity.filter(|o| o.is_finite()) {
        out.insert("opacity".into(), json!(opacity));
    }
    details(record, &mut out);
    (!out.is_empty()).then_some(Value::Object(out))
}

/// #2372 SEL-STYLE2: the stroke details the panel reflects (a ribbon is a filled outline: its paint order only). INSP-READBACK adds the
/// Stroke Style (`strokeStyle`, read back from the dash pattern) and a texture anchor's `brushPreset`.
fn details(record: &ObjectRecord, out: &mut Map<String, Value>) {
    let Some(body) = record.vector() else { return };
    let ext = record.vector_ext.value();
    if let Some(order) = body.paint_order.value() {
        out.insert("paintOrder".into(), json!(order));
    }
    if ext.is_none_or(|e| e.pressure.missing()) {
        out.insert("cap".into(), json!(body.stroke_cap));
        out.insert("join".into(), json!(body.stroke_join));
        if let Some(limit) = body.miter_limit.value().and_then(|n| n.as_f64()) {
            out.insert("miter".into(), json!(limit));
        }
        if let Some(offset) = body.dash_offset.value().and_then(|n| n.as_f64()) {
            out.insert("dashOffset".into(), json!(offset));
        }
        if let Some(kind) = stroke_style(body) {
            out.insert("strokeStyle".into(), json!(kind));
        }
    }
    // A texture anchor names the preset it was stamped with (a dab is a copy of one, a bitmap brush has its own panel).
    if let Some(texture) = ext
        .and_then(|e| e.brush_texture.value())
        .filter(|t| t.is_brush_texture_copy.value() != Some(&true))
    {
        if let Some(preset) = texture.brush_texture_preset.value() {
            out.insert("brushPreset".into(), json!(preset));
        }
    }
}

/// The Stroke Style the panel shows: the dash pattern read back the way `strokeStyle` writes it (`[on, off] * width`, dashed 2.5 / 1.8,
/// dotted 0.5 / 1.2); no pattern is `solid`, a pattern of another shape (a hand-made dash) names no style.
fn stroke_style(body: &VectorBody) -> Option<&'static str> {
    let dashes: Vec<f64> = match body.dash_array.value() {
        None => return Some("solid"),
        Some(d) if d.is_empty() => return Some("solid"),
        Some(d) => d.iter().filter_map(|n| n.as_f64()).collect(),
    };
    let width = body.stroke_width.as_f64().filter(|w| w.is_finite() && *w > 0.0)?;
    let near = |value: f64, factor: f64| (value - width * factor).abs() <= width * 0.01 + 1e-9;
    match dashes.as_slice() {
        [on, off] if near(*on, 2.5) && near(*off, 1.8) => Some("dashed"),
        [on, off] if near(*on, 0.5) && near(*off, 1.2) => Some("dotted"),
        _ => None,
    }
}

/// A raster image shows its opacity; a text block its colour and the colour's alpha.
pub(crate) fn raster(opacity: f64) -> Option<Value> {
    opacity.is_finite().then(|| json!({"opacity": opacity}))
}
pub(crate) fn text(color: &str) -> Option<Value> {
    let alpha = color
        .get(7..9)
        .and_then(|a| u8::from_str_radix(a, 16).ok())
        .map_or(1.0, |a| f64::from(a) / 255.0);
    Some(json!({"fill": color.to_lowercase(), "opacity": alpha}))
}
/// A mask shows its mode (an unknown spelling renders as `add`, as the mask consumer reads it) and the layer's shared feather.
pub(crate) fn mask(mode: Option<&str>, feather: f64) -> Option<Value> {
    let mode = match mode {
        Some("subtract") => "subtract",
        Some("intersect") => "intersect",
        _ => "add",
    };
    feather.is_finite().then(|| json!({"maskMode": mode, "maskFeather": feather.max(0.0)}))
}
