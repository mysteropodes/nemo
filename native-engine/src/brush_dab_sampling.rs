//! The baseline stamper's random and width helpers (`seededRng`, `sampleAcrossWidth`, `sampleGaussian`,
//! `pressureCurveMul`, `widthAtFrac` and `jitterColorValue` in `src/js/tools.js` @1065e9d8).
use crate::evaluation::brush_dab::presets::DabPreset;
use std::f64::consts::PI;

/// `seededRng(seed)`: mulberry32 over the seed's unsigned 32 bits.
pub(crate) struct SeededRng(u32);
impl SeededRng {
    pub(crate) fn new(seed: u32) -> Self {
        Self(seed)
    }
    pub(crate) fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let t = self.0;
        let mut r = (t ^ (t >> 15)).wrapping_mul(1 | t);
        r = r.wrapping_add((r ^ (r >> 7)).wrapping_mul(61 | r)) ^ r;
        f64::from(r ^ (r >> 14)) / 4294967296.0
    }
}

pub(crate) fn sample_across_width(
    rand: &mut dyn FnMut() -> f64,
    distribution: Option<&str>,
) -> f64 {
    let u = rand() * 2.0 - 1.0;
    let inside = |v: f64| (-1.0..=1.0).contains(&v);
    match distribution {
        Some("normal") => {
            let (u1, u2) = (1e-12f64.max(rand()), rand());
            let n = (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos() / 3.0;
            if inside(n) {
                n
            } else {
                u
            }
        }
        Some("center") => {
            let e = -(1e-12f64.max(rand())).ln() / 4.0;
            let s = (if rand() < 0.5 { -1.0 } else { 1.0 }) * e;
            if inside(s) {
                s
            } else {
                u
            }
        }
        Some("edge") => {
            let e = -(1e-12f64.max(rand())).ln() / 4.0;
            let s = if rand() < 0.5 { -1.0 + e } else { 1.0 - e };
            if inside(s) {
                s
            } else {
                u
            }
        }
        _ => u,
    }
}
pub(crate) fn sample_gaussian(rand: &mut dyn FnMut() -> f64) -> f64 {
    let (u1, u2) = (1e-12f64.max(rand()), rand());
    (-2.0 * u1.ln()).sqrt() * (2.0 * PI * u2).cos()
}
pub(crate) fn pressure_curve_mul(preset: &DabPreset, frac: f64) -> f64 {
    let (s, m, e) = (
        preset.pressure_start.unwrap_or(1.0),
        preset.pressure_mid.unwrap_or(1.0),
        preset.pressure_end.unwrap_or(1.0),
    );
    if s == 1.0 && m == 1.0 && e == 1.0 {
        return 1.0;
    }
    if frac <= 0.5 {
        s + (m - s) * (frac / 0.5)
    } else {
        m + (e - m) * ((frac - 0.5) / 0.5)
    }
}
/// `widthAtFrac(profile, frac)` over `(t, width)` stops.
pub(crate) fn width_at_frac(profile: &[(f64, f64)], frac: f64) -> f64 {
    let (first, last) = (profile[0], profile[profile.len() - 1]);
    if frac <= first.0 {
        return first.1;
    }
    if frac >= last.0 {
        return last.1;
    }
    let (mut lo, mut hi) = (0usize, profile.len() - 1);
    while hi - lo > 1 {
        let mid = (lo + hi) >> 1;
        if profile[mid].0 < frac {
            lo = mid
        } else {
            hi = mid
        }
    }
    let (a, b) = (profile[lo], profile[hi]);
    let span = 0.0000001f64.max(b.0 - a.0);
    a.1 + (b.1 - a.1) * ((frac - a.0) / span)
}
/// `jitterColorValue(color, delta)` on straight 0..1 channels: toward black or white by `|delta|`, alpha untouched.
pub(crate) fn jitter_color_value(rgba: [f64; 4], delta: f64) -> [f64; 4] {
    if delta == 0.0 || delta.is_nan() {
        return rgba;
    }
    let (mix, amount) = (if delta < 0.0 { 0.0 } else { 1.0 }, 1f64.min(delta.abs()));
    [
        rgba[0] + (mix - rgba[0]) * amount,
        rgba[1] + (mix - rgba[1]) * amount,
        rgba[2] + (mix - rgba[2]) * amount,
        rgba[3],
    ]
}
