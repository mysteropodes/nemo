//! Motion guide: a tweened instance under a guide layer travels along the
//! guide's stroke instead of a straight line.
//!
//! The guide's stroke contours are chained into paths, each path is cut into
//! pieces short enough to be straight (adaptive subdivision of its quadratic
//! segments) and measured. The two keyframe positions are projected onto the
//! path; the eased progress then moves along the arc length between the two
//! projections.

use std::f64::consts::PI;

use serde::Serialize;

use crate::model::*;

/// A keyframe further than this from the guide stroke (px) is not attached
/// to it, and the tween stays a straight line.
pub const ATTACH_TOLERANCE: f64 = 5.0;
/// Contour ends closer than this (px; a twip is 0.05) are the same point.
const JOIN_EPS: f64 = 0.03;
/// A piece is straight when its control polygon exceeds its chord by less (px).
const FLATNESS: f64 = 1e-5;
const MAX_DEPTH: u32 = 16;
/// Beyond this many stroke contours the guide is not chained (cost is cubic).
const MAX_CHAINED: usize = 256;

fn sub(a: Point, b: Point) -> Point {
    Point {
        x: a.x - b.x,
        y: a.y - b.y,
    }
}

fn lerp(a: Point, b: Point, k: f64) -> Point {
    Point {
        x: a.x + (b.x - a.x) * k,
        y: a.y + (b.y - a.y) * k,
    }
}

fn dist(a: Point, b: Point) -> f64 {
    (a.x - b.x).hypot(a.y - b.y)
}

/// Angle into (-π, π].
pub(crate) fn wrap(a: f64) -> f64 {
    let mut r = a % (2.0 * PI);
    if r <= -PI {
        r += 2.0 * PI;
    } else if r > PI {
        r -= 2.0 * PI;
    }
    r
}

/// A quadratic Bézier; a line is the quad whose control is its midpoint.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Quad {
    p0: Point,
    c: Point,
    p1: Point,
}

impl Quad {
    fn at(&self, u: f64) -> Point {
        lerp(lerp(self.p0, self.c, u), lerp(self.c, self.p1, u), u)
    }

    /// Direction of travel; the chord where the derivative vanishes.
    fn tangent(&self, u: f64) -> Point {
        let d = lerp(sub(self.c, self.p0), sub(self.p1, self.c), u);
        if d.x.hypot(d.y) > 1e-12 {
            d
        } else {
            sub(self.p1, self.p0)
        }
    }

    fn reversed(&self) -> Quad {
        Quad {
            p0: self.p1,
            c: self.c,
            p1: self.p0,
        }
    }
}

/// A stretch of one quad short enough to be measured as its chord.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Piece {
    quad: usize,
    u0: f64,
    u1: f64,
    a: Point,
    b: Point,
    /// Arc length from the start of the path to `a`.
    start: f64,
    len: f64,
}

/// One connected guide stroke, parameterised by arc length.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GuidePath {
    pub length: f64,
    pub closed: bool,
    #[serde(skip)]
    quads: Vec<Quad>,
    #[serde(skip)]
    pieces: Vec<Piece>,
}

impl GuidePath {
    /// `None` when the contour has no length.
    pub fn from_contour(contour: &Contour) -> Option<GuidePath> {
        GuidePath::from_quads(contour_quads(contour, &Matrix::IDENTITY))
    }

    fn from_quads(quads: Vec<Quad>) -> Option<GuidePath> {
        let mut path = GuidePath {
            length: 0.0,
            closed: false,
            pieces: Vec::new(),
            quads,
        };
        for i in 0..path.quads.len() {
            path.subdivide(i, 0.0, 1.0, 0);
        }
        let (first, last) = (path.quads.first()?, path.quads.last()?);
        path.closed = dist(first.p0, last.p1) < JOIN_EPS;
        (path.length.is_finite() && path.length > 1e-9).then_some(path)
    }

    fn subdivide(&mut self, quad: usize, u0: f64, u1: f64, depth: u32) {
        let q = self.quads[quad];
        let (a, m, b) = (q.at(u0), q.at(0.5 * (u0 + u1)), q.at(u1));
        let chord = dist(a, b);
        if depth < MAX_DEPTH && dist(a, m) + dist(m, b) - chord > FLATNESS {
            self.subdivide(quad, u0, 0.5 * (u0 + u1), depth + 1);
            self.subdivide(quad, 0.5 * (u0 + u1), u1, depth + 1);
            return;
        }
        // Chord and two-chord polygon bracket the arc; this blend cancels
        // the leading error term of both.
        let len = (4.0 * (dist(a, m) + dist(m, b)) - chord) / 3.0;
        if len > 0.0 {
            self.pieces.push(Piece {
                quad,
                u0,
                u1,
                a,
                b,
                start: self.length,
                len,
            });
            self.length += len;
        }
    }

