//! CPU obstacle-map fallback of tools.js fillVectorFindRaster. The bounded 1400px
//! map, round wall inflation, four-neighbour flood and largest boundary loop stay
//! local to one trace. No renderer readback or second document writer is involved.
use super::{area, contains, polygon, StoredWall};
use crate::object_document::object_group_kernels::object_boolean_ops::{
    simplify_closed, Point, RingSeg,
};
use crate::object_document::object_group_kernels::object_fill_trace_ops::{TraceFailure, TraceHit};
use std::collections::{BTreeMap, BTreeSet};

fn distance(p: Point, a: Point, b: Point) -> f64 {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0] * d[0] + d[1] * d[1];
    let t = if length == 0.0 {
        0.0
    } else {
        ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / length
    }
    .clamp(0.0, 1.0);
    (p[0] - a[0] - t * d[0]).hypot(p[1] - a[1] - t * d[1])
}
fn inside(poly: &[Point], p: Point) -> bool {
    // Existing painted obstacles use Canvas fill()'s default nonzero rule.
    // Repeated same-direction loops stay painted; opposite windings cancel.
    // This predicate does not change the graph walk's face-selection policy.
    let mut winding = 0i32;
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            winding += if b[1] > a[1] { 1 } else { -1 };
        }
    }
    winding != 0
}

type Pixel = (usize, usize);
fn contour(visited: &[bool], w: usize, h: usize) -> Vec<Point> {
    let filled = |x: usize, y: usize| x < w && y < h && visited[y * w + x];
    let mut edges: BTreeMap<Pixel, Vec<Pixel>> = BTreeMap::new();
    let mut add = |a, b| {
        edges.entry(a).or_default().push(b);
        edges.entry(b).or_default().push(a);
    };
    for y in 0..h {
        for x in 0..w {
            if !filled(x, y) {
                continue;
            }
            if x == 0 || !filled(x - 1, y) {
                add((x, y), (x, y + 1));
            }
            if !filled(x + 1, y) {
                add((x + 1, y), (x + 1, y + 1));
            }
            if y == 0 || !filled(x, y - 1) {
                add((x, y), (x + 1, y));
            }
            if !filled(x, y + 1) {
                add((x, y + 1), (x + 1, y + 1));
            }
        }
    }
    let key = |a: Pixel, b: Pixel| if a < b { (a, b) } else { (b, a) };
    let mut used = BTreeSet::new();
    let mut best = Vec::new();
    let mut best_area = 0.0;
    for (&start, neighbors) in &edges {
        for &first in neighbors {
            if used.contains(&key(start, first)) {
                continue;
            }
            let mut points = vec![[start.0 as f64, start.1 as f64]];
            let mut previous = start;
            let mut current = first;
            used.insert(key(start, first));
            for _ in 0..=edges.len() * 2 {
                if current == start {
                    break;
                }
                points.push([current.0 as f64, current.1 as f64]);
                let next = edges.get(&current).and_then(|ns| {
                    ns.iter()
                        .copied()
                        .find(|&n| n != previous && !used.contains(&key(current, n)))
                });
                let Some(next) = next else {
                    break;
                };
                used.insert(key(current, next));
                previous = current;
                current = next;
            }
            if current != start || points.len() < 3 {
                continue;
            }
            let a = (0..points.len())
                .map(|i| {
                    let p = points[i];
                    let q = points[(i + 1) % points.len()];
                    p[0] * q[1] - q[0] * p[1]
                })
                .sum::<f64>()
                .abs()
                / 2.0;
            if a > best_area {
                best_area = a;
                best = points;
            }
        }
    }
    best
}

