//! Cubic subdivision and deterministic contour tracing shared by ribbon cleanup and ring subtraction.
use crate::object_document::pressure_geometry::outline_cleanup::segments;
use crate::object_document::pressure_geometry::*;
use kurbo::CubicBez;
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
pub(super) struct Edge {
    pub curve: CubicBez,
    start: usize,
    end: usize,
    pub source: usize,
    next: usize,
}
pub(super) fn split(
    curves: &[CubicBez],
    next_curve: &[usize],
    mut cuts: Vec<Vec<f64>>,
    links: &[(usize, f64, usize, f64)],
) -> Vec<Edge> {
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
            offsets[next_curve[i]],
        );
    }
    for &(i, t, j, u) in links {
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
    let mut first_edges = vec![];
    for (i, c) in curves.iter().enumerate() {
        first_edges.push(edges.len());
        // divideLocations visits times right-to-left on the same mutable curve.
        let mut remaining = *c;
        let mut previous = 1.;
        let mut pieces = vec![];
        for &t in cuts[i][1..cuts[i].len() - 1].iter().rev() {
            let (left, right) = subdivide(remaining, t / previous);
            pieces.push(right);
            remaining = left;
            previous = t;
        }
        pieces.push(remaining);
        pieces.reverse();
        let clear = c.p0 == c.p1 && c.p2 == c.p3;
        for (k, mut curve) in pieces.into_iter().enumerate() {
            if clear {
                curve.p1 = curve.p0;
                curve.p2 = curve.p3;
            }
            edges.push(Edge {
                curve,
                start: root(&parents, offsets[i] + k),
                end: root(&parents, offsets[i] + k + 1),
                source: i,
                next: 0,
            });
        }
    }
    for i in 0..edges.len() {
        let source = edges[i].source;
        edges[i].next = if i + 1 < edges.len() && edges[i + 1].source == source {
            i + 1
        } else {
            first_edges[next_curve[source]]
        };
    }
    edges
}
pub(super) fn trace(edges: &[Edge], valid: &[bool]) -> R<Vec<Vec<SegOut>>> {
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
        if used[start] || !valid[start] {
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
            let natural = edges[index].next;
            let next = edges
                .iter()
                .enumerate()
                .filter(|(i, e)| !used[*i] && valid[*i] && e.start == edge.end)
                .min_by_key(|(i, _)| (*i == natural, *i))
                .map(|(i, _)| i);
            let Some(next) = next else {
                return Err(Error::Unavailable);
            };
            index = next;
        }
        if contour.len() >= 2 {
            let contour = segments(&contour);
            if outline_area(&contour) != 0. {
                loops.push(contour);
            }
        }
    }
    Ok(loops)
}

fn subdivide(c: CubicBez, t: f64) -> (CubicBez, CubicBez) {
    let values = [
        c.p0.x, c.p0.y, c.p1.x, c.p1.y, c.p2.x, c.p2.y, c.p3.x, c.p3.y,
    ];
    let (left, right) =
        crate::object_document::pressure_geometry::outline_cleanup::crossings::clip::split(
            values, t,
        );
    let curve = |v: [f64; 8]| {
        let p0 = kurbo::Point::new(v[0], v[1]);
        let p3 = kurbo::Point::new(v[6], v[7]);
        // Paper stores relative handles after every division and reads them back
        // for the next split. This rounding matters for almost-touching loops.
        CubicBez::new(
            p0,
            kurbo::Point::new(p0.x + (v[2] - p0.x), p0.y + (v[3] - p0.y)),
            kurbo::Point::new(p3.x + (v[4] - p3.x), p3.y + (v[5] - p3.y)),
            p3,
        )
    };
    (curve(left), curve(right))
}