    /// Piece holding arc length `s`, and the quad parameter there. On a
    /// closed path `s` wraps around; on an open one it is clamped.
    fn locate(&self, s: f64) -> (&Piece, f64) {
        let s = if self.closed {
            s.rem_euclid(self.length)
        } else {
            s.clamp(0.0, self.length)
        };
        let i = self
            .pieces
            .partition_point(|p| p.start + p.len < s)
            .min(self.pieces.len() - 1);
        let p = &self.pieces[i];
        let f = ((s - p.start) / p.len).clamp(0.0, 1.0);
        (p, p.u0 + (p.u1 - p.u0) * f)
    }

    pub fn point_at(&self, s: f64) -> Point {
        let (p, u) = self.locate(s);
        self.quads[p.quad].at(u)
    }

    /// Direction of the path at arc length `s`, as an angle (y down).
    pub fn tangent_angle(&self, s: f64) -> f64 {
        let (p, u) = self.locate(s);
        let d = self.quads[p.quad].tangent(u);
        d.y.atan2(d.x)
    }

    /// Arc length of the path point nearest to `p`, and its distance.
    pub fn project(&self, p: Point) -> (f64, f64) {
        let mut best = (0.0, f64::INFINITY);
        for piece in &self.pieces {
            let ab = sub(piece.b, piece.a);
            let ap = sub(p, piece.a);
            let f = ((ap.x * ab.x + ap.y * ab.y) / (ab.x * ab.x + ab.y * ab.y)).clamp(0.0, 1.0);
            let f = if f.is_finite() { f } else { 0.0 };
            let d = dist(p, lerp(piece.a, piece.b, f));
            if d < best.1 {
                best = (piece.start + piece.len * f, d);
            }
        }
        best
    }
}

/// Which point of the instance rides the guide.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GuideAnchor {
    TransformationPoint,
    /// The symbol's origin.
    Registration,
}

/// The travel of one tween along a guide path.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GuideMotion {
    pub path: GuidePath,
    pub anchor: GuideAnchor,
    /// Arc lengths of the two keyframes; on a closed path `end` may lie
    /// outside 0..length (the shorter way round crosses the seam).
    pub start: f64,
    pub end: f64,
    /// What separates each keyframe's anchor from its projection.
    pub start_offset: Point,
    pub end_offset: Point,
    pub orient: bool,
}

impl GuideMotion {
    /// `None` when either position is further than [`ATTACH_TOLERANCE`]
    /// from the path. The score is the larger of the two distances.
    fn attach(path: &GuidePath, from: Point, to: Point) -> Option<(GuideMotion, f64)> {
        let (s0, d0) = path.project(from);
        let (mut s1, d1) = path.project(to);
        if d0.max(d1) > ATTACH_TOLERANCE {
            return None;
        }
        if path.closed && (s1 - s0).abs() > path.length / 2.0 {
            s1 -= path.length * (s1 - s0).signum();
        }
        let motion = GuideMotion {
            path: path.clone(),
            anchor: GuideAnchor::TransformationPoint,
            start: s0,
            end: s1,
            start_offset: sub(from, path.point_at(s0)),
            end_offset: sub(to, path.point_at(s1)),
            orient: false,
        };
        Some((motion, d0.max(d1)))
    }

    /// Anchor position at eased progress `k`, and the rotation the path
    /// direction adds since the start (0 unless oriented to the path).
    pub fn sample(&self, k: f64) -> (Point, f64) {
        let s = self.start + (self.end - self.start) * k;
        let on_path = self.path.point_at(s);
        let off = lerp(self.start_offset, self.end_offset, k);
        let turn = if self.orient {
            wrap(self.path.tangent_angle(s) - self.path.tangent_angle(self.start))
        } else {
            0.0
        };
        (
            Point {
                x: on_path.x + off.x,
                y: on_path.y + off.y,
            },
            turn,
        )
    }
}

/// The contour's segments as quads, placed by `m`.
fn contour_quads(contour: &Contour, m: &Matrix) -> Vec<Quad> {
    let mut p0 = m.apply(contour.start);
    let mut quads = Vec::new();
    for s in &contour.segments {
        let p1 = m.apply(s.end());
        let c = match *s {
            Segment::Line { .. } => lerp(p0, p1, 0.5),
            Segment::Quad { ctrl, .. } => m.apply(ctrl),
        };
        quads.push(Quad { p0, c, p1 });
        p0 = p1;
    }
    quads
}

fn collect_strokes(elements: &[Element], out: &mut Vec<Vec<Quad>>) {
    for e in elements {
        match e {
            Element::Shape(shape) => {
                let stroked = shape.paths.iter().filter(|p| p.stroke.is_some());
                for contour in stroked.flat_map(|p| p.contours.iter()) {
                    let quads = contour_quads(contour, &shape.matrix);
                    if !quads.is_empty() {
                        out.push(quads);
                    }
                }
            }
            Element::Group { members } => collect_strokes(members, out),
            _ => {}
        }
    }
}

