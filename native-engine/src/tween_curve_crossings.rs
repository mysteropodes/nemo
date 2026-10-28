//! Bounded cubic/cubic intersections for pressure-outline crossing cleanup.
use crate::object_document::pressure_geometry::outline_cleanup::*;
use kurbo::{CubicBez, ParamCurve, ParamCurveDeriv};
fn bounds(c: &CubicBez) -> Rect {
    let mut r = Rect::from_points(c.p0, c.p3);
    r = r.union_pt(c.p1);
    r.union_pt(c.p2)
}
fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 <= b.x1 + 1e-10 && b.x0 <= a.x1 + 1e-10 && a.y0 <= b.y1 + 1e-10 && b.y0 <= a.y1 + 1e-10
}
fn cross(a: kurbo::Vec2, b: kurbo::Vec2) -> f64 {
    a.x * b.y - a.y * b.x
}
fn line(c: &CubicBez) -> bool {
    let v = c.p3 - c.p0;
    let scale = v.hypot().max(1.);
    cross(c.p1 - c.p0, v).abs() < 1e-10 * scale && cross(c.p2 - c.p0, v).abs() < 1e-10 * scale
}
#[path = "tween_curve_clip.rs"]
pub(crate) mod clip;
pub(crate) fn cubic_crossings(a: CubicBez, b: CubicBez) -> R<Vec<(f64, f64)>> {
    if !overlaps(bounds(&a), bounds(&b)) {
        return Ok(vec![]);
    }
    if line(&a) && line(&b) && cross(a.p3 - a.p0, b.p3 - b.p0).abs() < 1e-12 {
        let mut endpoints = vec![];
        for (t, p) in [(0., a.p0), (1., a.p3)] {
            for (u, q) in [(0., b.p0), (1., b.p3)] {
                if p.distance(q) <= 1e-12 {
                    endpoints.push((t, u));
                }
            }
        }
        return Ok(endpoints);
    }
    clip::paper_crossings(a, b)
}
pub(crate) fn cubic_self_crossing(c: CubicBez) -> Option<(f64, f64)> {
    let a = (c.p3 - c.p0) - 3. * (c.p2 - c.p1);
    let b = 3. * ((c.p2 - c.p1) - (c.p1 - c.p0));
    let d = 3. * (c.p1 - c.p0);
    let den = cross(a, b);
    if den.abs() < 1e-14 {
        return None;
    }
    let sum = -cross(a, d) / den;
    let product = sum * sum - cross(b, d) / den;
    let disc = sum * sum - 4. * product;
    if disc <= 0. {
        return None;
    }
    let t = (sum - disc.sqrt()) / 2.;
    let u = (sum + disc.sqrt()) / 2.;
    (t > 1e-8 && u < 1. - 1e-8 && u - t > 1e-7).then_some((t, u))
}
