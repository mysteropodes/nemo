//! #2372 SEL-STYLE2: the rest of the Select selection's style kinds as members of a `restyle` step, each by the baseline rule
//! (1065e9d8 timeline.js setFillEnabled / setStrokeEnabled / setStrokeCap / setStrokeJoin / setMiterLimit / setDashOffset /
//! setPaintOrder / setStrokeStyle / setBrushPreset and the mask panel's feather):
//! - `fillEnabled` / `strokeEnabled` turn the paint on with the colour named beside them (`fill` / `stroke`) or off. A pressure ribbon
//!   skips the fill switch and keeps its colour through the stroke switch (only its ink alpha changes, a zero alpha comes back opaque).
//! - `cap`, `join`, `miter`, `dashOffset` reach a plain record only (a ribbon is a filled outline); `dashOffset` alone only a dashed one.
//!   `paintOrder` reaches every vector record. `strokeStyle` (solid, dashed, dotted) rewrites the dash pattern from the stroke width of a
//!   stroked plain record, as `applyStrokeStyle` does.
//! - `brushPreset` strips the record's brush texture (restoring what the anchor hid) and then, unless it is `none`, makes it an anchor of
//!   the preset (a fresh group and seed; native stamps the dabs). A ribbon, a Fill Brush shape and a record with no stroke are left as they
//!   are, as the baseline's eligibility filter leaves them.
//! - `maskFeather` is the layer's shared feather: every mask of the named mask's cell takes it.
//! A bitmap brush, a stamped dab, stored dabs and a stroke switch on a texture anchor are refused (`Unavailable`) by the caller.
use crate::commands::object_fill_owner::vector_edit::batch_ops::style::{
    num, parse, Checked, Style,
};
use crate::object_document::{
    BrushTexture, ObjectDocument, ObjectRecord, Presence, ProjectPresetCatalog, SolidFill,
};
use crate::request_receipts::DispatchErrorCode;
use serde_json::Number;

const CAPS: [&str; 3] = ["butt", "round", "square"];
const JOINS: [&str; 3] = ["miter", "round", "bevel"];
const ORDERS: [&str; 2] = ["fillFirst", "strokeFirst"];
const STYLES: [&str; 3] = ["solid", "dashed", "dotted"];

/// The validated detail members of a restyle step.
#[derive(Debug, Default)]
pub(super) struct Detail {
    pub(super) fill_enabled: Option<bool>,
    pub(super) stroke_enabled: Option<bool>,
    cap: Option<String>,
    join: Option<String>,
    miter: Option<f64>,
    dash_offset: Option<f64>,
    paint_order: Option<String>,
    stroke_style: Option<String>,
    brush_preset: Option<String>,
    mask_feather: Option<f64>,
}

/// A member that names one of `allowed`.
fn choice(v: &Presence<String>, allowed: &[&str]) -> Result<Option<String>, DispatchErrorCode> {
    v.value()
        .map(|s| {
            allowed
                .contains(&s.as_str())
                .then(|| s.clone())
                .ok_or(DispatchErrorCode::InvalidRequest)
        })
        .transpose()
}

fn finite(v: &Presence<Number>, lo: f64, hi: f64) -> Result<Option<f64>, DispatchErrorCode> {
    v.value()
        .map(|n| {
            n.as_f64()
                .filter(|x| x.is_finite() && (lo..=hi).contains(x))
                .ok_or(DispatchErrorCode::InvalidRequest)
        })
        .transpose()
}

