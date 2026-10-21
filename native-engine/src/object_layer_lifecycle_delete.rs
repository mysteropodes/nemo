//! Delete-layer plan (F07/F08, L02-E): `deleteLayer` (`src/js/timeline.js`,
//! fbe4b8a3) over the native layer, object, holder and order records.
use std::collections::HashSet;

use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    consult, index_of, order, resolve_all, LayerEditPolicy, LayerOp, LayerSelection,
    LifecycleError, LifecyclePlan, SelectionOutcome,
};
use crate::object_document::{ObjectDocument, Presence};

/// `deleteLayer`: the selection (or the active layer when none) goes, highest
/// slot first, until one layer is left. The active slot number is kept and
/// clamped, and the layer there becomes the whole selection. A selection that
/// covers every layer spares the lowest slot, reported in `spared`.
pub(crate) fn plan_delete(
    document: &ObjectDocument,
    selection: &LayerSelection,
    policy: &dyn LayerEditPolicy,
) -> Result<LifecyclePlan, LifecycleError> {
    let len = document.layers.len();
    if len <= 1 {
        return Err(LifecycleError::LastLayer);
    }
    let active = index_of(document, &selection.active)?;
    let mut targets: Vec<usize> = Vec::new();
    if selection.selected.is_empty() {
        targets.push(active);
    } else {
        for index in resolve_all(document, &selection.selected)? {
            if !targets.contains(&index) {
                targets.push(index);
            }
        }
    }
    targets.sort_unstable_by(|left, right| right.cmp(left));
    let mut remaining = len;
    let mut removed: Vec<usize> = Vec::new();
    let mut spared: Option<usize> = None;
    for &index in &targets {
        if remaining <= 1 {
            spared = Some(index);
            continue;
        }
        removed.push(index);
        remaining -= 1;
    }
    removed.sort_unstable();
    for &index in &removed {
        consult(document, policy, LayerOp::Delete, &document.layers[index])?;
    }
    let gone: HashSet<&str> = removed
        .iter()
        .map(|&index| document.layers[index].layer_uid.as_str())
        .collect();
    for layer in &document.layers {
        let orphaned = layer
            .parent_layer_uid
            .value()
            .and_then(|parent| parent.as_deref())
            .is_some_and(|parent| gone.contains(parent));
        if orphaned && !gone.contains(layer.layer_uid.as_str()) {
            consult(document, policy, LayerOp::DetachFromDeletedParent, layer)?;
        }
    }
    let mut candidate = document.clone();
    candidate.layers = document
        .layers
        .iter()
        .filter(|layer| !gone.contains(layer.layer_uid.as_str()))
        .cloned()
        .collect();
    candidate
        .objects
        .retain(|record| !gone.contains(record.target.layer_uid.as_str()));
    if let Presence::Present(holders) = &mut candidate.property_holders {
        holders.retain(|holder| !gone.contains(holder.target.layer_uid.as_str()));
    }
    order::remove_layers(&mut candidate, &gone);
    let slot = active.min(candidate.layers.len() - 1);
    Ok(LifecyclePlan {
        document: candidate,
        changed: true,
        selection: Some(SelectionOutcome {
            active: slot,
            selected: vec![slot],
            anchor: Some(slot),
        }),
        removed: removed
            .iter()
            .map(|&index| document.layers[index].layer_uid.clone())
            .collect(),
        touched: Vec::new(),
        spared: spared.map(|index| document.layers[index].layer_uid.clone()),
        created: None,
    })
}
