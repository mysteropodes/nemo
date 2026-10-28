//! The baseline vector-brush dab stamper (`seededRng`, `sampleAcrossWidth`, `sampleGaussian`, `pressureCurveMul`,
//! `widthAtFrac`, `roughenPath`, `buildDabShape`, `buildBrushDabs` and `jitterColorValue` in `src/js/tools.js`
//! @1065e9d8), ported line for line: the same random draws in the same order (every `(rand()*2-1)*(x||0)` draws, even
//! when `x` is zero), the same spacing walk and 900-dab budget, and the same Paper geometry (`brush_dab_shape`) along
//! the stroke's Paper arc length (`property_follow_path_sample`, bit-exact with Paper 0.12.18).
use crate::object_document::brush_dab::path::{PathLike, Rough, StrokePath};
use crate::object_document::brush_dab::presets::{or, truthy, DabPreset, BRUSH_MAX_DABS, SHARPNESS_LAYERS};
use crate::object_document::brush_dab::sampling::{
    pressure_curve_mul, sample_across_width, sample_gaussian, width_at_frac, SeededRng,
};
use crate::object_document::brush_dab::shape::{
    ellipse, rectangle, rotate, set_position, smooth_catmull_rom, smooth_continuous, DabPath, Seg,
};
use std::f64::consts::PI;

/// One dab: its closed path and the per-dab opacity and value delta the anchor's colour is jittered by.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Dab {
    pub(crate) path: DabPath,
    pub(crate) opacity: f64,
    pub(crate) value_delta: f64,
}

/// A stamping walk ran off its path: the baseline throws (`getPointAt` returned null), so no dab is produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DabPathUnavailable;

fn closed_polygon(points: Vec<[f64; 2]>) -> DabPath {
    let z = [0.0, 0.0];
    DabPath {
        segments: points
            .into_iter()
            .map(|p| Seg {
                p,
                h_in: z,
                h_out: z,
            })
            .collect(),
        closed: true,
    }
}
/// `buildDabShape(w, h, preset, rand, edgeNoiseOverride)`: one closed dab around the origin.
fn build_dab_shape(
    w: f64,
    h: f64,
    preset: &DabPreset,
    rand: &mut dyn FnMut() -> f64,
    edge_override: Option<f64>,
) -> DabPath {
    let shape = preset
        .tip_shape
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("ellipse");
    let custom = preset
        .custom_stamp
        .as_ref()
        .filter(|stamp| shape == "custom" && !stamp.segments.is_empty());
    let mut path = if shape == "rect" {
        let rx = w.min(h) * preset.tip_corner.unwrap_or(0.15);
        rectangle(-w / 2.0, -h / 2.0, w, h, rx)
    } else if shape == "polygon" {
        let sides = 3f64.max(or(preset.poly_sides, 5.0));
        let mut points = Vec::new();
        let mut i = 0.0;
        while i < sides {
            let a = (i / sides) * PI * 2.0;
            let rr = (w / 2.0) * (0.8 + rand() * 0.35);
            let rh = (h / 2.0) * (0.8 + rand() * 0.35);
            points.push([a.cos() * rr, a.sin() * rh]);
            i += 1.0;
        }
        let mut path = closed_polygon(points);
        smooth_continuous(&mut path);
        path
    } else if shape == "splatter" {
        let pts = 6.0 + (rand() * 5.0).floor();
        let mut points = Vec::new();
        let mut j = 0.0;
        while j < pts {
            let ang = (j / pts) * PI * 2.0;
            let rr = (w / 2.0) * (0.35 + rand() * 1.1);
            let rh = (h / 2.0) * (0.35 + rand() * 1.1);
            points.push([ang.cos() * rr, ang.sin() * rh]);
            j += 1.0;
        }
        let mut path = closed_polygon(points);
        smooth_catmull_rom(&mut path, 0.5);
        path
    } else if let Some(stamp) = custom {
        let segments = stamp
            .segments
            .iter()
            .map(|s| Seg {
                p: [s[0] * w, s[1] * h],
                h_in: [s[2] * w, s[3] * h],
                h_out: [s[4] * w, s[5] * h],
            })
            .collect();
        DabPath {
            segments,
            closed: stamp.closed,
        }
    } else {
        ellipse([0.0, 0.0], [w / 2.0, h / 2.0])
    };
    let edge_noise = edge_override.unwrap_or_else(|| or(preset.edge_noise, 0.0));
    if edge_noise > 0.0 && !path.segments.is_empty() {
        for segment in &mut path.segments {
            let k = 1.0 + (rand() * 2.0 - 1.0) * edge_noise;
            segment.p = [segment.p[0] * k, segment.p[1] * k];
        }
        if shape != "rect" {
            smooth_continuous(&mut path);
        }
    }
    path
}