impl Detail {
    pub(super) fn of(style: &Style) -> Result<Self, DispatchErrorCode> {
        let preset = style
            .brush_preset
            .value()
            .map(|k| {
                (!k.is_empty() && k.len() <= 96 && !k.chars().any(char::is_control))
                    .then(|| k.clone())
                    .ok_or(DispatchErrorCode::InvalidRequest)
            })
            .transpose()?;
        Ok(Self {
            fill_enabled: style.fill_enabled.value().copied(),
            stroke_enabled: style.stroke_enabled.value().copied(),
            cap: choice(&style.cap, &CAPS)?,
            join: choice(&style.join, &JOINS)?,
            miter: finite(&style.miter, 1.0, 1000.0)?,
            dash_offset: finite(&style.dash_offset, -1.0e6, 1.0e6)?,
            paint_order: choice(&style.paint_order, &ORDERS)?,
            stroke_style: choice(&style.stroke_style, &STYLES)?,
            brush_preset: preset,
            mask_feather: finite(&style.mask_feather, 0.0, 500.0)?,
        })
    }
    /// A preset or a mask feather is its own edit: it combines with nothing else (not with the other, not with a plain member).
    pub(super) fn exclusive(&self) -> bool {
        self.brush_preset.is_some() || self.mask_feather.is_some()
    }
    /// Any member that is not a preset or a feather.
    pub(super) fn has_plain(&self) -> bool {
        self.fill_enabled.is_some()
            || self.stroke_enabled.is_some()
            || self.cap.is_some()
            || self.join.is_some()
            || self.miter.is_some()
            || self.dash_offset.is_some()
            || self.paint_order.is_some()
            || self.stroke_style.is_some()
    }
    pub(super) fn both_exclusive(&self) -> bool {
        self.brush_preset.is_some() && self.mask_feather.is_some()
    }
    /// The switches that bring the stroke back: a texture anchor hides its stroke, so these would show its outline again.
    pub(super) fn touches_stroke(&self) -> bool {
        self.stroke_enabled.is_some()
    }
    pub(super) fn brush_preset(&self) -> Option<&str> {
        self.brush_preset.as_deref()
    }
    pub(super) fn mask_feather(&self) -> Option<f64> {
        self.mask_feather
    }
}

/// `paint` with its alpha set (a ribbon's ink coming back opaque from zero).
fn alpha(mut paint: SolidFill, a: f64) -> Result<SolidFill, DispatchErrorCode> {
    paint.a = num(a)?;
    Ok(paint)
}

/// The fill and stroke switches of `style` on `record`. `ribbon` is a pressure ribbon (its ink is its fill). Returns whether the
/// ribbon's ink was decided here (so the plain colour members must not overwrite it).
pub(super) fn toggles(
    record: &mut ObjectRecord,
    style: &Checked,
    ribbon: bool,
) -> Result<bool, DispatchErrorCode> {
    use DispatchErrorCode::InvalidRequest;
    let detail = &style.detail;
    if !ribbon {
        match (detail.fill_enabled, &style.fill) {
            (Some(true), Some((paint, _))) => {
                record.fill = Presence::Present(Some(paint.clone()));
                clear_gradient(record);
            }
            (Some(false), _) => {
                record.fill = Presence::Present(None);
                clear_gradient(record);
            }
            _ => {}
        }
        match (detail.stroke_enabled, &style.stroke) {
            (Some(true), Some(paint)) => {
                if let Some(body) = record.vector.value_mut() {
                    body.stroke = Presence::Present(Some(paint.clone()));
                    body.has_real_stroke = true;
                }
            }
            (Some(false), _) => {
                if let Some(body) = record.vector.value_mut() {
                    body.stroke = Presence::Present(None);
                    body.has_real_stroke = false;
                }
            }
            _ => {}
        }
        return Ok(false);
    }
    // A ribbon: the stroke switch is its ink alpha (`applyBrushKeyline`'s keyline stays derived from it).
    let Some(on) = detail.stroke_enabled else {
        return Ok(false);
    };
    let ink = record.solid_fill().cloned();
    let target = style.stroke.clone();
    let next = if on {
        let target = target.ok_or(InvalidRequest)?;
        let target_alpha = target.a.as_f64().filter(|a| *a > 0.0).unwrap_or(1.0);
        let base = ink
            .filter(|c| c.a.as_f64().is_some_and(|a| a > 0.0))
            .unwrap_or(target);
        alpha(base, target_alpha)?
    } else {
        alpha(ink.or(target).ok_or(InvalidRequest)?, 0.0)?
    };
    record.fill = Presence::Present(Some(next));
    Ok(true)
}

fn clear_gradient(record: &mut ObjectRecord) {
    if let Some(ext) = record.vector_ext.value_mut() {
        ext.fill_gradient = Presence::Missing;
    }
}

