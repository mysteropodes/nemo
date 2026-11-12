//! Shared Paper-compatible cubic sampling for pressure and tween geometry.
use kurbo::{BezPath, PathSeg, Point};
// ---- arc-length sampling over a whole (possibly multi-segment) BezPath ----
// The active JS generation path uses Paper sampling. Reuse its already-ported
// D30 sampler: geometry-wasm's 0.1 inverse-arclength tolerance moves even a
// straight-line sample by 0.01848 px (independent baseline fixture), and finite
// differences also change tangents. Do not loosen the oracle to hide that drift.
pub(crate) struct PathSampler {
    pub(crate) curves: Vec<[f64; 8]>,
    pub(crate) closed: bool,
    pub(crate) total: f64,
}
impl PathSampler {
    pub(crate) fn new(path: &BezPath) -> Self {
        use crate::object_document::property_follow_path_sample::path_length;
        let curves: Vec<_> = path
            .segments()
            .map(|seg| {
                let c = match seg {
                    PathSeg::Line(l) => [l.p0, l.p0, l.p1, l.p1],
                    PathSeg::Quad(q) => {
                        let c = q.raise();
                        [c.p0, c.p1, c.p2, c.p3]
                    }
                    PathSeg::Cubic(c) => [c.p0, c.p1, c.p2, c.p3],
                };
                [
                    c[0].x, c[0].y, c[1].x, c[1].y, c[2].x, c[2].y, c[3].x, c[3].y,
                ]
            })
            .collect();
        let total = path_length(&curves);
        let closed = matches!(path.elements().last(), Some(kurbo::PathEl::ClosePath));
        Self {
            curves,
            closed,
            total,
        }
    }
    pub(crate) fn point_at(&self, dist: f64) -> Point {
        use crate::object_document::property_follow_path_sample::point_tangent_at;
        let point = point_tangent_at(&self.curves, self.closed, dist.clamp(0.0, self.total))
            .map(|p| p.point)
            .unwrap_or([0.0; 2]);
        Point::new(point[0], point[1])
    }
    pub(crate) fn tangent_at(&self, dist: f64) -> (f64, f64) {
        use crate::object_document::property_follow_path_sample::point_tangent_at;
        let tangent = point_tangent_at(&self.curves, self.closed, dist.clamp(0.0, self.total))
            .and_then(|p| p.tangent)
            .unwrap_or([1.0, 0.0]);
        (tangent[0], tangent[1])
    }
}

pub(crate) fn add_segs(
    path: &mut BezPath,
    iter: impl Iterator<Item = ([f64; 2], [f64; 2], [f64; 2])>,
) {
    let items: Vec<_> = iter.collect();
    if items.is_empty() {
        return;
    }
    path.move_to((items[0].0[0], items[0].0[1]));
    for i in 0..items.len() - 1 {
        let (a_pt, _a_in, a_out) = items[i];
        let (b_pt, b_in, _b_out) = items[i + 1];
        let c1 = (a_pt[0] + a_out[0], a_pt[1] + a_out[1]);
        let c2 = (b_pt[0] + b_in[0], b_pt[1] + b_in[1]);
        path.curve_to(c1, c2, (b_pt[0], b_pt[1]));
    }
}
