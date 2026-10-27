//! Paint-bucket commit join (G18, L02-I): which traced region wins, whether the
//! click recolours a shape that already exists, where the new fill is stacked,
//! which provenance it is stamped with and which same-colour neighbours it
//! absorbs. Mirrors `fillVectorFind`, `fillFindExistingMatch`,
//! `fillInsertIndexFor` and the commit half of `onDown` (`src/js/fill-bridge.js`
//! and `src/js/tools.js` at fbe4b8a3), composed with the same-colour merge
//! predicates of `object_boolean_topology` (L02-D).
//!
//! The retained tracer and native union are connected through the typed ports
//! below by the native provider children. A provider that fails one step (the baseline
//! `catch`) simply finds nothing at that gap; the baseline's second, stale JS
//! tracer has no native twin and is not reproduced.
use crate::object_document::object_group_kernels::object_boolean_ops::RingSeg;
#[path = "object_fill_trace_plan.rs"]
pub(crate) mod document;
// #2361: the Fill Brush commit (placement, then the same-colour merge with Fill Brush absorb).
#[path = "object_fill_brush_plan.rs"]
pub(crate) mod brush;
#[path = "object_fill_geometry.rs"]
mod geometry;
#[path = "object_fill_native_provider.rs"]
pub(crate) mod native;
#[path = "object_fill_native_merge.rs"]
mod native_merge;
#[path = "object_fill_regenerate.rs"]
pub(crate) mod regenerate;
use crate::object_document::object_group_kernels::object_boolean_topology::{
    merge_candidates, plan_union, resolve_union, select_absorbed, Absorbed, AreaFacts, Bounds,
    FillFacts, MergeDecision, MergeError, TouchProbe, UnionPlan,
};

/// `FILL_GAP_STEPS`: gap thresholds tried smallest first.
pub(crate) const FILL_GAP_STEPS: [u32; 7] = [0, 10, 24, 48, 90, 160, 280];
/// `FILL_MIN_AREA`: below this a traced loop is numerical noise.
pub(crate) const FILL_MIN_AREA: f64 = 4.0;
/// The raster fallback's gap (`fillVectorFindRaster(..., 24, ...)`).
pub(crate) const RASTER_GAP: u32 = 24;

/// One traced closed region, curve accurate.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TraceHit {
    pub(crate) ring: Vec<RingSeg>,
    /// Signed or absolute area of the ring; only its magnitude is read.
    pub(crate) area: f64,
    /// Ids of the strokes the loop is made of.
    pub(crate) wall_ids: Vec<String>,
    /// The loop needed a gap bridged.
    pub(crate) used_gap: bool,
    /// The gap threshold this hit was found at (set by [`find_fill_region`] for graph hits).
    pub(crate) gap_px: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TraceFailure {
    /// No tracer is attached.
    Unavailable,
    /// The tracer threw for this call (the baseline `catch`).
    Failed,
}

pub(crate) trait FillTraceProvider {
    /// `fillVectorFindWasm(click, gap, ...)`: `Ok(None)` is a clean "nothing closes at this gap".
    fn trace(&mut self, gap: u32) -> Result<Option<TraceHit>, TraceFailure>;
    /// `fillVectorFindRaster(click, ..., 24, ...)`, tried only when no gap found anything.
    fn raster(&mut self) -> Result<Option<TraceHit>, TraceFailure>;
}

/// Explicit unavailable port retained for refusal controls; production uses `native::NativeTrace`.
pub(crate) struct UnavailableTrace;

