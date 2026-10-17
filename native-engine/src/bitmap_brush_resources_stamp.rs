//! Bitmap-brush stamp plan (L07-E / G06, G17): the deterministic placement the
//! baseline `bakeToCanvas` (`src/js/bitmap-brush.js`, fbe4b8a3) performs before
//! it touches a single pixel.
//!
//! A plan is the texture geometry (world rectangle, `SCALE`, canvas size, the
//! effective canvas alpha) plus the ordered stamp list (walk point, jitter, world
//! size and the `drawImage` rectangle). It is a pure function of the brush spec,
//! the anchor path and the stored pressure profile, so a re-stamp with the stored
//! seed never "boils". The Canvas2D compositing itself stays with the JS baker;
//! this kernel only supplies what a published bitmap must agree with
//! (`StampPlan::validate_published`, `StampPlan::byte_cost`).
//!
//! Baseline facts mirrored here (each pinned by `tests/` against the verbatim
//! function run on real Paper.js 0.12.18):
//! - `seededRng(seed)`: `s = seed >>> 0`, then `s = (s * 1664525 + 1013904223) >>> 0`
//!   and `s / 4294967296` per draw; three draws per stamp, in the order jitter x,
//!   jitter y, size, even when scatter is 0.
//! - `spacing = max(1, size * spacing / 100)`, `scatter = size * scatter / 100`,
//!   `pad = size * 1.25 / 2 + scatter + 4`, the box is `path.bounds` grown by `pad`
//!   (`w`, `h` floored at 1), `SCALE = min(2, 4096 / w, 4096 / h)` and the canvas is
//!   `max(1, ceil(w * SCALE))` by `max(1, ceil(h * SCALE))`.
//! - The walk stamps the first segment point, then samples every curve at running
//!   arc-length offsets (`Curve.getPointAt`; curves shorter than 1e-6 are skipped and
//!   the offset carries over), accepting a sample only when its squared distance from
//!   the previous ACCEPTED stamp is at least `(spacing * 0.5)^2` (the cusp-loop gate),
//!   and a closed path re-stamps its first point.
//! - Pressure is the nearest stored sample (`<` keeps the first of equals), 1 without
//!   a profile or with `spec.pressure` off (a stale profile is ignored).
//! - A canvas ignores a `globalAlpha` that is NaN, infinite or outside `[0, 1]`, so the
//!   effective alpha of such an opacity is the initial 1.
//!
//! Not baseline, a resource guard: a walk of more than `max_samples` offsets is the
//! typed `TooManySamples`, never a silent truncation. Non-finite brush numbers or
//! path coordinates are the typed `NonFinite` (the baseline would feed NaN to a
//! canvas size). `StampRules` carries the baseline constants so the controls can
//! mutate each one; `BASELINE` is the only value production callers use.

use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_policy::StoreRules;
use crate::evaluation::object_scene_style_kernels::bitmap_brush_resources_stamp_bounds::path_bounds;
use crate::evaluation::property_follow_path_sample::{
    path_curves, path_length, point_tangent_at, SegmentIn,
};

/// The brush fields of `spec` the placement reads (`tip` and `color` only matter to the baker).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StampSpec {
    pub size: f64,
    /// Percent of `size`.
    pub spacing: f64,
    /// Percent of `size`.
    pub scatter: f64,
    /// Any JS number; the RNG reads `seed >>> 0`.
    pub seed: f64,
    pub opacity: f64,
    pub pressure: bool,
}

/// The anchor path as `basePath.segments` / `closed` and `data.bitmapPressureProfile`
/// (`[x, y, pressure]` samples, `None` when the path has none).
#[derive(Clone, Copy, Debug)]
pub(crate) struct StampPath<'a> {
    pub segments: &'a [SegmentIn],
    pub closed: bool,
    pub pressure_profile: Option<&'a [[f64; 3]]>,
}

/// One accepted stamp. `x`/`y` is the walk point (the pressure lookup key); the
/// jitter is added before the world-to-canvas mapping.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Stamp {
    pub x: f64,
    pub y: f64,
    pub jitter_x: f64,
    pub jitter_y: f64,
    /// World size `size * (0.75 + r * 0.5) * pressure`.
    pub size: f64,
    /// Canvas-space centre and size: `(x + jx - minX) * SCALE`, `size * SCALE`.
    pub px: f64,
    pub py: f64,
    pub ps: f64,
}

