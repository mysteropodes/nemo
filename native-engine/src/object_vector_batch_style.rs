//! #2372 Select restyle: a `restyle` step of `object.vector.batch` changes only the style members it names on a presented
//! record (and what belongs to it), so one panel edit over a mixed selection is one revision and one history entry.
//! Baseline (1065e9d8 timeline.js setFillColor / setStrokeColor / setOpacity / setBrushSize on `selectedPaths`) rules:
//! - `fill` and `stroke` are `#rrggbb` or `#rrggbbaa`. A fill colour reaches only a record that already paints a solid fill (a
//!   solid replaces a stored gradient); a stroke colour only a record with a real stroke. A pressure ribbon paints with its fill, so
//!   either member sets its ink; the two together must agree.
//! - `opacity` (0..1) is the alpha of the solid fill, else the item opacity (an image, an unfilled stroke).
//! - `width` is the stroke width of a plain record. A ribbon rescales every centre width and the width profile by `width / peak`
//!   (the profile's own peak is the current size) and rebuilds its outline and linked fill as the sculpt does.
//! The panel's other style kinds (switches, cap, join, miter, dash, paint order, stroke style, brush preset, mask feather, mask mode, unmask) are the
//! members of `object_vector_batch_style_detail.rs`.
//! Brush families (A-bitmap-brush-edit, baseline `regenerateBrushTexture` / `SMBitmapBrush.regenerate`, see `object_vector_batch_style_dabs.rs`):
//! - a bitmap brush anchor takes `width` as its dab size (`bitmapBrushSpec.size`, and the stroke width), the stroke colour as its ink
//!   (`spec.color`) and, with no fill of its own, the opacity as the stamp opacity; the batch re-bakes its image from the new spec;
//! - a texture anchor whose dabs are stored records re-stamps them from its preset (the stored dabs are deleted, the seeded anchor paints
//!   natively) after a width, a stroke colour or a preset edit; a filled anchor's stroke colour is the stroke it remembers;
//! - a stored dab whose anchor is gone is an ordinary record; one with an anchor follows its anchor and is refused (`Unavailable`) alone.
//! A texture anchor's opacity without a fill is stored as the opacity it remembers (`preTextureOpacity`; the anchor stays hidden, so the
//! outline never comes back), and its stroke switch sets or clears the ink its dabs are stamped in (the stroke, else `preTextureStroke`): the
//! stored dabs are stamped again from the seeded anchor. A bitmap anchor's switch only decides whether a new ink reaches its spec.
use crate::commands::object_fill_owner::vector_edit::batch_ops::detail::{self, Detail};
use crate::commands::object_fill_owner::vector_edit::batch_ops::gradient;
use crate::commands::object_fill_owner::vector_edit::batch_ops::ribbon;
use crate::commands::object_fill_owner::vector_edit::is_vector_record;
use crate::object_document::{
    object_deserialize, CenterSegment, ObjectDocument, ObjectRecord, ObjectTarget, Presence,
    ProjectPresetCatalog, SolidFill, StoredGradient, WidthStop,
};
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;

#[path = "object_vector_batch_style_dabs.rs"]
pub(super) mod dabs;

/// The wire members of a restyle step; each absent member leaves that style alone.
#[derive(Debug, Default)]
pub(super) struct Style {
    fill: Presence<String>,
    stroke: Presence<String>,
    width: Presence<Number>,
    opacity: Presence<Number>,
    pub(super) fill_enabled: Presence<bool>,
    pub(super) stroke_enabled: Presence<bool>,
    pub(super) cap: Presence<String>,
    pub(super) join: Presence<String>,
    pub(super) miter: Presence<Number>,
    pub(super) dash_offset: Presence<Number>,
    pub(super) paint_order: Presence<String>,
    pub(super) stroke_style: Presence<String>,
    pub(super) brush_preset: Presence<String>,
    pub(super) mask_feather: Presence<Number>,
    pub(super) mask_mode: Presence<String>,
    pub(super) unmask: Presence<bool>,
    pub(super) fill_gradient: Presence<Option<StoredGradient>>,
    pub(super) xform_anchor_key: Presence<Option<String>>,
    pub(super) xform_anchor_custom: Presence<Option<Vec<Number>>>,
}
object_deserialize!(Style {
    #[serde(default)] fill: Presence<String>,
    #[serde(default)] stroke: Presence<String>,
    #[serde(default)] width: Presence<Number>,
    #[serde(default)] opacity: Presence<Number>,
    #[serde(default)] fill_enabled: Presence<bool>,
    #[serde(default)] stroke_enabled: Presence<bool>,
    #[serde(default)] cap: Presence<String>,
    #[serde(default)] join: Presence<String>,
    #[serde(default)] miter: Presence<Number>,
    #[serde(default)] dash_offset: Presence<Number>,
    #[serde(default)] paint_order: Presence<String>,
    #[serde(default)] stroke_style: Presence<String>,
    #[serde(default)] brush_preset: Presence<String>,
    #[serde(default)] mask_feather: Presence<Number>,
    #[serde(default)] mask_mode: Presence<String>,
    #[serde(default)] unmask: Presence<bool>,
    #[serde(default)] fill_gradient: Presence<Option<StoredGradient>>,
    #[serde(default)] xform_anchor_key: Presence<Option<String>>,
    #[serde(default)] xform_anchor_custom: Presence<Option<Vec<Number>>>
});

