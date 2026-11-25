//! Contours as Nemo path anchors (`segments` of a stored stroke).
//!
//! A Nemo path is one contour of cubic anchors with relative handles. An XFL
//! fill can hold several loops, some of them holes. Each outer loop becomes one
//! path. Its holes are joined to it by a zero-width bridge: the path runs out
//! to the hole, around it, and back the same way. The hole is wound the
//! opposite way, so the non-zero rule leaves it empty.

use serde_json::{json, Value};

use crate::report::ImportReport;

use crate::model::*;

/// Most anchors one Nemo path may carry.
pub const MAX_ANCHORS: usize = 256;

/// One anchor: absolute point, handles relative to it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Anchor {
    pub point: Point,
    pub handle_in: Point,
    pub handle_out: Point,
}

pub const ZERO: Point = Point { x: 0.0, y: 0.0 };

fn sub(a: Point, b: Point) -> Point {
    Point {
        x: a.x - b.x,
        y: a.y - b.y,
    }
}

fn linear(m: &Matrix, v: Point) -> Point {
    Point {
        x: m.a * v.x + m.c * v.y,
        y: m.b * v.x + m.d * v.y,
    }
}

/// The contour's anchors after `m`. A closed contour does not repeat its first
/// anchor at the end; its closing segment is the first anchor's `handle_in`.
pub fn anchors(c: &Contour, m: &Matrix) -> Vec<Anchor> {
    let mut out = vec![Anchor {
        point: m.apply(c.start),
        handle_in: ZERO,
        handle_out: ZERO,
    }];
    let mut from = c.start;
    for seg in &c.segments {
        let (c1, c2, to) = seg.to_cubic(from);
        let last = out.len() - 1;
        out[last].handle_out = linear(m, sub(c1, from));
        out.push(Anchor {
            point: m.apply(to),
            handle_in: linear(m, sub(c2, to)),
            handle_out: ZERO,
        });
        from = to;
    }
    if c.closed && out.len() > 1 {
        let end = out.pop().expect("at least two anchors");
        out[0].handle_in = end.handle_in;
    }
    out
}

/// The loop flattened into points (each cubic sampled `STEPS` times), so
/// area and containment see the curves, not just the anchors.
fn flatten(a: &[Anchor]) -> Vec<Point> {
    const STEPS: usize = 8;
    let n = a.len();
    let mut out = Vec::with_capacity(n * STEPS);
    for i in 0..n {
        let (p, q) = (a[i], a[(i + 1) % n]);
        let c1 = Point {
            x: p.point.x + p.handle_out.x,
            y: p.point.y + p.handle_out.y,
        };
        let c2 = Point {
            x: q.point.x + q.handle_in.x,
            y: q.point.y + q.handle_in.y,
        };
        for k in 0..STEPS {
            let t = k as f64 / STEPS as f64;
            let u = 1.0 - t;
            let (w0, w1, w2, w3) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            out.push(Point {
                x: w0 * p.point.x + w1 * c1.x + w2 * c2.x + w3 * q.point.x,
                y: w0 * p.point.y + w1 * c1.y + w2 * c2.y + w3 * q.point.y,
            });
        }
    }
    out
}

/// Signed area of a polygon, y down.
fn area(pts: &[Point]) -> f64 {
    let n = pts.len();
    (0..n)
        .map(|i| {
            let (p, q) = (pts[i], pts[(i + 1) % n]);
            p.x * q.y - q.x * p.y
        })
        .sum::<f64>()
        / 2.0
}

/// Even-odd test of `p` against a polygon.
fn inside(p: Point, pts: &[Point]) -> bool {
    let n = pts.len();
    let mut hit = false;
    for i in 0..n {
        let (u, v) = (pts[i], pts[(i + 1) % n]);
        if (u.y > p.y) != (v.y > p.y) && p.x < (v.x - u.x) * (p.y - u.y) / (v.y - u.y) + u.x {
            hit = !hit;
        }
    }
    hit
}

/// Whether most of `hole`'s points lie inside `outer` (robust to a hole
/// touching the outer boundary at a point or along an edge).
fn holds(outer: &[Point], hole: &[Point]) -> bool {
    let hits = hole.iter().filter(|p| inside(**p, outer)).count();
    hits * 2 > hole.len()
}

fn dist2(a: Point, b: Point) -> f64 {
    (a.x - b.x).powi(2) + (a.y - b.y).powi(2)
}

