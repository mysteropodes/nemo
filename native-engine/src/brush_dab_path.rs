//! What the baseline stamper walks: a Paper path's arc length, point and tangent (`property_follow_path_sample`,
//! bit-exact with Paper 0.12.18), or `roughenPath`'s seeded displacement of it.
use crate::object_document::property_follow_path_sample::{
    path_curves, path_length, point_tangent_at, SegmentIn,
};
use std::f64::consts::PI;

/// What `buildBrushDabs` reads of its path: Paper's arc length, point and tangent.
pub(crate) trait PathLike {
    fn length(&self) -> f64;
    fn point_at(&self, at: f64) -> Option<[f64; 2]>;
    /// `getTangentAt(at) || new Point(1, 0)`.
    fn tangent_at(&self, at: f64) -> [f64; 2];
}

/// A Paper path from its segments.
pub(crate) struct StrokePath {
    curves: Vec<[f64; 8]>,
    closed: bool,
    length: f64,
}
impl StrokePath {
    /// `new Path(segments)`: `[x, y, inX, inY, outX, outY]` per segment.
    pub(crate) fn new(segments: &[[f64; 6]], closed: bool) -> Self {
        let input: Vec<SegmentIn> = segments
            .iter()
            .map(|s| SegmentIn {
                point: [s[0], s[1]],
                handle_in: Some([s[2], s[3]]),
                handle_out: Some([s[4], s[5]]),
            })
            .collect();
        let curves = path_curves(&input, closed);
        let length = path_length(&curves);
        Self {
            curves,
            closed,
            length,
        }
    }
}
impl PathLike for StrokePath {
    fn length(&self) -> f64 {
        self.length
    }
    fn point_at(&self, at: f64) -> Option<[f64; 2]> {
        point_tangent_at(&self.curves, self.closed, at).map(|p| p.point)
    }
    fn tangent_at(&self, at: f64) -> [f64; 2] {
        point_tangent_at(&self.curves, self.closed, at)
            .and_then(|p| p.tangent)
            .unwrap_or([1.0, 0.0])
    }
}

/// `roughenPath`: the stroke displaced along its normal by a seeded, cosine-eased bump field every 120 px.
pub(crate) struct Rough<'a> {
    pub(crate) base: &'a dyn PathLike,
    pub(crate) length: f64,
    pub(crate) bump_seed: u32,
    pub(crate) magnitude: f64,
    pub(crate) eps: f64,
}
impl Rough<'_> {
    fn bump(&self, i: i64) -> f64 {
        let mut h = self.bump_seed ^ ((i + 1) as u32).wrapping_mul(0x9E37_79B1);
        h = (h ^ (h >> 15)).wrapping_mul(1 | h);
        h = h.wrapping_add((h ^ (h >> 7)).wrapping_mul(61 | h)) ^ h;
        (f64::from(h ^ (h >> 14)) / 4294967296.0) * 2.0 - 1.0
    }
    fn smooth_at(&self, at: f64) -> f64 {
        let f = at / 120.0;
        let i0 = f.floor();
        let t = f - i0;
        let ease = 0.5 - 0.5 * (t * PI).cos();
        self.bump(i0 as i64) * (1.0 - ease) + self.bump(i0 as i64 + 1) * ease
    }
    fn offset(&self, at: f64) -> Option<[f64; 2]> {
        let clamped = 0f64.max(self.length.min(at));
        let pt = self.base.point_at(clamped)?;
        let tan = self.base.tangent_at(clamped);
        let k = self.smooth_at(clamped) * self.magnitude;
        Some([pt[0] + -tan[1] * k, pt[1] + tan[0] * k])
    }
}
impl PathLike for Rough<'_> {
    fn length(&self) -> f64 {
        self.length
    }
    fn point_at(&self, at: f64) -> Option<[f64; 2]> {
        self.offset(at)
    }
    fn tangent_at(&self, at: f64) -> [f64; 2] {
        let (a, b) = (0f64.max(at - self.eps), self.length.min(at + self.eps));
        match (self.offset(b), self.offset(a)) {
            (Some(pb), Some(pa)) => {
                let d = [pb[0] - pa[0], pb[1] - pa[1]];
                let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
                if len > 1e-9 {
                    let scale = 1.0 / len;
                    return [d[0] * scale, d[1] * scale];
                }
                self.base.tangent_at(at)
            }
            _ => self.base.tangent_at(at),
        }
    }
}