/// A validated style: parsed colours and finite numbers in range.
#[derive(Debug)]
pub(super) struct Checked {
    pub(super) fill: Option<(SolidFill, String)>,
    pub(super) stroke: Option<SolidFill>,
    pub(super) width: Option<f64>,
    pub(super) opacity: Option<f64>,
    pub(super) detail: Detail,
}

/// A number the way a hand-edited record spells it: an exact integer stays an integer.
pub(super) fn num(v: f64) -> Result<Number, DispatchErrorCode> {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 9_007_199_254_740_992.0 {
        return Ok(Number::from(v as i64));
    }
    Number::from_f64(v).ok_or(DispatchErrorCode::InvalidRequest)
}

/// `#rrggbb` / `#rrggbbaa` as the solid paint the record stores (channels 0..1, alpha 1 when omitted).
pub(super) fn parse(hex: &str) -> Result<SolidFill, DispatchErrorCode> {
    let digits = hex
        .strip_prefix('#')
        .filter(|d| [6, 8].contains(&d.len()) && d.bytes().all(|b| b.is_ascii_hexdigit()));
    let digits = digits.ok_or(DispatchErrorCode::InvalidRequest)?;
    let byte = |at: usize| {
        u8::from_str_radix(&digits[at..at + 2], 16)
            .map(|v| f64::from(v) / 255.0)
            .map_err(|_| DispatchErrorCode::InvalidRequest)
    };
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
        let number = |n: &Presence<Number>| {
            n.value()
                .map(|n| n.as_f64().filter(|v| v.is_finite()).ok_or(InvalidRequest))
                .transpose()
        };
        let checked = Checked {
            fill: self
                .fill
                .value()
                .map(|h| Ok::<_, DispatchErrorCode>((parse(h)?, h.clone())))
                .transpose()?,
            stroke: self.stroke.value().map(|h| parse(h)).transpose()?,
            width: number(&self.width)?,
            opacity: number(&self.opacity)?,
            detail: Detail::of(self)?,
        };
        let sound = checked.width.is_none_or(|w| w > 0.0 && w <= 10_000.0)
            && checked.opacity.is_none_or(|o| (0.0..=1.0).contains(&o));
        let own = checked.fill.is_some()
            || checked.stroke.is_some()
            || checked.width.is_some()
            || checked.opacity.is_some();
        // A switch that turns a paint on names its colour; a preset or a feather is an edit of its own.
        let switches = (checked.detail.fill_enabled == Some(true) && checked.fill.is_none())
            || (checked.detail.stroke_enabled == Some(true) && checked.stroke.is_none())
            || (checked.detail.exclusive() && (own || checked.detail.has_plain()))
            || checked.detail.both_exclusive();
        if !sound || switches || !(own || checked.detail.has_plain() || checked.detail.exclusive()) {
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
    let widths = profile.filter(|p| !p.is_empty()).map(|p| {
        p.iter()
            .map(|s| s.width.as_f64().unwrap_or(0.0))
            .collect::<Vec<_>>()
    });
    widths
        .unwrap_or_else(|| {
            center
                .iter()
                .map(|c| c.width.as_f64().unwrap_or(0.0))
                .collect()
        })
        .into_iter()
        .fold(0.0, f64::max)
}

/// The ribbon at `existing` with every width scaled so its peak is `width`, rebuilt from its centreline with its linked fill.
fn resized(
    source: &ObjectDocument,
    existing: &ObjectRecord,
    width: f64,
) -> Result<(ObjectRecord, Option<ObjectRecord>), DispatchErrorCode> {
    use DispatchErrorCode::{InvalidRequest, Unavailable};
    let pressure = existing
        .vector_ext
        .value()
        .and_then(|e| e.pressure.value())
        .ok_or(InvalidRequest)?;
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
        .map(|c| {
            Ok(CenterSegment {
                point: c.point.clone(),
                handle_in: c.handle_in.clone(),
                handle_out: c.handle_out.clone(),
                width: scale(&c.width)?,
            })
        })
        .collect::<Result<Vec<_>, DispatchErrorCode>>()?;
    let mut scaled = existing.clone();
    if let Some(stops) = scaled
        .vector_ext
        .value_mut()
        .and_then(|e| e.pressure.value_mut())
        .and_then(|p| p.width_profile.value_mut())
    {
        for stop in stops {
            stop.width = scale(&stop.width)?;
        }
    }
    ribbon::rebuild(source, &scaled, moved)
}

/// The records `target` becomes under `style` (the named one first, then a ribbon's re-synced linked fill).
pub(super) fn restyle(
    source: &ObjectDocument,
    target: &ObjectTarget,
    style: &Checked,
    catalog: Option<&ProjectPresetCatalog>,
    deleted: &mut Vec<ObjectTarget>,
) -> Result<Vec<ObjectRecord>, DispatchErrorCode> {
    use DispatchErrorCode::{InvalidRequest, NotFound, Unavailable};
    let existing = source
        .objects
        .iter()
        .find(|r| &r.target == target)
        .ok_or(NotFound)?;
    if !is_vector_record(existing) {
        return Err(InvalidRequest);
    }
    if let Some(pivot) = style.detail.pivot() {
        return detail::pivot(existing, pivot);
    }
    if let Some(feather) = style.detail.mask_feather() {
        return detail::feather(source, existing, feather);
    }
    if let Some(mode) = style.detail.mask_mode() {
        return detail::mode(existing, mode);
    }
    if style.detail.unmask() {
        return detail::unmask(existing);
    }
    if let Some(gradient) = style.detail.gradient() {
        return gradient::gradient(existing, gradient);
    }
    let ext = existing.vector_ext.value();
    let texture = ext.and_then(|e| e.brush_texture.value());
    let bitmap = ext.is_some_and(|e| !e.bitmap_brush.missing());
    let dab = texture.is_some_and(|t| t.is_brush_texture_copy.value() == Some(&true));
    if dab && dabs::has_anchor(source, existing) {
        return Err(Unavailable);
    }
    let anchor = texture.is_some() && !dab;
    let stored = if anchor { dabs::copies(source, existing) } else { Vec::new() };
    if let Some(key) = style.detail.brush_preset() {
        // A re-textured anchor paints new dabs of its own: its stored ones go.
        let out = detail::preset(existing, key, catalog)?;
        if out.first() != Some(existing) {
            deleted.extend(stored);
        }
        return Ok(out);
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
    // The switches first: an off paint ignores the colour named beside it; a ribbon's stroke switch decides its ink.
    let decided = detail::toggles(&mut record, style, ribbon_ink)?;
    if bitmap {
        // A bitmap anchor's stroke stays camouflaged (its ink is the spec colour below); the switch only decides whether a new colour reaches it.
        if let (Some(kept), Some(body)) = (existing.vector(), record.vector.value_mut()) {
            body.stroke = kept.stroke.clone();
            body.has_real_stroke = kept.has_real_stroke;
        }
    }
    let fill_colour = style.fill.as_ref().filter(|_| ribbon_ink || style.detail.fill_enabled != Some(false));
    let stroke_colour = style.stroke.as_ref().filter(|_| style.detail.stroke_enabled != Some(false));
    let ink = stroke_colour
        .or(fill_colour.map(|(paint, _)| paint))
        .filter(|_| ribbon_ink && !decided);
    if let Some(paint) = ink {
        record.fill = Presence::Present(Some(paint.clone()));
    } else if let Some((paint, _)) = fill_colour.filter(|_| !ribbon_ink) {
        if record.solid_fill().is_some() {
            record.fill = Presence::Present(Some(paint.clone()));
            if let Some(ext) = record.vector_ext.value_mut() {
                ext.fill_gradient = Presence::Missing;
            }
        }
    }
    if let (Some(paint), false) = (stroke_colour, ribbon_ink) {
        if let Some(body) = record.vector.value_mut().filter(|b| b.has_real_stroke) {
            body.stroke = Presence::Present(Some(paint.clone()));
        }
    }
    if let Some(opacity) = style.opacity {
        match record.solid_fill().cloned() {
            Some(paint) => record.fill = Presence::Present(Some(with_alpha(paint, opacity)?)),
            // A bitmap anchor's opacity is its stamp opacity (below), never the item opacity that hides it.
            None if bitmap => {}
            // A texture anchor with no fill stays hidden (an item opacity of 0): its opacity is the one it remembers and gives back when stripped.
            None if anchor => dabs::remember_opacity(&mut record, opacity)?,
            None => {
                if let Some(body) = record.vector.value_mut() {
                    body.opacity = num(opacity)?;
                }
            }
        }
    }
    let filled = record.solid_fill().is_some();
    if bitmap {
        dabs::bitmap_style(&mut record, style.width, stroke_colour, style.opacity.filter(|_| !filled), filled)?;
    } else if let (true, true, Some(paint)) = (anchor, filled, stroke_colour) {
        dabs::remember_stroke(&mut record, paint);
    }
    // Stored dabs keep the look they were stamped with: a new width or ink stamps them again from the anchor.
    if anchor && !stored.is_empty() && (style.width.is_some() || stroke_colour.is_some() || style.detail.touches_stroke()) && dabs::seed(&mut record) {
        deleted.extend(stored);
    }
    let stroke_width = style
        .width
        .or_else(|| record.vector().and_then(|b| b.stroke_width.as_f64()))
        .unwrap_or(0.0);
    detail::stroke_members(&mut record, style, ribbon_ink, stroke_width)?;
    // The named record leads; a re-synced linked fill follows it.
    out.insert(0, record);
    Ok(out)
}
