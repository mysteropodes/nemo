//! Resolve ribbon crossings by tracing original cubic edges, then keep the largest contour.
//! Intersection connectivity is explicit: distinct nearby roots must never be merged by position.
use crate::object_document::pressure_geometry::*;
use kurbo::{CubicBez, ParamCurve};
#[path = "tween_curve_crossings.rs"]
pub(super) mod crossings;
use crossings::*;
#[path = "pressure_curve_topology.rs"]
pub(super) mod topology;
pub(super) fn segments(cs: &[CubicBez]) -> Vec<SegOut> {
    cs.iter()
        .enumerate()
        .map(|(i, c)| {
            let previous = &cs[(i + cs.len() - 1) % cs.len()];
            SegOut {
                point: [c.p0.x, c.p0.y],
                handle_in: [previous.p2.x - previous.p3.x, previous.p2.y - previous.p3.y],
                handle_out: [c.p1.x - c.p0.x, c.p1.y - c.p0.y],
            }
        })
        .collect()
}
pub(super) fn resolve_contours(original: Vec<SegOut>, guard: bool) -> R<Vec<Vec<SegOut>>> {
    let curves: Vec<_> = path_of(&original, true)
        .segments()
        .filter_map(|s| {
            if let PathSeg::Cubic(c) = s {
                Some(c)
            } else {
                None
            }
        })
        .collect();
    if curves.len() > 2048 {
        return Err(Error::Unavailable);
    }
    let values = |c: CubicBez| {
        [
            c.p0.x, c.p0.y, c.p1.x, c.p1.y, c.p2.x, c.p2.y, c.p3.x, c.p3.y,
        ]
    };
    let length = |i, t| {
        crate::object_document::property_follow_path_sample::range_length(&values(curves[i]), 0., t)
    };
    let mut offsets = vec![0.];
    for i in 0..curves.len() {
        offsets.push(offsets.last().unwrap() + length(i, 1.));
    }
    let total = *offsets.last().unwrap();
    let same = |a: f64, b: f64| (a - b).abs() < 1e-7 || (total - (a - b).abs()).abs() < 1e-7;
    let mut locations = vec![];
    let mut cuts = vec![vec![0., 1.]; curves.len()];
    let mut links = vec![];
    let mut real_intersection = false;
    for i in 0..curves.len() {
        if let Some((a, b)) = cubic_self_crossing(curves[i]) {
            real_intersection = true;
            cuts[i].extend([a, b]);
            links.push((i, a, i, b));
        }
        for j in i + 1..curves.len() {
            for (t, u) in cubic_crossings(curves[i], curves[j])? {
                let end = |t: f64| t < 1e-6 || t > 1. - 1e-6;
                if end(t) && end(u) && (j == i + 1 || (i == 0 && j + 1 == curves.len())) {
                    // The outer cleanup guard ignores these, but resolveCrossings still classifies them.
                } else {
                    real_intersection = true;
                }
                // addLocation excludes the ordinary shared endpoint of neighboring curves.
                // Test the original curve times before CurveLocation transfers an end to its successor.
                if (j == i + 1 && (t > 1. - 1e-8 || u < 1e-8))
                    || (i == 0 && j + 1 == curves.len() && (t < 1e-8 || u > 1. - 1e-8))
                {
                    continue;
                }
                // Paper CurveLocation hands an end intersection to the next curve at time zero.
                let normalize = |i, t: f64| {
                    if t > 1. - 1e-8 {
                        ((i + 1) % curves.len(), 0.)
                    } else if t < 1e-8 {
                        (i, 0.)
                    } else {
                        (i, t)
                    }
                };
                let (i, t) = normalize(i, t);
                let (j, u) = normalize(j, u);
                if !topology::is_crossing(&curves, i, t, j, u) {
                    continue;
                }
                let (a, b) = (offsets[i] + length(i, t), offsets[j] + length(j, u));
                if locations
                    .iter()
                    .any(|&(x, y)| (same(a, x) && same(b, y)) || (same(a, y) && same(b, x)))
                {
                    continue;
                }
                locations.push((a, b));
                cuts[i].push(t);
                cuts[j].push(u);
                links.push((i, t, j, u));
            }
        }
    }
    if (guard && !real_intersection) || links.is_empty() {
        return Ok(vec![original]);
    }
    let next: Vec<_> = (0..curves.len()).map(|i| (i + 1) % curves.len()).collect();
    let edges =
        crate::object_document::pressure_geometry::graph::split(&curves, &next, cuts, &links);
    crate::object_document::pressure_geometry::graph::trace(&edges, &vec![true; edges.len()])
}
pub(crate) fn cleanup_outline(original: Vec<SegOut>) -> R<Vec<SegOut>> {
    let loops = resolve_contours(original.clone(), true)?;
    // No crossing means the original orientation and start are untouched.
    if loops.len() == 1
        && loops[0].len() == original.len()
        && loops[0].iter().zip(&original).all(|(a, b)| {
            a.point == b.point && a.handle_in == b.handle_in && a.handle_out == b.handle_out
        })
    {
        return Ok(original);
    }
    let mut chosen = loops
        .into_iter()
        .max_by(|a, b| outline_area(a).abs().total_cmp(&outline_area(b).abs()))
        .ok_or(Error::Unavailable)?;
    if outline_area(&chosen) < 0. {
        chosen.reverse();
        for s in &mut chosen {
            std::mem::swap(&mut s.handle_in, &mut s.handle_out);
        }
    }
    Ok(chosen)
}
