//! Closed-contour preparation from Paper.js 0.12.18 Path.reduce / reorientPaths.
//! Contour order is observable: the pressure ring caller takes the first hole.
//! Copyright (c) 2011–2020 Jürg Lehni & Jonathan Puckey. MIT (tween_paper_license.txt).
use crate::object_document::pressure_geometry::*;
use kurbo::{
    common::{solve_cubic, solve_quadratic},
    CubicBez, ParamCurve, Point,
};

fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
fn dot(a: [f64; 2], b: [f64; 2]) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}
fn zero(a: [f64; 2]) -> bool {
    a[0].abs() <= 1e-12 && a[1].abs() <= 1e-12
}
fn collinear(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] * b[1] - a[1] * b[0]).abs() <= 1e-8 * (dot(a, a) * dot(b, b)).sqrt()
}
fn straight(a: &SegOut, b: &SegOut) -> bool {
    let (h, k) = (a.handle_out, b.handle_in);
    if zero(h) && zero(k) {
        return true;
    }
    let d = sub(b.point, a.point);
    if zero(d) || !collinear(d, h) || !collinear(d, k) {
        return false;
    }
    let length = dot(d, d).sqrt();
    let distance = |v: [f64; 2]| (d[0] * v[1] - d[1] * v[0]).abs() / length;
    let (u, v) = (dot(d, h) / dot(d, d), dot(d, k) / dot(d, d));
    distance(h) < 1e-7 && distance(k) < 1e-7 && (0. ..=1.).contains(&u) && (-1. ..=0.).contains(&v)
}

/// Paper removes the curve's second segment, carrying its outgoing handle to
/// the first. Process the original curves backwards, not to a fixed point.
pub(super) fn reduce(segments: &mut Vec<SegOut>) {
    let mut ids: Vec<usize> = (0..segments.len()).collect();
    for id in (0..segments.len()).rev() {
        if segments.len() < 2 {
            break;
        }
        let Some(i) = ids.iter().position(|&n| n == id) else {
            continue;
        };
        let j = (i + 1) % segments.len();
        let k = (j + 1) % segments.len();
        if !zero(segments[i].handle_out) || !zero(segments[j].handle_in) {
            continue;
        }
        let short = dot(
            sub(segments[j].point, segments[i].point),
            sub(segments[j].point, segments[i].point),
        )
        .sqrt()
            <= 1e-7;
        let aligned = straight(&segments[j], &segments[k])
            && collinear(
                sub(segments[j].point, segments[i].point),
                sub(segments[k].point, segments[j].point),
            );
        if short || aligned {
            segments[i].handle_out = segments[j].handle_out;
            segments.remove(j);
            ids.remove(j);
        }
    }
}

fn reverse(segments: &mut [SegOut]) {
    segments.reverse();
    for segment in segments {
        std::mem::swap(&mut segment.handle_in, &mut segment.handle_out);
    }
}
fn curves(path: &BezPath) -> Vec<CubicBez> {
    path.segments()
        .filter_map(|segment| match segment {
            PathSeg::Cubic(c) => Some(c),
            _ => None, // path_of emits cubic segments, including the closing edge.
        })
        .collect()
}
fn contains(path: &BezPath, point: Point) -> bool {
    path.winding(point) != 0
}