/// Join chains that share an end point (strokes drawn in several gestures,
/// or split where they cross a fill, arrive as separate contours).
fn chain(mut chains: Vec<Vec<Quad>>) -> Vec<Vec<Quad>> {
    if chains.len() > MAX_CHAINED {
        return chains;
    }
    let ends = |c: &[Quad]| (c[0].p0, c[c.len() - 1].p1);
    let mut i = 0;
    while i < chains.len() {
        let (start, end) = ends(&chains[i]);
        // A closed loop takes no more.
        let found = (dist(start, end) >= JOIN_EPS)
            .then(|| {
                (i + 1..chains.len()).find_map(|j| {
                    let (s, e) = ends(&chains[j]);
                    [(end, s), (end, e), (start, e), (start, s)]
                        .iter()
                        .position(|(a, b)| dist(*a, *b) < JOIN_EPS)
                        .map(|how| (j, how))
                })
            })
            .flatten();
        let Some((j, how)) = found else {
            i += 1;
            continue;
        };
        let mut other = chains.remove(j);
        if how == 1 || how == 3 {
            other.reverse();
            other.iter_mut().for_each(|q| *q = q.reversed());
        }
        if how < 2 {
            chains[i].extend(other);
        } else {
            other.append(&mut chains[i]);
            chains[i] = other;
        }
    }
    chains
}

/// Every connected stroke path of `layer` on the keyframe covering `frame`.
pub fn guide_paths(layer: &Layer, frame: u32) -> Vec<GuidePath> {
    let Some(key) = layer
        .frames
        .iter()
        .find(|f| f.index <= frame && frame < f.index.saturating_add(f.duration))
    else {
        return Vec::new();
    };
    let mut strokes = Vec::new();
    collect_strokes(&key.elements, &mut strokes);
    chain(strokes)
        .into_iter()
        .filter_map(GuidePath::from_quads)
        .collect()
}