impl Stamp {
    /// Left edge of the `drawImage` rectangle (`px - ps / 2`).
    pub(crate) fn left(&self) -> f64 {
        self.px - self.ps / 2.0
    }

    /// Top edge of the `drawImage` rectangle (`py - ps / 2`).
    pub(crate) fn top(&self) -> f64 {
        self.py - self.ps / 2.0
    }
}

/// The world rectangle a baked bitmap covers (`minX, minY, w, h` of `bakeToCanvas`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WorldRect {
    pub min_x: f64,
    pub min_y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextureGeometry {
    pub rect: WorldRect,
    pub scale: f64,
    pub pixel_width: u32,
    pub pixel_height: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StampPlan {
    pub texture: TextureGeometry,
    /// The `globalAlpha` the canvas actually applies to every stamp.
    pub alpha: f64,
    pub stamps: Vec<Stamp>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum StampPlanError {
    /// A brush number, path coordinate or pressure sample is NaN or infinite.
    NonFinite(&'static str),
    /// The padded box or the canvas size overflowed.
    NonFiniteGeometry,
    PixelSizeOutOfRange,
    TooManySamples {
        limit: usize,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PublishError {
    /// The published bitmap is not the planned canvas size.
    PixelSize {
        expected: (u32, u32),
        actual: (u32, u32),
    },
    /// The published world rectangle is not the planned one.
    WorldRect,
}

/// The baseline `seededRng` closure.
#[derive(Clone, Debug)]
pub(crate) struct Lcg {
    state: u64,
    multiplier: u64,
    increment: u64,
}

impl Lcg {
    /// One draw in `[0, 1)`: `s = (s * mul + inc) >>> 0`, `s / 2^32` (exact in doubles).
    pub(crate) fn next(&mut self) -> f64 {
        self.state = self
            .state
            .wrapping_mul(self.multiplier)
            .wrapping_add(self.increment)
            & 0xFFFF_FFFF;
        self.state as f64 / 4294967296.0
    }
}

/// ECMAScript `ToUint32` (`seed >>> 0`).
pub(crate) fn to_uint32(value: f64) -> u32 {
    if !value.is_finite() {
        return 0;
    }
    let modulo = value.trunc() % 4294967296.0;
    let wrapped = if modulo < 0.0 {
        modulo + 4294967296.0
    } else {
        modulo
    };
    wrapped as u32
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct StampRules {
    pub lcg_multiplier: u64,
    pub lcg_increment: u64,
    /// Baseline false: jitter x, jitter y, then size.
    pub size_draw_first: bool,
    /// The cusp gate is `(spacing * gate_factor)^2`.
    pub gate_factor: f64,
    /// Baseline true: a sample at exactly the gate distance is kept (skip only below it).
    pub gate_keeps_tie: bool,
    /// `Math.max(1, size * spacing / 100)`.
    pub min_spacing: f64,
    /// `Math.max(1, ...)` on the padded box width and height.
    pub min_extent: f64,
    pub max_tex: f64,
    pub max_scale: f64,
    pub pad_factor: f64,
    pub pad_margin: f64,
    /// Baseline true: a closed path stamps its first point again.
    pub restamp_closed_start: bool,
    /// Resource guard (not baseline): sampled offsets per plan.
    pub max_samples: usize,
}

impl StampRules {
    pub(crate) const BASELINE: StampRules = StampRules {
        lcg_multiplier: 1664525,
        lcg_increment: 1013904223,
        size_draw_first: false,
        gate_factor: 0.5,
        gate_keeps_tie: true,
        min_spacing: 1.0,
        min_extent: 1.0,
        max_tex: 4096.0,
        max_scale: 2.0,
        pad_factor: 1.25,
        pad_margin: 4.0,
        restamp_closed_start: true,
        max_samples: 1 << 20,
    };

    /// `seededRng(seed)`.
    pub(crate) fn rng(&self, seed: f64) -> Lcg {
        Lcg {
            state: u64::from(to_uint32(seed)),
            multiplier: self.lcg_multiplier,
            increment: self.lcg_increment,
        }
    }

    /// The placement `bakeToCanvas` performs for `spec` over `path`.
    pub(crate) fn plan(
        &self,
        spec: &StampSpec,
        path: &StampPath<'_>,
    ) -> Result<StampPlan, StampPlanError> {
        validate(spec, path)?;
        let base_size = spec.size;
        let spacing = f64::max(self.min_spacing, base_size * (spec.spacing / 100.0));
        let scatter_px = base_size * (spec.scatter / 100.0);
        let bounds = path_bounds(path.segments, path.closed);
        let pad = base_size * self.pad_factor / 2.0 + scatter_px + self.pad_margin;
        let min_x = bounds.x - pad;
        let min_y = bounds.y - pad;
        let w = f64::max(self.min_extent, bounds.width + pad * 2.0);
        let h = f64::max(self.min_extent, bounds.height + pad * 2.0);
        if !(min_x.is_finite() && min_y.is_finite() && w.is_finite() && h.is_finite()) {
            return Err(StampPlanError::NonFiniteGeometry);
        }
        let scale = self.max_scale.min(self.max_tex / w).min(self.max_tex / h);
        let pixel_width = pixel_extent(w * scale)?;
        let pixel_height = pixel_extent(h * scale)?;
        let alpha = if (0.0..=1.0).contains(&spec.opacity) {
            spec.opacity
        } else {
            1.0
        };
        let profile = if spec.pressure {
            path.pressure_profile
        } else {
            None
        };
        let mut walk = Walk {
            rules: self,
            rng: self.rng(spec.seed),
            base_size,
            scatter_px,
            min_x,
            min_y,
            scale,
            gate_sq: (spacing * self.gate_factor) * (spacing * self.gate_factor),
            profile,
            previous: None,
            stamps: Vec::new(),
        };
        if let Some(first) = path.segments.first() {
            walk.attempt(first.point[0], first.point[1]);
        }
        let mut offset = 0.0f64;
        let mut samples = 0usize;
        for values in path_curves(path.segments, path.closed) {
            let one = [values];
            let length = path_length(&one);
            if length < 1e-6 {
                continue;
            }
            while offset < length {
                samples += 1;
                if samples > self.max_samples {
                    return Err(StampPlanError::TooManySamples {
                        limit: self.max_samples,
                    });
                }
                if let Some(found) = point_tangent_at(&one, false, offset) {
                    walk.attempt(found.point[0], found.point[1]);
                }
                offset += spacing;
            }
            offset -= length;
        }
        if self.restamp_closed_start && path.closed {
            if let Some(first) = path.segments.first() {
                walk.attempt(first.point[0], first.point[1]);
            }
        }
        Ok(StampPlan {
            texture: TextureGeometry {
                rect: WorldRect { min_x, min_y, w, h },
                scale,
                pixel_width,
                pixel_height,
            },
            alpha,
            stamps: walk.stamps,
        })
    }
}

/// `canvas.width = Math.max(1, Math.ceil(extent))` as an unsigned size.
fn pixel_extent(extent: f64) -> Result<u32, StampPlanError> {
    let pixels = f64::max(1.0, extent.ceil());
    if pixels.is_finite() && pixels <= f64::from(u32::MAX) {
        Ok(pixels as u32)
    } else {
        Err(StampPlanError::PixelSizeOutOfRange)
    }
}

fn validate(spec: &StampSpec, path: &StampPath<'_>) -> Result<(), StampPlanError> {
    for (name, value) in [
        ("size", spec.size),
        ("spacing", spec.spacing),
        ("scatter", spec.scatter),
    ] {
        if !value.is_finite() {
            return Err(StampPlanError::NonFinite(name));
        }
    }
    for segment in path.segments {
        let handle_in = segment.handle_in.unwrap_or([0.0, 0.0]);
        let handle_out = segment.handle_out.unwrap_or([0.0, 0.0]);
        let all = [
            segment.point[0],
            segment.point[1],
            handle_in[0],
            handle_in[1],
            handle_out[0],
            handle_out[1],
        ];
        if all.iter().any(|v| !v.is_finite()) {
            return Err(StampPlanError::NonFinite("segment"));
        }
    }
    if spec.pressure {
        if let Some(profile) = path.pressure_profile {
            if profile.iter().flatten().any(|v| !v.is_finite()) {
                return Err(StampPlanError::NonFinite("pressure"));
            }
        }
    }
    Ok(())
}

/// `pressureAt(x, y)`: the nearest stored sample, the first of equals; 1 for none.
fn pressure_at(profile: Option<&[[f64; 3]]>, x: f64, y: f64) -> f64 {
    let samples = match profile {
        Some(samples) if !samples.is_empty() => samples,
        _ => return 1.0,
    };
    let mut best = samples[0];
    let mut best_distance = f64::INFINITY;
    for sample in samples {
        let dx = sample[0] - x;
        let dy = sample[1] - y;
        let distance = dx * dx + dy * dy;
        if distance < best_distance {
            best_distance = distance;
            best = *sample;
        }
    }
    best[2]
}

/// `tryStamp` / `stampAt` with their closure state.
struct Walk<'a> {
    rules: &'a StampRules,
    rng: Lcg,
    base_size: f64,
    scatter_px: f64,
    min_x: f64,
    min_y: f64,
    scale: f64,
    gate_sq: f64,
    profile: Option<&'a [[f64; 3]]>,
    previous: Option<(f64, f64)>,
    stamps: Vec<Stamp>,
}

impl Walk<'_> {
    fn attempt(&mut self, x: f64, y: f64) {
        if let Some((previous_x, previous_y)) = self.previous {
            let dx = x - previous_x;
            let dy = y - previous_y;
            let distance = dx * dx + dy * dy;
            let skip = if self.rules.gate_keeps_tie {
                distance < self.gate_sq
            } else {
                distance <= self.gate_sq
            };
            if skip {
                return;
            }
        }
        self.previous = Some((x, y));
        let jitter_x: f64;
        let jitter_y: f64;
        let unit: f64;
        if self.rules.size_draw_first {
            unit = self.rng.next();
            jitter_x = (self.rng.next() - 0.5) * 2.0 * self.scatter_px;
            jitter_y = (self.rng.next() - 0.5) * 2.0 * self.scatter_px;
        } else {
            jitter_x = (self.rng.next() - 0.5) * 2.0 * self.scatter_px;
            jitter_y = (self.rng.next() - 0.5) * 2.0 * self.scatter_px;
            unit = self.rng.next();
        }
        let size = self.base_size * (0.75 + unit * 0.5) * pressure_at(self.profile, x, y);
        self.stamps.push(Stamp {
            x,
            y,
            jitter_x,
            jitter_y,
            size,
            px: (x + jitter_x - self.min_x) * self.scale,
            py: (y + jitter_y - self.min_y) * self.scale,
            ps: size * self.scale,
        });
    }
}

impl StampPlan {
    /// Decoded RGBA bytes of the planned canvas, through the store's own checked cost rule.
    pub(crate) fn byte_cost(&self) -> Option<usize> {
        StoreRules::BASELINE.cost(self.texture.pixel_width, self.texture.pixel_height)
    }

    /// Refuse a published bitmap whose pixel size or world rectangle disagrees with the plan.
    pub(crate) fn validate_published(
        &self,
        width: u32,
        height: u32,
        rect: WorldRect,
    ) -> Result<(), PublishError> {
        let expected = (self.texture.pixel_width, self.texture.pixel_height);
        if (width, height) != expected {
            return Err(PublishError::PixelSize {
                expected,
                actual: (width, height),
            });
        }
        if rect != self.texture.rect {
            return Err(PublishError::WorldRect);
        }
        Ok(())
    }
}

/// The baseline plan for `spec` over `path`.
pub(crate) fn plan_stamps(
    spec: &StampSpec,
    path: &StampPath<'_>,
) -> Result<StampPlan, StampPlanError> {
    StampRules::BASELINE.plan(spec, path)
}

#[cfg(test)]
#[path = "../tests/bitmap_brush_resources_stamp_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/bitmap_brush_resources_stamp_rows.rs"]
mod rows;
#[cfg(test)]
#[path = "../tests/bitmap_brush_resources_stamp.rs"]
mod tests;
