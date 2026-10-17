//! Same-colour fill merge, candidate half (`fillMergeSameColor` and
//! `_isNearIdenticalArea` of `src/js/tools.js`, L02-D).
//!
//! Which neighbours the new fill may absorb: a closed, filled, unstroked
//! path of exactly the same `colorHex8` text, not a vector-brush ink, linked
//! fill companion, brush-texture copy or revision ghost, and a Fill Brush
//! shape only when the caller allows it or the new fill was traced against its
//! own stroke id. The geometric tests (bounds overlap, a drawn divider between
//! the two interior points, strict touch, the 3 px near miss) are a typed port;
//! the union itself is the provider's. Layer order is preserved throughout.
use crate::object_document::object_group_kernels::object_boolean_topology::{
    Bounds, TopologyError,
};

/// The facts `fillMergeSameColor` reads from one layer child.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct FillFacts {
    /// `instanceof Path` (a compound path is never a candidate).
    pub(crate) is_path: bool,
    pub(crate) closed: bool,
    /// `colorHex8(fillColor)`; `None` or empty means no fill.
    pub(crate) fill_hex8: Option<String>,
    /// `strokeColor` is set.
    pub(crate) stroked: bool,
    pub(crate) is_vector_brush: bool,
    pub(crate) is_linked_fill_companion: bool,
    pub(crate) is_brush_texture_copy: bool,
    pub(crate) is_revision_ghost: bool,
    /// `data.ghostFrame !== undefined` (frame 0 counts).
    pub(crate) ghost_frame_defined: bool,
    pub(crate) is_fill_shape: bool,
    /// `data.strokeId` when truthy.
    pub(crate) stroke_id: Option<String>,
    /// `data.fillWalls` when present.
    pub(crate) fill_walls: Option<Vec<String>>,
}

fn has_color(facts: &FillFacts) -> bool {
    match &facts.fill_hex8 {
        Some(color) => !color.is_empty(),
        None => false,
    }
}

/// Layer indices (ascending) that pass every non-geometric test. A new fill
/// without a colour has no candidates (`fillMergeSameColor` returns it as is).
pub(crate) fn merge_candidates(
    new_index: usize,
    layer: &[FillFacts],
    allow_fill_shape_absorb: bool,
) -> Vec<usize> {
    let new_fill = match layer.get(new_index) {
        Some(facts) => facts,
        None => return Vec::new(),
    };
    if !has_color(new_fill) {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (index, child) in layer.iter().enumerate() {
        if index == new_index
            || !child.is_path
            || !child.closed
            || !has_color(child)
            || child.stroked
        {
            continue;
        }
        if child.is_vector_brush
            || child.is_linked_fill_companion
            || child.is_brush_texture_copy
            || child.is_revision_ghost
            || child.ghost_frame_defined
        {
            continue;
        }
        if child.is_fill_shape {
            let traced = match (&new_fill.fill_walls, &child.stroke_id) {
                (Some(walls), Some(id)) => walls.iter().any(|wall| wall == id),
                _ => false,
            };
            if !allow_fill_shape_absorb && !traced {
                continue;
            }
        }
        if child.fill_hex8 != new_fill.fill_hex8 {
            continue;
        }
        out.push(index);
    }
    out
}

/// Why a geometric probe could not answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeError {
    /// No geometry provider is attached.
    Unavailable,
    /// The probe threw (the baseline's `catch`).
    Failed,
}

/// The four geometric questions, asked in the baseline's order. `candidate` is
/// a layer index returned by [`merge_candidates`].
pub(crate) trait TouchProbe {
    /// `c.bounds.intersects(newFill.bounds)`.
    fn bounds_intersect(&mut self, candidate: usize) -> Result<bool, ProbeError>;
    /// `_strokeBetween(layer, newFill.interiorPoint, c.interiorPoint)`.
    fn stroke_between(&mut self, candidate: usize) -> Result<bool, ProbeError>;
    /// `newFill.intersects(c)` or either interior point inside the other shape.
    fn strict_touch(&mut self, candidate: usize) -> Result<bool, ProbeError>;
    /// Any sampled curve start of the new fill within 3 px of the candidate.
    fn near_miss(&mut self, candidate: usize) -> Result<bool, ProbeError>;
}

/// The production provider until a geometry provider is admitted.
pub(crate) struct UnavailableTouch;

