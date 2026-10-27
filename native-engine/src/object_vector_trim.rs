//! #2361: the baseline Trim Stroke Ends (`trimStrokeEnds`, tools.js @1065e9d8) for a freshly drawn open
//! stroke. Its dangling head/tail is cut back to the first/last crossing with the other paths of its layer
//! and frame when that overshoot is at most `max`. Symmetry copies receive the same arc-length cuts: the
//! baseline trims before its copies are made, and a mirror or rotation preserves arc length.
//!
//! Numerics follow the baseline's Paper.js steps (curve lengths, earliest/latest crossing offset, tail split
//! first, de Casteljau division keeping both divided handles on the split anchor) within floating-point
//! tolerance; they are not bit-identical to Paper's own integrators.
use crate::object_document::{ObjectPoint, ObjectRecord, ObjectSegment, PressureFields};
#[cfg(feature = "application")]
use crate::object_document::{CenterSegment, Presence, WidthStop};
use crate::request_receipts::DispatchErrorCode;
// The ribbon rebuild is the tween outline kernel, which only the application build carries.
#[cfg(feature = "application")]
use crate::tween_kernels::{profiled_outline, width_at_frac, SegOut};
use kurbo::{CubicBez, ParamCurve, ParamCurveArclen, ParamCurveExtrema, Point, Rect, Vec2};
use serde_json::Number;

