//! #2372 Select restyle: a `restyle` step of `object.vector.batch` changes only the style members it names on a presented
//! record (and what belongs to it), so one panel edit over a mixed selection is one revision and one history entry.
//! Baseline (1065e9d8 timeline.js setFillColor / setStrokeColor / setOpacity / setBrushSize on `selectedPaths`) rules:
//! - `fill` and `stroke` are `#rrggbb` or `#rrggbbaa`. A fill colour reaches only a record that already paints a solid fill (a
//!   solid replaces a stored gradient); a stroke colour only a record with a real stroke. A pressure ribbon paints with its fill, so
//!   either member sets its ink; the two together must agree.
//! - `opacity` (0..1) is the alpha of the solid fill, else the item opacity (an image, an unfilled stroke).
//! - `width` is the stroke width of a plain record. A ribbon rescales every centre width and the width profile by `width / peak`
//!   (the profile's own peak is the current size) and rebuilds its outline and linked fill as the sculpt does.
//! A bitmap brush, a stamped brush dab, a texture anchor's stroke colour, a texture anchor's opacity without a fill, and a width on
//! an anchor whose copies are stored are refused (`Unavailable`): their image derives from more than the record.
use crate::commands::object_fill_owner::vector_edit::batch_ops::ribbon;
use crate::commands::object_fill_owner::vector_edit::is_vector_record;
use crate::object_document::{object_deserialize, CenterSegment, ObjectDocument, ObjectRecord, ObjectTarget, Presence, SolidFill, WidthStop};
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;

/// The wire members of a restyle step; each absent member leaves that style alone.
#[derive(Debug, Default)]
pub(super) struct Style {
    fill: Presence<String>,
    stroke: Presence<String>,
    width: Presence<Number>,
    opacity: Presence<Number>,
}
object_deserialize!(Style {
    #[serde(default)] fill: Presence<String>,
    #[serde(default)] stroke: Presence<String>,
    #[serde(default)] width: Presence<Number>,
    #[serde(default)] opacity: Presence<Number>
});

/// A validated style: parsed colours and finite numbers in range.
#[derive(Debug)]
pub(super) struct Checked {
    pub(super) fill: Option<(SolidFill, String)>,
    pub(super) stroke: Option<SolidFill>,
    pub(super) width: Option<f64>,
    pub(super) opacity: Option<f64>,
}

/// A number the way a hand-edited record spells it: an exact integer stays an integer.
pub(super) fn num(v: f64) -> Result<Number, DispatchErrorCode> {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 9_007_199_254_740_992.0 {
        return Ok(Number::from(v as i64));
    }
    Number::from_f64(v).ok_or(DispatchErrorCode::InvalidRequest)
}

/// `#rrggbb` / `#rrggbbaa` as the solid paint the record stores (channels 0..1, alpha 1 when omitted).
fn parse(hex: &str) -> Result<SolidFill, DispatchErrorCode> {
    let digits = hex.strip_prefix('#').filter(|d| [6, 8].contains(&d.len()) && d.bytes().all(|b| b.is_ascii_hexdigit()));
    let digits = digits.ok_or(DispatchErrorCode::InvalidRequest)?;
    let byte = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).map(|v| f64::from(v) / 255.0).map_err(|_| DispatchErrorCode::InvalidRequest);
    Ok(SolidFill {
        kind: "solid".into(),
        r: num(byte(0)?)?,
        g: num(byte(2)?)?,
        b: num(byte(4)?)?,
        a: num(if digits.len() == 8 { byte(6)? } else { 1.0 })?,
    })
}

impl Style {
    pub(super) fn checked(&self) -> Result<Checked, DispatchErrorCode> {
        use DispatchErrorCode::InvalidRequest;
        let number = |n: &Presence<Number>| n.value().map(|n| n.as_f64().filter(|v| v.is_finite()).ok_or(InvalidRequest)).transpose();
        let checked = Checked {
            fill: self.fill.value().map(|h| Ok::<_, DispatchErrorCode>((parse(h)?, h.clone()))).transpose()?,
            stroke: self.stroke.value().map(|h| parse(h)).transpose()?,
            width: number(&self.width)?,
            opacity: number(&self.opacity)?,
        };
        let sound = checked.width.is_none_or(|w| w > 0.0 && w <= 10_000.0) && checked.opacity.is_none_or(|o| (0.0..=1.0).contains(&o));
        if !sound || (checked.fill.is_none() && checked.stroke.is_none() && checked.width.is_none() && checked.opacity.is_none()) {
            return Err(InvalidRequest);
        }
        Ok(checked)
    }
}

fn with_alpha(mut paint: SolidFill, alpha: f64) -> Result<SolidFill, DispatchErrorCode> {
    paint.a = num(alpha)?;
    Ok(paint)
}

/// The peak of the profile the ribbon renders from (the dense width profile, else the anchor widths): the size the baseline reads.
fn peak(center: &[CenterSegment], profile: Option<&[WidthStop]>) -> f64 {
    let widths = profile.filter(|p| !p.is_empty()).map(|p| p.iter().map(|s| s.width.as_f64().unwrap_or(0.0)).collect::<Vec<_>>());
    widths.unwrap_or_else(|| center.iter().map(|c| c.width.as_f64().unwrap_or(0.0)).collect()).into_iter().fold(0.0, f64::max)
}