/// Splice `hole` into `outer` with a bridge from the outer anchor nearest the
/// hole's first anchor.
fn bridge(outer: &mut Vec<Anchor>, hole: &[Anchor]) {
    let target = hole[0].point;
    let k = (0..outer.len())
        .min_by(|&i, &j| dist2(outer[i].point, target).total_cmp(&dist2(outer[j].point, target)))
        .expect("outer loop is not empty");
    let at = outer[k];
    // Leave `at` along the bridge, go round the hole, come back to `at`.
    let mut spliced = Vec::with_capacity(hole.len() + 2);
    let mut first = hole[0];
    first.handle_in = ZERO;
    spliced.push(first);
    spliced.extend_from_slice(&hole[1..]);
    spliced.push(Anchor {
        point: target,
        handle_in: hole[0].handle_in,
        handle_out: ZERO,
    });
    spliced.push(Anchor {
        point: at.point,
        handle_in: ZERO,
        handle_out: at.handle_out,
    });
    outer[k].handle_out = ZERO;
    outer.splice(k + 1..k + 1, spliced);
}

/// Closed fill loops grouped into one anchor list per outer loop, holes
/// bridged in, plus the number of holes that sat in no outer loop (left out:
/// drawn alone they would fill). Degenerate loops are dropped.
pub fn fill_regions(contours: &[Contour], m: &Matrix) -> (Vec<Vec<Anchor>>, usize) {
    let loops: Vec<(Vec<Anchor>, Vec<Point>, f64)> = contours
        .iter()
        .filter(|c| c.closed)
        .map(|c| anchors(c, m))
        .filter(|a| a.len() >= 2)
        .map(|a| {
            let pts = flatten(&a);
            let ar = area(&pts);
            (a, pts, ar)
        })
        .filter(|(_, _, ar)| ar.abs() > 1e-9)
        .collect();
    let Some(outer_sign) = loops
        .iter()
        .map(|l| l.2)
        .max_by(|a, b| a.abs().total_cmp(&b.abs()))
        .map(f64::signum)
    else {
        return (Vec::new(), 0);
    };
    let (mut outers, holes): (Vec<_>, Vec<_>) =
        loops.into_iter().partition(|l| l.2.signum() == outer_sign);
    let mut dropped = 0;
    for (hole, hole_pts, _) in holes {
        // The smallest outer loop holding the hole.
        let owner = (0..outers.len())
            .filter(|&i| holds(&outers[i].1, &hole_pts))
            .min_by(|&i, &j| outers[i].2.abs().total_cmp(&outers[j].2.abs()));
        match owner {
            Some(i) => bridge(&mut outers[i].0, &hole),
            None => dropped += 1,
        }
    }
    (outers.into_iter().map(|l| l.0).collect(), dropped)
}

fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

fn pair(p: Point) -> Value {
    json!([round(p.x), round(p.y)])
}

/// The `segments` member of a stored stroke.
pub fn segments_json(a: &[Anchor]) -> Value {
    Value::Array(
        a.iter()
            .map(|k| {
                json!({
                    "point": pair(k.point),
                    "handleIn": pair(k.handle_in),
                    "handleOut": pair(k.handle_out),
                })
            })
            .collect(),
    )
}

/// Most paths drawn into one frame of one layer.
pub const MAX_PATHS_PER_FRAME: usize = 20_000;

pub fn hex(c: Rgba, alpha: f64) -> String {
    Rgba {
        a: c.a * alpha,
        ..c
    }
    .hex8()
}

/// One colour for a paint; gradients fall back to their middle stop.
pub fn solid(paint: &Paint, alpha: f64, report: &mut ImportReport, loc: &str) -> Option<String> {
    match paint {
        Paint::Solid { color } => Some(hex(*color, alpha)),
        Paint::Linear { stops, .. } | Paint::Radial { stops, .. } => {
            report.skip("gradient drawn as its middle colour", loc);
            stops.get(stops.len() / 2).map(|s| hex(s.color, alpha))
        }
        Paint::Bitmap { .. } => {
            report.skip("bitmap fill", loc);
            None
        }
    }
}

/// Average linear scale of `m`, for stroke widths.
pub fn scale(m: &Matrix) -> f64 {
    (m.a * m.d - m.b * m.c).abs().sqrt()
}