/// The path both keyframes sit on, if any: the best fit over every path and
/// both candidate anchors, the transformation point winning ties.
/// Each candidate is `(anchor, start position, end position)`.
pub fn attach(
    paths: &[GuidePath],
    candidates: &[(GuideAnchor, Point, Point)],
    orient: bool,
) -> Option<GuideMotion> {
    let mut best: Option<(GuideMotion, f64)> = None;
    for &(anchor, from, to) in candidates {
        for path in paths {
            let Some((mut motion, score)) = GuideMotion::attach(path, from, to) else {
                continue;
            };
            if best.as_ref().is_none_or(|(_, b)| score < *b - 1e-9) {
                motion.anchor = anchor;
                motion.orient = orient;
                best = Some((motion, score));
            }
        }
    }
    best.map(|(m, _)| m)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pt(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    /// Half circle of radius `r` around the origin from (-r, 0) over the top
    /// (0, -r) to (r, 0), as `n` quads tangent to the circle at their ends.
    fn semicircle(r: f64, n: usize) -> Contour {
        let on = |a: f64, rad: f64| pt(rad * a.cos(), rad * a.sin());
        let step = PI / n as f64;
        let segments = (0..n)
            .map(|i| {
                let a0 = PI + step * i as f64;
                Segment::Quad {
                    ctrl: on(a0 + step / 2.0, r / (step / 2.0).cos()),
                    to: on(a0 + step, r),
                }
            })
            .collect();
        Contour {
            start: pt(-r, 0.0),
            segments,
            closed: false,
        }
    }

    #[test]
    fn straight_path_measures_and_projects() {
        let line = Contour {
            start: pt(0.0, 0.0),
            segments: vec![
                Segment::Line { to: pt(30.0, 40.0) },
                Segment::Line { to: pt(60.0, 80.0) },
            ],
            closed: false,
        };
        let path = GuidePath::from_contour(&line).unwrap();
        assert!((path.length - 100.0).abs() < 1e-9 && !path.closed);
        let p = path.point_at(25.0);
        assert!((p.x - 15.0).abs() < 1e-9 && (p.y - 20.0).abs() < 1e-9);
        // (15, 20) pushed 2 px off the line, along its normal (-0.8, 0.6).
        let (s, d) = path.project(pt(15.0 - 1.6, 20.0 + 1.2));
        assert!((s - 25.0).abs() < 1e-9 && (d - 2.0).abs() < 1e-9);
        assert!((path.tangent_angle(70.0) - (40f64).atan2(30.0)).abs() < 1e-12);
        // Out of range clamps on an open path.
        assert_eq!(path.point_at(-5.0), pt(0.0, 0.0));
        assert!(GuidePath::from_contour(&Contour {
            start: pt(1.0, 1.0),
            segments: vec![],
            closed: false
        })
        .is_none());
    }

    #[test]
    fn semicircle_arc_length_midpoint_and_tangent() {
        // 8 tangent quads are within 0.02 % of a true half circle.
        let path = GuidePath::from_contour(&semicircle(100.0, 8)).unwrap();
        assert!((path.length - 100.0 * PI).abs() < 0.1, "{}", path.length);
        // The arc midpoint is the top of the circle, by symmetry exactly.
        let mid = path.point_at(path.length / 2.0);
        assert!(
            mid.x.abs() < 1e-6 && (mid.y + 100.0).abs() < 1e-6,
            "{mid:?}"
        );
        // Travel starts straight up (-90°) and is horizontal at the top.
        assert!((path.tangent_angle(0.0) + PI / 2.0).abs() < 1e-9);
        assert!(path.tangent_angle(path.length / 2.0).abs() < 1e-6);
        // A quarter of the way is at 45° on the circle, to quad accuracy.
        let q = path.point_at(path.length / 4.0);
        let h = 100.0 * (0.5f64).sqrt();
        assert!((q.x + h).abs() < 0.02 && (q.y + h).abs() < 0.02, "{q:?}");
    }

    #[test]
    fn attach_follows_the_path_and_orients() {
        let paths = [GuidePath::from_contour(&semicircle(100.0, 8)).unwrap()];
        let ends = (pt(-100.0, 0.0), pt(100.0, 0.0));
        let anchor = GuideAnchor::TransformationPoint;
        let m = attach(&paths, &[(anchor, ends.0, ends.1)], true).unwrap();
        let (mid, turn) = m.sample(0.5);
        assert!(mid.x.abs() < 1e-6 && (mid.y + 100.0).abs() < 1e-6);
        assert!((turn - PI / 2.0).abs() < 1e-6, "{turn}");
        assert_eq!(m.sample(0.0), (ends.0, 0.0));
        // Travelling the other way turns the other way.
        let back = attach(&paths, &[(anchor, ends.1, ends.0)], true).unwrap();
        assert!((back.sample(0.5).1 + PI / 2.0).abs() < 1e-6);
        // A keyframe 3 px off the path keeps its offset: keyframes are exact.
        let off = attach(&paths, &[(anchor, pt(-103.0, 0.0), ends.1)], false).unwrap();
        let (p0, _) = off.sample(0.0);
        assert!((p0.x + 103.0).abs() < 1e-6 && p0.y.abs() < 1e-6);
        let (p1, _) = off.sample(1.0);
        assert!((p1.x - 100.0).abs() < 1e-6 && p1.y.abs() < 1e-6);
        // Too far from the stroke: not attached.
        assert!(attach(&paths, &[(anchor, pt(-150.0, 0.0), ends.1)], false).is_none());
    }

    #[test]
    fn split_strokes_chain_and_closed_paths_take_the_short_way() {
        let q = |a: (f64, f64), b: (f64, f64)| Quad {
            p0: pt(a.0, a.1),
            c: pt((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0),
            p1: pt(b.0, b.1),
        };
        // Three pieces out of order, one of them drawn backwards.
        let pieces = vec![
            vec![q((10.0, 0.0), (20.0, 0.0))],
            vec![q((10.0, 0.0), (0.0, 0.0))],
            vec![q((20.0, 0.0), (20.0, 10.0))],
        ];
        let chained = chain(pieces);
        assert_eq!(chained.len(), 1);
        let path = GuidePath::from_quads(chained.into_iter().next().unwrap()).unwrap();
        assert!((path.length - 30.0).abs() < 1e-9);
        let (a, b) = (path.point_at(0.0), path.point_at(30.0));
        assert!(a == pt(0.0, 0.0) && b == pt(20.0, 10.0), "{a:?} {b:?}");

        // A 10 px square, closed: from 1 px after the seam to 1 px before it
        // is 2 px across the seam, not 38 px around.
        let sq = [
            (0.0, 0.0),
            (10.0, 0.0),
            (10.0, 10.0),
            (0.0, 10.0),
            (0.0, 0.0),
        ];
        let quads = sq.windows(2).map(|w| q(w[0], w[1])).collect();
        let path = GuidePath::from_quads(quads).unwrap();
        assert!(path.closed);
        let anchor = GuideAnchor::Registration;
        let m = attach(&[path], &[(anchor, pt(1.0, 0.0), pt(0.0, 1.0))], false).unwrap();
        assert!((m.end - m.start + 2.0).abs() < 1e-9 && m.anchor == anchor);
        let (mid, _) = m.sample(0.5);
        assert!(mid.x.abs() < 1e-9 && mid.y.abs() < 1e-9, "{mid:?}");
    }
}
