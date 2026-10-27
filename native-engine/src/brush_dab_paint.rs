//! G16 option C (#2361): a seeded brush-texture anchor's dabs as paint items, as `applyBrushTexture` paints them
//! (`src/js/tools.js` @1065e9d8). Each dab is a closed fill of the anchor's base colour (`strokeColor`, else the hidden
//! `preTextureStroke`) jittered by its value delta, at the dab's own opacity, never multiplied by the anchor's
//! camouflaged opacity; the layer factor applies as to every paint. The baseline inserts each dab directly above the
//! anchor, so the last dab made is the lowest: items come in that paint order. Unseeded anchors (imported projects
//! store their dabs as records) are not stamped.
use crate::evaluation::brush_dab::presets::builtin;
use crate::evaluation::brush_dab::sampling::{jitter_color_value, SeededRng};
use crate::evaluation::brush_dab::shape::DabPath;
use crate::evaluation::brush_dab::stamp::build_brush_dabs;
use crate::evaluation::object_scene_style_kernels::object_scene_style_item::SceneItemStyle;
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::{
    FillResolution, PaintOrder, SceneSolid,
};
use crate::object_document::{ObjectRecord, SolidFill};
use serde_json::Number;

/// One dab to paint: its closed path and its fill style.
#[derive(Debug, Clone)]
pub(crate) struct DabItem {
    pub(crate) path: DabPath,
    pub(crate) style: SceneItemStyle,
}

/// The anchor's preset key and seed, when the record is a seeded brush-texture anchor.
fn seeded(record: &ObjectRecord) -> Option<(&str, u32)> {
    let texture = record.vector_ext.value()?.brush_texture.value()?;
    let key = texture.brush_texture_preset.value()?;
    let seed = texture
        .stroke_seed
        .value()?
        .as_u64()
        .and_then(|s| u32::try_from(s).ok())?;
    Some((key.as_str(), seed))
}
/// Whether native stamps dabs for this record.
pub(crate) fn is_seeded_anchor(record: &ObjectRecord) -> bool {
    seeded(record).is_some()
}
/// The brush group of an anchor (`Some(false)`) or of a stored dab copy (`Some(true)`): stored dabs (an imported
/// project, a tween re-stamp) already paint their anchor's texture, so native does not stamp that group again.
pub(crate) fn texture_group(record: &ObjectRecord) -> Option<(&str, bool)> {
    let texture = record.vector_ext.value()?.brush_texture.value()?;
    let group = texture.brush_group_id.value()?;
    Some((
        group.as_str(),
        texture.is_brush_texture_copy.value() == Some(&true),
    ))
}

fn hex_rgba(hex: &str) -> Option<[f64; 4]> {
    let digits = hex.strip_prefix('#')?;
    let expanded: String = match digits.len() {
        3 => digits.chars().flat_map(|c| [c, c]).collect(),
        6 | 8 => digits.to_owned(),
        _ => return None,
    };
    let byte = |at: usize| {
        u8::from_str_radix(expanded.get(at..at + 2)?, 16)
            .ok()
            .map(|b| f64::from(b) / 255.0)
    };
    Some([
        byte(0)?,
        byte(2)?,
        byte(4)?,
        if expanded.len() == 8 { byte(6)? } else { 1.0 },
    ])
}

/// The centreline and width profile a pressure ribbon stamps along, or `None` for any other anchor. The stored
/// centreline is unposed: a ribbon whose drawn outline is not its stored one (motion, a pose) awaits native centreline
/// posing and is refused rather than textured in the wrong place.
#[allow(clippy::type_complexity)]
fn ribbon_walk(
    record: &ObjectRecord,
    drawn: &[[f64; 6]],
) -> Result<Option<(Vec<[f64; 6]>, Vec<(f64, f64)>)>, &'static str> {
    let Some(pressure) = record
        .vector_ext
        .value()
        .and_then(|ext| ext.pressure.value())
    else {
        return Ok(None);
    };
    let (Some(centre), Some(profile)) = (
        pressure.center_segments.value(),
        pressure.width_profile.value(),
    ) else {
        return Ok(None);
    };
    if centre.len() < 2 || profile.is_empty() {
        return Ok(None);
    }
    let f = |n: &Number| {
        n.as_f64()
            .filter(|v| v.is_finite())
            .ok_or("non-finite ribbon geometry")
    };
    let stored = &record.geometry.segments;
    let near = |a: f64, b: f64| (a - b).abs() <= 1e-9 * (1.0 + b.abs());
    let unposed = stored.len() == drawn.len()
        && stored.iter().zip(drawn).all(|(s, d)| {
            [
                &s.point.x,
                &s.point.y,
                &s.handle_in.x,
                &s.handle_in.y,
                &s.handle_out.x,
                &s.handle_out.y,
            ]
            .iter()
            .zip(d)
            .enumerate()
            // An open outline's end handles never reach the drawn path; a ribbon outline is closed.
            .all(|(_, (s, d))| s.as_f64().is_some_and(|s| near(*d, s)))
        });
    if !unposed {
        return Err("a posed textured pressure ribbon awaits native centreline posing");
    }
    let centre = centre
        .iter()
        .map(|s| {
            Ok([
                f(&s.point.x)?,
                f(&s.point.y)?,
                f(&s.handle_in.x)?,
                f(&s.handle_in.y)?,
                f(&s.handle_out.x)?,
                f(&s.handle_out.y)?,
            ])
        })
        .collect::<Result<Vec<_>, &'static str>>()?;
    let profile = profile
        .iter()
        .map(|stop| Ok((f(&stop.t)?, f(&stop.width)?)))
        .collect::<Result<Vec<_>, &'static str>>()?;
    Ok(Some((centre, profile)))
}

