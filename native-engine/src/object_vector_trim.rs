//! #2361: the baseline Trim Stroke Ends (`trimStrokeEnds`, tools.js @1065e9d8) for a freshly drawn open
//! stroke. Its dangling head/tail is cut back to the first/last crossing with the other paths of its layer
//! and frame when that overshoot is at most `max`. Symmetry copies receive the same arc-length cuts: the
//! baseline trims before its copies are made, and a mirror or rotation preserves arc length.
//!
//! Numerics follow the baseline's Paper.js steps (curve lengths, earliest/latest crossing offset, tail split
//! first, de Casteljau division keeping both divided handles on the split anchor) within floating-point
//! tolerance; they are not bit-identical to Paper's own integrators.
use crate::object_document::{ObjectPoint, ObjectRecord, ObjectSegment, PressureFields};
use kurbo::{CubicBez, ParamCurve, ParamCurveArclen, ParamCurveExtrema, Point, Rect, Vec2};
use serde_json::Number;

const ACCURACY: f64 = 1e-9;
/// Intersection search budget per curve pair; a degenerate overlap stops here instead of exploding.
const NODE_BUDGET: u32 = 1 << 14;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Anchor {
    p: Point,
    handle_in: Vec2,
    handle_out: Vec2,
}
fn number(n: &Number) -> Option<f64> {
    n.as_f64().filter(|v| v.is_finite())
}
fn anchor(point: &ObjectPoint, handle_in: &ObjectPoint, handle_out: &ObjectPoint) -> Option<Anchor> {
    Some(Anchor {
        p: Point::new(number(&point.x)?, number(&point.y)?),
        handle_in: Vec2::new(number(&handle_in.x)?, number(&handle_in.y)?),
        handle_out: Vec2::new(number(&handle_out.x)?, number(&handle_out.y)?),
    })
}
fn anchors(segments: &[ObjectSegment]) -> Option<Vec<Anchor>> {
    segments.iter().map(|s| anchor(&s.point, &s.handle_in, &s.handle_out)).collect()
}
/// A vector-brush record: the baseline `isVectorBrush` is the presence of its pressure fields.
fn pressure(record: &ObjectRecord) -> Option<&PressureFields> {
    record.vector_ext.value().and_then(|ext| ext.pressure.value())
}
pub(super) fn is_vector_brush(record: &ObjectRecord) -> bool {
    pressure(record).is_some()
}
/// The baseline wall filter (`layer.children` of the frame): an uncompounded path of two or more anchors that
/// strokes, fills or is a vector brush, excluding a linked-fill companion. `fillWallPath` reads a vector
/// brush's centreline (two or more centre segments) instead of its outline.
fn wall(record: &ObjectRecord) -> Option<(Vec<Anchor>, bool)> {
    let ext = record.vector_ext.value();
    if record.geometry.kind != "cubic-path"
        || record.geometry.segments.len() < 2
        || ext.is_some_and(|e| e.is_linked_fill_companion.value() == Some(&true))
    {
        return None;
    }
    let stroke = record.vector.value().is_some_and(|v| v.stroke.value().is_some_and(Option::is_some));
    let fill = record.fill.value().is_some_and(Option::is_some);
    let brush = pressure(record);
    if !stroke && !fill && brush.is_none() {
        return None;
    }
    let centre = brush.and_then(|p| p.center_segments.value()).filter(|c| c.len() > 1);
    let points = match centre {
        Some(centre) => centre.iter().map(|c| anchor(&c.point, &c.handle_in, &c.handle_out)).collect::<Option<Vec<_>>>()?,
        None => anchors(&record.geometry.segments)?,
    };
    // A centreline is rebuilt as an open path; an outline keeps its own closure.
    let closed = centre.is_none() && record.geometry.closed;
    (points.len() >= 2).then_some((points, closed))
}
fn curve(a: &Anchor, b: &Anchor) -> CubicBez {
    CubicBez::new(a.p, a.p + a.handle_out, b.p + b.handle_in, b.p)
}
/// Paper's `curves`: one per consecutive anchor pair, plus the closing curve of a closed path.
fn curves(anchors: &[Anchor], closed: bool) -> Vec<CubicBez> {
    let mut out: Vec<CubicBez> = anchors.windows(2).map(|w| curve(&w[0], &w[1])).collect();
    if closed && anchors.len() > 1 {
        out.push(curve(&anchors[anchors.len() - 1], &anchors[0]));
    }
    out
}
fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}
fn halves(c: &CubicBez) -> (CubicBez, CubicBez) {
    divide(c, 0.5)
}
/// De Casteljau division, the arithmetic of Paper's `Curve.subdivide`.
fn divide(c: &CubicBez, t: f64) -> (CubicBez, CubicBez) {
    let lerp = |a: Point, b: Point| a.lerp(b, t);
    let (p01, p12, p23) = (lerp(c.p0, c.p1), lerp(c.p1, c.p2), lerp(c.p2, c.p3));
    let (p012, p123) = (lerp(p01, p12), lerp(p12, p23));
    let mid = lerp(p012, p123);
    (
        CubicBez::new(c.p0, p01, p012, mid),
        CubicBez::new(mid, p123, p23, c.p3),
    )
}
/// Parameters on `a` where it meets `b`, found by bounding-box subdivision to sub-1e-9 boxes.
fn crossings(a: CubicBez, range: (f64, f64), b: CubicBez, budget: &mut u32, out: &mut Vec<f64>) {
    if *budget == 0 {
        return;
    }
    *budget -= 1;
    let (ra, rb) = (a.bounding_box(), b.bounding_box());
    if !overlaps(ra, rb) {
        return;
    }
    let size = |r: Rect| r.width().max(r.height());
    if size(ra) < ACCURACY && size(rb) < ACCURACY {
        out.push((range.0 + range.1) / 2.);
        return;
    }
    let mid = (range.0 + range.1) / 2.;
    let (a0, a1) = halves(&a);
    if size(rb) < ACCURACY {
        crossings(a0, (range.0, mid), b, budget, out);
        crossings(a1, (mid, range.1), b, budget, out);
        return;
    }
    let (b0, b1) = halves(&b);
    for (part, r) in [(a0, (range.0, mid)), (a1, (mid, range.1))] {
        crossings(part, r, b0, budget, out);
        crossings(part, r, b1, budget, out);
    }
}
fn lengths(curves: &[CubicBez]) -> Vec<f64> {
    curves.iter().map(|c| c.arclen(ACCURACY)).collect()
}
/// Paper's `getLocationAt(offset)`: the curve holding the offset and the time at that length.
fn location(curves: &[CubicBez], lengths: &[f64], offset: f64) -> Option<(usize, f64)> {
    let mut start = 0.;
    for (i, (c, len)) in curves.iter().zip(lengths).enumerate() {
        if offset <= start + len || i + 1 == curves.len() {
            let along = (offset - start).clamp(0., *len);
            return Some((i, if *len > 0. { c.inv_arclen(along, ACCURACY) } else { 0. }));
        }
        start += len;
    }
    None
}
/// Paper's `splitAt` on an open anchor list: divides curve `i` at `t` (inserting the anchor) and
/// returns the split anchor's index. Times within 1e-8 of an end reuse the existing anchor.
fn split(anchors: &mut Vec<Anchor>, i: usize, t: f64) -> usize {
    if t > 1. - 1e-8 {
        return i + 1;
    }
    if t < 1e-8 {
        return i;
    }
    let (left, right) = divide(&curve(&anchors[i], &anchors[i + 1]), t);
    // Paper's `divideAtTime` writes divided handles only when the curve has handles (`hasHandles`).
    let zero = |v: Vec2| v.x.abs() <= 1e-12 && v.y.abs() <= 1e-12;
    let handles = !zero(anchors[i].handle_out) || !zero(anchors[i + 1].handle_in);
    let (handle_in, handle_out) = if handles {
        anchors[i].handle_out = left.p1 - left.p0;
        anchors[i + 1].handle_in = right.p2 - right.p3;
        (left.p2 - left.p3, right.p1 - right.p0)
    } else {
        (Vec2::ZERO, Vec2::ZERO)
    };
    anchors.insert(i + 1, Anchor { p: left.p3, handle_in, handle_out });
    i + 1
}
/// Head/tail overshoot to remove from `record`, measured against `walls`, or `None` when nothing is cut.
pub(super) fn cuts<'a>(record: &ObjectRecord, walls: impl Iterator<Item = &'a ObjectRecord>, max: f64) -> Option<(f64, f64)> {
    if !(max > 0.) || is_vector_brush(record) || record.geometry.closed || record.geometry.segments.len() < 2 {
        return None;
    }
    let work = curves(&anchors(&record.geometry.segments)?, false);
    let lens = lengths(&work);
    let total: f64 = lens.iter().sum();
    if !(total > 0.) {
        return None;
    }
    let mut first: Option<f64> = None;
    let mut last: Option<f64> = None;
    for record in walls {
        let Some((points, closed)) = wall(record) else { continue };
        for other in curves(&points, closed) {
            let mut start = 0.;
            for (c, len) in work.iter().zip(&lens) {
                let (mut hits, mut budget) = (Vec::new(), NODE_BUDGET);
                crossings(*c, (0., 1.), other, &mut budget, &mut hits);
                for t in hits {
                    let offset = start + c.subsegment(0.0..t).arclen(ACCURACY);
                    first = Some(first.map_or(offset, |f: f64| f.min(offset)));
                    last = Some(last.map_or(offset, |l: f64| l.max(offset)));
                }
                start += len;
            }
        }
    }
    let (head, tail) = (first?, total - last?);
    let cut_head = head > 0.01 && head <= max;
    let cut_tail = tail > 0.01 && tail <= max;
    if !cut_head && !cut_tail {
        return None;
    }
    let (head, tail) = (if cut_head { head } else { 0. }, if cut_tail { tail } else { 0. });
    // Guard against eating the whole stroke when every crossing sits at one end.
    (total - head - tail >= (max * 0.25).max(1.)).then_some((head, tail))
}
/// Applies a head/tail cut to one record's open geometry. `None` leaves fewer than two anchors.
pub(super) fn apply(record: &ObjectRecord, (head, tail): (f64, f64)) -> Option<Vec<ObjectSegment>> {
    let mut points = anchors(&record.geometry.segments)?;
    if record.geometry.closed || points.len() < 2 {
        return None;
    }
    let total: f64 = lengths(&curves(&points, false)).iter().sum();
    // Tail first: cutting the head would shift every offset measured from the start.
    if tail > 0. {
        let work = curves(&points, false);
        let (i, t) = location(&work, &lengths(&work), total - tail)?;
        let at = split(&mut points, i, t);
        points.truncate(at + 1);
    }
    if head > 0. {
        let work = curves(&points, false);
        let (i, t) = location(&work, &lengths(&work), head)?;
        let at = split(&mut points, i, t);
        points.drain(..at);
    }
    if points.len() < 2 {
        return None;
    }
    let n = |v: f64| Number::from_f64(v);
    points
        .iter()
        .map(|a| {
            let point = |x: f64, y: f64| Some(ObjectPoint { x: n(x)?, y: n(y)? });
            Some(ObjectSegment {
                point: point(a.p.x, a.p.y)?,
                handle_in: point(a.handle_in.x, a.handle_in.y)?,
                handle_out: point(a.handle_out.x, a.handle_out.y)?,
            })
        })
        .collect()
}