/// Paper starts at the exact curve-bounds center. If outside, intersect its
/// horizontal scanline with y-monotone cubic pieces and take the first span.
fn interior(path: &BezPath) -> Point {
    let mut point = path.bounding_box().center();
    if contains(path, point) {
        return point;
    }
    let mut xs = Vec::new();
    for c in curves(path) {
        let [y0, y1, y2, y3] = [c.p0.y, c.p1.y, c.p2.y, c.p3.y];
        let a = -y0 + 3. * y1 - 3. * y2 + y3;
        let b = 3. * y0 - 6. * y1 + 3. * y2;
        let d = -3. * y0 + 3. * y1;
        let mut cuts = vec![0., 1.];
        cuts.extend(
            solve_quadratic(d, 2. * b, 3. * a)
                .into_iter()
                .filter(|t| *t > 0. && *t < 1.),
        );
        cuts.sort_by(f64::total_cmp);
        let roots = solve_cubic(y0 - point.y, d, b, a);
        for pair in cuts.windows(2) {
            let (lo, hi) = (c.eval(pair[0]), c.eval(pair[1]));
            if lo.y == hi.y || point.y < lo.y.min(hi.y) || point.y > lo.y.max(hi.y) {
                continue;
            }
            let x = if point.y == lo.y {
                lo.x
            } else if point.y == hi.y {
                hi.x
            } else {
                let matches: Vec<_> = roots
                    .iter()
                    .copied()
                    .filter(|t| *t >= pair[0] && *t <= pair[1])
                    .collect();
                if matches.len() == 1 {
                    c.eval(matches[0]).x
                } else {
                    (lo.x + hi.x) / 2.
                }
            };
            xs.push(x);
        }
    }
    if xs.len() > 1 {
        xs.sort_by(f64::total_cmp);
        point.x = (xs[0] + xs[1]) / 2.;
    }
    point
}

/// Inputs are already self-resolved. With `subtract`, operand two must already
/// have been reversed by the caller: retain winding == 1 instead of nonzero.
/// Sorting is only for containment analysis; output keeps original array order.
pub(super) fn reorient(mut contours: Vec<Vec<SegOut>>, subtract: bool) -> Vec<Vec<SegOut>> {
    if contours.is_empty() {
        return contours;
    }
    let paths: Vec<_> = contours.iter().map(|p| path_of(p, true)).collect();
    let areas: Vec<_> = contours.iter().map(|p| outline_area(p)).collect();
    let mut order: Vec<_> = (0..contours.len()).collect();
    order.sort_by(|&a, &b| areas[b].abs().total_cmp(&areas[a].abs()));
    let clockwise = if subtract {
        areas[order[0]] >= 0.
    } else {
        true
    };
    let mut windings: Vec<i32> = areas
        .iter()
        .map(|a| if *a >= 0. { 1 } else { -1 })
        .collect();
    let mut containers: Vec<Option<usize>> = vec![None; contours.len()];
    let mut excluded = vec![false; contours.len()];
    let mut directions: Vec<_> = areas.iter().map(|a| *a >= 0.).collect();
    let inside = |w: i32| if subtract { w == 1 } else { w != 0 };
    for (position, &index) in order.iter().enumerate() {
        let mut container_winding = 0;
        let mut point = None;
        // Paper examines potential containing paths smallest-first among the
        // already processed larger contours, finding the immediate container.
        for &candidate in order[..position].iter().rev() {
            let bounds = paths[candidate].bounding_box();
            let other = paths[index].bounding_box();
            if bounds.x1 < other.x0 - 1e-7
                || other.x1 < bounds.x0 - 1e-7
                || bounds.y1 < other.y0 - 1e-7
                || other.y1 < bounds.y0 - 1e-7
            {
                continue;
            }
            let point = *point.get_or_insert_with(|| interior(&paths[index]));
            if contains(&paths[candidate], point) {
                container_winding = windings[candidate];
                windings[index] += container_winding;
                containers[index] = if excluded[candidate] {
                    containers[candidate]
                } else {
                    Some(candidate)
                };
                break;
            }
        }
        if inside(windings[index]) == inside(container_winding) {
            excluded[index] = true;
        } else {
            let desired = containers[index].map_or(clockwise, |parent| !directions[parent]);
            if directions[index] != desired {
                reverse(&mut contours[index]);
                directions[index] = desired;
            }
        }
    }
    contours
        .into_iter()
        .enumerate()
        .filter_map(|(i, contour)| (!excluded[i]).then_some(contour))
        .collect()
}