impl FillTraceProvider for UnavailableTrace {
    fn trace(&mut self, _gap: u32) -> Result<Option<TraceHit>, TraceFailure> {
        Err(TraceFailure::Unavailable)
    }
    fn raster(&mut self) -> Result<Option<TraceHit>, TraceFailure> {
        Err(TraceFailure::Unavailable)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FillTraceError {
    /// A provider reported `Unavailable`: nothing is traced, nothing changes.
    Unavailable,
}

/// `fillVectorFind`: the smallest-area region over the gap steps. A step above
/// `max_gap` ends the walk; a sliver under [`FILL_MIN_AREA`] is discarded; the
/// walk stops as soon as the best region needed no gap; only when no step found
/// anything is the raster fallback tried (and kept only above the area floor).
pub(crate) fn find_fill_region<P: FillTraceProvider>(
    provider: &mut P,
    max_gap: Option<u32>,
) -> Result<Option<TraceHit>, FillTraceError> {
    let mut best: Option<TraceHit> = None;
    let mut best_area = f64::INFINITY;
    for gap in FILL_GAP_STEPS {
        if max_gap.is_some_and(|cap| gap > cap) {
            break;
        }
        let found = match provider.trace(gap) {
            Ok(found) => found,
            Err(TraceFailure::Unavailable) => return Err(FillTraceError::Unavailable),
            Err(TraceFailure::Failed) => None,
        };
        let found = found.filter(|hit| hit.area.abs() >= FILL_MIN_AREA);
        if let Some(mut hit) = found {
            let area = hit.area.abs();
            if area < best_area {
                hit.gap_px = gap;
                best_area = area;
                best = Some(hit);
            }
            if best.as_ref().is_some_and(|kept| !kept.used_gap) {
                break;
            }
        }
    }
    if best.is_none() {
        match provider.raster() {
            Ok(Some(hit)) if hit.area.abs() >= FILL_MIN_AREA => best = Some(hit),
            Ok(_) | Err(TraceFailure::Failed) => {}
            Err(TraceFailure::Unavailable) => return Err(FillTraceError::Unavailable),
        }
    }
    Ok(best)
}

/// What `fillFindExistingMatch` and `fillInsertIndexFor` read off one layer child.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ChildFacts {
    /// `instanceof Path || CompoundPath` with a `fillColor`.
    pub(crate) filled_shape: bool,
    /// `isLinkedFillCompanion || isBrushTextureCopy`.
    pub(crate) excluded: bool,
    pub(crate) bounds: Bounds,
    /// Absolute area.
    pub(crate) area: f64,
    /// The click point lies inside the child's fill (a raster child: inside its bounds).
    pub(crate) covers_click: bool,
}

/// `fillFindExistingMatch`: the filled child whose bounds sit within 4 px and
/// whose area within max(4, 4 %) of the traced region, nearest in area first.
pub(crate) fn find_existing_match(
    children: &[ChildFacts],
    area: f64,
    bounds: &Bounds,
) -> Option<usize> {
    let area = area.abs();
    if !(area > 0.0) {
        return None;
    }
    let mut best: Option<usize> = None;
    let mut best_diff = f64::INFINITY;
    for (index, child) in children.iter().enumerate() {
        if !child.filled_shape || child.excluded {
            continue;
        }
        let cb = &child.bounds;
        if (cb.x - bounds.x).abs() > 4.0
            || (cb.y - bounds.y).abs() > 4.0
            || (cb.width - bounds.width).abs() > 4.0
            || (cb.height - bounds.height).abs() > 4.0
        {
            continue;
        }
        let diff = (child.area.abs() - area).abs();
        if diff > f64::max(4.0, area * 0.04) {
            continue;
        }
        if diff < best_diff {
            best_diff = diff;
            best = Some(index);
        }
    }
    best
}

/// `fillInsertIndexFor`: just above the topmost child that covers the click, else the bottom.
pub(crate) fn fill_insert_index(children: &[ChildFacts]) -> usize {
    for index in (0..children.len()).rev() {
        if children[index].covers_click {
            return index + 1;
        }
    }
    0
}

/// `data.fillSeed`, `data.fillGapPx` and (only when walls exist) `data.fillWalls`.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FillStamp {
    pub(crate) seed: [f64; 2],
    pub(crate) gap_px: u32,
    pub(crate) walls: Option<Vec<String>>,
}

pub(crate) fn stamp_for(click: [f64; 2], hit: &TraceHit) -> FillStamp {
    FillStamp {
        seed: click,
        gap_px: hit.gap_px,
        walls: if hit.wall_ids.is_empty() {
            None
        } else {
            Some(hit.wall_ids.clone())
        },
    }
}

/// The curved union, owned by the geometry provider. `unite` joins the accumulator
/// with the child at layer index `step` and says whether it worked.
pub(crate) trait UnionProvider {
    fn unite(&mut self, step: usize) -> Result<bool, MergeError>;
}

/// Explicit unavailable union retained for refusal controls; production uses the native union provider.
pub(crate) struct UnavailableUnion;