pub(super) fn find(
    walls: &[StoredWall],
    click: Point,
    gap: u32,
) -> Result<Option<TraceHit>, TraceFailure> {
    if walls.is_empty() {
        return Ok(None);
    }
    let mut low = [f64::INFINITY; 2];
    let mut high = [f64::NEG_INFINITY; 2];
    let paths: Vec<Vec<Point>> = walls
        .iter()
        .map(|wall| polygon(&wall.ring, wall.closed))
        .collect();
    for p in paths.iter().flatten() {
        for axis in 0..2 {
            low[axis] = low[axis].min(p[axis]);
            high[axis] = high[axis].max(p[axis]);
        }
    }
    let margin = f64::from(gap) * 2.0 + 24.0;
    for axis in 0..2 {
        low[axis] -= margin / 2.0;
        high[axis] += margin / 2.0;
    }
    let size = [high[0] - low[0], high[1] - low[1]];
    if !size.iter().all(|v| v.is_finite() && *v > 0.0) {
        return Err(TraceFailure::Failed);
    }
    let scale = (1400.0 / size[0].max(size[1]).max(1.0)).min(1.0);
    let w = (size[0] * scale).round().max(1.0) as usize;
    let h = (size[1] * scale).round().max(1.0) as usize;
    if w * h > 2_352_000 {
        return Ok(None);
    }
    let mut blocked = vec![false; w * h];
    for (wall, points) in walls.iter().zip(paths) {
        let pts: Vec<Point> = points
            .iter()
            .map(|p| [(p[0] - low[0]) * scale, (p[1] - low[1]) * scale])
            .collect();
        let width = if wall.width == 0.0 { 2.0 } else { wall.width };
        let radius = (width.max(1.5) / 2.0 + f64::from(gap)) * scale;
        let edges = if wall.closed {
            pts.len()
        } else {
            pts.len().saturating_sub(1)
        };
        for i in 0..edges {
            let a = pts[i];
            let b = pts[(i + 1) % pts.len()];
            let x0 = (a[0].min(b[0]) - radius).floor().max(0.0) as usize;
            let y0 = (a[1].min(b[1]) - radius).floor().max(0.0) as usize;
            let x1 = ((a[0].max(b[0]) + radius).ceil().max(0.0) as usize).min(w - 1);
            let y1 = ((a[1].max(b[1]) + radius).ceil().max(0.0) as usize).min(h - 1);
            for y in y0..=y1 {
                for x in x0..=x1 {
                    if distance([x as f64 + 0.5, y as f64 + 0.5], a, b) <= radius {
                        blocked[y * w + x] = true;
                    }
                }
            }
        }
        if wall.closed && wall.filled {
            for y in 0..h {
                for x in 0..w {
                    if inside(&pts, [x as f64 + 0.5, y as f64 + 0.5]) {
                        blocked[y * w + x] = true;
                    }
                }
            }
        }
    }
    let x = ((click[0] - low[0]) * scale).round();
    let y = ((click[1] - low[1]) * scale).round();
    if x < 0.0 || y < 0.0 || x >= w as f64 || y >= h as f64 {
        return Ok(None);
    }
    let start = y as usize * w + x as usize;
    if blocked[start] {
        return Ok(None);
    }
    let mut seen = vec![false; w * h];
    seen[start] = true;
    let mut todo = vec![start];
    let mut count = 0;
    while let Some(i) = todo.pop() {
        let x = i % w;
        let y = i / w;
        count += 1;
        if x == 0 || y == 0 || x == w - 1 || y == h - 1 {
            return Ok(None);
        }
        for n in [i - 1, i + 1, i - w, i + w] {
            if !seen[n] && !blocked[n] {
                seen[n] = true;
                todo.push(n);
            }
        }
    }
    if count < 4 {
        return Ok(None);
    }
    let raw: Vec<Point> = contour(&seen, w, h)
        .iter()
        .map(|p| [low[0] + p[0] / scale, low[1] + p[1] / scale])
        .collect();
    if raw.len() < 3 {
        return Ok(None);
    }
    let ring = simplify_closed(&raw, (1.5 / scale).max(1.0)).unwrap_or_else(|| {
        raw.iter()
            .map(|&p| RingSeg {
                point: p,
                handle_in: [0.0; 2],
                handle_out: [0.0; 2],
            })
            .collect()
    });
    if !contains(&ring, click) {
        return Ok(None);
    }
    Ok(Some(TraceHit {
        area: area(&ring),
        ring,
        wall_ids: walls.iter().map(|w| w.id.clone()).collect(),
        used_gap: gap > 0,
        gap_px: gap,
    }))
}
