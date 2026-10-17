//! Polygon boolean / eraser engine and the real combine provider (G19 + G18, L02-I).
//!
//! The retained geometry owner of the baseline is `boolean_op_multi` /
//! `erase_at_point` (`geometry-wasm`, geo-booleanop) behind `booleanOpWasm` of
//! `src/js/tools.js` at fbe4b8a3. This module is its native counterpart on the
//! admitted `i_overlay` float API (i64 engine, even-odd fill), with the same
//! fold: the accumulator is a whole multi-polygon, every next operand is
//! folded in with the SAME operation, and an empty accumulator ends the walk
//! with an empty result. It also implements the `CombineFold` port of the
//! combine plan (L02-B) so nested combined children stay ONE operand with all
//! their pieces, and it builds the island rings (module `ring`), refits them
//! (module `fit`), bakes element Motion (module `bake`) and plans the document
//! edits (module `plan`).
//!
//! Deliberate, reported differences from the baseline: result order is a
//! deterministic canonical one (islands and holes by bounds), because the
//! provider's own sweep order was never a contract; a coordinate that is not
//! finite or exceeds 1e12 is a typed refusal instead of a provider panic; and
//! an eraser result the plausibility guard rejects is refused instead of
//! silently retried with another clipper (there is no second clipper).
use std::fmt;

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::float::overlay::FloatOverlay;

use crate::object_document::object_group_kernels::object_boolean_topology::{
    wasm_result_plausible, Polygon,
};
use crate::object_document::object_group_kernels::object_group_combine_plan::{
    CombineFold, FoldError,
};
use crate::object_document::object_group_kernels::object_group_hierarchy::CombineMode;

#[path = "object_boolean_ops_bake.rs"]
mod bake;
#[path = "object_boolean_ops_fit.rs"]
mod fit;
#[path = "object_boolean_ops_plan.rs"]
pub(crate) mod plan;
#[path = "object_boolean_ops_ring.rs"]
mod ring;
#[path = "object_boolean_ops_stack.rs"]
pub(crate) mod stack;

#[allow(unused_imports)]
pub(crate) use bake::{bake_multi, bake_ring, ElementMat};
pub(crate) use fit::simplify_closed;
#[allow(unused_imports)]
pub(crate) use ring::{
    erase_degenerate_self_loops, flatten_polygons, is_clockwise, merge_hole_into_exterior,
    set_clockwise, signed_area, Island,
};

/// One vertex of a ring.
pub(crate) type Point = [f64; 2];
/// Disjoint polygons (each with its holes): the shape the fold passes around.
pub(crate) type MultiPolygon = Vec<Polygon>;

/// Coordinates above this magnitude are refused before they reach the clipper.
pub(crate) const MAX_ABS_COORD: f64 = 1.0e12;
/// `DEDUP_EPS_SQ` of `eraser.rs`: consecutive points closer than 1e-3 are one.
const DEDUP_EPS_SQ: f64 = 1e-6;

/// One anchor of a finished ring: point plus handles relative to it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RingSeg {
    pub(crate) point: Point,
    pub(crate) handle_in: Point,
    pub(crate) handle_out: Point,
}

/// The four Paper.js boolean names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoolOp {
    Unite,
    Subtract,
    Intersect,
    Exclude,
}

