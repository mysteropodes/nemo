// Fill-engine graph tracing, ported from the JS reference implementation
// in src/js/tools.js (fillBuildGraph/fillEdgeDir/fillTraceLoop/fillVectorFind).
//
// Was previously a hand-rolled polyline-only affair: JS flattened each wall
// Path to a polyline (Path#flatten), Rust traced the loop over THOSE points
// and only ever handed back which wall/fraction-range/direction made up each
// hop, and JS re-cut the REAL bezier curve for the final result via
// Path#getLocationAt/splitAt so the fill stayed curve-accurate rather than
// polygon-approximated. That split existed because Rust had no curve
// primitives of its own to work with.
//
// It does now: engine.rs's VelloEngine already builds real
// vello::kurbo::BezPath scene geometry from the exact same Paper.js segment
// shape (point/handleIn/handleOut) fill.rs receives, via
// `build_bezpath_from_segments`. Reusing that here means the fill engine
// builds its OWN flattened polylines (via kurbo::flatten, replacing
// Path#flatten) and its OWN curve-accurate final result (via arc-length
// sampling over the real BezPath, replacing getLocationAt/splitAt) — JS no
// longer touches Paper.js curve APIs for fill-finding at all, it only hands
// over raw segment data and gets back a finished segment list to build a
// Path from directly.
//
// One seam intentionally stays JS-side: exact wall-wall crossing points
// (CrossingIn, still optional/falls back to find_crossings below) are still
// computed via Paper.js's own curve-curve intersection (Path#getIntersections).
// kurbo has no general bezier-bezier intersection primitive, and
// reimplementing one robustly is a separate, riskier project — not folded
// into this pass.
use crate::engine::{build_bezpath_from_segments, SegIn};
use serde::{Deserialize, Serialize};
use vello::kurbo::{BezPath, ParamCurve, ParamCurveArclen, PathEl, PathSeg};
use wasm_bindgen::prelude::*;