/// The ribbon at `existing` with every width scaled so its peak is `width`, rebuilt from its centreline with its linked fill.
fn resized(source: &ObjectDocument, existing: &ObjectRecord, width: f64) -> Result<(ObjectRecord, Option<ObjectRecord>), DispatchErrorCode> {
    use DispatchErrorCode::{InvalidRequest, Unavailable};
    let pressure = existing.vector_ext.value().and_then(|e| e.pressure.value()).ok_or(InvalidRequest)?;
    let center = pressure.center_segments.value().ok_or(Unavailable)?;
    let profile = pressure.width_profile.value();
    let top = peak(center, profile.map(Vec::as_slice));
    let ratio = if top > 0.0 { width / top } else { 1.0 };
    let scale = |w: &Number| -> Result<Number, DispatchErrorCode> {
        let w = w.as_f64().ok_or(InvalidRequest)?;
        num((if w == 0.0 { width } else { w }) * ratio)
    };
    let moved = center
        .iter()
        .map(|c| Ok(CenterSegment { point: c.point.clone(), handle_in: c.handle_in.clone(), handle_out: c.handle_out.clone(), width: scale(&c.width)? }))
        .collect::<Result<Vec<_>, DispatchErrorCode>>()?;
    let mut scaled = existing.clone();
    if let Some(stops) = scaled.vector_ext.value_mut().and_then(|e| e.pressure.value_mut()).and_then(|p| p.width_profile.value_mut()) {
        for stop in stops {
            stop.width = scale(&stop.width)?;
        }
    }
    ribbon::rebuild(source, &scaled, moved)
}

/// The records `target` becomes under `style` (the named one first, then a ribbon's re-synced linked fill).
pub(super) fn restyle(source: &ObjectDocument, target: &ObjectTarget, style: &Checked) -> Result<Vec<ObjectRecord>, DispatchErrorCode> {
    use DispatchErrorCode::{InvalidRequest, NotFound, Unavailable};
    let existing = source.objects.iter().find(|r| &r.target == target).ok_or(NotFound)?;
    if !is_vector_record(existing) {
        return Err(InvalidRequest);
    }
    let ext = existing.vector_ext.value();
    let texture = ext.and_then(|e| e.brush_texture.value());
    if ext.is_some_and(|e| !e.bitmap_brush.missing()) || texture.is_some_and(|t| t.is_brush_texture_copy.value() == Some(&true)) {
        return Err(Unavailable);
    }
    let group = texture.and_then(|t| t.brush_group_id.value());
    let stored_copies = group.is_some_and(|g| {
        source.objects.iter().any(|r| {
            r.target.context_id == target.context_id
                && r.target.layer_uid == target.layer_uid
                && r.target.frame_scope == target.frame_scope
                && r.vector_ext.value().and_then(|e| e.brush_texture.value()).is_some_and(|t| t.is_brush_texture_copy.value() == Some(&true) && t.brush_group_id.value() == Some(g))
        })
    });
    // An anchor hides its own outline (an opacity of 0, or no stroke) while its dabs show: a stroke colour or an opacity that is not
    // a fill alpha would bring the outline back, and a width has dabs of its own to follow.
    if texture.is_some() && (style.stroke.is_some() || (style.opacity.is_some() && existing.solid_fill().is_none()) || (style.width.is_some() && stored_copies)) {
        return Err(Unavailable);
    }
    let ribbon_ink = ext.is_some_and(|e| !e.pressure.missing());
    let mut out = Vec::new();
    let mut record = existing.clone();
    if ribbon_ink {
        if let (Some((fill, _)), Some(stroke)) = (&style.fill, &style.stroke) {
            if fill != stroke {
                return Err(InvalidRequest);
            }
        }
    }
    if let Some(width) = style.width.filter(|_| ribbon_ink) {
        let (resized, fill) = resized(source, existing, width)?;
        record = resized;
        if let Some(fill) = fill {
            out.push(fill);
        }
    } else if let Some(width) = style.width {
        if let Some(body) = record.vector.value_mut() {
            body.stroke_width = num(width)?;
        }
    }
    let ink = style.stroke.as_ref().or(style.fill.as_ref().map(|(paint, _)| paint)).filter(|_| ribbon_ink);
    if let Some(paint) = ink {
        record.fill = Presence::Present(Some(paint.clone()));
    } else if let Some((paint, _)) = style.fill.as_ref().filter(|_| !ribbon_ink) {
        if record.solid_fill().is_some() {
            record.fill = Presence::Present(Some(paint.clone()));
            if let Some(ext) = record.vector_ext.value_mut() {
                ext.fill_gradient = Presence::Missing;
            }
        }
    }
    if let (Some(paint), false) = (&style.stroke, ribbon_ink) {
        if let Some(body) = record.vector.value_mut().filter(|b| b.has_real_stroke) {
            body.stroke = Presence::Present(Some(paint.clone()));
        }
    }
    if let Some(opacity) = style.opacity {
        match record.solid_fill().cloned() {
            Some(paint) => record.fill = Presence::Present(Some(with_alpha(paint, opacity)?)),
            None => {
                if let Some(body) = record.vector.value_mut() {
                    body.opacity = num(opacity)?;
                }
            }
        }
    }
    // The named record leads; a re-synced linked fill follows it.
    out.insert(0, record);
    Ok(out)
}
