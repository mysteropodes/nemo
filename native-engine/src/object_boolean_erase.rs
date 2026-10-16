//! Eraser admission (`eraseAtPoint` of `src/js/tools.js`, L02-D): what happens
//! to the target once the provider has subtracted the eraser footprint.
//!
//! One erase result replaces its target in the target's own z-position with
//! one island per exterior (the first island inherits the identity, the others
//! get fresh ids), the original outline is redrawn unless the target was a
//! vector-brush ink, and a result with no area deletes the target. The
//! subtraction itself is the provider's; this module never computes geometry.
use crate::object_document::object_group_kernels::object_boolean_topology::{
    admit_polygons, plan_insert, InsertPlan, InsertRequest, PaintArg, Polygon, ResultShape,
    SourceData, StrokeInfo, TopologyError,
};

/// The target at the moment of the erase, after any ribbon / stroke-colour
/// substitution `eraseAtPoint` performs (those are geometry-provider concerns).
pub(crate) struct EraseTarget<'a> {
    /// Index of the target in its layer.
    pub(crate) index: usize,
    /// Children in the layer including the target.
    pub(crate) layer_len: usize,
    /// `target.fillColor` as `colorHex8` text; `None` or empty means no fill.
    pub(crate) fill: Option<String>,
    pub(crate) opacity: f64,
    /// `origStrokeInfo` (already filtered through [`erase_stroke_info`]).
    pub(crate) stroke: Option<StrokeInfo>,
    /// `origData`: the target's `.data` before it is replaced.
    pub(crate) data: Option<&'a SourceData>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum EraseOutcome {
    /// No fill to erase against: `eraseAtPoint` returns without touching anything.
    Unchanged,
    /// The result has no area: the target is removed and nothing is inserted.
    DeleteTarget,
    Replace(InsertPlan),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ErasePlan {
    pub(crate) outcome: EraseOutcome,
    /// Provider polygons dropped for having fewer than three exterior points.
    pub(crate) dropped: Vec<usize>,
}

/// `origStrokeInfo`: the outline is only redrawn for a non-vector-brush target
/// that actually has a stroke colour.
pub(crate) fn erase_stroke_info(
    is_vector_brush: bool,
    stroke: Option<StrokeInfo>,
) -> Option<StrokeInfo> {
    if is_vector_brush {
        return None;
    }
    stroke.filter(|info| !info.color.is_empty())
}

/// The plausibility guard around a provider result: a shape larger than four
/// eraser footprints must not lose more than six of them. Areas are absolute
/// values; a NaN result area counts as zero (`item.area || 0`).
pub(crate) fn wasm_result_plausible(target_area: f64, result_area: f64, eraser_area: f64) -> bool {
    let target = target_area.abs();
    let result = if result_area.is_nan() {
        0.0
    } else {
        result_area.abs()
    };
    let removed = target - result;
    !(target > eraser_area * 4.0 && removed > eraser_area * 6.0)
}

/// Admission of an already-built result shape (the kernel's own provider port).
pub(crate) fn plan_erase_shape(
    target: &EraseTarget,
    shape: &ResultShape,
) -> Result<ErasePlan, TopologyError> {
    plan_shape(target, shape, Vec::new())
}

/// Admission of the polygon list a provider returned: degenerate polygons are
/// dropped and reported, holes are never silently lost.
pub(crate) fn plan_erase_polygons(
    target: &EraseTarget,
    polygons: &[Polygon],
) -> Result<ErasePlan, TopologyError> {
    check_target(target)?;
    if !has_fill(target) {
        return Ok(ErasePlan {
            outcome: EraseOutcome::Unchanged,
            dropped: Vec::new(),
        });
    }
    let admission = admit_polygons(polygons)?;
    plan_shape(target, &admission.shape, admission.dropped)
}

fn has_fill(target: &EraseTarget) -> bool {
    match &target.fill {
        Some(fill) => !fill.is_empty(),
        None => false,
    }
}

fn check_target(target: &EraseTarget) -> Result<(), TopologyError> {
    if target.index >= target.layer_len {
        return Err(TopologyError::TargetOutOfRange {
            index: target.index,
            layer_len: target.layer_len,
        });
    }
    Ok(())
}

fn plan_shape(
    target: &EraseTarget,
    shape: &ResultShape,
    dropped: Vec<usize>,
) -> Result<ErasePlan, TopologyError> {
    check_target(target)?;
    let fill = match &target.fill {
        Some(fill) if !fill.is_empty() => fill.clone(),
        _ => {
            return Ok(ErasePlan {
                outcome: EraseOutcome::Unchanged,
                dropped,
            })
        }
    };
    if !shape.has_area() {
        return Ok(ErasePlan {
            outcome: EraseOutcome::DeleteTarget,
            dropped,
        });
    }
    match shape {
        ResultShape::Single(path) if path.segments < 3 => {
            return Err(TopologyError::UnusableRing {
                ring: 0,
                segments: path.segments,
            })
        }
        ResultShape::Compound(rings) => {
            if let Some(ring) = rings.iter().position(|ring| ring.segments < 3) {
                return Err(TopologyError::UnusableRing {
                    ring,
                    segments: rings[ring].segments,
                });
            }
        }
        _ => {}
    }
    let remaining = target.layer_len - 1;
    let request = InsertRequest {
        insert_at: target.index.min(remaining),
        layer_len: remaining,
        fill: PaintArg::Set(fill),
        opacity: Some(target.opacity),
        stroke: target.stroke.as_ref(),
        src: target.data,
        preserve_geometry: false,
    };
    Ok(ErasePlan {
        outcome: EraseOutcome::Replace(plan_insert(shape, &request)?),
        dropped,
    })
}