impl TouchProbe for UnavailableTouch {
    fn bounds_intersect(&mut self, _candidate: usize) -> Result<bool, ProbeError> {
        Err(ProbeError::Unavailable)
    }
    fn stroke_between(&mut self, _candidate: usize) -> Result<bool, ProbeError> {
        Err(ProbeError::Unavailable)
    }
    fn strict_touch(&mut self, _candidate: usize) -> Result<bool, ProbeError> {
        Err(ProbeError::Unavailable)
    }
    fn near_miss(&mut self, _candidate: usize) -> Result<bool, ProbeError> {
        Err(ProbeError::Unavailable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MergeError {
    /// A probe reported [`ProbeError::Unavailable`]: nothing is merged, nothing changes.
    GeometryUnavailable,
    /// A probe the baseline does not wrap in `try` threw.
    ProbeFailed(&'static str),
    OutcomeMismatch {
        steps: usize,
        outcomes: usize,
    },
    ParticipantMismatch {
        expected: usize,
        given: usize,
    },
    IndexOutOfRange(usize),
    DuplicateIndex(usize),
    Topology(TopologyError),
}

fn hard(result: Result<bool, ProbeError>, what: &'static str) -> Result<bool, MergeError> {
    match result {
        Ok(value) => Ok(value),
        Err(ProbeError::Unavailable) => Err(MergeError::GeometryUnavailable),
        Err(ProbeError::Failed) => Err(MergeError::ProbeFailed(what)),
    }
}

fn caught(result: Result<bool, ProbeError>) -> Result<bool, MergeError> {
    match result {
        Ok(value) => Ok(value),
        Err(ProbeError::Unavailable) => Err(MergeError::GeometryUnavailable),
        Err(ProbeError::Failed) => Ok(false),
    }
}

/// The candidates that really touch the new fill, in layer order. The strict
/// test and the near miss each sit inside a `try` in the baseline, so a failing
/// probe counts as "no touch"; the near miss is asked only after a strict miss.
pub(crate) fn select_absorbed<P: TouchProbe>(
    candidates: &[usize],
    probe: &mut P,
) -> Result<Vec<usize>, MergeError> {
    let mut absorbed = Vec::new();
    for &candidate in candidates {
        if !hard(probe.bounds_intersect(candidate), "bounds_intersect")? {
            continue;
        }
        if hard(probe.stroke_between(candidate), "stroke_between")? {
            continue;
        }
        let mut touches = caught(probe.strict_touch(candidate))?;
        if !touches {
            touches = caught(probe.near_miss(candidate))?;
        }
        if touches {
            absorbed.push(candidate);
        }
    }
    Ok(absorbed)
}

/// Signed area and bounds of one path.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct AreaFacts {
    pub(crate) area: f64,
    pub(crate) bounds: Bounds,
}

/// `_isNearIdenticalArea`: areas within max(4, 4 % of the larger) and every
/// bounds component within max(4, 4 % of the first shape's longer side). Any
/// NaN makes the answer false, as it does in JavaScript.
pub(crate) fn is_near_identical_area(a: &AreaFacts, b: &AreaFacts) -> bool {
    let (ba, bb) = (&a.bounds, &b.bounds);
    let numbers = [
        a.area, b.area, ba.x, ba.y, ba.width, ba.height, bb.x, bb.y, bb.width, bb.height,
    ];
    if numbers.iter().any(|value| value.is_nan()) {
        return false;
    }
    let area_a = a.area.abs();
    let area_b = b.area.abs();
    if area_a <= 0.0 || area_b <= 0.0 {
        return false;
    }
    let max_area = area_a.max(area_b);
    if (area_a - area_b).abs() > f64::max(4.0, max_area * 0.04) {
        return false;
    }
    let tol = f64::max(4.0, f64::max(ba.width, ba.height) * 0.04);
    (ba.x - bb.x).abs() <= tol
        && (ba.y - bb.y).abs() <= tol
        && (ba.width - bb.width).abs() <= tol
        && (ba.height - bb.height).abs() <= tol
}

/// An absorbed layer child with its area facts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Absorbed {
    pub(crate) index: usize,
    pub(crate) area: AreaFacts,
}

/// Which absorbed children go through a union and which are skipped as copies
/// of the new fill (compared with the ORIGINAL new fill, not the accumulator).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnionPlan {
    /// Layer indices handed to the union, in order.
    pub(crate) steps: Vec<usize>,
    /// The first near-identical child (the shape kept if no union succeeds).
    pub(crate) near_identical: Option<usize>,
    pub(crate) skipped: Vec<usize>,
}

pub(crate) fn plan_union(new_fill: &AreaFacts, absorbed: &[Absorbed]) -> UnionPlan {
    let mut plan = UnionPlan {
        steps: Vec::new(),
        near_identical: None,
        skipped: Vec::new(),
    };
    for child in absorbed {
        if is_near_identical_area(new_fill, &child.area) {
            if plan.near_identical.is_none() {
                plan.near_identical = Some(child.index);
            }
            plan.skipped.push(child.index);
        } else {
            plan.steps.push(child.index);
        }
    }
    plan
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UniteDecision {
    /// Steps whose union succeeded, in order.
    pub(crate) united: Vec<usize>,
    /// Steps whose union failed: the baseline removes these children anyway.
    pub(crate) failed: Vec<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MergeDecision {
    /// Every union failed (or none was needed): the new fill stays as drawn.
    NoChange,
    /// Only a near-identical copy existed: the new fill is dropped, this child stays.
    KeepExisting(usize),
    Unite(UniteDecision),
}

/// Fold the union outcomes (one flag per step, in step order) into a decision.
pub(crate) fn resolve_union(
    plan: &UnionPlan,
    outcomes: &[bool],
) -> Result<MergeDecision, MergeError> {
    if outcomes.len() != plan.steps.len() {
        return Err(MergeError::OutcomeMismatch {
            steps: plan.steps.len(),
            outcomes: outcomes.len(),
        });
    }
    let mut united = Vec::new();
    let mut failed = Vec::new();
    for (step, ok) in plan.steps.iter().zip(outcomes.iter()) {
        if *ok {
            united.push(*step);
        } else {
            failed.push(*step);
        }
    }
    if united.is_empty() {
        return Ok(match plan.near_identical {
            Some(child) => MergeDecision::KeepExisting(child),
            None => MergeDecision::NoChange,
        });
    }
    Ok(MergeDecision::Unite(UniteDecision { united, failed }))
}