const ACCURACY: f64 = 1e-9;
/// Intersection search budget per curve pair, and for one whole trim: a degenerate overlap (tracing over an
/// existing stroke) stops at these bounds instead of exploding, and an exhausted search trims nothing.
const NODE_BUDGET: u32 = 1 << 14;
const TRIM_BUDGET: u32 = 1 << 21;

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
/// strokes, fills or is a vector brush, excluding a linked-fill companion and a brush-texture dab. `fillWallPath` reads a vector
/// brush's centreline (two or more centre segments) instead of its outline.
fn wall(record: &ObjectRecord) -> Option<(Vec<Anchor>, bool)> {
    let ext = record.vector_ext.value();
    if record.geometry.kind != "cubic-path"
        || record.geometry.segments.len() < 2
        || ext.is_some_and(|e| {
            e.is_linked_fill_companion.value() == Some(&true)
                || e.brush_texture.value().is_some_and(|t| t.is_brush_texture_copy.value() == Some(&true))
        })
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
    if is_vector_brush(record) || record.geometry.closed || record.geometry.segments.len() < 2 {
        return None;
    }
    cuts_on(&anchors(&record.geometry.segments)?, walls, max)
}
/// The cut on one open run of anchors: a stroke's own, or a pressure ribbon's centreline (`fillWallPath`).
fn cuts_on<'a>(points: &[Anchor], walls: impl Iterator<Item = &'a ObjectRecord>, max: f64) -> Option<(f64, f64)> {
    if !(max > 0.) || points.len() < 2 {
        return None;
    }
    let work = curves(points, false);
    let lens = lengths(&work);
    let total: f64 = lens.iter().sum();
    if !(total > 0.) {
        return None;
    }
    let mut first: Option<f64> = None;
    let mut last: Option<f64> = None;
    let mut spent: u32 = 0;
    let reach = work.iter().map(ParamCurveExtrema::bounding_box).reduce(|a, b| a.union(b))?;
    for record in walls {
        let Some((points, closed)) = wall(record) else { continue };
        for other in curves(&points, closed) {
            if !overlaps(reach, other.bounding_box()) {
                continue;
            }
            let mut start = 0.;
            for (c, len) in work.iter().zip(&lens) {
                let allowance = TRIM_BUDGET.saturating_sub(spent).min(NODE_BUDGET);
                let (mut hits, mut budget) = (Vec::new(), allowance);
                crossings(*c, (0., 1.), other, &mut budget, &mut hits);
                spent += allowance - budget;
                // An exhausted search may have missed the latest crossing: trim nothing rather than guess.
                if budget == 0 {
                    return None;
                }
                // Only this pair's earliest and latest crossings can move the stroke's first/last offset.
                let lo = hits.iter().copied().reduce(f64::min);
                let hi = hits.iter().copied().reduce(f64::max);
                for t in lo.into_iter().chain(hi) {
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
pub(super) fn apply(record: &ObjectRecord, cut: (f64, f64)) -> Option<Vec<ObjectSegment>> {
    if record.geometry.closed {
        return None;
    }
    segments_of(&cut_anchors(anchors(&record.geometry.segments)?, cut)?)
}
fn cut_anchors(mut points: Vec<Anchor>, (head, tail): (f64, f64)) -> Option<Vec<Anchor>> {
    if points.len() < 2 {
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
    (points.len() >= 2).then_some(points)
}
fn segments_of(points: &[Anchor]) -> Option<Vec<ObjectSegment>> {
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

#[cfg(feature = "application")]
fn linked(fill: &ObjectRecord, ribbon: &ObjectRecord) -> bool {
    let (f, r) = (fill.vector_ext.value(), ribbon.vector_ext.value());
    f.is_some_and(|f| f.is_linked_fill_companion.value() == Some(&true))
        && f.and_then(|f| f.linked_fill_id.value()).is_some_and(|id| r.and_then(|r| r.linked_fill_id.value()) == Some(id))
}
/// Trim Stroke Ends on a pressure ribbon (tools.js trimStrokeEnds isVB @1065e9d8). The cut is measured on the drawn
/// ribbon's centreline; each kept anchor's width is the old profile's at its chord position in the untrimmed length (the
/// stroke width when the ribbon has no profile), the profile is rebuilt from those widths (buildWidthProfile), and
/// rebuildVectorBrushOutline rebuilds the outline, whose fitted centreline becomes the linked fill's outline.
/// The baseline trims before symmetry-bridge copies the ribbon and before multiframe-draw stamps it, and a mirror or a
/// rotation keeps arc length: every other cell of the create (a copy or a stamp, each with its linked fill, in either
/// stack order) takes the same cut on its own centreline. adoptFreshIdentity rebuilds a copy's outline only when it has
/// a linked fill; one without keeps the drawn ribbon's (trimmed) outline carried by the copy's own map (`carried`).
#[cfg(feature = "application")]
pub(super) fn ribbon<'a>(records: &mut [ObjectRecord], walls: impl Iterator<Item = &'a ObjectRecord>, max: f64) -> Result<(), DispatchErrorCode> {
    let mut widths = Vec::new();
    let mut i = 0;
    while i < records.len() {
        let pair = records.get(i + 1).is_some_and(|b| linked(&records[i], b) || linked(b, &records[i]));
        let width = if pair { 2 } else { 1 };
        widths.push(width);
        i += width;
    }
    let lead = match widths.first() {
        Some(2) if !is_vector_brush(&records[0]) => &records[1],
        Some(_) => &records[0],
        None => return Err(DispatchErrorCode::InvalidRequest),
    };
    let Some(cut) = cuts_on(&centre_of(lead)?, walls, max) else { return Ok(()) };
    let outline = |r: &ObjectRecord| anchors(&r.geometry.segments).ok_or(DispatchErrorCode::InvalidRequest);
    let ribbon_of = |cell: &[ObjectRecord]| if cell.len() == 2 && !is_vector_brush(&cell[0]) { 1 } else { 0 };
    let lead_before = outline(lead)?;
    let mut lead_after = None;
    let mut rest: &mut [ObjectRecord] = records;
    for width in widths {
        let (cell, tail) = std::mem::take(&mut rest).split_at_mut(width);
        let at = ribbon_of(cell);
        let before = outline(&cell[at])?;
        trim_cell(cell, cut)?;
        match &lead_after {
            None => lead_after = Some(outline(&cell[at])?),
            Some(after) if width == 1 => {
                let mapped = carried(&lead_before, &before, after).ok_or(DispatchErrorCode::Unavailable)?;
                cell[at].geometry.segments = segments_of(&mapped).ok_or(DispatchErrorCode::InvalidRequest)?;
            }
            Some(_) => {}
        }
        rest = tail;
    }
    Ok(())
}
/// The affine that carried the drawn ribbon's untrimmed outline onto a copy's (a mirror's copy reversed, as
/// `mirrored.reverse()` re-orients it), applied to the drawn ribbon's trimmed outline. `None` when no map fits.
#[cfg(feature = "application")]
fn carried(before: &[Anchor], copy: &[Anchor], after: &[Anchor]) -> Option<Vec<Anchor>> {
    let n = before.len();
    if n < 3 || copy.len() != n {
        return None;
    }
    for reversed in [false, true] {
        let at = |i: usize| if reversed { copy[n - 1 - i].p } else { copy[i].p };
        let (p0, p1, p2) = (before[0].p, before[n / 3].p, before[2 * n / 3].p);
        let (u, v, du, dv) = (p1 - p0, p2 - p0, at(n / 3) - at(0), at(2 * n / 3) - at(0));
        let det = u.x * v.y - u.y * v.x;
        if det.abs() < 1e-9 {
            continue;
        }
        // x' = a x + c y + e, y' = b x + d y + f: the linear part maps u, v onto du, dv.
        let (a, c) = ((du.x * v.y - dv.x * u.y) / det, (dv.x * u.x - du.x * v.x) / det);
        let (b, d) = ((du.y * v.y - dv.y * u.y) / det, (dv.y * u.x - du.y * v.x) / det);
        let (e, f) = (at(0).x - (a * p0.x + c * p0.y), at(0).y - (b * p0.x + d * p0.y));
        let point = |p: Point| Point::new(a * p.x + c * p.y + e, b * p.x + d * p.y + f);
        let vector = |h: Vec2| Vec2::new(a * h.x + c * h.y, b * h.x + d * h.y);
        if !(0..n).all(|i| point(before[i].p).distance(at(i)) <= 1e-6 * (1.0 + at(i).to_vec2().hypot())) {
            continue;
        }
        let mut mapped: Vec<Anchor> =
            after.iter().map(|s| Anchor { p: point(s.p), handle_in: vector(s.handle_in), handle_out: vector(s.handle_out) }).collect();
        if reversed {
            mapped.reverse();
            for s in &mut mapped {
                std::mem::swap(&mut s.handle_in, &mut s.handle_out);
            }
        }
        return Some(mapped);
    }
    None
}
#[cfg(feature = "application")]
fn centre_of(ribbon: &ObjectRecord) -> Result<Vec<Anchor>, DispatchErrorCode> {
    let centre = pressure(ribbon).and_then(|p| p.center_segments.value()).filter(|c| c.len() > 1).ok_or(DispatchErrorCode::Unavailable)?;
    centre.iter().map(|c| anchor(&c.point, &c.handle_in, &c.handle_out)).collect::<Option<_>>().ok_or(DispatchErrorCode::InvalidRequest)
}
#[cfg(feature = "application")]
fn trim_cell(records: &mut [ObjectRecord], cut: (f64, f64)) -> Result<(), DispatchErrorCode> {
    let (fill, ribbon) = match records {
        [ribbon] => (None, ribbon),
        [fill, ribbon] if linked(fill, ribbon) => (Some(fill), ribbon),
        [ribbon, fill] if linked(fill, ribbon) => (Some(fill), ribbon),
        _ => return Err(DispatchErrorCode::Unavailable),
    };
    let points = centre_of(ribbon)?;
    let pressure = pressure(ribbon).ok_or(DispatchErrorCode::Unavailable)?;
    // An empty profile is not the baseline's absent one (widthAtFrac would answer null for every anchor).
    let profile = match pressure.width_profile.value() {
        Some(stops) if stops.is_empty() => return Err(DispatchErrorCode::Unavailable),
        Some(stops) => Some(stops.iter().map(|s| Some((number(&s.t)?, number(&s.width)?))).collect::<Option<Vec<_>>>().ok_or(DispatchErrorCode::InvalidRequest)?),
        None => None,
    };
    let stroke_width = ribbon.vector.value().and_then(|v| number(&v.stroke_width)).ok_or(DispatchErrorCode::InvalidRequest)?;
    let source: f64 = lengths(&curves(&points, false)).iter().sum();
    let Some(kept) = cut_anchors(points, cut) else { return Ok(()) };
    let (mut chord, mut lens, mut widths) = (0., Vec::with_capacity(kept.len()), Vec::with_capacity(kept.len()));
    for (i, a) in kept.iter().enumerate() {
        if i > 0 {
            chord += a.p.distance(kept[i - 1].p);
        }
        lens.push(chord);
        widths.push(match &profile {
            Some(p) => width_at_frac(p, (cut.0 + chord) / source).ok_or(DispatchErrorCode::InvalidRequest)?,
            None => stroke_width,
        });
    }
    let total = if chord == 0. { 1. } else { chord };
    let rebuilt: Vec<(f64, f64)> = lens.iter().zip(&widths).map(|(l, w)| (l / total, *w)).collect();
    let center: Vec<SegOut> = kept
        .iter()
        .map(|a| SegOut { point: [a.p.x, a.p.y], handle_in: [a.handle_in.x, a.handle_in.y], handle_out: [a.handle_out.x, a.handle_out.y] })
        .collect();
    let outline = profiled_outline(&center, &rebuilt)?;
    if outline.len() > crate::object_document::object_group_kernels::object_boolean_ops::stack::MAX_SEGMENTS {
        return Err(DispatchErrorCode::Unavailable);
    }
    let anchors_out: Vec<Anchor> = outline
        .iter()
        .map(|s| Anchor { p: Point::new(s.point[0], s.point[1]), handle_in: Vec2::new(s.handle_in[0], s.handle_in[1]), handle_out: Vec2::new(s.handle_out[0], s.handle_out[1]) })
        .collect();
    let outline = segments_of(&anchors_out).ok_or(DispatchErrorCode::InvalidRequest)?;
    let kept_segments = segments_of(&kept).ok_or(DispatchErrorCode::InvalidRequest)?;
    let n = |v: f64| Number::from_f64(v).ok_or(DispatchErrorCode::InvalidRequest);
    let centre_out = kept_segments
        .iter()
        .zip(&widths)
        .map(|(s, &w)| Ok(CenterSegment { point: s.point.clone(), handle_in: s.handle_in.clone(), handle_out: s.handle_out.clone(), width: n(w)? }))
        .collect::<Result<Vec<_>, DispatchErrorCode>>()?;
    let profile_out = rebuilt.iter().map(|&(t, w)| Ok(WidthStop { t: n(t)?, width: n(w)? })).collect::<Result<Vec<_>, DispatchErrorCode>>()?;
    if let Presence::Present(ext) = &mut ribbon.vector_ext {
        if let Presence::Present(p) = &mut ext.pressure {
            p.center_segments = Presence::Present(centre_out);
            p.width_profile = Presence::Present(profile_out);
        }
    }
    ribbon.geometry.segments = outline;
    ribbon.geometry.closed = true;
    // applyBrushKeyline: a pressure ribbon never keeps a stroke of its own.
    if let Presence::Present(v) = &mut ribbon.vector {
        if v.stroke.value().is_some_and(Option::is_some) {
            v.stroke = Presence::Present(None);
            v.has_real_stroke = false;
        }
    }
    if let Some(fill) = fill {
        fill.geometry.segments = kept_segments;
        fill.geometry.closed = true;
    }
    Ok(())
}
/// Without the application build there is no outline kernel to rebuild a trimmed ribbon with.
#[cfg(not(feature = "application"))]
pub(super) fn ribbon<'a>(_: &mut [ObjectRecord], _: impl Iterator<Item = &'a ObjectRecord>, _: f64) -> Result<(), DispatchErrorCode> {
    Err(DispatchErrorCode::Unavailable)
}
