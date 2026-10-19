//! Native retained fill geometry, adapted from geometry-wasm/src/fill.rs at protected52425370.
//! Pure geometry only: no document state, Paper, WASM transport or renderer.
use crate::object_document::object_group_kernels::object_fill_trace_ops::geometry::*;
use kurbo::{ParamCurve, ParamCurveArclen, PathEl, PathSeg};
pub(super) fn build_bezpath_from_segments(segments: &[RingSeg], closed: bool) -> BezPath {
    let mut path = BezPath::new();
    if segments.is_empty() {
        return path;
    }
    path.move_to((segments[0].point[0], segments[0].point[1]));
    let n = segments.len();
    let last_idx = if closed { n } else { n - 1 };
    for i in 0..last_idx {
        let a = &segments[i];
        let b = &segments[(i + 1) % n];
        let c1 = (a.point[0] + a.handle_out[0], a.point[1] + a.handle_out[1]);
        let c2 = (b.point[0] + b.handle_in[0], b.point[1] + b.handle_in[1]);
        path.curve_to(c1, c2, (b.point[0], b.point[1]));
    }
    if closed {
        path.close_path();
    }
    path
}
pub(super) fn wall_geometry(w: &Wall, closed: bool) -> (BezPath, Vec<[f64; 2]>) {
    let bez = build_bezpath_from_segments(&w.segments, closed);
    let mut pts: Vec<[f64; 2]> = Vec::new();
    kurbo::flatten(bez.elements().iter().copied(), 0.75, |el| match el {
        PathEl::MoveTo(p) | PathEl::LineTo(p) => pts.push([p.x, p.y]),
        _ => {}
    });
    (bez, pts)
}

// Arc-length sampler over a wall's real BezPath (not the flattened
// polyline) — lets the final result be reconstructed by sampling the TRUE
// curve at fine resolution instead of inheriting the coarser flatten
// tolerance used just for graph-building. Mirrors the PathSampler idiom
// already established in tweenmatch.rs.
pub(super) struct PathSampler {
    pub(super) segs: Vec<PathSeg>,
    pub(super) cum_lens: Vec<f64>,
    pub(super) seg_lens: Vec<f64>,
    pub(super) total: f64,
}
impl PathSampler {
    pub(super) fn new(path: &BezPath) -> Self {
        let segs: Vec<PathSeg> = path.segments().collect();
        let mut cum_lens = Vec::with_capacity(segs.len());
        let mut seg_lens = Vec::with_capacity(segs.len());
        let mut cum = 0.0;
        for s in &segs {
            cum_lens.push(cum);
            let l = s.arclen(0.1);
            seg_lens.push(l);
            cum += l;
        }
        PathSampler {
            segs,
            cum_lens,
            seg_lens,
            total: cum,
        }
    }
    pub(super) fn point_at(&self, dist: f64) -> Vec2 {
        if self.segs.is_empty() {
            return Vec2 { x: 0.0, y: 0.0 };
        }
        let d = dist.clamp(0.0, self.total);
        let mut idx = self.segs.len() - 1;
        for i in 0..self.segs.len() {
            if d < self.cum_lens[i] + self.seg_lens[i] {
                idx = i;
                break;
            }
        }
        let local = (d - self.cum_lens[idx]).max(0.0);
        let t = if self.seg_lens[idx] > 1e-9 {
            self.segs[idx].inv_arclen(local, 0.1)
        } else {
            0.0
        };
        let p = self.segs[idx].eval(t);
        Vec2 { x: p.x, y: p.y }
    }
}