pub fn push_path(
    out: &mut Vec<Value>,
    anchors: &[Anchor],
    closed: bool,
    mut body: Value,
    report: &mut ImportReport,
    loc: &str,
) {
    if anchors.len() < 2 {
        return;
    }
    if out.len() >= MAX_PATHS_PER_FRAME {
        report.skip(
            format!("more than {MAX_PATHS_PER_FRAME} paths in one frame"),
            loc,
        );
        return;
    }
    if anchors.len() > MAX_ANCHORS {
        if closed {
            report.skip(
                format!("filled region with more than {MAX_ANCHORS} anchors"),
                loc,
            );
            return;
        }
        // A long stroke is cut into pieces sharing their end anchors.
        let mut start = 0;
        while start + 1 < anchors.len() {
            let end = (start + MAX_ANCHORS).min(anchors.len());
            let mut piece = anchors[start..end].to_vec();
            piece[0].handle_in = ZERO;
            let last = piece.len() - 1;
            piece[last].handle_out = ZERO;
            push_path(out, &piece, false, body.clone(), report, loc);
            start = end - 1;
        }
        return;
    }
    body["segments"] = segments_json(anchors);
    body["closed"] = json!(closed);
    out.push(body);
}

fn pt(v: &Value) -> Option<(f64, f64)> {
    Some((v[0].as_f64()?, v[1].as_f64()?))
}

/// Grow `b` (min x, min y, max x, max y) by the curve bounds of a stored
/// stroke: anchors and the extrema of each cubic, no stroke width.
fn add_bounds(b: &mut [f64; 4], stroke: &Value) {
    let Some(segs) = stroke["segments"].as_array() else {
        return;
    };
    let a: Vec<[(f64, f64); 3]> = segs
        .iter()
        .filter_map(|s| Some([pt(&s["point"])?, pt(&s["handleIn"])?, pt(&s["handleOut"])?]))
        .collect();
    let mut add = |x: f64, y: f64| {
        *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
    };
    for k in &a {
        add(k[0].0, k[0].1);
    }
    let n = a.len();
    let curves = if stroke["closed"] == true {
        n
    } else {
        n.saturating_sub(1)
    };
    for i in 0..curves {
        let (p, q) = (a[i], a[(i + 1) % n]);
        let xs = [p[0].0, p[0].0 + p[2].0, q[0].0 + q[1].0, q[0].0];
        let ys = [p[0].1, p[0].1 + p[2].1, q[0].1 + q[1].1, q[0].1];
        for t in extrema(xs).into_iter().chain(extrema(ys)).flatten() {
            add(cubic(xs, t), cubic(ys, t));
        }
    }
}

fn cubic(p: [f64; 4], t: f64) -> f64 {
    let u = 1.0 - t;
    u * u * u * p[0] + 3.0 * u * u * t * p[1] + 3.0 * u * t * t * p[2] + t * t * t * p[3]
}

/// Parameters in (0, 1) where one coordinate of a cubic has zero slope.
fn extrema(p: [f64; 4]) -> [Option<f64>; 2] {
    let a = -p[0] + 3.0 * p[1] - 3.0 * p[2] + p[3];
    let b = 2.0 * (p[0] - 2.0 * p[1] + p[2]);
    let c = p[1] - p[0];
    let inside = |t: f64| (t > 0.0 && t < 1.0).then_some(t);
    if a.abs() < 1e-12 {
        return [(b.abs() > 1e-12).then(|| -c / b).and_then(inside), None];
    }
    let disc = b * b - 4.0 * a * c;
    if disc < 0.0 {
        return [None, None];
    }
    let r = disc.sqrt();
    [inside((-b + r) / (2.0 * a)), inside((-b - r) / (2.0 * a))]
}

