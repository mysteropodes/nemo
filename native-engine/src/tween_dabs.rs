//! Deterministic native vector-dab stamping along the interpolated anchor, never independent matching.
use super::*;
use crate::object_document::BrushPresetRecord;
use crate::request_receipts::DispatchErrorCode;
#[path = "tween_dab_sampling.rs"]
mod sampling;
use sampling::*;
#[path = "tween_dab_emit.rs"]
mod emit;
pub(super) use emit::Dab;
use emit::*;
pub(super) fn build_dabs(
    s: &StrokeIn,
    p: &BrushPresetRecord,
    base_width: f64,
    seed: u32,
    mul: f64,
) -> R<Vec<Dab>> {
    // Pathological custom loops refuse before consuming any state; built-in budgets match exactly.
    if val(&p.poly_sides, 5.) > 256.
        || val(&p.bristle_count, 5.) > 900.
        || val(&p.scribble_count, 8.) > 900.
        || p.custom_stamp
            .value()
            .is_some_and(|s| s.segments.len() > 256)
    {
        return Err(DispatchErrorCode::Unavailable);
    }
    let mut base = if base_width == 0. { 3. } else { base_width };
    let mut profile = vec![];
    if is_vb(s) {
        let cs = s.center_segments.as_ref().unwrap();
        let mut lengths = vec![0.];
        for c in cs.windows(2) {
            lengths.push(
                lengths.last().unwrap()
                    + (c[1].point[0] - c[0].point[0]).hypot(c[1].point[1] - c[0].point[1]),
            );
        }
        let total = lengths.last().copied().filter(|&l| l != 0.).unwrap_or(1.);
        profile = cs
            .iter()
            .zip(lengths)
            .map(|(s, l)| WidthSample {
                t: l / total,
                width: if s.width == 0. { base } else { s.width },
            })
            .collect();
        base = profile.iter().map(|p| p.width).sum::<f64>() / profile.len() as f64;
    }
    let mut rng = TweenRng::new(seed);
    let path = DabPath::new(s, p, base, &mut rng);
    let len = path.sampler.total;
    if len <= 0. {
        return Ok(vec![]);
    }
    let nib = val(&p.nib_size, 1.);
    let grain = val(&p.grain, 1.);
    let spacing = val(&p.spacing, 0.35) * if grain <= 0. { 4. } else { 1. / grain.max(0.1) };
    let noise = val(&p.noise, 0.);
    let noise = if noise > 0. {
        (1. + gaussian(&mut rng) * noise * 0.3).max(0.)
    } else {
        1.
    };
    let local = |at: f64| {
        let width = if profile.is_empty() {
            base
        } else {
            width_at(&profile, at / len)
        };
        width * pressure_mul(p, at / len)
    };
    let min = profile
        .iter()
        .map(|p| p.width)
        .fold(f64::INFINITY, f64::min);
    let max = profile
        .iter()
        .map(|p| p.width)
        .fold(f64::NEG_INFINITY, f64::max);
    let signal = !profile.is_empty() && max - min > 1e-6;
    let min_width = profile.iter().map(|p| p.width).fold(base, f64::min);
    let min_space = ((min_width * nib).max(0.5) * spacing).max(0.4);
    let shape = p.tip_shape.value().map(String::as_str);
    let bristles = if shape == Some("bristle") {
        truth(&p.bristle_count, 5.).max(1.)
    } else {
        1.
    };
    let scribbles = if shape == Some("scribble") {
        truth(&p.scribble_count, 8.).max(1.)
    } else {
        1.
    };
    let sharpness = val(&p.sharpness, 1.);
    let soft = if bristles == 1. && scribbles == 1. && sharpness < 1. {
        3.
    } else {
        1.
    };
    let max_positions = (900. / bristles.max(scribbles).max(soft)).floor().max(1.);
    let cap = if len / min_space > max_positions {
        len / max_positions
    } else {
        0.
    };
    let mut dabs = vec![];
    let (mut d, mut guard) = (0., 0usize);
    while d <= len && (guard as f64) < max_positions + 2. && dabs.len() < 900 {
        guard += 1;
        let at = d.min(len);
        let width = local(at);
        let gm = if signal {
            1.5 - ((width_at(&profile, at / len) - min) / (max - min)).clamp(0., 1.)
        } else {
            1.
        };
        let diam = (width * nib).max(0.5);
        let spacing = min_space.max((diam * spacing).max(0.4)).max(cap);
        let gap = truth(&p.dash_gap, 0.);
        if !(gap != 0. && rng.next() < gap) {
            emit_dabs(
                &mut dabs,
                p,
                &mut rng,
                path.point(at),
                path.tangent(at),
                diam,
                gm,
                noise,
                bristles,
                sharpness,
            );
        }
        d += spacing * (1. + (rng.next() * 2. - 1.) * truth(&p.space_jitter, 0.));
    }
    emit_marker(&mut dabs, p, &mut rng, &path, local, noise);
    for dab in &mut dabs {
        dab.opacity *= mul;
        if !dab.opacity.is_finite()
            || dab.segments.iter().any(|s| {
                s.point
                    .iter()
                    .chain(&s.handle_in)
                    .chain(&s.handle_out)
                    .any(|v| !v.is_finite())
            })
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
    }
    Ok(dabs)
}