#[derive(Deserialize)]
struct Wall {
    segments: Vec<SegIn>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CrossingIn {
    wall: usize,
    frac: f64,
    pt: [f64; 2],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FillInput {
    open_walls: Vec<Wall>,
    closed_walls: Vec<Wall>,
    gap_thr: f64,
    click: [f64; 2],
    #[serde(default)]
    crossings: Option<Vec<CrossingIn>>,
}

#[derive(Clone, Copy, PartialEq)]
struct Vec2 {
    x: f64,
    y: f64,
}
impl Vec2 {
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2 { x: self.x - o.x, y: self.y - o.y }
    }
    fn dist(self, o: Vec2) -> f64 {
        ((self.x - o.x).powi(2) + (self.y - o.y).powi(2)).sqrt()
    }
    fn normalize(self) -> Vec2 {
        let l = (self.x * self.x + self.y * self.y).sqrt();
        if l < 1e-9 { Vec2 { x: 1.0, y: 0.0 } } else { Vec2 { x: self.x / l, y: self.y / l } }
    }
}

// Builds an open (non-closed) BezPath for a wall plus its flattened
// polyline (via kurbo's own adaptive flattener) — the polyline is what the
// existing graph-tracing math below operates on, same tolerance class as
// the JS-side `Path#flatten(1)` it replaces.
fn wall_geometry(w: &Wall, closed: bool) -> (BezPath, Vec<[f64; 2]>) {
    let bez = build_bezpath_from_segments(&w.segments, closed);
    let mut pts: Vec<[f64; 2]> = Vec::new();
    vello::kurbo::flatten(bez.elements().iter().copied(), 0.75, |el| match el {
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
struct PathSampler {
    segs: Vec<PathSeg>,
    cum_lens: Vec<f64>,
    seg_lens: Vec<f64>,
    total: f64,
}
impl PathSampler {
    fn new(path: &BezPath) -> Self {
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
        PathSampler { segs, cum_lens, seg_lens, total: cum }
    }
    fn point_at(&self, dist: f64) -> Vec2 {
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
        let t = if self.seg_lens[idx] > 1e-9 { self.segs[idx].inv_arclen(local, 0.1) } else { 0.0 };
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
fn simplify_polyline(pts: &[Vec2], eps: f64) -> Vec<Vec2> {
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
        let proj = Vec2 { x: a.x + d.x * t, y: a.y + d.y * t };
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

struct Node {
    pt: Vec2,
    edges: Vec<usize>,
}
#[derive(Clone, Copy, PartialEq)]
enum EdgeType {
    Stroke,
    Gap,
}
struct Edge {
    kind: EdgeType,
    stroke_idx: usize, // valid only for Stroke edges — which ORIGINAL wall this sub-segment came from
    // Arc-length fraction range [0,1] along the original wall this edge
    // covers (measured over the flattened polyline) — used to re-sample
    // the real BezPath for that wall between these two fractions when
    // reconstructing the final result.
    frac_a: f64,
    frac_b: f64,
    pts: Vec<Vec2>, // this sub-edge's own flattened points, a-to-b order
    a: usize,
    b: usize,
}

// Arc-length cumulative-distance table for a polyline: cum[i] = distance
// from points[0] to points[i]; cum[0] = 0, cum.last() = total length.
fn arc_lengths(pts: &[[f64; 2]]) -> (Vec<f64>, f64) {
    let mut cum = vec![0.0; pts.len()];
    for i in 1..pts.len() {
        let a = Vec2 { x: pts[i - 1][0], y: pts[i - 1][1] };
        let b = Vec2 { x: pts[i][0], y: pts[i][1] };
        cum[i] = cum[i - 1] + a.dist(b);
    }
    let total = *cum.last().unwrap_or(&0.0);
    (cum, total)
}

// Point + arc-length fraction at a given target distance along the polyline.
fn point_at_distance(pts: &[[f64; 2]], cum: &[f64], total: f64, dist: f64) -> Vec2 {
    if total < 1e-9 {
        return Vec2 { x: pts[0][0], y: pts[0][1] };
    }
    let d = dist.max(0.0).min(total);
    for i in 1..cum.len() {
        if d <= cum[i] || i == cum.len() - 1 {
            let seg_len = (cum[i] - cum[i - 1]).max(1e-9);
            let t = ((d - cum[i - 1]) / seg_len).max(0.0).min(1.0);
            let a = Vec2 { x: pts[i - 1][0], y: pts[i - 1][1] };
            let b = Vec2 { x: pts[i][0], y: pts[i][1] };
            return Vec2 { x: a.x + (b.x - a.x) * t, y: a.y + (b.y - a.y) * t };
        }
    }
    Vec2 { x: pts[pts.len() - 1][0], y: pts[pts.len() - 1][1] }
}

// Exact-segment intersection between two 2D line segments, returning the
// intersection point plus the fractional position along EACH segment
// (0..1), or None if parallel or the crossing falls outside either segment.
fn segseg_intersect(p1: Vec2, p2: Vec2, p3: Vec2, p4: Vec2) -> Option<(Vec2, f64, f64)> {
    let d1 = p2.sub(p1);
    let d2 = p4.sub(p3);
    let denom = d1.x * d2.y - d1.y * d2.x;
    if denom.abs() < 1e-12 {
        return None;
    }
    let dx = p3.sub(p1);
    let t = (dx.x * d2.y - dx.y * d2.x) / denom;
    let u = (dx.x * d1.y - dx.y * d1.x) / denom;
    let eps = 1e-9;
    if t < -eps || t > 1.0 + eps || u < -eps || u > 1.0 + eps {
        return None;
    }
    Some((Vec2 { x: p1.x + d1.x * t, y: p1.y + d1.y * t }, t.max(0.0).min(1.0), u.max(0.0).min(1.0)))
}

// Finds every point where two DIFFERENT open walls actually cross each
// other (not just come near each other) — this is what lets the fill
// engine trace the small lens/petal shape bounded by two crossing strokes
// (e.g. an "X" of two curves), which the endpoint-only gap-bridging below
// can never reach since the crossing point isn't either wall's endpoint.
// Self-intersections within a single wall are intentionally out of scope
// for now — a real but separate case, not what was reported.
fn find_crossings(open_pts: &[Vec<[f64; 2]>]) -> Vec<(usize, f64, Vec2)> {
    let mut hits: Vec<(usize, f64, Vec2)> = Vec::new();
    let tables: Vec<(Vec<f64>, f64)> = open_pts.iter().map(|p| arc_lengths(p)).collect();
    for i in 0..open_pts.len() {
        for j in (i + 1)..open_pts.len() {
            let pi = &open_pts[i];
            let pj = &open_pts[j];
            let (cum_i, total_i) = &tables[i];
            let (cum_j, total_j) = &tables[j];
            if *total_i < 1e-9 || *total_j < 1e-9 {
                continue;
            }
            for ki in 0..pi.len() - 1 {
                let a1 = Vec2 { x: pi[ki][0], y: pi[ki][1] };
                let a2 = Vec2 { x: pi[ki + 1][0], y: pi[ki + 1][1] };
                for kj in 0..pj.len() - 1 {
                    let b1 = Vec2 { x: pj[kj][0], y: pj[kj][1] };
                    let b2 = Vec2 { x: pj[kj + 1][0], y: pj[kj + 1][1] };
                    if let Some((pt, t, u)) = segseg_intersect(a1, a2, b1, b2) {
                        let di = cum_i[ki] + t * (cum_i[ki + 1] - cum_i[ki]);
                        let dj = cum_j[kj] + u * (cum_j[kj + 1] - cum_j[kj]);
                        hits.push((i, di / total_i, pt));
                        hits.push((j, dj / total_j, pt));
                    }
                }
            }
        }
    }
    hits
}

fn uf_find(parent: &mut Vec<usize>, x: usize) -> usize {
    if parent[x] != x {
        let root = uf_find(parent, parent[x]);
        parent[x] = root;
    }
    parent[x]
}
fn uf_union(parent: &mut Vec<usize>, a: usize, b: usize) {
    let ra = uf_find(parent, a);
    let rb = uf_find(parent, b);
    if ra != rb {
        parent[ra] = rb;
    }
}

fn build_graph(
    open_pts: &[Vec<[f64; 2]>],
    gap_thr: f64,
    external_crossings: Option<&[CrossingIn]>,
) -> (Vec<Node>, Vec<Edge>) {
    let join_eps = (1.5_f64).max(gap_thr * 0.15).max(1.5);

    let tables: Vec<(Vec<f64>, f64)> = open_pts.iter().map(|p| arc_lengths(p)).collect();
    // Prefer exact, JS-computed crossings (real bezier-curve intersections)
    // over our own polyline-approximated find_crossings() — see CrossingIn's
    // own doc comment for why this matters (eliminates a small overshoot/
    // flap artifact at intersections). Falls back to find_crossings() if the
    // caller didn't provide any.
    let crossings: Vec<(usize, f64, Vec2)> = match external_crossings {
        Some(cs) => cs.iter().map(|c| (c.wall, c.frac, Vec2 { x: c.pt[0], y: c.pt[1] })).collect(),
        None => find_crossings(open_pts),
    };

    // Per-wall sorted cut-fraction list: always includes 0.0/1.0 (the real
    // endpoints), plus every crossing found on that wall — this is what
    // turns "one edge per whole wall" into "one edge per sub-segment
    // between consecutive cuts", so the wall-follower can route through a
    // crossing exactly like it already routes through a gap-bridge node.
    let mut wall_cuts: Vec<Vec<(f64, Option<Vec2>)>> = Vec::with_capacity(open_pts.len());
    for i in 0..open_pts.len() {
        let mut cuts: Vec<(f64, Option<Vec2>)> = vec![(0.0, None), (1.0, None)];
        for (wi, frac, pt) in &crossings {
            if *wi == i {
                cuts.push((*frac, Some(*pt)));
            }
        }
        // total_cmp, not partial_cmp().unwrap(): a NaN frac (degenerate wall
        // geometry) would panic the whole wasm call — and the JS fallback
        // would mask that as a silent slow path (CLAUDE.md §3), never as a
        // visible bug.
        cuts.sort_by(|a, b| a.0.total_cmp(&b.0));
        cuts.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-6);
        wall_cuts.push(cuts);
    }

    // Resolve every cut to its actual point, and flatten into one list
    // (with a back-reference to which wall/cut it came from) — the input
    // to the clustering pass below.
    let mut all_points: Vec<Vec2> = Vec::new();
    let mut wall_point_start: Vec<usize> = Vec::with_capacity(open_pts.len());
    for (wi, cuts) in wall_cuts.iter().enumerate() {
        wall_point_start.push(all_points.len());
        let (cum, total) = &tables[wi];
        let pts = &open_pts[wi];
        for (f, popt) in cuts {
            all_points.push(popt.unwrap_or_else(|| point_at_distance(pts, cum, *total, f * total)));
        }
    }

    // Cluster all cut points by mutual proximity (union-find over every
    // pair within join_eps) rather than the old greedy "match the first
    // existing node found within eps" scan, which processed points in
    // discovery order and could transitively chain-merge many genuinely
    // DISTINCT crossing points into one corrupted node — e.g. up to 15
    // pairwise crossings when 6+ curves converge near one hub, each
    // individually within eps of *some* earlier point but collectively
    // spanning well past it. A merged hub node silently deletes the small
    // lens/petal faces between the real crossings from the graph, breaking
    // the wall-follower's ability to find the correct local face there —
    // it falls through to the outer silhouette instead (reported:
    // "il me donne ça" — a fan/star arrangement's small regions grabbing
    // unrelated other regions). True clustering (connected components,
    // one Node per component at its centroid) can't snowball past eps.
    let m = all_points.len();
    let mut parent: Vec<usize> = (0..m).collect();
    for i in 0..m {
        for j in (i + 1)..m {
            if all_points[i].dist(all_points[j]) <= join_eps {
                uf_union(&mut parent, i, j);
            }
        }
    }
    let mut cluster_of: Vec<usize> = vec![0; m];
    let mut cluster_node: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    let mut cluster_sum: std::collections::HashMap<usize, (Vec2, usize)> = std::collections::HashMap::new();
    for i in 0..m {
        let root = uf_find(&mut parent, i);
        let entry = cluster_sum.entry(root).or_insert((Vec2 { x: 0.0, y: 0.0 }, 0));
        entry.0.x += all_points[i].x;
        entry.0.y += all_points[i].y;
        entry.1 += 1;
    }
    let mut nodes: Vec<Node> = Vec::new();
    for i in 0..m {
        let root = uf_find(&mut parent, i);
        let node_idx = *cluster_node.entry(root).or_insert_with(|| {
            let (sum, count) = cluster_sum[&root];
            nodes.push(Node { pt: Vec2 { x: sum.x / count as f64, y: sum.y / count as f64 }, edges: Vec::new() });
            nodes.len() - 1
        });
        cluster_of[i] = node_idx;
    }

    // Build stroke sub-edges using the resolved cluster node ids.
    let mut edges: Vec<Edge> = Vec::new();
    for (wi, cuts) in wall_cuts.iter().enumerate() {
        let (cum, total) = &tables[wi];
        let pts = &open_pts[wi];
        let base = wall_point_start[wi];
        for w in 0..cuts.len() - 1 {
            let (f0, _) = cuts[w];
            let (f1, _) = cuts[w + 1];
            if (f1 - f0) * total < 1e-6 {
                continue; // degenerate zero-length sub-edge (crossing exactly at an existing cut)
            }
            let idx0 = base + w;
            let idx1 = base + w + 1;
            let a = cluster_of[idx0];
            let b = cluster_of[idx1];
            if a == b {
                continue; // both ends collapsed into the same cluster — no real edge
            }
            let p0 = all_points[idx0];
            let p1 = all_points[idx1];

            // Rebuild this sub-edge's own point list: the original points
            // strictly between f0/f1, plus exact interpolated endpoints.
            let d0 = f0 * total;
            let d1 = f1 * total;
            let mut edge_pts = vec![p0];
            for (k, cd) in cum.iter().enumerate() {
                if *cd > d0 + 1e-6 && *cd < d1 - 1e-6 {
                    edge_pts.push(Vec2 { x: pts[k][0], y: pts[k][1] });
                }
            }
            edge_pts.push(p1);

            edges.push(Edge { kind: EdgeType::Stroke, stroke_idx: wi, frac_a: f0, frac_b: f1, pts: edge_pts, a, b });
        }
    }

    let n = nodes.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let d = nodes[i].pt.dist(nodes[j].pt);
            if d > join_eps && d <= gap_thr {
                edges.push(Edge { kind: EdgeType::Gap, stroke_idx: 0, frac_a: 0.0, frac_b: 0.0, pts: Vec::new(), a: i, b: j });
            }
        }
    }
    for (idx, e) in edges.iter().enumerate() {
        nodes[e.a].edges.push(idx);
        nodes[e.b].edges.push(idx);
    }
    (nodes, edges)
}

// Direction of travel leaving `node` via `edge`, using the endpoint-adjacent
// polyline segment as a stand-in for the true curve tangent there (accurate
// in the limit as the flatten tolerance shrinks — same tradeoff already
// accepted for boolean.rs).
fn edge_dir(nodes: &[Node], edge: &Edge, node: usize) -> Vec2 {
    if edge.kind == EdgeType::Gap {
        let other = if edge.a == node { edge.b } else { edge.a };
        return nodes[other].pt.sub(nodes[node].pt).normalize();
    }
    let pts = &edge.pts;
    if edge.a == node {
        pts[1.min(pts.len() - 1)].sub(pts[0]).normalize()
    } else {
        let n = pts.len();
        pts[n - 2].sub(pts[n - 1]).normalize()
    }
}

fn poly_area(pts: &[Vec2]) -> f64 {
    let mut a = 0.0;
    let n = pts.len();
    for i in 0..n {
        let p1 = pts[i];
        let p2 = pts[(i + 1) % n];
        a += p1.x * p2.y - p2.x * p1.y;
    }
    (a / 2.0).abs()
}

fn point_in_poly(pt: Vec2, pts: &[Vec2]) -> bool {
    let mut inside = false;
    let n = pts.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = (pts[i].x, pts[i].y);
        let (xj, yj) = (pts[j].x, pts[j].y);
        if (yi > pt.y) != (yj > pt.y) && pt.x < (xj - xi) * (pt.y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

// trace_loop's turn-picking (sharpest relative angle at each node) can walk
// onto a different, non-adjacent face's edge when several curves cross
// near-tangentially at the same node — the loop still closes, but as a
// self-crossing "bow-tie" polygon spanning disconnected regions, whose
// shoelace area can come out smaller than the true tight region's and win
// the caller's area-only comparison (JS side: tools.js's fillPolySelfIntersects,
// kept in sync with this — see that comment for the full mechanism). A
// genuine planar face never self-intersects, so reject any candidate that
// does before it's ever compared by area.
fn poly_self_intersects(pts: &[Vec2]) -> bool {
    let n = pts.len();
    if n < 4 {
        return false;
    }
    fn ccw(a: Vec2, b: Vec2, c: Vec2) -> f64 {
        (c.y - a.y) * (b.x - a.x) - (b.y - a.y) * (c.x - a.x)
    }
    fn segs_cross(p1: Vec2, p2: Vec2, p3: Vec2, p4: Vec2) -> bool {
        let d1 = ccw(p3, p4, p1);
        let d2 = ccw(p3, p4, p2);
        let d3 = ccw(p1, p2, p3);
        let d4 = ccw(p1, p2, p4);
        ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
            && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    }
    for i in 0..n {
        let (a1, a2) = (pts[i], pts[(i + 1) % n]);
        for j in (i + 1)..n {
            // Skip edge j when it shares a vertex with edge i — either the
            // NEXT edge (j == i+1, sharing a2/pts[i+1]) or, via wraparound,
            // the PREVIOUS edge ((j+1)%n == i, sharing a1/pts[i]). Missing
            // the j == i+1 case (mirrors a bug just fixed on the JS side,
            // tools.js's fillPolySelfIntersects) let every pair of adjacent
            // edges get segment-crossing-tested against each other, and
            // that test is exactly where floating-point noise flips sign
            // right at a shared endpoint — with hundreds of closely-spaced
            // points from dense curve sampling, this false-flagged nearly
            // every legitimate simple loop as self-intersecting, silently
            // discarding every fill candidate.
            if j == i || j == i + 1 || (j + 1) % n == i {
                continue;
            }
            if segs_cross(a1, a2, pts[j], pts[(j + 1) % n]) {
                return true;
            }
        }
    }
    false
}

struct Hop {
    edge_idx: usize,
    from: usize,
    to: usize,
}

fn trace_loop(
    nodes: &[Node],
    edges: &[Edge],
    start_node: usize,
    first_edge: usize,
    turn_sign: f64,
    max_steps: usize,
) -> Option<Vec<Hop>> {
    let mut seq: Vec<Hop> = Vec::new();
    let mut cur_node = start_node;
    let mut cur_edge = first_edge;
    let mut to_node = if edges[cur_edge].a == cur_node { edges[cur_edge].b } else { edges[cur_edge].a };
    seq.push(Hop { edge_idx: cur_edge, from: cur_node, to: to_node });
    let mut steps = 0;
    while to_node != start_node {
        steps += 1;
        if steps > max_steps {
            return None;
        }
        let arrival = edge_dir(nodes, &edges[cur_edge], cur_node);
        let arrival_dir = Vec2 { x: -arrival.x, y: -arrival.y };
        let back_angle = arrival_dir.y.atan2(arrival_dir.x);
        let node = &nodes[to_node];
        let mut best_edge: Option<usize> = None;
        let mut best_rel = f64::INFINITY;
        for &e2i in &node.edges {
            if e2i == cur_edge {
                continue;
            }
            let out_dir = edge_dir(nodes, &edges[e2i], to_node);
            let ang = out_dir.y.atan2(out_dir.x);
            let mut rel = if turn_sign > 0.0 { ang - back_angle } else { back_angle - ang };
            let two_pi = std::f64::consts::PI * 2.0;
            rel = ((rel % two_pi) + two_pi) % two_pi;
            if rel < 1e-6 {
                rel = two_pi;
            }
            if rel < best_rel {
                best_rel = rel;
                best_edge = Some(e2i);
            }
        }
        let best_edge = best_edge?;
        let next_node = if edges[best_edge].a == to_node { edges[best_edge].b } else { edges[best_edge].a };
        seq.push(Hop { edge_idx: best_edge, from: to_node, to: next_node });
        cur_node = to_node;
        cur_edge = best_edge;
        to_node = next_node;
    }
    Some(seq)
}

fn loop_points(nodes: &[Node], edges: &[Edge], seq: &[Hop]) -> Vec<Vec2> {
    let mut pts = Vec::new();
    for hop in seq {
        let e = &edges[hop.edge_idx];
        if e.kind == EdgeType::Gap {
            pts.push(nodes[hop.to].pt);
        } else if e.a == hop.from {
            for p in &e.pts {
                pts.push(*p);
            }
        } else {
            for p in e.pts.iter().rev() {
                pts.push(*p);
            }
        }
    }
    pts
}

// Rebuilds the winning loop as an exact, curve-accurate point sequence by
// sampling each Stroke hop's REAL bezier wall (via PathSampler, not the
// coarser flattened polyline used for graph-tracing) between its frac_a/
// frac_b arc-length range, and each Gap hop as a straight line to the next
// node. This is what used to require handing frac_a/frac_b back to JS for
// Path#getLocationAt/splitAt — now done once, here, with kurbo.
fn resample_result(open_samplers: &[PathSampler], nodes: &[Node], edges: &[Edge], seq: &[Hop]) -> Vec<Vec2> {
    let mut pts: Vec<Vec2> = Vec::new();
    for hop in seq {
        let e = &edges[hop.edge_idx];
        if e.kind == EdgeType::Gap {
            pts.push(nodes[hop.to].pt);
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
            pts.push(sampler.point_at(d));
        }
    }
    pts
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SegOut {
    point: [f64; 2],
    handle_in: [f64; 2],
    handle_out: [f64; 2],
}

#[derive(Serialize)]
#[serde(tag = "kind")]
enum FillResult {
    #[serde(rename = "closedWall")]
    ClosedWall { index: usize },
    #[serde(rename = "traced")]
    // `walls`: unique input open-wall indices whose sub-edges make up the
    // winning loop — lets JS record exactly WHICH source strokes bound this
    // fill (data.fillWalls), so later regeneration can be restricted to
    // those same strokes instead of re-deriving the region from a bare
    // seed point against the whole layer.
    // `usedGap`: whether the winning loop needed any Gap (bridged, not a
    // real stroke) edge to close — a loop that closes on real strokes/
    // crossings ALONE is provably the tightest possible closure through the
    // click point, since it needs no bridging at all; the caller uses this
    // to stop escalating gapThr the moment such a "pure" candidate is
    // found, rather than continuing to try larger gapThr values whose extra
    // (spurious) gap edges can occasionally trace a smaller-by-area but
    // topologically wrong shortcut loop that cuts across the real shape.
    Traced {
        segments: Vec<SegOut>,
        walls: Vec<usize>,
        #[serde(rename = "usedGap")]
        used_gap: bool,
    },
    #[serde(rename = "notFound")]
    NotFound,
}

#[wasm_bindgen]
pub fn fill_find(input_json: &str) -> Result<String, JsValue> {
    let input: FillInput =
        serde_json::from_str(input_json).map_err(|e| JsValue::from_str(&e.to_string()))?;
    let click = Vec2 { x: input.click[0], y: input.click[1] };

    // Build real BezPaths + flattened polylines once, up front, for every
    // wall — the polylines feed the existing graph/area math unchanged, the
    // BezPaths (open walls only) feed the curve-accurate resampling of
    // whichever loop wins below.
    let closed_geo: Vec<(BezPath, Vec<[f64; 2]>)> =
        input.closed_walls.iter().map(|w| wall_geometry(w, true)).collect();
    let open_geo: Vec<(BezPath, Vec<[f64; 2]>)> =
        input.open_walls.iter().map(|w| wall_geometry(w, false)).collect();
    let open_pts: Vec<Vec<[f64; 2]>> = open_geo.iter().map(|(_, p)| p.clone()).collect();

    // Candidate winner, tracked as (area, kind) so the "smallest containing
    // loop wins" comparison is a single flat rule across BOTH closed walls
    // and traced open-wall loops — the traced Hop sequence is only resampled
    // into real segments once, after the winner is fully decided, since
    // that's the expensive step.
    enum Best {
        Closed(usize),
        Traced(Vec<Hop>),
    }
    let mut best: Option<(f64, Best)> = None;

    // closed standalone walls: same "smallest containing loop wins" rule as
    // the traced candidates below, so collect both kinds before comparing.
    for (i, (_, pts)) in closed_geo.iter().enumerate() {
        let vpts: Vec<Vec2> = pts.iter().map(|p| Vec2 { x: p[0], y: p[1] }).collect();
        let area = poly_area(&vpts);
        // A hand-drawn "closed" wall (Paper.js path.closed===true, or a
        // near-matching start/end the caller already snapped shut) is not
        // guaranteed to be a simple ring — an organic stroke that loops back
        // near its own earlier run can self-cross exactly like a traced
        // loop can (see poly_self_intersects's own comment). This candidate
        // used to accept purely on area/containment with no such guard,
        // which is exactly what won the reported "star-burst"/diamond-
        // artifact fill (fb_mrgedu37) even after the Traced branch below
        // got its own self-intersection guard — this wall came in already
        // closed, never went through trace_loop at all.
        if area >= 1.0 && point_in_poly(click, &vpts) && !poly_self_intersects(&vpts) {
            if best.as_ref().map_or(true, |(a, _)| area < *a) {
                best = Some((area, Best::Closed(i)));
            }
        }
    }

    let mut nodes_edges: Option<(Vec<Node>, Vec<Edge>)> = None;
    if !open_pts.is_empty() {
        let (nodes, edges) = build_graph(&open_pts, input.gap_thr, input.crossings.as_deref());
        let max_steps = edges.len() * 2 + 8;
        // Exhaustive planar-face enumeration, not a capped sample of
        // stroke-only seeds. trace_loop's "always take the sharpest turn"
        // rule is the textbook-correct method for walking the boundary of
        // the SPECIFIC face touching a given directed half-edge — every
        // directed half-edge belongs to exactly one face under this rule.
        // The old code only tried up to 150 STROKE edges as seeds (missing
        // every edge past that cap on a busy scene, and never trying Gap
        // edges as seeds at all) — on a complex multi-overlap arrangement
        // that silently left real faces completely undiscovered, not
        // rejected by the filters below, just never traced (mirrors the
        // JS-side fix, tools.js's fillVectorFindJS, kept in sync with this
        // for the same reason). Walking every edge from both endpoints in
        // both turn directions, skipping a (edge,node,turn_sign) combo
        // once its face has already been traced from elsewhere on its own
        // boundary, visits every face exactly once — no cap, no missed
        // faces, and cheaper than the old approach on a busy scene since
        // it stops re-tracing the same face redundantly from every one of
        // its boundary edges.
        let mut visited: std::collections::HashSet<(usize, usize, i8)> = std::collections::HashSet::new();
        for seed_idx in 0..edges.len() {
            let seed_edge = &edges[seed_idx];
            for &start_node in &[seed_edge.a, seed_edge.b] {
                for &turn_sign in &[1.0_f64, -1.0] {
                    let ts_key: i8 = if turn_sign > 0.0 { 1 } else { -1 };
                    let key = (seed_idx, start_node, ts_key);
                    if visited.contains(&key) {
                        continue;
                    }
                    visited.insert(key);
                    if let Some(seq) = trace_loop(&nodes, &edges, start_node, seed_idx, turn_sign, max_steps) {
                        if seq.len() < 2 {
                            continue;
                        }
                        for hop in &seq {
                            visited.insert((hop.edge_idx, hop.from, ts_key));
                        }
                        let pts = loop_points(&nodes, &edges, &seq);
                        let area = poly_area(&pts);
                        if area < 1.0 || !point_in_poly(click, &pts) || poly_self_intersects(&pts) {
                            continue;
                        }
                        if best.as_ref().map_or(true, |(a, _)| area < *a) {
                            best = Some((area, Best::Traced(seq)));
                        }
                    }
                }
            }
        }
        nodes_edges = Some((nodes, edges));
    }

    let result = match best {
        Some((_, Best::Closed(i))) => FillResult::ClosedWall { index: i },
        Some((_, Best::Traced(seq))) => {
            let (nodes, edges) = nodes_edges.as_ref().unwrap();
            let mut walls: Vec<usize> = seq
                .iter()
                .filter(|h| edges[h.edge_idx].kind == EdgeType::Stroke)
                .map(|h| edges[h.edge_idx].stroke_idx)
                .collect();
            walls.sort_unstable();
            walls.dedup();
            let used_gap = seq.iter().any(|h| edges[h.edge_idx].kind == EdgeType::Gap);
            let samplers: Vec<PathSampler> = open_geo.iter().map(|(bez, _)| PathSampler::new(bez)).collect();
            let raw_pts = resample_result(&samplers, nodes, edges, &seq);
            // The candidate that won `best` was only ever checked against
            // loop_points()'s COARSE flattened polyline (0.75px tolerance,
            // built purely for cheap topology/area comparison across many
            // candidates) — resample_result rebuilds the same loop from the
            // REAL bezier curves at full precision, and a subtle
            // self-crossing invisible at 0.75px tolerance can become
            // visible here. Confirmed live against a real hand-drawn
            // character (fb_mrgedu37): the coarse check passed, but this
            // exact accurate reconstruction was self-intersecting — the
            // "star-burst"/diamond artifact reported. Re-check on the
            // accurate points before committing to this result; reject
            // (NotFound) rather than return a wrong fill — the caller
            // (fillVectorFind, tools.js) escalates to a larger gapThr and
            // tries again rather than accepting nothing.
            if poly_self_intersects(&raw_pts) {
                FillResult::NotFound
            } else {
                // 0.35px tolerance: well under a single rendered pixel at any
                // normal zoom, so this is a point-count optimization only —
                // visually lossless, not a visible simplification.
                let simplified = simplify_polyline(&raw_pts, 0.35);
                let segments = simplified
                    .iter()
                    .map(|p| SegOut { point: [p.x, p.y], handle_in: [0.0, 0.0], handle_out: [0.0, 0.0] })
                    .collect();
                FillResult::Traced { segments, walls, used_gap }
            }
        }
        None => FillResult::NotFound,
    };
    serde_json::to_string(&result).map_err(|e| JsValue::from_str(&e.to_string()))
}
