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
    if ext.pressure.value().is_some() {
        return Err("a textured pressure ribbon awaits native centreline stamping");
    }
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
    let base = match body.stroke.value().and_then(Option::as_ref) {
        Some(paint) => match [&paint.r, &paint.g, &paint.b, &paint.a].map(|n| n.as_f64()) {
            [Some(r), Some(g), Some(b), Some(a)] => Some([r, g, b, a]),
            _ => None,
        },
        None => texture
            .pre_texture_stroke
            .value()
            .and_then(Option::as_ref)
            .and_then(|hex| hex_rgba(hex)),
    };
    // `jitterColorValue(null)` stays null: dabs without a colour paint nothing.
    let Some(base) = base else {
        return Ok(Vec::new());
    };
    let dabs = build_brush_dabs(
        drawn,
        closed,
        &preset,
        width,
        &mut SeededRng::new(seed),
        None,
    )
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
