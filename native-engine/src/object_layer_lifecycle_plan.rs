//! Layer lifecycle plan kernel (F07/F08, L02-E): delete, duplicate, rename,
//! recolor and batch reorder of document layers, as detached candidate
//! documents over the existing `ObjectDocument` and its order plan.
//!
//! Mirrors `reorderLayersAtGap` / `uniqueLayerName` (`src/js/app.js`) and
//! `deleteLayer` / `duplicateLayer` / `renameLayer` / `applyLayerColor`
//! (`src/js/timeline.js`) at fbe4b8a3. Every function reads an immutable
//! document and either returns a [`LifecyclePlan`] holding a complete
//! candidate (the input is never touched) or one typed refusal; there is no
//! partial result. The document layer is the dependency floor, so storage
//! admission of the candidate (`ObjectSnapshot::validate_drawing`, as
//! layer-create does) belongs to the transaction leaf that seals the plan
//! against an origin snapshot and publishes it; every test here runs it on
//! each candidate. The kernel owns no history, id source beyond the revision
//! it is handed, clock or persistence. Lock and transform-parent decisions
//! belong to Root F06; the kernel only forwards them through
//! [`LayerEditPolicy`].
use crate::object_document::{ObjectDocument, ObjectLayer};

#[path = "object_layer_lifecycle_delete.rs"]
mod delete;
#[path = "object_layer_lifecycle_duplicate.rs"]
mod duplicate;
#[path = "object_layer_lifecycle_fields.rs"]
mod fields;
#[path = "object_layer_lifecycle_order.rs"]
mod order;

#[allow(unused_imports)]
pub(crate) use delete::plan_delete;
#[allow(unused_imports)]
pub(crate) use duplicate::{plan_duplicate, unique_layer_name};
#[allow(unused_imports)]
pub(crate) use fields::{plan_recolor, plan_rename};

/// Largest content revision an identity may embed (2^53 - 1, as the sealed
/// layer-create path bounds it).
pub(crate) const MAX_REVISION: u64 = 9_007_199_254_740_991;

/// The operation a policy decision is requested for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LayerOp {
    Delete,
    Duplicate,
    Rename,
    Recolor,
    Reorder,
    /// A surviving layer names a deleted layer as its transform parent.
    DetachFromDeletedParent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PolicyRefusal {
    /// No provider answered; no decision is invented.
    Unavailable,
    Denied,
}

/// Consumer port for Root F06 (locked / transform-parented layers).
pub(crate) trait LayerEditPolicy {
    fn permit(&self, op: LayerOp, layer: &ObjectLayer) -> Result<(), PolicyRefusal>;
}

/// The stand-in until F06 lands: every decision it is asked for is unavailable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct UnavailableLayerEditPolicy;

impl LayerEditPolicy for UnavailableLayerEditPolicy {
    fn permit(&self, _op: LayerOp, _layer: &ObjectLayer) -> Result<(), PolicyRefusal> {
        Err(PolicyRefusal::Unavailable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LifecycleError {
    UnknownLayer(String),
    /// The document must keep one layer; checked before any selection is read.
    LastLayer,
    NonFiniteGap,
    InvalidColor(String),
    /// Duplicating a layer whose name was never authored has no baseline name.
    SourceUnnamed(String),
    OrderPlanIncomplete {
        context_id: String,
        layer_uid: String,
    },
    RevisionOutOfRange,
    IdUnavailable,
    TooManyLayers,
    Policy {
        op: LayerOp,
        layer_uid: String,
        refusal: PolicyRefusal,
    },
}

/// Editor selection by layer identity (`activeLayerIdx`, `_layerSel`,
/// `_layerSelAnchor` in the baseline are raw array slots).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LayerSelection {
    pub(crate) active: String,
    pub(crate) selected: Vec<String>,
    pub(crate) anchor: Option<String>,
}

/// The same selection as slots of the candidate's layer array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SelectionOutcome {
    pub(crate) active: usize,
    pub(crate) selected: Vec<usize>,
    pub(crate) anchor: Option<usize>,
}

/// A candidate after-state plus what the caller needs to report it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LifecyclePlan {
    pub(crate) document: ObjectDocument,
    /// `false`: the candidate equals the input (no revision is warranted).
    pub(crate) changed: bool,
    /// Present for reorder, delete and duplicate; rename and recolor leave
    /// the selection alone.
    pub(crate) selection: Option<SelectionOutcome>,
    /// Deleted layers, in document order.
    pub(crate) removed: Vec<String>,
    /// Rename / recolor targets whose fields actually changed.
    pub(crate) touched: Vec<String>,
    /// A selected layer the baseline keeps because a document needs one layer.
    pub(crate) spared: Option<String>,
    pub(crate) created: Option<String>,
}

impl LifecyclePlan {
    pub(crate) fn unchanged(
        document: &ObjectDocument,
        selection: Option<SelectionOutcome>,
    ) -> Self {
        Self {
            document: document.clone(),
            changed: false,
            selection,
            removed: Vec::new(),
            touched: Vec::new(),
            spared: None,
            created: None,
        }
    }
}

pub(crate) fn index_of(document: &ObjectDocument, uid: &str) -> Result<usize, LifecycleError> {
    document
        .layers
        .iter()
        .position(|layer| layer.layer_uid == uid)
        .ok_or_else(|| LifecycleError::UnknownLayer(uid.to_string()))
}

