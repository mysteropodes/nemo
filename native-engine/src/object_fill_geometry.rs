//! Native retained fill geometry, adapted from geometry-wasm/src/fill.rs at protected52425370.
//! Pure geometry only: no document state, Paper, WASM transport or renderer.
use super::RingSeg;
use kurbo::BezPath;
#[path = "object_fill_geometry_curve.rs"]
mod curve;
#[path = "object_fill_geometry_graph.rs"]
mod graph;
#[path = "object_fill_geometry_math.rs"]
mod math;
#[path = "object_fill_geometry_walk.rs"]
mod walk;
use curve::*;
use graph::*;
use math::*;
use walk::*;
pub(super) fn segment_crossing(
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
    d: [f64; 2],
) -> Option<([f64; 2], f64, f64)> {
    math::segseg_intersect(
        Vec2 { x: a[0], y: a[1] },
        Vec2 { x: b[0], y: b[1] },
        Vec2 { x: c[0], y: c[1] },
        Vec2 { x: d[0], y: d[1] },
    )
    .map(|(p, t, u)| ([p.x, p.y], t, u))
}
#[derive(Clone)]
pub(super) struct Wall {
    pub(super) segments: Vec<RingSeg>,
}
pub(super) struct CrossingIn {
    pub(super) wall: usize,
    pub(super) frac: f64,
    pub(super) pt: [f64; 2],
}
pub(super) struct FillInput {
    pub(super) open_walls: Vec<Wall>,
    pub(super) closed_walls: Vec<Wall>,
    pub(super) gap_thr: f64,
    pub(super) click: [f64; 2],
    pub(super) crossings: Option<Vec<CrossingIn>>,
}
struct Node {
    pt: Vec2,
    edges: Vec<usize>,
}
#[derive(Clone, Copy, PartialEq, Debug)]
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

pub(super) enum FillResult {
    ClosedWall {
        index: usize,
    },
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
        segments: Vec<RingSeg>,
        walls: Vec<usize>,
        used_gap: bool,
    },
    NotFound,
}

pub(super) fn find(input: FillInput) -> FillResult {
    let click = Vec2 {
        x: input.click[0],
        y: input.click[1],
    };

    // Build real BezPaths + flattened polylines once, up front, for every
    // wall — the polylines feed the existing graph/area math unchanged, the
    // BezPaths (open walls only) feed the curve-accurate resampling of
    // whichever loop wins below.
    let closed_geo: Vec<(BezPath, Vec<[f64; 2]>)> = input
        .closed_walls
        .iter()
        .map(|w| wall_geometry(w, true))
        .collect();
    let open_geo: Vec<(BezPath, Vec<[f64; 2]>)> = input
        .open_walls
        .iter()
        .map(|w| wall_geometry(w, false))
        .collect();
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
        if std::env::var("FILL_DEBUG").is_ok() {
            eprintln!("[fill_debug] nodes={}", nodes.len());
            for (i, n) in nodes.iter().enumerate() {
                eprintln!(
                    "  node[{}] = ({:.1},{:.1}) edges={:?}",
                    i, n.pt.x, n.pt.y, n.edges
                );
            }
            eprintln!("[fill_debug] edges={}", edges.len());
            for (i, e) in edges.iter().enumerate() {
                let mid = e
                    .pts
                    .get(e.pts.len() / 2)
                    .copied()
                    .unwrap_or(Vec2 { x: 0.0, y: 0.0 });
                eprintln!("  edge[{}] a={} b={} kind={:?} wall={} frac=[{:.3},{:.3}] mid=({:.1},{:.1}) npts={}", i, e.a, e.b, e.kind, e.stroke_idx, e.frac_a, e.frac_b, mid.x, mid.y, e.pts.len());
            }
            for (ni, n) in nodes.iter().enumerate() {
                eprintln!("  node[{}] outgoing dirs:", ni);
                for &ei in &n.edges {
                    let d = edge_dir(&nodes, &edges[ei], ni);
                    let ang = d.y.atan2(d.x).to_degrees();
                    eprintln!(
                        "    edge[{}] dir=({:.3},{:.3}) angle={:.2}deg",
                        ei, d.x, d.y, ang
                    );
                }
            }
        }
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
        let mut visited: std::collections::HashSet<(usize, usize, i8)> =
            std::collections::HashSet::new();
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
                    let debug = std::env::var("FILL_DEBUG").is_ok();
                    if let Some(seq) =
                        trace_loop(&nodes, &edges, start_node, seed_idx, turn_sign, max_steps)
                    {
                        if seq.len() < 2 {
                            if debug {
                                eprintln!(
                                    "[fill_debug] seed={} start={} turn={} -> seq too short ({})",
                                    seed_idx,
                                    start_node,
                                    turn_sign,
                                    seq.len()
                                );
                            }
                            continue;
                        }
                        for hop in &seq {
                            visited.insert((hop.edge_idx, hop.from, ts_key));
                        }
                        let pts = loop_points(&nodes, &edges, &seq);
                        let area = poly_area(&pts);
                        let contains = point_in_poly(click, &pts);
                        let selfx = poly_self_intersects(&pts);
                        if debug {
                            let hops: Vec<(usize, usize, usize)> =
                                seq.iter().map(|h| (h.edge_idx, h.from, h.to)).collect();
                            eprintln!("[fill_debug] seed={} start={} turn={} -> hops={:?} area={:.1} contains={} selfx={}", seed_idx, start_node, turn_sign, hops, area, contains, selfx);
                        }
                        if area < 1.0 || !contains || selfx {
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
            let samplers: Vec<PathSampler> = open_geo
                .iter()
                .map(|(bez, _)| PathSampler::new(bez))
                .collect();
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
                    .map(|p| RingSeg {
                        point: [p.x, p.y],
                        handle_in: [0.0, 0.0],
                        handle_out: [0.0, 0.0],
                    })
                    .collect();
                FillResult::Traced {
                    segments,
                    walls,
                    used_gap,
                }
            }
        }
        None => FillResult::NotFound,
    };
    result
}
