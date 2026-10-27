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
/// centreline is stored unposed, like the outline: the stamp is posed afterwards (`fit_pose`).
#[allow(clippy::type_complexity)]
fn ribbon_walk(
    record: &ObjectRecord,
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

/// The record's own (unposed) outline as `[x, y, inX, inY, outX, outY]` segments.
fn stored_segments(record: &ObjectRecord) -> Result<Vec<[f64; 6]>, &'static str> {
    let f = |n: &Number| {
        n.as_f64()
            .filter(|v| v.is_finite())
            .ok_or("non-finite anchor geometry")
    };
    record
        .geometry
        .segments
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
        .collect()
}

fn det3(m: [[f64; 3]; 3]) -> f64 {
    m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
}
/// The pose that carries the stored outline onto the drawn one (the baseline stamps the stored stroke once and then
/// poses its dabs with everything else): `None` when they coincide; otherwise `[a, b, c, d, e, f]` (`x' = a x + c y + e`,
/// `y' = b x + d y + f`) fitted on three spread anchors, or a similarity when the anchors are collinear, and checked on
/// every anchor. A pose no affine carries (per-vertex offsets) is unavailable rather than textured in the wrong place.
fn fit_pose(stored: &[[f64; 6]], drawn: &[[f64; 6]]) -> Result<Option<[f64; 6]>, &'static str> {
    let refuse = "a non-affine pose of a textured stroke awaits native dab posing";
    if stored.len() != drawn.len() || stored.is_empty() {
        return Err(refuse);
    }
    if stored
        .iter()
        .zip(drawn)
        .all(|(s, d)| s[0] == d[0] && s[1] == d[1])
    {
        return Ok(None);
    }
    let pt = |v: &[f64; 6]| [v[0], v[1]];
    let s0 = pt(&stored[0]);
    let dist = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
    let i1 = (0..stored.len())
        .max_by(|&a, &b| dist(s0, pt(&stored[a])).total_cmp(&dist(s0, pt(&stored[b]))))
        .unwrap_or(0);
    let s1 = pt(&stored[i1]);
    let area =
        |q: [f64; 2]| ((s1[0] - s0[0]) * (q[1] - s0[1]) - (s1[1] - s0[1]) * (q[0] - s0[0])).abs();
    let i2 = (0..stored.len())
        .max_by(|&a, &b| area(pt(&stored[a])).total_cmp(&area(pt(&stored[b]))))
        .unwrap_or(0);
    let (d0, d1) = (pt(&drawn[0]), pt(&drawn[i1]));
    let span = dist(s0, s1);
    let m = if area(pt(&stored[i2])) > 1e-9 * (1.0 + span * span) {
        let (s2, d2) = (pt(&stored[i2]), pt(&drawn[i2]));
        let rows = [
            [s0[0], s0[1], 1.0],
            [s1[0], s1[1], 1.0],
            [s2[0], s2[1], 1.0],
        ];
        let det = det3(rows);
        let solve = |rhs: [f64; 3]| {
            let column = |c: usize| {
                let mut m = rows;
                for (k, row) in m.iter_mut().enumerate() {
                    row[c] = rhs[k];
                }
                det3(m) / det
            };
            [column(0), column(1), column(2)]
        };
        let ([a, c, e], [b, d, f]) = (solve([d0[0], d1[0], d2[0]]), solve([d0[1], d1[1], d2[1]]));
        [a, b, c, d, e, f]
    } else if span == 0.0 {
        [1.0, 0.0, 0.0, 1.0, d0[0] - s0[0], d0[1] - s0[1]]
    } else {
        // Collinear anchors fix rotation, uniform scale and translation: the ratio of the two spans as complex numbers.
        let (sx, sy, dx, dy) = (s1[0] - s0[0], s1[1] - s0[1], d1[0] - d0[0], d1[1] - d0[1]);
        let n = sx * sx + sy * sy;
        let (a, b) = ((dx * sx + dy * sy) / n, (dy * sx - dx * sy) / n);
        [
            a,
            b,
            -b,
            a,
            d0[0] - (a * s0[0] - b * s0[1]),
            d0[1] - (b * s0[0] + a * s0[1]),
        ]
    };
    let carry = |p: [f64; 2]| {
        [
            m[0] * p[0] + m[2] * p[1] + m[4],
            m[1] * p[0] + m[3] * p[1] + m[5],
        ]
    };
    for (s, d) in stored.iter().zip(drawn) {
        let q = carry(pt(s));
        if (q[0] - d[0]).abs() > 1e-6 * (1.0 + d[0].abs())
            || (q[1] - d[1]).abs() > 1e-6 * (1.0 + d[1].abs())
        {
            return Err(refuse);
        }
    }
    Ok(Some(m))
}
/// A dab carried through the pose: anchors through the affine, relative handles through its linear part.
fn posed(mut path: DabPath, m: [f64; 6]) -> DabPath {
    for s in &mut path.segments {
        let lin = |v: [f64; 2]| [m[0] * v[0] + m[2] * v[1], m[1] * v[0] + m[3] * v[1]];
        let p = lin(s.p);
        s.p = [p[0] + m[4], p[1] + m[5]];
        s.h_in = lin(s.h_in);
        s.h_out = lin(s.h_out);
    }
    path
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
    let ribbon = ribbon_walk(record)?;
    // Stamp the stored stroke, then pose every dab as the anchor is posed.
    let stored = stored_segments(record)?;
    let pose = fit_pose(&stored, drawn)?;
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
        None => build_brush_dabs(
            &stored,
            record.geometry.closed,
            &preset,
            width,
            &mut rng,
            None,
        ),
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
                path: match pose {
                    Some(m) => posed(dab.path, m),
                    None => dab.path,
                },
                style: SceneItemStyle {
                    fill: FillResolution::Solid(solid),
                    stroke: None,
                    order: PaintOrder::FillFirst,
                },
            })
        })
        .collect()
}
