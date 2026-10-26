//! Resolve ribbon crossings by tracing original cubic edges, then keep the largest contour.
//! Intersection connectivity is explicit: distinct nearby roots must never be merged by position.
use super::*;
use crate::request_receipts::DispatchErrorCode;
use kurbo::{CubicBez, ParamCurve};
#[path = "tween_curve_crossings.rs"]
mod crossings;
use crossings::*;
fn segments(cs: &[CubicBez]) -> Vec<SegOut> {
    cs.iter()
        .enumerate()
        .map(|(i, c)| {
            let previous = &cs[(i + cs.len() - 1) % cs.len()];
            SegOut {
                point: [c.p0.x, c.p0.y],
                handle_in: [previous.p2.x - c.p0.x, previous.p2.y - c.p0.y],
                handle_out: [c.p1.x - c.p0.x, c.p1.y - c.p0.y],
            }
        })
        .collect()
}
fn root(parents: &[usize], mut i: usize) -> usize {
    while parents[i] != i {
        i = parents[i];
    }
    i
}
fn join(parents: &mut [usize], a: usize, b: usize) {
    let a = root(parents, a);
    let b = root(parents, b);
    parents[b] = a;
}
struct Edge {
    curve: CubicBez,
    start: usize,
    end: usize,
}
pub(super) fn cleanup_outline(original: Vec<SegOut>) -> R<Vec<SegOut>> {
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
        return Err(DispatchErrorCode::Unavailable);
    }
    let mut cuts = vec![vec![0., 1.]; curves.len()];
    let mut links = vec![];
    for i in 0..curves.len() {
        if let Some((a, b)) = cubic_self_crossing(curves[i]) {
            cuts[i].extend([a, b]);
            links.push((i, a, i, b));
        }
        for j in i + 1..curves.len() {
            for (t, u) in cubic_crossings(curves[i], curves[j])? {
                let end = |t: f64| t < 1e-6 || t > 1. - 1e-6;
                if end(t) && end(u) && (j == i + 1 || (i == 0 && j + 1 == curves.len())) {
                    continue;
                }
                cuts[i].push(t);
                cuts[j].push(u);
                links.push((i, t, j, u));
            }
        }
    }
    if links.is_empty() {
        return Ok(original);
    }
    let mut offsets = vec![];
    let mut count = 0;
    for cuts in &mut cuts {
        cuts.sort_by(f64::total_cmp);
        cuts.dedup_by(|a, b| (*a - *b).abs() < 1e-8);
        offsets.push(count);
        count += cuts.len();
    }
    let mut parents: Vec<_> = (0..count).collect();
    for i in 0..curves.len() {
        join(
            &mut parents,
            offsets[i] + cuts[i].len() - 1,
            offsets[(i + 1) % curves.len()],
        );
    }
    for (i, t, j, u) in links {
        let at = |i: usize, t: f64| {
            offsets[i]
                + cuts[i]
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| (**a - t).abs().total_cmp(&(**b - t).abs()))
                    .unwrap()
                    .0
        };
        join(&mut parents, at(i, t), at(j, u));
    }
    let mut edges = vec![];
    for (i, c) in curves.iter().enumerate() {
        for (k, pair) in cuts[i].windows(2).enumerate() {
            if pair[1] - pair[0] < 1e-12 {
                continue;
            }
            edges.push(Edge {
                curve: c.subsegment(pair[0]..pair[1]),
                start: root(&parents, offsets[i] + k),
                end: root(&parents, offsets[i] + k + 1),
            });
        }
    }
    let mut used = vec![false; edges.len()];
    let mut loops = vec![];
    let mut starts: Vec<_> = (0..edges.len()).collect();
    starts.sort_by_key(|&i| {
        (
            edges
                .iter()
                .enumerate()
                .any(|(j, e)| i != j && e.start == edges[i].start),
            i,
        )
    });
    for start in starts {
        if used[start] {
            continue;
        }
        let mut index = start;
        let mut contour = vec![];
        loop {
            if used[index] {
                break;
            }
            used[index] = true;
            let edge = &edges[index];
            contour.push(edge.curve);
            if edge.end == edges[start].start {
                break;
            }
            // Paper's resolveCrossings traces every source edge, switching before continuing
            // the same source path. A boundary-only union would incorrectly erase the ring seam.
            let natural = (index + 1) % edges.len();
            let next = edges
                .iter()
                .enumerate()
                .filter(|(i, e)| !used[*i] && e.start == edge.end)
                .min_by_key(|(i, _)| (*i == natural, *i))
                .map(|(i, _)| i);
            let Some(next) = next else {
                return Err(DispatchErrorCode::Unavailable);
            };
            index = next;
        }
        if contour.len() >= 2 {
            loops.push(segments(&contour));
        }
    }
    let mut chosen = loops
        .into_iter()
        .max_by(|a, b| {
            path_of(a, true)
                .area()
                .abs()
                .total_cmp(&path_of(b, true).area().abs())
        })
        .ok_or(DispatchErrorCode::Unavailable)?;
    if path_of(&chosen, true).area() < 0. {
        chosen.reverse();
        for s in &mut chosen {
            std::mem::swap(&mut s.handle_in, &mut s.handle_out);
        }
    }
    Ok(chosen)
}
