//! Native retained fill geometry, adapted from geometry-wasm/src/fill.rs at protected52425370.
//! Pure geometry only: no document state, Paper, WASM transport or renderer.
use super::*;
pub(super) fn build_graph(
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
        Some(cs) => cs
            .iter()
            .map(|c| {
                (
                    c.wall,
                    c.frac,
                    Vec2 {
                        x: c.pt[0],
                        y: c.pt[1],
                    },
                )
            })
            .collect(),
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
    let mut cluster_node: std::collections::HashMap<usize, usize> =
        std::collections::HashMap::new();
    let mut cluster_sum: std::collections::HashMap<usize, (Vec2, usize)> =
        std::collections::HashMap::new();
    for i in 0..m {
        let root = uf_find(&mut parent, i);
        let entry = cluster_sum
            .entry(root)
            .or_insert((Vec2 { x: 0.0, y: 0.0 }, 0));
        entry.0.x += all_points[i].x;
        entry.0.y += all_points[i].y;
        entry.1 += 1;
    }
    let mut nodes: Vec<Node> = Vec::new();
    for i in 0..m {
        let root = uf_find(&mut parent, i);
        let node_idx = *cluster_node.entry(root).or_insert_with(|| {
            let (sum, count) = cluster_sum[&root];
            nodes.push(Node {
                pt: Vec2 {
                    x: sum.x / count as f64,
                    y: sum.y / count as f64,
                },
                edges: Vec::new(),
            });
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
                    edge_pts.push(Vec2 {
                        x: pts[k][0],
                        y: pts[k][1],
                    });
                }
            }
            edge_pts.push(p1);

            // Coincident-duplicate guard (2026-07, kept in sync with the JS
            // fillBuildGraph — see its comment): duplicated wall geometry is
            // STRUCTURAL in this app (a paint-bucket fill's outline retraces
            // the stroke centerlines it was traced against, so adjacent
            // fills + the original stroke put 2-3 identical copies of the
            // same arc into the wall set). Two same-geometry edges between
            // the same node pair form a degenerate two-edge "lens" the
            // sharpest-turn walk orbits forever. Midpoint comparison keeps
            // genuine two-arc lenses (different arcs between the same
            // nodes — the diamond case) while dropping exact copies.
            let mid_new = edge_pts[edge_pts.len() / 2];
            let dup = edges.iter().any(|pe| {
                if pe.kind != EdgeType::Stroke {
                    return false;
                }
                if !((pe.a == a && pe.b == b) || (pe.a == b && pe.b == a)) {
                    return false;
                }
                let mid_old = pe.pts[pe.pts.len() / 2];
                mid_old.dist(mid_new) <= join_eps.max(1.5)
            });
            if dup {
                continue;
            }
            edges.push(Edge {
                kind: EdgeType::Stroke,
                stroke_idx: wi,
                frac_a: f0,
                frac_b: f1,
                pts: edge_pts,
                a,
                b,
            });
        }
    }

    let n = nodes.len();
    for i in 0..n {
        for j in (i + 1)..n {
            let d = nodes[i].pt.dist(nodes[j].pt);
            if d > join_eps && d <= gap_thr {
                edges.push(Edge {
                    kind: EdgeType::Gap,
                    stroke_idx: 0,
                    frac_a: 0.0,
                    frac_b: 0.0,
                    pts: Vec::new(),
                    a: i,
                    b: j,
                });
            }
        }
    }
    // Spur pruning (2026-07, Umoupen-parity — kept in sync with the JS
    // fillBuildGraph, see its comment for the full story): iteratively drop
    // every edge with a degree-1 endpoint. A dangling stroke tail — the
    // natural overshoot past the last crossing in any hand-drawn
    // intersection — can never bound a face, but trace_loop walks INTO it
    // and dies at the tip (best_edge? -> None), killing the whole face
    // trace. Confirmed live: on a dense real drawing no interior face was
    // ever traced; the bucket silently fell back to the whole surrounding
    // closed wall. Umoupen's triangulation sidesteps this by construction;
    // pruning is the graph-world equivalent.
    loop {
        let mut degree = vec![0usize; nodes.len()];
        for e in &edges {
            degree[e.a] += 1;
            degree[e.b] += 1;
        }
        let before = edges.len();
        edges.retain(|e| degree[e.a] >= 2 && degree[e.b] >= 2);
        if edges.len() == before {
            break;
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