impl UnionProvider for UnavailableUnion {
    fn unite(&mut self, _step: usize) -> Result<bool, MergeError> {
        Err(MergeError::GeometryUnavailable)
    }
}

/// Same-colour merge of the new fill with its touching neighbours.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MergeStep {
    /// Every absorbed neighbour (layer indices with the new fill already placed).
    pub(crate) absorbed: Vec<usize>,
    pub(crate) union: UnionPlan,
    pub(crate) decision: MergeDecision,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FillTracePlan {
    /// "Aucune zone fermee ici".
    NoRegion,
    /// The traced region IS an existing shape: recolour it in place.
    Recolor { child: usize },
    /// Insert the new fill at `insert_at` (before any merge), stamped; `merge` when it touched same-colour fills.
    Place {
        insert_at: usize,
        stamp: FillStamp,
        merge: Option<MergeStep>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum FillTraceJoinError {
    Trace(FillTraceError),
    Merge(MergeError),
}

/// The region a click traced, the colour it paints and the children of the layer.
pub(crate) struct CommitInput<'a> {
    pub(crate) click: [f64; 2],
    pub(crate) hit: Option<&'a TraceHit>,
    /// `colorHex8` of the fill colour the tool paints.
    pub(crate) fill_hex8: &'a str,
    pub(crate) bounds: Bounds,
    /// One entry per layer child, bottom to top, before the new fill exists.
    pub(crate) children: &'a [ChildFacts],
    /// The merge facts of the same children.
    pub(crate) facts: &'a [FillFacts],
    pub(crate) allow_fill_shape_absorb: bool,
}

/// The commit half of the paint-bucket `onDown`.
pub(crate) fn plan_fill_trace_commit<P: TouchProbe, U: UnionProvider>(
    input: &CommitInput<'_>,
    probe: &mut P,
    unions: &mut U,
) -> Result<FillTracePlan, FillTraceJoinError> {
    let Some(hit) = input.hit else {
        return Ok(FillTracePlan::NoRegion);
    };
    if let Some(child) = find_existing_match(input.children, hit.area, &input.bounds) {
        return Ok(FillTracePlan::Recolor { child });
    }
    let insert_at = fill_insert_index(input.children);
    let stamp = stamp_for(input.click, hit);
    let mut layer: Vec<FillFacts> = input.facts.to_vec();
    let new_index = insert_at.min(layer.len());
    layer.insert(
        new_index,
        FillFacts {
            is_path: true,
            closed: true,
            fill_hex8: Some(input.fill_hex8.to_string()),
            fill_walls: stamp.walls.clone(),
            ..FillFacts::default()
        },
    );
    let candidates = merge_candidates(new_index, &layer, input.allow_fill_shape_absorb);
    let absorbed = select_absorbed(&candidates, probe).map_err(FillTraceJoinError::Merge)?;
    if absorbed.is_empty() {
        return Ok(FillTracePlan::Place {
            insert_at,
            stamp,
            merge: None,
        });
    }
    let new_area = AreaFacts {
        area: hit.area,
        bounds: input.bounds,
    };
    let children: Vec<Absorbed> = absorbed
        .iter()
        .map(|&index| {
            let child = &input.children[if index > new_index { index - 1 } else { index }];
            Absorbed {
                index,
                area: AreaFacts {
                    area: child.area,
                    bounds: child.bounds,
                },
            }
        })
        .collect();
    let union = plan_union(&new_area, &children);
    let mut outcomes = Vec::with_capacity(union.steps.len());
    for &step in &union.steps {
        outcomes.push(unions.unite(step).map_err(FillTraceJoinError::Merge)?);
    }
    let decision = resolve_union(&union, &outcomes).map_err(FillTraceJoinError::Merge)?;
    Ok(FillTracePlan::Place {
        insert_at,
        stamp,
        merge: Some(MergeStep {
            absorbed,
            union,
            decision,
        }),
    })
}

#[cfg(test)]
#[path = "../tests/object_fill_trace_ops_commit.rs"]
mod commit_tests;
#[cfg(test)]
#[path = "../tests/object_fill_trace_native.rs"]
mod native_tests;
#[cfg(test)]
#[path = "../tests/object_fill_trace_ops.rs"]
mod tests;