/// cap / join / miter / dash offset / paint order / stroke style on `record` (a `ribbon` takes only the paint order).
pub(super) fn stroke_members(
    record: &mut ObjectRecord,
    style: &Checked,
    ribbon: bool,
    width: f64,
) -> Result<(), DispatchErrorCode> {
    let detail = &style.detail;
    let Some(body) = record.vector.value_mut() else {
        return Ok(());
    };
    if let Some(order) = &detail.paint_order {
        body.paint_order = Presence::Present(order.clone());
    }
    if ribbon {
        return Ok(());
    }
    if let Some(cap) = &detail.cap {
        body.stroke_cap = cap.clone();
    }
    if let Some(join) = &detail.join {
        body.stroke_join = join.clone();
    }
    if let Some(limit) = detail.miter {
        body.miter_limit = Presence::Present(num(limit)?);
    }
    if let Some(kind) = detail
        .stroke_style
        .as_deref()
        .filter(|_| body.has_real_stroke)
    {
        body.dash_array = match kind {
            "dashed" => dashes(width, 2.5, 1.8)?,
            "dotted" => dashes(width, 0.5, 1.2)?,
            _ => Presence::Missing,
        };
        body.dash_offset = Presence::Present(num(detail.dash_offset.unwrap_or(0.0))?);
    } else if let Some(offset) = detail.dash_offset {
        if body.dash_array.value().is_some_and(|d| !d.is_empty()) {
            body.dash_offset = Presence::Present(num(offset)?);
        }
    }
    Ok(())
}

/// The dash pattern `[on * width, off * width]`; a zero width has none.
fn dashes(width: f64, on: f64, off: f64) -> Result<Presence<Vec<Number>>, DispatchErrorCode> {
    if width <= 0.0 {
        return Ok(Presence::Missing);
    }
    Ok(Presence::Present(vec![num(width * on)?, num(width * off)?]))
}

/// A 32-bit FNV-1a hash of `parts`: the seed and group of a re-textured stroke are derived, so a replay makes the same dabs.
fn hash(parts: &[&str]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for part in parts {
        for byte in part.bytes().chain(std::iter::once(0xff)) {
            h = (h ^ u32::from(byte)).wrapping_mul(0x0100_0193);
        }
    }
    h
}

/// A `#rrggbbaa` the record hides its stroke as (the baseline's `colorHex8`).
fn hex8(paint: &SolidFill) -> Option<String> {
    let byte = |n: &Number| {
        n.as_f64()
            .filter(|v| v.is_finite())
            .map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
    };
    Some(format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        byte(&paint.r)?,
        byte(&paint.g)?,
        byte(&paint.b)?,
        byte(&paint.a)?
    ))
}

/// `existing` re-textured with preset `key` (`none` strips): `stripAnyBrushTexture` then `applyBrushTexture`, with a derived group and seed.
pub(super) fn preset(
    existing: &ObjectRecord,
    key: &str,
    catalog: Option<&ProjectPresetCatalog>,
) -> Result<Vec<ObjectRecord>, DispatchErrorCode> {
    use crate::evaluation::brush_dab::presets::{builtin, from_catalog};
    use DispatchErrorCode::InvalidRequest;
    let ext = existing.vector_ext.value();
    let body = existing.vector().ok_or(InvalidRequest)?;
    let previous = ext.and_then(|e| e.brush_texture.value()).cloned();
    let anchored = previous
        .as_ref()
        .is_some_and(|t| !t.brush_texture_preset.missing());
    let special =
        ext.is_some_and(|e| !e.pressure.missing() || e.is_fill_shape.value() == Some(&true));
    // Eligible (baseline): a plain stroke with a stroke or a texture already. Anything else is left exactly as it is.
    if special || !(body.has_real_stroke || anchored) {
        return Ok(vec![existing.clone()]);
    }
    let known = key == "none"
        || builtin(key).is_some()
        || catalog
            .and_then(|c| c.entries().iter().find(|(id, _)| id == key))
            .is_some_and(|(_, p)| from_catalog(p).is_some());
    if !known {
        return Err(InvalidRequest);
    }
    let mut record = existing.clone();
    // Strip: give back what the anchor hid.
    if let Some(t) = &previous {
        let restored = t
            .pre_texture_stroke
            .value()
            .map(|s| s.as_deref().map(parse).transpose())
            .transpose()?;
        if let Some(b) = record.vector.value_mut() {
            if let Some(opacity) = t.pre_texture_opacity.value() {
                b.opacity = opacity.clone();
            }
            if let Some(stroke) = restored {
                b.has_real_stroke = stroke.is_some();
                b.stroke = Presence::Present(stroke);
            }
        }
    }
    if let Some(e) = record.vector_ext.value_mut() {
        e.brush_texture = Presence::Missing;
        if e.is_empty() {
            record.vector_ext = Presence::Missing;
        }
    }
    if key == "none" {
        return Ok(vec![record]);
    }
    stamp(record, key, previous)
}

