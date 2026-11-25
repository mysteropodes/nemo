//! Contours as Nemo path anchors (`segments` of a stored stroke).
//!
//! A Nemo path is one contour of cubic anchors with relative handles. An XFL
//! fill can hold several loops, some of them holes. Each outer loop becomes one
//! path. Its holes are joined to it by a zero-width bridge: the path runs out
//! to the hole, around it, and back the same way. The hole is wound the
//! opposite way, so the non-zero rule leaves it empty.

use serde_json::{json, Value};

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

const ZERO: Point = Point { x: 0.0, y: 0.0 };

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

/// Signed area of the anchor polygon (handles ignored), y down.
fn area(a: &[Anchor]) -> f64 {
    let n = a.len();
    (0..n)
        .map(|i| {
            let (p, q) = (a[i].point, a[(i + 1) % n].point);
            p.x * q.y - q.x * p.y
        })
        .sum::<f64>()
        / 2.0
}

/// Even-odd test of `p` against the anchor polygon.
fn inside(p: Point, a: &[Anchor]) -> bool {
    let n = a.len();
    let mut hit = false;
    for i in 0..n {
        let (u, v) = (a[i].point, a[(i + 1) % n].point);
        if (u.y > p.y) != (v.y > p.y) && p.x < (v.x - u.x) * (p.y - u.y) / (v.y - u.y) + u.x {
            hit = !hit;
        }
    }
    hit
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
/// bridged in. Loops with fewer than two anchors are dropped.
pub fn fill_regions(contours: &[Contour], m: &Matrix) -> Vec<Vec<Anchor>> {
    let loops: Vec<Vec<Anchor>> = contours
        .iter()
        .filter(|c| c.closed)
        .map(|c| anchors(c, m))
        .filter(|a| a.len() >= 2)
        .collect();
    let Some(outer_sign) = loops
        .iter()
        .map(|l| area(l))
        .max_by(|a, b| a.abs().total_cmp(&b.abs()))
        .map(f64::signum)
    else {
        return Vec::new();
    };
    let (outers, holes): (Vec<_>, Vec<_>) = loops
        .into_iter()
        .partition(|l| area(l).signum() == outer_sign || area(l) == 0.0);
    let mut regions = outers;
    for hole in holes {
        // The smallest outer loop holding the hole, else the hole stands alone.
        let owner = (0..regions.len())
            .filter(|&i| inside(hole[0].point, &regions[i]))
            .min_by(|&i, &j| area(&regions[i]).abs().total_cmp(&area(&regions[j]).abs()));
        match owner {
            Some(i) => bridge(&mut regions[i], &hole),
            None => regions.push(hole),
        }
    }
    regions
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
        assert!(area(&a) > 0.0);
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
        let r = fill_regions(&[outer, hole, lone], &Matrix::IDENTITY);
        assert_eq!(r.len(), 2);
        // 4 outer + 4 hole + hole start again + outer anchor again.
        assert_eq!(r[0].len(), 10);
        // The net area is the outer minus the hole.
        assert!((area(&r[0]) - (10_000.0 - 400.0)).abs() < 1e-9);
    }
}