/// Centre of the content bounds at each frame of a component: the union of
/// every layer's held keyframe (`getEffectiveStrokes`, symbol branch).
pub(crate) fn content_centers(layers: &[Value], len: u32) -> Vec<Option<Point>> {
    let empty = [f64::INFINITY, f64::INFINITY, -f64::INFINITY, -f64::INFINITY];
    let mut held = vec![empty; layers.len()];
    let mut out = Vec::with_capacity(len as usize);
    for f in 0..len as usize {
        let mut b = empty;
        for (l, layer) in layers.iter().enumerate() {
            let frame = &layer["frames"][f];
            if frame["isKeyframe"] == true {
                held[l] = empty;
                for s in frame["strokes"].as_array().into_iter().flatten() {
                    add_bounds(&mut held[l], s);
                }
            }
            let h = held[l];
            b = [
                b[0].min(h[0]),
                b[1].min(h[1]),
                b[2].max(h[2]),
                b[3].max(h[3]),
            ];
        }
        out.push((b[0] <= b[2]).then(|| Point {
            x: (b[0] + b[2]) / 2.0,
            y: (b[1] + b[3]) / 2.0,
        }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, y0: f64, s: f64, cw: bool) -> Contour {
        let mut pts = vec![(x0 + s, y0), (x0 + s, y0 + s), (x0, y0 + s), (x0, y0)];
        if !cw {
            pts = vec![(x0, y0 + s), (x0 + s, y0 + s), (x0 + s, y0), (x0, y0)];
        }
        Contour {
            start: Point { x: x0, y: y0 },
            segments: pts
                .into_iter()
                .map(|(x, y)| Segment::Line { to: Point { x, y } })
                .collect(),
            closed: true,
        }
    }

    #[test]
    fn closed_contour_does_not_repeat_its_start() {
        let a = anchors(&square(0.0, 0.0, 10.0, true), &Matrix::IDENTITY);
        assert_eq!(a.len(), 4);
        assert!(area(&flatten(&a)) > 0.0);
    }

    #[test]
    fn quad_handles_are_relative_and_transformed() {
        let c = Contour {
            start: Point { x: 0.0, y: 0.0 },
            segments: vec![Segment::Quad {
                ctrl: Point { x: 3.0, y: 3.0 },
                to: Point { x: 6.0, y: 0.0 },
            }],
            closed: false,
        };
        let m = Matrix {
            a: 2.0,
            d: 2.0,
            tx: 100.0,
            ..Matrix::IDENTITY
        };
        let a = anchors(&c, &m);
        assert_eq!(a[0].point, Point { x: 100.0, y: 0.0 });
        assert_eq!(a[0].handle_out, Point { x: 4.0, y: 4.0 });
        assert_eq!(a[1].point, Point { x: 112.0, y: 0.0 });
        assert_eq!(a[1].handle_in, Point { x: -4.0, y: 4.0 });
    }

    #[test]
    fn hole_is_bridged_into_its_outer_loop() {
        let outer = square(0.0, 0.0, 100.0, true);
        let hole = square(40.0, 40.0, 20.0, false);
        let lone = square(300.0, 0.0, 10.0, true);
        let (r, dropped) = fill_regions(&[outer, hole, lone], &Matrix::IDENTITY);
        assert_eq!((r.len(), dropped), (2, 0));
        // 4 outer + 4 hole + hole start again + outer anchor again.
        assert_eq!(r[0].len(), 10);
        // The net area is the outer minus the hole.
        assert!((area(&flatten(&r[0])) - (10_000.0 - 400.0)).abs() < 1e-6);
    }

    #[test]
    fn hole_starting_on_the_outer_edge_is_still_bridged() {
        let outer = square(0.0, 0.0, 100.0, true);
        // Triangle hole, wound against the outer, first point on the edge.
        let hole = Contour {
            start: Point { x: 100.0, y: 50.0 },
            segments: [(50.0, 20.0), (50.0, 80.0), (100.0, 50.0)]
                .into_iter()
                .map(|(x, y)| Segment::Line { to: Point { x, y } })
                .collect(),
            closed: true,
        };
        let (r, dropped) = fill_regions(&[outer, hole], &Matrix::IDENTITY);
        assert_eq!((r.len(), dropped), (1, 0));
    }

    #[test]
    fn curved_two_anchor_outer_keeps_its_hole() {
        // A circle-ish outer from two quads (two anchors, zero polygon area).
        let outer = Contour {
            start: Point { x: 0.0, y: 0.0 },
            segments: vec![
                Segment::Quad {
                    ctrl: Point { x: 50.0, y: -60.0 },
                    to: Point { x: 100.0, y: 0.0 },
                },
                Segment::Quad {
                    ctrl: Point { x: 50.0, y: 60.0 },
                    to: Point { x: 0.0, y: 0.0 },
                },
            ],
            closed: true,
        };
        let sign = area(&flatten(&anchors(&outer, &Matrix::IDENTITY))).signum();
        let hole = square(45.0, -5.0, 10.0, sign < 0.0);
        let (r, dropped) = fill_regions(&[outer, hole], &Matrix::IDENTITY);
        assert_eq!((r.len(), dropped), (1, 0));
        assert_eq!(r[0].len(), 2 + 4 + 2);
    }
}
