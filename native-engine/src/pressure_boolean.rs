//! Cubic ring subtraction for the pressure ribbon producer. Keeps curves and contour order.
//! Paper.js 0.12.18 preparation / subtraction contract; MIT (tween_paper_license.txt).
use crate::object_document::pressure_geometry::outline_cleanup::{
    crossings::cubic_crossings, resolve_contours, topology::is_crossing_between,
};
use crate::object_document::pressure_geometry::*;
use kurbo::{CubicBez, ParamCurve};
fn curves(s: &[SegOut]) -> Vec<CubicBez> {
    path_of(s, true)
        .segments()
        .filter_map(|c| match c {
            PathSeg::Cubic(c) => Some(c),
            _ => None,
        })
        .collect()
}
fn prepare(mut s: Vec<SegOut>) -> R<Vec<Vec<SegOut>>> {
    crate::object_document::pressure_geometry::contours::reduce(&mut s);
    // preparePath applies even an identity matrix through absolute controls.
    for segment in &mut s {
        for h in [&mut segment.handle_in, &mut segment.handle_out] {
            if h[0].abs() > 1e-12 || h[1].abs() > 1e-12 {
                for axis in 0..2 {
                    h[axis] = (segment.point[axis] + h[axis]) - segment.point[axis];
                }
            }
        }
    }
    Ok(
        crate::object_document::pressure_geometry::contours::reorient(
            resolve_contours(s, false)?,
            false,
        ),
    )
}
pub(super) fn subtract(outer: &[SegOut], inner: &[SegOut]) -> R<Vec<Vec<SegOut>>> {
    let a = prepare(outer.to_vec())?;
    let mut b = prepare(inner.to_vec())?;
    for s in &mut b {
        reverse(s);
    }
    let a_paths: Vec<_> = a.iter().map(|s| path_of(s, true)).collect();
    let b_paths: Vec<_> = b.iter().map(|s| path_of(s, true)).collect();
    let split_at = a.len();
    let paths: Vec<_> = a.into_iter().chain(b).collect();
    let sets: Vec<_> = paths.iter().map(|s| curves(s)).collect();
    let mut all = vec![];
    let mut offsets = vec![];
    let mut next = vec![];
    let mut is_inner = vec![];
    for (i, cs) in sets.iter().enumerate() {
        let base = all.len();
        offsets.push(base);
        for j in 0..cs.len() {
            next.push(base + (j + 1) % cs.len());
            is_inner.push(i >= split_at);
        }
        all.extend_from_slice(cs);
    }
    if all.len() > 2048 {
        return Err(Error::Unavailable);
    }
    let mut cuts = vec![vec![0., 1.]; all.len()];
    let mut links: Vec<(usize, f64, usize, f64)> = vec![];
    for p in 0..split_at {
        for q in split_at..sets.len() {
            for i in 0..sets[p].len() {
                for j in 0..sets[q].len() {
                    for (t, u) in cubic_crossings(sets[p][i], sets[q][j])? {
                        let normalize = |i, t: f64, n| {
                            if t > 1. - 1e-8 {
                                ((i + 1) % n, 0.)
                            } else if t < 1e-8 {
                                (i, 0.)
                            } else {
                                (i, t)
                            }
                        };
                        let (i, t) = normalize(i, t, sets[p].len());
                        let (j, u) = normalize(j, u, sets[q].len());
                        if !is_crossing_between(&sets[p], i, t, &sets[q], j, u) {
                            continue;
                        }
                        let (i, j) = (offsets[p] + i, offsets[q] + j);
                        if links.iter().any(|&(x, a, y, b)| {
                            x == i && y == j && (t - a).abs() < 1e-8 && (u - b).abs() < 1e-8
                        }) {
                            continue;
                        }
                        cuts[i].push(t);
                        cuts[j].push(u);
                        links.push((i, t, j, u));
                    }
                }
            }
        }
    }
    if links.is_empty() {
        return Ok(crate::object_document::pressure_geometry::contours::reorient(paths, true));
    }
    let edges = crate::object_document::pressure_geometry::graph::split(&all, &next, cuts, &links);
    let valid: Vec<_> = edges
        .iter()
        .map(|e| {
            let point = e.curve.eval(0.5);
            if is_inner[e.source] {
                a_paths.iter().map(|p| p.winding(point)).sum::<i32>() != 0
            } else {
                b_paths.iter().map(|p| p.winding(point)).sum::<i32>() == 0
            }
        })
        .collect();
    let mut result = crate::object_document::pressure_geometry::graph::trace(&edges, &valid)?;
    for s in &mut result {
        crate::object_document::pressure_geometry::contours::reduce(s);
    }
    Ok(result)
}
pub(super) fn reverse(s: &mut Vec<SegOut>) {
    s.reverse();
    for s in s {
        std::mem::swap(&mut s.handle_in, &mut s.handle_out);
    }
}
