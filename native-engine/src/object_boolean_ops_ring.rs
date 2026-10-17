//! Island rings of a boolean result (`flattenBooleanResult`, `_mergeHoleIntoExterior`
//! and `_eraseDegenerateSelfLoops` of `src/js/tools.js` at fbe4b8a3, G19, L02-I).
//!
//! A provider result is a list of polygons (exterior plus holes). Every island
//! becomes ONE closed ring that existing flat-path consumers understand: a plain
//! result stays as it is, a result with several islands and no hole keeps each
//! exterior (forced clockwise), and every island that owns holes gets them
//! slit into its ring. Single and merged rings then lose their near-coincident
//! revisits and, unless the caller preserves geometry, are refitted with
//! `simplify(2.5)` (module `fit`).
//!
//! Deliberate, reported differences from the baseline: a refit that collapses to a single anchor (storage
//! admits 2..=256) keeps the exact polygon instead, a hole belongs to the
//! polygon the provider returned it with, not to every exterior whose bounds
//! contain it (the baseline cut an island's hole into the donut around it too),
//! and a hole of fewer than three points is a typed refusal (see
//! `admit_polygons`). Faithfully kept baseline quirk: `_polyClosestPair` is fed
//! `{point:[x,y]}` arrays by `flattenBooleanResult`, reads `.x` of an array,
//! compares NaN and therefore always answers `(0, 0)`; the slit therefore joins
//! the first exterior vertex to the first hole vertex.
use crate::object_document::object_group_kernels::object_boolean_ops::{
    simplify_closed, Point, RingSeg,
};
use crate::object_document::object_group_kernels::object_boolean_topology::{
    admit_polygons, IslandBuild, Polygon, TopologyError,
};

/// Fewest anchors of a stored cubic ring (storage admission, `object_codec`: 2..=256).
const MIN_STORED_SEGMENTS: usize = 2;
/// `REVISIT_TOL`: two ring vertices this close are the same junction.
const REVISIT_TOL: f64 = 1.5;
/// `MIN_LOOP_FRAC`: a loop above this share of the bounding box is real geometry.
const MIN_LOOP_FRAC: f64 = 0.002;
/// `path.simplify(2.5)`.
const REFIT_TOLERANCE: f64 = 2.5;

/// One finished island: its ring and whether the refit ran on it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Island {
    pub(crate) build: IslandBuild,
    pub(crate) segments: Vec<RingSeg>,
    pub(crate) refit: bool,
}

/// Paper's `Path#getArea` sign for a straight-edged closed ring (shoelace / 2).
pub(crate) fn signed_area(points: &[Point]) -> f64 {
    let n = points.len();
    let mut sum = 0.0;
    for i in 0..n {
        let p = points[i];
        let q = points[(i + 1) % n];
        sum += p[0] * q[1] - q[0] * p[1];
    }
    sum / 2.0
}

/// `Path#isClockwise`: the area is not negative (a zero-area ring counts).
pub(crate) fn is_clockwise(points: &[Point]) -> bool {
    signed_area(points) >= 0.0
}

/// The `clockwise` setter: a wrong-way ring is reversed end to end.
pub(crate) fn set_clockwise(points: &mut [Point], clockwise: bool) {
    if is_clockwise(points) != clockwise {
        points.reverse();
    }
}

fn plain(points: &[Point]) -> Vec<RingSeg> {
    points
        .iter()
        .map(|&point| RingSeg {
            point,
            handle_in: [0.0, 0.0],
            handle_out: [0.0, 0.0],
        })
        .collect()
}

/// `_polyClosestPair` as `flattenBooleanResult` really calls it (see the module
/// note): the comparison never succeeds, so the answer is the first pair.
fn baseline_closest_pair() -> (usize, usize) {
    (0, 0)
}

/// `_mergeHoleIntoExterior`: the exterior up to the slit vertex, the whole hole
/// ring starting at its slit vertex and back to it, the slit vertex again, then
/// the rest of the exterior.
pub(crate) fn merge_hole_into_exterior(exterior: &[Point], hole: &[Point]) -> Vec<Point> {
    let (i, j) = baseline_closest_pair();
    let mut out: Vec<Point> = exterior[..=i].to_vec();
    for h in 0..=hole.len() {
        out.push(hole[(j + h) % hole.len()]);
    }
    out.push(exterior[i]);
    out.extend_from_slice(&exterior[i + 1..]);
    out
}