pub(crate) fn resolve_all(
    document: &ObjectDocument,
    uids: &[String],
) -> Result<Vec<usize>, LifecycleError> {
    uids.iter().map(|uid| index_of(document, uid)).collect()
}

/// Layers that carry lock or transform-parent facts need a decision; ordinary
/// layers follow the default edit policy, which the baseline never gates.
fn needs_decision(document: &ObjectDocument, layer: &ObjectLayer) -> bool {
    document.layer_is_locked(layer.layer_uid())
        || layer.parent_layer_uid.value().is_some_and(Option::is_some)
}

pub(crate) fn consult(
    document: &ObjectDocument,
    policy: &dyn LayerEditPolicy,
    op: LayerOp,
    layer: &ObjectLayer,
) -> Result<(), LifecycleError> {
    if !needs_decision(document, layer) {
        return Ok(());
    }
    policy
        .permit(op, layer)
        .map_err(|refusal| LifecycleError::Policy {
            op,
            layer_uid: layer.layer_uid.clone(),
            refusal,
        })
}

/// Slots of `layers` for the identities in `selection`; a missing active layer
/// falls back to slot 0 (`newActiveIdx>=0 ? newActiveIdx : 0`).
fn outcome(layers: &[ObjectLayer], selection: &LayerSelection) -> SelectionOutcome {
    let slot = |uid: &str| layers.iter().position(|layer| layer.layer_uid == uid);
    SelectionOutcome {
        active: slot(&selection.active).unwrap_or(0),
        selected: selection
            .selected
            .iter()
            .filter_map(|uid| slot(uid))
            .collect(),
        anchor: selection.anchor.as_deref().and_then(slot),
    }
}

/// `Math.round`: ties go toward +infinity.
fn js_round(value: f64) -> f64 {
    let floor = value.floor();
    if value - floor >= 0.5 {
        floor + 1.0
    } else {
        floor
    }
}

/// `reorderLayersAtGap(fromIndices, gapIdx)`: the moving set (in-range,
/// deduplicated, ascending) lands as one block at `gap`, counted over the layers
/// that stay. Dropping the block into its own gap is a no-op. Layers in the order
/// plan keep the natural slot of the array position they land on, so a stack that
/// follows the array (every plan layer-create writes) follows the move.
pub(crate) fn plan_reorder(
    document: &ObjectDocument,
    moving: &[i64],
    gap: f64,
    selection: &LayerSelection,
    policy: &dyn LayerEditPolicy,
) -> Result<LifecyclePlan, LifecycleError> {
    if gap.is_nan() {
        return Err(LifecycleError::NonFiniteGap);
    }
    let len = document.layers.len();
    index_of(document, &selection.active)?;
    resolve_all(document, &selection.selected)?;
    if let Some(anchor) = &selection.anchor {
        index_of(document, anchor)?;
    }
    let mut block: Vec<usize> = Vec::new();
    for &raw in moving {
        if let Ok(index) = usize::try_from(raw) {
            if index < len && !block.contains(&index) {
                block.push(index);
            }
        }
    }
    block.sort_unstable();
    let gap = js_round(gap).max(0.0).min(len as f64) as usize;
    let insert_at = (0..gap).filter(|index| !block.contains(index)).count();
    let in_place = block
        .iter()
        .enumerate()
        .all(|(slot, original)| *original == insert_at + slot);
    if in_place {
        let stay = outcome(&document.layers, selection);
        return Ok(LifecyclePlan::unchanged(document, Some(stay)));
    }
    for &index in &block {
        consult(document, policy, LayerOp::Reorder, &document.layers[index])?;
    }
    let kept: Vec<usize> = (0..len).filter(|index| !block.contains(index)).collect();
    let mut sequence: Vec<usize> = Vec::with_capacity(len);
    sequence.extend_from_slice(&kept[..insert_at]);
    sequence.extend_from_slice(&block);
    sequence.extend_from_slice(&kept[insert_at..]);
    let mut candidate = document.clone();
    candidate.layers = sequence
        .iter()
        .map(|&old| document.layers[old].clone())
        .collect();
    order::reseat_slots(&mut candidate, &document.layers)?;
    let selection = outcome(&candidate.layers, selection);
    Ok(LifecyclePlan {
        document: candidate,
        changed: true,
        selection: Some(selection),
        removed: Vec::new(),
        touched: Vec::new(),
        spared: None,
        created: None,
    })
}

#[cfg(test)]
#[path = "../tests/object_layer_lifecycle_plan_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_layer_lifecycle_plan_duplicate.rs"]
mod duplicate_tests;
#[cfg(test)]
#[path = "../tests/object_layer_lifecycle_plan_fixture.rs"]
mod fixture;
#[cfg(test)]
#[path = "../tests/object_layer_lifecycle_plan_mutations.rs"]
mod mutations;
#[cfg(test)]
#[path = "../tests/object_layer_lifecycle_plan_policy.rs"]
mod policy;
#[cfg(test)]
#[path = "../tests/object_layer_lifecycle_plan_replay_fields.rs"]
mod replay_fields;
#[cfg(test)]
#[path = "../tests/object_layer_lifecycle_plan.rs"]
mod tests;