fn clamp01(v: f64) -> f64 {
    0f64.max(1f64.min(v))
}
fn placed(mut path: DabPath, degrees: f64, center: [f64; 2]) -> DabPath {
    rotate(&mut path, degrees);
    set_position(&mut path, center);
    path
}

/// `buildBrushDabs(path, preset, baseWidth, seededRng(seed), widthProfile)` over the stroke `segments`.
pub(crate) fn build_brush_dabs(
    segments: &[[f64; 6]],
    closed: bool,
    preset: &DabPreset,
    base_width: f64,
    rng: &mut SeededRng,
    profile: Option<&[(f64, f64)]>,
) -> Result<Vec<Dab>, DabPathUnavailable> {
    let mut rand = || rng.next();
    let stroke = StrokePath::new(segments, closed);
    if !(stroke.length() > 0.0) {
        return Ok(Vec::new());
    }
    let rough;
    let path: &dyn PathLike = if truthy(preset.roughness) {
        let length = stroke.length();
        let bump_seed = (rand() * 4294967295.0) as u32;
        rough = Rough {
            base: &stroke,
            length,
            bump_seed,
            magnitude: preset.roughness.unwrap_or(0.0) * 1f64.max(base_width) * 0.8,
            eps: 0.01f64.max(length / 2000.0),
        };
        &rough
    } else {
        &stroke
    };
    let len = path.length();
    let profile = profile.filter(|p| !p.is_empty());
    let nib_scale = preset.nib_size.unwrap_or(1.0);
    let grain = preset.grain.unwrap_or(1.0);
    let spacing_frac = preset.spacing.unwrap_or(0.35)
        * if grain <= 0.0 {
            4.0
        } else {
            1.0 / 0.1f64.max(grain)
        };
    let noise = preset.noise.unwrap_or(0.0);
    let stroke_noise = if noise > 0.0 {
        0f64.max(1.0 + sample_gaussian(&mut rand) * noise * 0.3)
    } else {
        1.0
    };
    let frac_at = |at: f64| if len > 0.0 { at / len } else { 0.0 };
    let local_width = |at: f64| {
        let base = profile.map_or(base_width, |p| width_at_frac(p, frac_at(at)));
        base * pressure_curve_mul(preset, frac_at(at))
    };
    let (wp_min, wp_max) = profile.map_or((f64::INFINITY, f64::NEG_INFINITY), |p| {
        p.iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), s| {
                (lo.min(s.1), hi.max(s.1))
            })
    });
    let pressure_signal = profile.is_some() && wp_max - wp_min > 1e-6;
    let grain_mul = |at: f64| match profile {
        Some(p) if pressure_signal => {
            1.5 - 0f64.max(1f64.min((width_at_frac(p, frac_at(at)) - wp_min) / (wp_max - wp_min)))
        }
        _ => 1.0,
    };
    let min_width = profile.map_or(base_width, |p| p.iter().fold(base_width, |m, s| m.min(s.1)));
    let min_allowed = 0.4f64.max(0.5f64.max(min_width * nib_scale) * spacing_frac);
    let tip = preset.tip_shape.as_deref();
    let bristles = if tip == Some("bristle") {
        1f64.max(or(preset.bristle_count, 5.0))
    } else {
        1.0
    };
    let scribbles = if tip == Some("scribble") {
        1f64.max(or(preset.scribble_count, 8.0))
    } else {
        1.0
    };
    let sharpness = preset.sharpness.unwrap_or(1.0);
    let soft_layers = if bristles == 1.0 && scribbles == 1.0 && sharpness < 1.0 {
        SHARPNESS_LAYERS as f64
    } else {
        1.0
    };
    let per_position = bristles.max(scribbles).max(soft_layers);
    let max_positions = 1f64.max((BRUSH_MAX_DABS as f64 / per_position).floor());
    let cap_floor = if len / min_allowed > max_positions {
        len / max_positions
    } else {
        0.0
    };
    let roundness = preset.roundness.unwrap_or(1.0);
    let base_opacity = preset.opacity.unwrap_or(0.5);
    let jitter =
        |rand: &mut dyn FnMut() -> f64, amount: Option<f64>| (rand() * 2.0 - 1.0) * or(amount, 0.0);
    let (mut dabs, mut d, mut guard) = (Vec::<Dab>::new(), 0.0, 0.0);
    while d <= len
        && {
            let g = guard;
            guard += 1.0;
            g < max_positions + 2.0
        }
        && dabs.len() < BRUSH_MAX_DABS
    {
        let at = len.min(d);
        let nib = 0.5f64.max(local_width(at) * nib_scale);
        let gm = grain_mul(at);
        let mut spacing = min_allowed.max(0.4f64.max(nib * spacing_frac));
        if spacing < cap_floor {
            spacing = cap_floor;
        }
        if !(truthy(preset.dash_gap) && rand() < preset.dash_gap.unwrap_or(0.0)) {
            let pt = path.point_at(at).ok_or(DabPathUnavailable)?;
            let tan = path.tangent_at(at);
            let normal = [-tan[1], tan[0]];
            let angle_base = match preset.rotation_mode.as_deref() {
                Some("random") => rand() * 360.0,
                Some("fixed") => or(preset.fixed_angle, 0.0),
                _ => tan[1].atan2(tan[0]) * 180.0 / PI,
            };
            if tip == Some("scribble") {
                let count = 1f64.max(or(preset.scribble_count, 8.0));
                let length = preset.scribble_len.unwrap_or(1.4) * nib;
                let length_jitter = preset.scribble_len_jitter.unwrap_or(0.4);
                let width = 0.25f64.max(preset.scribble_width.unwrap_or(0.12) * nib);
                let spread = preset.scribble_spread.unwrap_or(0.6) * nib;
                let angle_spread = preset.scribble_angle_spread.unwrap_or(70.0);
                let mut sc = 0.0;
                while sc < count && dabs.len() < BRUSH_MAX_DABS {
                    let (along, across) =
                        ((rand() * 2.0 - 1.0) * spread, (rand() * 2.0 - 1.0) * spread);
                    let center = [
                        pt[0] + tan[0] * along + normal[0] * across,
                        pt[1] + tan[1] * along + normal[1] * across,
                    ];
                    let mark_len =
                        0.4f64.max(length * (1.0 + (rand() * 2.0 - 1.0) * length_jitter));
                    let angle = angle_base + (rand() * 2.0 - 1.0) * angle_spread;
                    let mark =
                        rectangle(-mark_len / 2.0, -width / 2.0, mark_len, width, width / 2.0);
                    let path = placed(mark, angle, center);
                    let opacity = clamp01(
                        base_opacity
                            * (1.0 + jitter(&mut rand, preset.opacity_jitter))
                            * 0.6
                            * stroke_noise,
                    );
                    let value_delta = jitter(&mut rand, preset.value_jitter);
                    dabs.push(Dab {
                        path,
                        opacity,
                        value_delta,
                    });
                    sc += 1.0;
                }
            } else if tip == Some("bristle") {
                let mut b = 0.0;
                while b < bristles && dabs.len() < BRUSH_MAX_DABS {
                    let frac = if bristles > 1.0 {
                        b / (bristles - 1.0) - 0.5
                    } else {
                        0.0
                    };
                    let scatter = or(preset.scatter, 0.0);
                    let strand = frac * nib * (0.5 + scatter * gm);
                    let offset = (rand() * 2.0 - 1.0) * nib * scatter * gm * 0.3;
                    let center = [
                        pt[0] + normal[0] * (strand + offset),
                        pt[1] + normal[1] * (strand + offset),
                    ];
                    let size = 1.0 + jitter(&mut rand, preset.size_jitter);
                    let w = 0.3f64.max((nib / 2f64.max(bristles * 0.7)) * size);
                    let h = 0.3f64.max(nib * roundness * (0.7 + rand() * 0.5));
                    let angle = angle_base + jitter(&mut rand, preset.rotation_jitter);
                    let shape = build_dab_shape(
                        w,
                        h,
                        preset,
                        &mut rand,
                        Some(or(preset.edge_noise, 0.0) * gm),
                    );
                    let path = placed(shape, angle, center);
                    let opacity = clamp01(
                        base_opacity
                            * (1.0 + jitter(&mut rand, preset.opacity_jitter))
                            * 0.7
                            * stroke_noise,
                    );
                    let value_delta = jitter(&mut rand, preset.value_jitter);
                    dabs.push(Dab {
                        path,
                        opacity,
                        value_delta,
                    });
                    b += 1.0;
                }
            } else {
                let scatter =
                    sample_across_width(&mut rand, preset.scatter_distribution.as_deref())
                        * nib
                        * or(preset.scatter, 0.0)
                        * gm;
                let center = [pt[0] + normal[0] * scatter, pt[1] + normal[1] * scatter];
                let size = 1.0 + jitter(&mut rand, preset.size_jitter);
                let w = 0.3f64.max(nib * size);
                let h = 0.3f64.max(w * roundness);
                let angle = angle_base + jitter(&mut rand, preset.rotation_jitter);
                let opacity = clamp01(
                    base_opacity * (1.0 + jitter(&mut rand, preset.opacity_jitter)) * stroke_noise,
                );
                let value_delta = jitter(&mut rand, preset.value_jitter);
                let edge = or(preset.edge_noise, 0.0) * gm;
                if sharpness >= 1.0 {
                    let path = placed(
                        build_dab_shape(w, h, preset, &mut rand, Some(edge)),
                        angle,
                        center,
                    );
                    dabs.push(Dab {
                        path,
                        opacity,
                        value_delta,
                    });
                } else {
                    let halo = 1.0 + (1.0 - sharpness) * 1.4;
                    let mut layer = 0usize;
                    while layer < SHARPNESS_LAYERS && dabs.len() < BRUSH_MAX_DABS {
                        let lt = if SHARPNESS_LAYERS > 1 {
                            layer as f64 / (SHARPNESS_LAYERS - 1) as f64
                        } else {
                            0.0
                        };
                        let scale = 1.0 + lt * (halo - 1.0);
                        let fade = (-lt * lt * 2.5).exp();
                        let path = placed(
                            build_dab_shape(w * scale, h * scale, preset, &mut rand, Some(edge)),
                            angle,
                            center,
                        );
                        dabs.push(Dab {
                            path,
                            opacity: opacity * fade,
                            value_delta,
                        });
                        layer += 1;
                    }
                }
            }
        }
        d += spacing * (1.0 + jitter(&mut rand, preset.space_jitter));
    }
    if tip == Some("rect") && preset.marker_tip != Some(false) {
        for end in [0.0, len] {
            let pt = path.point_at(end).ok_or(DabPathUnavailable)?;
            let tan = path.tangent_at(end);
            let nib = 0.5f64.max(local_width(end) * nib_scale);
            let opacity = clamp01(base_opacity * stroke_noise);
            let mut bk = 0.0;
            while bk < 2.0 && dabs.len() < BRUSH_MAX_DABS {
                let scale = 0.85 + bk * 0.2;
                let shape = build_dab_shape(
                    nib * scale,
                    nib * roundness * scale,
                    preset,
                    &mut rand,
                    None,
                );
                let path = placed(shape, tan[1].atan2(tan[0]) * 180.0 / PI, pt);
                let value_delta = jitter(&mut rand, preset.value_jitter);
                dabs.push(Dab {
                    path,
                    opacity: 1f64.min(opacity * 1.4),
                    value_delta,
                });
                bk += 1.0;
            }
        }
    }
    Ok(dabs)
}