/// `applyBrushTexture` on a stripped plain record: a new group and seed, and the camouflage of its own look (a filled anchor hides its
/// stroke and remembers it, an unfilled one hides itself by opacity).
fn stamp(
    mut record: ObjectRecord,
    key: &str,
    previous: Option<BrushTexture>,
) -> Result<Vec<ObjectRecord>, DispatchErrorCode> {
    use DispatchErrorCode::InvalidRequest;
    let id = record.target.stroke_id.clone();
    let before = previous
        .as_ref()
        .and_then(|t| t.brush_group_id.value())
        .map_or("", String::as_str);
    let seed = hash(&[&id, key, before]);
    let group = format!("bg{seed:08x}_{}", hash(&[key, &id]) % 1_000_000);
    let mut texture = BrushTexture {
        brush_group_id: Presence::Present(group),
        is_brush_texture_copy: Presence::Missing,
        brush_texture_preset: Presence::Present(key.to_string()),
        pre_texture_opacity: Presence::Missing,
        pre_texture_stroke: Presence::Missing,
        stroke_seed: Presence::Present(Number::from(seed)),
    };
    let filled = record.solid_fill().is_some();
    let body = record.vector.value_mut().ok_or(InvalidRequest)?;
    if filled {
        let paint = body.solid_stroke().cloned().ok_or(InvalidRequest)?;
        texture.pre_texture_stroke = Presence::Present(Some(hex8(&paint).ok_or(InvalidRequest)?));
        body.stroke = Presence::Present(None);
        body.has_real_stroke = false;
    } else {
        texture.pre_texture_opacity = Presence::Present(body.opacity.clone());
        body.opacity = Number::from(0);
    }
    let ext = record.vector_ext.value_mut();
    match ext {
        Some(e) => e.brush_texture = Presence::Present(texture),
        None => {
            record.vector_ext = Presence::Present(crate::object_document::VectorExt {
                brush_texture: Presence::Present(texture),
                ..Default::default()
            })
        }
    }
    Ok(vec![record])
}

/// Every mask of `named`'s cell takes the feather (the layer's shared value); the named mask leads.
pub(super) fn feather(
    source: &ObjectDocument,
    named: &ObjectRecord,
    value: f64,
) -> Result<Vec<ObjectRecord>, DispatchErrorCode> {
    let is_mask = |r: &ObjectRecord| {
        r.vector_ext
            .value()
            .is_some_and(|e| e.is_mask.value() == Some(&Some(true)))
    };
    if !is_mask(named) {
        return Err(DispatchErrorCode::Unavailable);
    }
    let number = num(value)?;
    let same_cell = |r: &ObjectRecord| {
        r.target.context_id == named.target.context_id
            && r.target.layer_uid == named.target.layer_uid
            && r.target.frame_scope == named.target.frame_scope
    };
    let mut out = vec![named.clone()];
    out.extend(
        source
            .objects
            .iter()
            .filter(|r| same_cell(r) && is_mask(r) && r.target != named.target)
            .cloned(),
    );
    for record in &mut out {
        if let Some(e) = record.vector_ext.value_mut() {
            e.mask_feather = Presence::Present(Some(number.clone()));
        }
    }
    Ok(out)
}