/// The dabs of a seeded anchor over its drawn path (`[x, y, inX, inY, outX, outY]` segments), in paint order.
pub(crate) fn anchor_dab_items(
    record: &ObjectRecord,
    drawn: &[[f64; 6]],
    closed: bool,
    layer_opacity: f64,
) -> Result<Vec<DabItem>, &'static str> {
    let (key, seed) = seeded(record).ok_or("not a seeded brush-texture anchor")?;
    let ext = record
        .vector_ext
        .value()
        .ok_or("not a seeded brush-texture anchor")?;
    let preset = builtin(key).ok_or("custom brush presets await native stamping")?;
    let body = record
        .vector()
        .ok_or("a brush-texture anchor needs a vector body")?;
    let width = body
        .stroke_width
        .as_f64()
        .filter(|w| w.is_finite())
        .ok_or("invalid anchor stroke width")?;
    let texture = ext
        .brush_texture
        .value()
        .ok_or("not a seeded brush-texture anchor")?;
    // A pressure ribbon (`isVectorBrush` with a centreline and a width profile) stamps along its centreline at the
    // mean profile width, through the profile; anything else stamps along its own drawn path at its stroke width.
    let ribbon = ribbon_walk(record, drawn)?;
    let channels =
        |paint: &SolidFill| match [&paint.r, &paint.g, &paint.b, &paint.a].map(|n| n.as_f64()) {
            [Some(r), Some(g), Some(b), Some(a)] => Some([r, g, b, a]),
            _ => None,
        };
    // `strokeColor || preTextureStroke || (isPressure ? fillColor : null)`.
    let base = match body.stroke.value().and_then(Option::as_ref) {
        Some(paint) => channels(paint),
        None => texture
            .pre_texture_stroke
            .value()
            .and_then(Option::as_ref)
            .and_then(|hex| hex_rgba(hex))
            .or_else(|| {
                ribbon
                    .as_ref()
                    .and_then(|_| record.solid_fill().and_then(channels))
            }),
    };
    // `jitterColorValue(null)` stays null: dabs without a colour paint nothing.
    let Some(base) = base else {
        return Ok(Vec::new());
    };
    let mut rng = SeededRng::new(seed);
    let dabs = match &ribbon {
        Some((centre, profile)) => {
            let base_width = profile.iter().map(|stop| stop.1).sum::<f64>() / profile.len() as f64;
            build_brush_dabs(centre, false, &preset, base_width, &mut rng, Some(profile))
        }
        None => build_brush_dabs(drawn, closed, &preset, width, &mut rng, None),
    }
    .map_err(|_| "the anchor's drawn path is unavailable")?;
    let number = |v: f64| Number::from_f64(v).ok_or("non-finite dab paint");
    dabs.into_iter()
        .rev()
        .map(|dab| {
            let rgba = jitter_color_value(base, dab.value_delta);
            let fill = SolidFill {
                kind: "solid".to_owned(),
                r: number(rgba[0])?,
                g: number(rgba[1])?,
                b: number(rgba[2])?,
                a: number((rgba[3] * dab.opacity * layer_opacity).min(1.0))?,
            };
            let solid = SceneSolid::from_fill(&fill).map_err(|_| "unrepresentable dab paint")?;
            Ok(DabItem {
                path: dab.path,
                style: SceneItemStyle {
                    fill: FillResolution::Solid(solid),
                    stroke: None,
                    order: PaintOrder::FillFirst,
                },
            })
        })
        .collect()
}
