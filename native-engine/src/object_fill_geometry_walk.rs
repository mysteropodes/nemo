//! Retained planar-face walk and topology checks from geometry-wasm/src/fill.rs at52425370.
use crate::object_document::object_group_kernels::object_fill_trace_ops::geometry::*;
pub(super) fn edge_dir(nodes: &[Node], edge: &Edge, node: usize) -> Vec2 {
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

pub(super) fn poly_area(pts: &[Vec2]) -> f64 {
    let mut a = 0.0;
    let n = pts.len();
    for i in 0..n {
        let p1 = pts[i];
        let p2 = pts[(i + 1) % n];
        a += p1.x * p2.y - p2.x * p1.y;
    }
    (a / 2.0).abs()
}

pub(super) fn point_in_poly(pt: Vec2, pts: &[Vec2]) -> bool {
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
pub(super) fn poly_self_intersects(pts: &[Vec2]) -> bool {
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

pub(super) struct Hop {
    pub(super) edge_idx: usize,
    pub(super) from: usize,
    pub(super) to: usize,
}

pub(super) fn trace_loop(
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
    let mut to_node = if edges[cur_edge].a == cur_node {
        edges[cur_edge].b
    } else {
        edges[cur_edge].a
    };
    seq.push(Hop {
        edge_idx: cur_edge,
        from: cur_node,
        to: to_node,
    });
    // Rho-orbit abort (2026-07, kept in sync with fillTraceLoop/tools.js —
    // see its comment): a walk that re-traverses a DIRECTED half-edge has
    // merged into a cycle that never includes start_node (possible when
    // node clustering / near-parallel edges make two arrivals pick the
    // same exit) — abort immediately instead of spinning to max_steps.
    let mut seen_he: std::collections::HashSet<(usize, usize)> = std::collections::HashSet::new();
    seen_he.insert((cur_edge, cur_node));
    let mut steps = 0;
    while to_node != start_node {
        steps += 1;
        if steps > max_steps {
            return None;
        }
        // Reference direction for the sharpest-turn comparison: the tangent
        // AT to_node pointing back into the edge we just arrived on.
        // edge_dir(edge, to_node) already returns exactly that.
        //
        // This used to be computed as -edge_dir(edge, cur_node) — the
        // departure tangent at the FAR end, negated. That is only equivalent
        // for a straight edge. On a curved one (every arc of a circle, every
        // hand-drawn stroke) the two differ by the arc's total turning, so
        // every turn angle at this node was measured against a wrong
        // reference and the "sharpest turn" pick could be flatly wrong.
        // Root cause of feedback #116 ("beaucoup de triangle alors que les
        // zones vecteurs sont clair"), reproduced on the lens between two
        // crossing circles: arriving at a crossing along one circle's arc,
        // the correct next edge scored a relative angle of exactly 0 against
        // the bogus reference and got bumped to 2π by the "ignore
        // zero-turn" guard below — i.e. the true lens boundary was ranked
        // LAST precisely when it should have ranked first, so that face was
        // never traced and the fill fell through to the raster fallback.
        let back = edge_dir(nodes, &edges[cur_edge], to_node);
        let back_angle = back.y.atan2(back.x);
        let node = &nodes[to_node];
        let mut best_edge: Option<usize> = None;
        let mut best_rel = f64::INFINITY;
        for &e2i in &node.edges {
            if e2i == cur_edge {
                continue;
            }
            let out_dir = edge_dir(nodes, &edges[e2i], to_node);
            let ang = out_dir.y.atan2(out_dir.x);
            let mut rel = if turn_sign > 0.0 {
                ang - back_angle
            } else {
                back_angle - ang
            };
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
        let next_node = if edges[best_edge].a == to_node {
            edges[best_edge].b
        } else {
            edges[best_edge].a
        };
        if !seen_he.insert((best_edge, to_node)) {
            return None; // merged into a foreign orbit — see comment above
        }
        seq.push(Hop {
            edge_idx: best_edge,
            from: to_node,
            to: next_node,
        });
        cur_node = to_node;
        cur_edge = best_edge;
        to_node = next_node;
    }
    Some(seq)
}

pub(super) fn loop_points(nodes: &[Node], edges: &[Edge], seq: &[Hop]) -> Vec<Vec2> {
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