// Douglas-Peucker polyline simplification — collapses the dense point
// sample resample_result() produces down to the minimum points needed to
// stay within `eps` of the original curve. Tried extracting EXACT bezier
// subsegments first (real cubic control points, not sampled points) to get
// a low point count directly — arc-length-parametrized subsegment
// extraction on a degenerate (zero-velocity-at-endpoints) cubic turned out
// numerically unstable in kurbo (inv_arclen occasionally converges to the
// wrong parametric t right at that singularity), producing a garbled
// self-crossing result once in a while. Simplifying an already-verified-
// correct dense sample is a much smaller blast radius: it can only ever
// REMOVE points from a polyline whose topology is already right, it can't
// introduce a new one.
pub(super) fn simplify_polyline(pts: &[Vec2], eps: f64) -> Vec<Vec2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    fn perp_dist(p: Vec2, a: Vec2, b: Vec2) -> f64 {
        let d = b.sub(a);
        let len_sq = d.x * d.x + d.y * d.y;
        if len_sq < 1e-12 {
            return p.dist(a);
        }
        let t = ((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len_sq;
        let proj = Vec2 {
            x: a.x + d.x * t,
            y: a.y + d.y * t,
        };
        p.dist(proj)
    }
    fn dp(pts: &[Vec2], eps: f64, out: &mut Vec<Vec2>) {
        let n = pts.len();
        if n < 2 {
            return;
        }
        let (a, b) = (pts[0], pts[n - 1]);
        let mut max_d = 0.0;
        let mut idx = 0;
        for i in 1..n - 1 {
            let d = perp_dist(pts[i], a, b);
            if d > max_d {
                max_d = d;
                idx = i;
            }
        }
        if max_d > eps && idx > 0 {
            dp(&pts[0..=idx], eps, out);
            out.pop(); // shared midpoint — the second half's dp() re-adds it
            dp(&pts[idx..], eps, out);
        } else {
            out.push(a);
            out.push(b);
        }
    }
    let mut out = Vec::new();
    dp(pts, eps, &mut out);
    out
}

pub(super) fn resample_result(
    open_samplers: &[PathSampler],
    nodes: &[Node],
    edges: &[Edge],
    seq: &[Hop],
) -> Vec<Vec2> {
    let mut pts: Vec<Vec2> = Vec::new();
    // Every hop boundary is the SAME real-world node (a crossing or a real
    // wall endpoint), but each hop's own end sample is computed by a
    // DIFFERENT independent source — a Gap hop's boundary is the node's own
    // stored (union-find-averaged) position, while a Stroke hop's boundary
    // is resampled fresh from ITS wall's own PathSampler at that node's
    // frac. These never land bit-identical, only very close — pushing BOTH
    // (the previous hop's last point AND the next hop's first point)
    // creates a near-zero-length "phantom edge" sandwiched between the two
    // real edges either side of it, and floating-point noise in that
    // degenerate edge's direction can flip poly_self_intersects' cross-
    // product sign test, false-flagging a perfectly simple loop as self-
    // intersecting (confirmed live: a clean two-arc lens between two
    // crossings, the coarse loop_points() check already accepted it, but
    // this fine resample duplicated the closing node from two different
    // samplers ~0.02px apart and that alone tripped the self-intersection
    // reject). Skipping a new hop's very first sample whenever it's
    // (nearly) the same point already at the end of `pts` collapses that
    // phantom edge to nothing, leaving one clean shared vertex instead.
    const SEAM_EPS: f64 = 0.1;
    for hop in seq {
        let e = &edges[hop.edge_idx];
        if e.kind == EdgeType::Gap {
            let p = nodes[hop.to].pt;
            if pts
                .last()
                .map_or(true, |last: &Vec2| last.dist(p) > SEAM_EPS)
            {
                pts.push(p);
            }
            continue;
        }
        let sampler = &open_samplers[e.stroke_idx];
        let total = sampler.total;
        let mut da = e.frac_a * total;
        let mut db = e.frac_b * total;
        let reversed = e.a != hop.from;
        if reversed {
            std::mem::swap(&mut da, &mut db);
        }
        // Density scales with the sub-edge's own length so long strokes
        // still look smooth without over-sampling short ones — the point
        // count this produces is cut back down by simplify_polyline() once
        // the whole loop is assembled (see fill_find), so oversampling a
        // little here is cheap and safe.
        let n_samples = ((db - da).abs() / 4.0).ceil().max(6.0) as usize;
        for si in 0..=n_samples {
            let t = si as f64 / n_samples as f64;
            let d = da + (db - da) * t;
            let p = sampler.point_at(d);
            if si == 0
                && pts
                    .last()
                    .map_or(false, |last: &Vec2| last.dist(p) <= SEAM_EPS)
            {
                continue;
            }
            pts.push(p);
        }
    }
    // Same phantom-edge fix at the wrap-around seam (poly_area/
    // poly_self_intersects treat `pts` as implicitly closed, last point
    // back to first) — the loop's very last sample and its very first
    // sample are the same real node too, from two more independent
    // samplers/sources.
    if pts.len() > 2 {
        let first = pts[0];
        if pts[pts.len() - 1].dist(first) <= SEAM_EPS {
            pts.pop();
        }
    }
    pts
}
