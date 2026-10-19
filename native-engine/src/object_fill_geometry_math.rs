//! Native retained fill geometry, adapted from geometry-wasm/src/fill.rs at protected52425370.
//! Pure geometry only: no document state, Paper, WASM transport or renderer.
use super::*;
#[derive(Clone, Copy, PartialEq)]
pub(super) struct Vec2 {
    pub(super) x: f64,
    pub(super) y: f64,
}
impl Vec2 {
    pub(super) fn sub(self, o: Vec2) -> Vec2 {
        Vec2 {
            x: self.x - o.x,
            y: self.y - o.y,
        }
    }
    pub(super) fn dist(self, o: Vec2) -> f64 {
        ((self.x - o.x).powi(2) + (self.y - o.y).powi(2)).sqrt()
    }
    pub(super) fn normalize(self) -> Vec2 {
        let l = (self.x * self.x + self.y * self.y).sqrt();
        if l < 1e-9 {
            Vec2 { x: 1.0, y: 0.0 }
        } else {
            Vec2 {
                x: self.x / l,
                y: self.y / l,
            }
        }
    }
}

pub(super) fn arc_lengths(pts: &[[f64; 2]]) -> (Vec<f64>, f64) {
    let mut cum = vec![0.0; pts.len()];
    for i in 1..pts.len() {
        let a = Vec2 {
            x: pts[i - 1][0],
            y: pts[i - 1][1],
        };
        let b = Vec2 {
            x: pts[i][0],
            y: pts[i][1],
        };
        cum[i] = cum[i - 1] + a.dist(b);
    }
    let total = *cum.last().unwrap_or(&0.0);
    (cum, total)
}

// Point + arc-length fraction at a given target distance along the polyline.
pub(super) fn point_at_distance(pts: &[[f64; 2]], cum: &[f64], total: f64, dist: f64) -> Vec2 {
    if total < 1e-9 {
        return Vec2 {
            x: pts[0][0],
            y: pts[0][1],
        };
    }
    let d = dist.max(0.0).min(total);
    for i in 1..cum.len() {
        if d <= cum[i] || i == cum.len() - 1 {
            let seg_len = (cum[i] - cum[i - 1]).max(1e-9);
            let t = ((d - cum[i - 1]) / seg_len).max(0.0).min(1.0);
            let a = Vec2 {
                x: pts[i - 1][0],
                y: pts[i - 1][1],
            };
            let b = Vec2 {
                x: pts[i][0],
                y: pts[i][1],
            };
            return Vec2 {
                x: a.x + (b.x - a.x) * t,
                y: a.y + (b.y - a.y) * t,
            };
        }
    }
    Vec2 {
        x: pts[pts.len() - 1][0],
        y: pts[pts.len() - 1][1],
    }
}

// Exact-segment intersection between two 2D line segments, returning the
// intersection point plus the fractional position along EACH segment
// (0..1), or None if parallel or the crossing falls outside either segment.
pub(super) fn segseg_intersect(p1: Vec2, p2: Vec2, p3: Vec2, p4: Vec2) -> Option<(Vec2, f64, f64)> {
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
    Some((
        Vec2 {
            x: p1.x + d1.x * t,
            y: p1.y + d1.y * t,
        },
        t.max(0.0).min(1.0),
        u.max(0.0).min(1.0),
    ))
}

// Finds every point where two DIFFERENT open walls actually cross each
// other (not just come near each other) — this is what lets the fill
// engine trace the small lens/petal shape bounded by two crossing strokes
// (e.g. an "X" of two curves), which the endpoint-only gap-bridging below
// can never reach since the crossing point isn't either wall's endpoint.
// Self-intersections within a single wall are intentionally out of scope
// for now — a real but separate case, not what was reported.
pub(super) fn find_crossings(open_pts: &[Vec<[f64; 2]>]) -> Vec<(usize, f64, Vec2)> {
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
                let a1 = Vec2 {
                    x: pi[ki][0],
                    y: pi[ki][1],
                };
                let a2 = Vec2 {
                    x: pi[ki + 1][0],
                    y: pi[ki + 1][1],
                };
                for kj in 0..pj.len() - 1 {
                    let b1 = Vec2 {
                        x: pj[kj][0],
                        y: pj[kj][1],
                    };
                    let b2 = Vec2 {
                        x: pj[kj + 1][0],
                        y: pj[kj + 1][1],
                    };
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

pub(super) fn uf_find(parent: &mut Vec<usize>, x: usize) -> usize {
    if parent[x] != x {
        let root = uf_find(parent, parent[x]);
        parent[x] = root;
    }
    parent[x]
}
pub(super) fn uf_union(parent: &mut Vec<usize>, a: usize, b: usize) {
    let ra = uf_find(parent, a);
    let rb = uf_find(parent, b);
    if ra != rb {
        parent[ra] = rb;
    }
}