fn sub_loop_area(points: &[Point], from: usize, to: usize) -> f64 {
    let mut area = 0.0;
    for k in from..=to {
        let p = points[k];
        let q = points[if k == to { from } else { k + 1 }];
        area += p[0] * q[1] - q[0] * p[1];
    }
    area.abs() / 2.0
}

fn distance(a: Point, b: Point) -> f64 {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    (dx * dx + dy * dy).sqrt()
}

/// `_eraseDegenerateSelfLoops` over a straight-edged closed ring: splice out every
/// revisit loop whose area is clipper noise, then refit unless the caller
/// preserves geometry or a deliberate revisit (a slit) was kept. Returns the
/// segments and whether the refit ran.
pub(crate) fn erase_degenerate_self_loops(
    mut points: Vec<Point>,
    skip_refit: bool,
) -> (Vec<RingSeg>, bool) {
    if points.is_empty() {
        return (Vec::new(), false);
    }
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for p in &points {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    }
    let bb_area = ((max_x - min_x) * (max_y - min_y)).max(1.0);
    let min_loop_area = (bb_area * MIN_LOOP_FRAC).max(1.0);
    let mut changed = true;
    let mut guard = 0;
    let mut kept_revisit = false;
    while changed && guard < 5000 {
        changed = false;
        guard += 1;
        let n = points.len();
        'scan: for i in 0..n {
            for j in 0..i {
                if distance(points[i], points[j]) <= REVISIT_TOL {
                    if sub_loop_area(&points, j, i) > min_loop_area {
                        kept_revisit = true;
                        continue;
                    }
                    points.drain(j + 1..=i);
                    changed = true;
                    break 'scan;
                }
            }
        }
    }
    if !skip_refit && !kept_revisit && points.len() >= 3 {
        // Paper may collapse a small ring into ONE looped anchor (the oracle's `keyhole-ring`); storage admits a
        // cubic ring of 2..=256 anchors only, so such a refit is dropped and the exact polygon is kept.
        if let Some(fitted) =
            simplify_closed(&points, REFIT_TOLERANCE).filter(|fit| fit.len() >= MIN_STORED_SEGMENTS)
        {
            return (fitted, true);
        }
    }
    (plain(&points), false)
}

/// `flattenBooleanResult(_polygonsToPaperItem(polygons), preserve)`.
pub(crate) fn flatten_polygons(
    polygons: &[Polygon],
    preserve: bool,
) -> Result<Vec<Island>, TopologyError> {
    admit_polygons(polygons)?;
    let kept: Vec<&Polygon> = polygons.iter().filter(|p| p.exterior.len() >= 3).collect();
    if kept.is_empty() {
        return Ok(Vec::new());
    }
    if kept.len() == 1 && kept[0].holes.is_empty() {
        let (segments, refit) = erase_degenerate_self_loops(kept[0].exterior.clone(), preserve);
        return Ok(vec![Island {
            build: IslandBuild::Single,
            segments,
            refit,
        }]);
    }
    let any_hole = kept.iter().any(|p| !p.holes.is_empty());
    let mut islands = Vec::with_capacity(kept.len());
    for polygon in kept {
        let mut ring = polygon.exterior.clone();
        set_clockwise(&mut ring, true);
        if !any_hole {
            islands.push(Island {
                build: IslandBuild::Flat,
                segments: plain(&ring),
                refit: false,
            });
            continue;
        }
        for hole in &polygon.holes {
            let mut hole_ring = hole.clone();
            set_clockwise(&mut hole_ring, false);
            ring = merge_hole_into_exterior(&ring, &hole_ring);
        }
        let (segments, refit) = erase_degenerate_self_loops(ring, preserve);
        islands.push(Island {
            build: IslandBuild::Merged,
            segments,
            refit,
        });
    }
    Ok(islands)
}

#[cfg(test)]
#[path = "../tests/object_boolean_ops_ring.rs"]
mod tests;
