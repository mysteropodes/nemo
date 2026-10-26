//! Seeded scatter and analytic trajectory roughness from the original vector dab producer.
use super::*;
use crate::object_document::{BrushPresetRecord, Presence};
pub(super) fn val(v: &Presence<f64>, default: f64) -> f64 {
    v.value().copied().unwrap_or(default)
}
pub(super) fn truth(v: &Presence<f64>, default: f64) -> f64 {
    v.value().copied().filter(|&x| x != 0.).unwrap_or(default)
}
pub(super) fn gaussian(r: &mut TweenRng) -> f64 {
    let a = r.next().max(1e-12);
    let b = r.next();
    (-2. * a.ln()).sqrt() * (std::f64::consts::TAU * b).cos()
}
pub(super) fn across(r: &mut TweenRng, distribution: Option<&str>) -> f64 {
    let u = r.next() * 2. - 1.;
    let sample = match distribution {
        Some("normal") => gaussian(r) / 3.,
        Some("center") => {
            let e = -r.next().max(1e-12).ln() / 4.;
            if r.next() < 0.5 {
                -e
            } else {
                e
            }
        }
        Some("edge") => {
            let e = -r.next().max(1e-12).ln() / 4.;
            if r.next() < 0.5 {
                -1. + e
            } else {
                1. - e
            }
        }
        _ => u,
    };
    if (-1. ..=1.).contains(&sample) {
        sample
    } else {
        u
    }
}
pub(super) fn width_at(p: &[WidthSample], f: f64) -> f64 {
    if f <= p[0].t {
        return p[0].width;
    }
    if f >= p[p.len() - 1].t {
        return p[p.len() - 1].width;
    }
    let (mut lo, mut hi) = (0, p.len() - 1);
    while hi - lo > 1 {
        let mid = (lo + hi) / 2;
        if p[mid].t < f {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    p[lo].width + (p[hi].width - p[lo].width) * (f - p[lo].t) / (p[hi].t - p[lo].t).max(0.0000001)
}
pub(super) fn pressure_mul(p: &BrushPresetRecord, f: f64) -> f64 {
    let (s, m, e) = (
        val(&p.pressure_start, 1.),
        val(&p.pressure_mid, 1.),
        val(&p.pressure_end, 1.),
    );
    if s == 1. && m == 1. && e == 1. {
        1.
    } else if f <= 0.5 {
        s + (m - s) * (f / 0.5)
    } else {
        m + (e - m) * ((f - 0.5) / 0.5)
    }
}
pub(super) struct DabPath {
    pub sampler: PathSampler,
    rough: Option<(u32, f64)>,
}
impl DabPath {
    pub(super) fn new(s: &StrokeIn, p: &BrushPresetRecord, width: f64, rng: &mut TweenRng) -> Self {
        let sampler = PathSampler::new(&build_feat_path(s));
        let amount = val(&p.roughness, 0.);
        let rough = (amount != 0. && sampler.total > 0.).then(|| {
            (
                (rng.next() * 4294967295.) as u32,
                amount * width.max(1.) * 0.8,
            )
        });
        Self { sampler, rough }
    }
    fn bump(seed: u32, i: u32) -> f64 {
        let mut h = seed ^ i.wrapping_add(1).wrapping_mul(0x9e3779b1);
        h = (h ^ (h >> 15)).wrapping_mul(1 | h);
        h = h.wrapping_add((h ^ (h >> 7)).wrapping_mul(61 | h)) ^ h;
        ((h ^ (h >> 14)) as f64 / 4294967296.) * 2. - 1.
    }
    pub(super) fn point(&self, at: f64) -> [f64; 2] {
        let at = at.clamp(0., self.sampler.total);
        let p = self.sampler.point_at(at);
        let Some((seed, mag)) = self.rough else {
            return [p.x, p.y];
        };
        let (x, y) = self.sampler.tangent_at(at);
        let f = at / 120.;
        let i = f.floor() as u32;
        let t = f - f.floor();
        let ease = 0.5 - 0.5 * (t * std::f64::consts::PI).cos();
        let offset = (Self::bump(seed, i) * (1. - ease) + Self::bump(seed, i + 1) * ease) * mag;
        [p.x - y * offset, p.y + x * offset]
    }
    pub(super) fn tangent(&self, at: f64) -> [f64; 2] {
        let fallback = || {
            let (x, y) = self.sampler.tangent_at(at);
            [x, y]
        };
        if self.rough.is_none() {
            return fallback();
        }
        let eps = (self.sampler.total / 2000.).max(0.01);
        let a = self.point((at - eps).max(0.));
        let b = self.point((at + eps).min(self.sampler.total));
        let d = [b[0] - a[0], b[1] - a[1]];
        let length = d[0].hypot(d[1]);
        if length > 1e-9 {
            [d[0] / length, d[1] / length]
        } else {
            fallback()
        }
    }
}