impl BoolOp {
    /// `none` is not an operation (the baseline never folds a non-combining group).
    pub(crate) fn from_mode(mode: CombineMode) -> Option<BoolOp> {
        match mode {
            CombineMode::None => None,
            CombineMode::Unite => Some(BoolOp::Unite),
            CombineMode::Subtract => Some(BoolOp::Subtract),
            CombineMode::Intersect => Some(BoolOp::Intersect),
            CombineMode::Exclude => Some(BoolOp::Exclude),
        }
    }
    pub(crate) fn from_name(name: &str) -> Option<BoolOp> {
        match name {
            "unite" => Some(BoolOp::Unite),
            "subtract" => Some(BoolOp::Subtract),
            "intersect" => Some(BoolOp::Intersect),
            "exclude" => Some(BoolOp::Exclude),
            _ => None,
        }
    }
    fn rule(self) -> OverlayRule {
        match self {
            BoolOp::Unite => OverlayRule::Union,
            BoolOp::Subtract => OverlayRule::Difference,
            BoolOp::Intersect => OverlayRule::Intersect,
            BoolOp::Exclude => OverlayRule::Xor,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpsError {
    NoOperands,
    /// A coordinate of operand `operand` is NaN or infinite.
    NonFinite {
        operand: usize,
    },
    /// A coordinate of operand `operand` is above [`MAX_ABS_COORD`].
    OutOfRange {
        operand: usize,
    },
    /// A ring has fewer than three distinct points once consecutive duplicates are removed.
    Degenerate,
    /// The eraser result removed far more area than the footprint can explain.
    Implausible,
    /// The combine mode `none` has no operation.
    ModeNone,
}

impl fmt::Display for OpsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

type Contours = Vec<Vec<Point>>;

fn check_ring(points: &[Point], operand: usize) -> Result<(), OpsError> {
    for point in points {
        if !(point[0].is_finite() && point[1].is_finite()) {
            return Err(OpsError::NonFinite { operand });
        }
        if point[0].abs() > MAX_ABS_COORD || point[1].abs() > MAX_ABS_COORD {
            return Err(OpsError::OutOfRange { operand });
        }
    }
    Ok(())
}

/// Consecutive exactly equal points collapse, and a closing point equal to the start goes.
fn clean_ring(points: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for &point in points {
        if out.last().is_some_and(|last| *last == point) {
            continue;
        }
        out.push(point);
    }
    while out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    out
}

/// `bezpath_to_polygon_in` of `eraser.rs`: points closer than 1e-3 to the previous kept
/// one are dropped, then a closing point back on the start is dropped.
pub(crate) fn dedupe_ring(points: &[Point]) -> Vec<Point> {
    let mut out: Vec<Point> = Vec::with_capacity(points.len());
    for &point in points {
        let duplicate = out.last().is_some_and(|last| {
            let dx = last[0] - point[0];
            let dy = last[1] - point[1];
            dx * dx + dy * dy < DEDUP_EPS_SQ
        });
        if !duplicate {
            out.push(point);
        }
    }
    if out.len() > 1 {
        let first = out[0];
        let last = out[out.len() - 1];
        let dx = first[0] - last[0];
        let dy = first[1] - last[1];
        if dx * dx + dy * dy < DEDUP_EPS_SQ {
            out.pop();
        }
    }
    out
}

fn to_shapes(multi: &[Polygon], operand: usize) -> Result<Vec<Contours>, OpsError> {
    let mut shapes: Vec<Contours> = Vec::new();
    for polygon in multi {
        check_ring(&polygon.exterior, operand)?;
        for hole in &polygon.holes {
            check_ring(hole, operand)?;
        }
        let exterior = clean_ring(&polygon.exterior);
        if exterior.len() < 3 {
            continue;
        }
        let mut contours: Contours = vec![exterior];
        for hole in &polygon.holes {
            let ring = clean_ring(hole);
            if ring.len() >= 3 {
                contours.push(ring);
            }
        }
        shapes.push(contours);
    }
    Ok(shapes)
}

fn ring_key(points: &[Point]) -> [f64; 4] {
    let mut key = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    for p in points {
        key[0] = key[0].min(p[0]);
        key[1] = key[1].min(p[1]);
        key[2] = key[2].max(p[0]);
        key[3] = key[3].max(p[1]);
    }
    key
}

fn compare_rings(a: &[Point], b: &[Point]) -> std::cmp::Ordering {
    let (ka, kb) = (ring_key(a), ring_key(b));
    for k in 0..4 {
        let order = ka[k]
            .partial_cmp(&kb[k])
            .unwrap_or(std::cmp::Ordering::Equal);
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    std::cmp::Ordering::Equal
}

/// Canonical order: islands by their exterior bounds, holes by theirs.
fn canonical(mut multi: MultiPolygon) -> MultiPolygon {
    for polygon in &mut multi {
        polygon.holes.sort_by(|a, b| compare_rings(a, b));
    }
    multi.sort_by(|a, b| compare_rings(&a.exterior, &b.exterior));
    multi
}

fn step(op: BoolOp, subject: &[Contours], clip: &[Contours]) -> MultiPolygon {
    if subject.is_empty() {
        return match op {
            BoolOp::Unite | BoolOp::Exclude => canonical(from_contours(clip)),
            BoolOp::Subtract | BoolOp::Intersect => Vec::new(),
        };
    }
    if clip.is_empty() {
        return match op {
            BoolOp::Intersect => Vec::new(),
            _ => canonical(from_contours(subject)),
        };
    }
    let subject_shapes: Vec<Vec<Vec<[f64; 2]>>> = subject.to_vec();
    let clip_shapes: Vec<Vec<Vec<[f64; 2]>>> = clip.to_vec();
    let mut overlay =
        FloatOverlay::<[f64; 2], i64>::from_subj_and_clip(&subject_shapes, &clip_shapes);
    let shapes = overlay.overlay(op.rule(), FillRule::EvenOdd);
    let mut out: MultiPolygon = Vec::new();
    for shape in shapes {
        let mut rings: Vec<Vec<Point>> = Vec::new();
        for contour in shape {
            let ring: Vec<Point> = contour.iter().map(|p| [p[0], p[1]]).collect();
            if ring.len() >= 3 {
                rings.push(ring);
            }
        }
        if rings.is_empty() {
            continue;
        }
        let exterior = rings.remove(0);
        out.push(Polygon {
            exterior,
            holes: rings,
        });
    }
    canonical(out)
}

fn from_contours(shapes: &[Contours]) -> MultiPolygon {
    shapes
        .iter()
        .map(|contours| Polygon {
            exterior: contours[0].clone(),
            holes: contours[1..].to_vec(),
        })
        .collect()
}

/// `booleanOpWasm`: fold every operand into the accumulator with `op`. One operand
/// returns itself; an empty accumulator after any step ends the walk empty.
pub(crate) fn fold_multi(op: BoolOp, operands: &[MultiPolygon]) -> Result<MultiPolygon, OpsError> {
    if operands.is_empty() {
        return Err(OpsError::NoOperands);
    }
    let mut prepared: Vec<Vec<Contours>> = Vec::with_capacity(operands.len());
    for (index, operand) in operands.iter().enumerate() {
        prepared.push(to_shapes(operand, index)?);
    }
    let mut accumulator: Vec<Contours> = prepared[0].clone();
    if prepared.len() == 1 {
        return Ok(canonical(from_contours(&accumulator)));
    }
    for clip in &prepared[1..] {
        let result = step(op, &accumulator, clip);
        if result.is_empty() {
            return Ok(Vec::new());
        }
        accumulator = result
            .iter()
            .map(|polygon| {
                let mut contours: Contours = vec![polygon.exterior.clone()];
                contours.extend(polygon.holes.iter().cloned());
                contours
            })
            .collect();
    }
    Ok(canonical(from_contours(&accumulator)))
}

/// Absolute area of a ring.
pub(crate) fn ring_area(points: &[Point]) -> f64 {
    signed_area(points).abs()
}

/// Exterior area minus hole areas, summed over the polygons.
pub(crate) fn multi_area(multi: &[Polygon]) -> f64 {
    multi
        .iter()
        .map(|polygon| {
            ring_area(&polygon.exterior)
                - polygon
                    .holes
                    .iter()
                    .map(|hole| ring_area(hole))
                    .sum::<f64>()
        })
        .sum()
}

/// `erase_at_point`: the target ring minus the footprint ring, after the eraser's
/// own cleaning of both rings and followed by the plausibility guard.
pub(crate) fn erase_difference(
    target: &[Point],
    footprint: &[Point],
) -> Result<MultiPolygon, OpsError> {
    check_ring(target, 0)?;
    check_ring(footprint, 1)?;
    let target = dedupe_ring(target);
    let footprint = dedupe_ring(footprint);
    if target.len() < 3 || footprint.len() < 3 {
        return Err(OpsError::Degenerate);
    }
    let target_area = ring_area(&target);
    let eraser_area = ring_area(&footprint);
    let operands = vec![
        vec![Polygon {
            exterior: target,
            holes: Vec::new(),
        }],
        vec![Polygon {
            exterior: footprint,
            holes: Vec::new(),
        }],
    ];
    let result = fold_multi(BoolOp::Subtract, &operands)?;
    guard_result(target_area, multi_area(&result), eraser_area)?;
    Ok(result)
}

/// The plausibility guard around an eraser result: a target larger than four footprints
/// may not lose more than six of them (`wasm_result_plausible`). The baseline retried
/// with Paper.js when it tripped; there is no second clipper here, so it is refused.
pub(crate) fn guard_result(
    target_area: f64,
    result_area: f64,
    eraser_area: f64,
) -> Result<(), OpsError> {
    if wasm_result_plausible(target_area, result_area, eraser_area) {
        Ok(())
    } else {
        Err(OpsError::Implausible)
    }
}

/// Where a combine operand's polygons come from: the current frame's member with
/// element Motion baked in and its linked-fill companion united by the caller.
pub(crate) trait MemberSource {
    fn member(&mut self, item: usize, companion: Option<usize>) -> Result<MultiPolygon, FoldError>;
}

/// The real [`CombineFold`] provider over a [`MemberSource`].
pub(crate) struct OverlayFold<S: MemberSource> {
    source: S,
}

impl<S: MemberSource> OverlayFold<S> {
    pub(crate) fn new(source: S) -> Self {
        OverlayFold { source }
    }
    #[allow(dead_code)]
    pub(crate) fn into_source(self) -> S {
        self.source
    }
}

impl<S: MemberSource> CombineFold for OverlayFold<S> {
    type Shape = MultiPolygon;
    fn member_shape(
        &mut self,
        item: usize,
        companion: Option<usize>,
    ) -> Result<MultiPolygon, FoldError> {
        self.source.member(item, companion)
    }
    fn fold(
        &mut self,
        mode: CombineMode,
        operands: &[MultiPolygon],
    ) -> Result<MultiPolygon, FoldError> {
        let op = BoolOp::from_mode(mode)
            .ok_or_else(|| FoldError::Failed(format!("{}", OpsError::ModeNone)))?;
        fold_multi(op, operands).map_err(|error| FoldError::Failed(format!("{error}")))
    }
    /// One island per polygon, each with its own holes.
    fn islands(&mut self, shape: &MultiPolygon) -> Result<Vec<MultiPolygon>, FoldError> {
        Ok(shape.iter().map(|polygon| vec![polygon.clone()]).collect())
    }
}

#[cfg(test)]
#[path = "../tests/object_boolean_ops_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_boolean_ops.rs"]
mod tests;
