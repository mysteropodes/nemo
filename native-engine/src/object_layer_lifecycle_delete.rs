//! Delete-layer plan (F07/F08, L02-E): `deleteLayer` (`src/js/timeline.js`,
//! fbe4b8a3) over the native layer, object, holder and order records.
use std::collections::HashSet;

use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::{
    consult, index_of, order, resolve_all, LayerEditPolicy, LayerOp, LayerSelection,
    LifecycleError, LifecyclePlan, SelectionOutcome,
};
use crate::object_document::{ItemRecords, ObjectDocument, Presence};

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
    // A child of a deleted layer is released: its parent link would name a layer that no longer exists.
    for layer in &mut candidate.layers {
        let orphaned = layer
            .parent_layer_uid
            .value()
            .and_then(|parent| parent.as_deref())
            .is_some_and(|parent| gone.contains(parent));
        if orphaned {
            layer.parent_layer_uid = Presence::Present(None);
        }
    }
    candidate
        .objects
        .retain(|record| !gone.contains(record.target.layer_uid.as_str()));
    // The raster placements and text blocks of the deleted layers go with them, every frame. An asset they named stays in
    // the project inventory (the undo brings the placement back; an asset still in use cannot be removed).
    if let Some(items) = document.item_records() {
        // Scene items only: an item a component carries keeps its own context (a merge moves the items of its source layers
        // there before the source layers are deleted).
        let keep = |target: &crate::object_document::ObjectTarget| {
            target.context_id() != "scene-root" || !gone.contains(target.layer_uid())
        };
        candidate.items = ItemRecords::normalized(
            items
                .raster_entries()
                .iter()
                .filter(|entry| keep(&entry.target))
                .cloned()
                .collect(),
            items
                .text_entries()
                .iter()
                .filter(|entry| keep(&entry.target))
                .cloned()
                .collect(),
        );
    }
    if let Presence::Present(holders) = &mut candidate.property_holders {
        holders.retain(|holder| !gone.contains(holder.target.layer_uid.as_str()));
    }
    release_layer_tables(&mut candidate, &gone);
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

/// The tables keyed by a scene layer: a placed component, a duplicator or effector hosted by the layer and its tween spans
/// leave with it, and a duplicator on another layer stops naming it (a path layer is cleared, a source pool member
/// dropped). The component a deleted instance placed stays in the document: it is a library entry the undo, and every other
/// instance, still use.
fn release_layer_tables(candidate: &mut ObjectDocument, gone: &HashSet<&str>) {
    const SCENE: &str = "scene-root";
    if let Presence::Present(tables) = &mut candidate.component_tables {
        if let Presence::Present(set) = &mut tables.components {
            set.instances.retain(|instance| {
                instance.context_id != SCENE || !gone.contains(instance.layer_uid.as_str())
            });
        }
        if let Presence::Present(set) = &mut tables.duplicators {
            let ours = |context: &str, layer: &str| context == SCENE && gone.contains(layer);
            set.duplicators
                .retain(|record| !ours(&record.context_id, &record.layer_uid));
            set.effector_layers
                .retain(|record| !ours(&record.context_id, &record.layer_uid));
            for record in &mut set.duplicators {
                if record.context_id != SCENE {
                    continue;
                }
                let duplicator = &mut record.duplicator;
                if duplicator
                    .path_layer_uid()
                    .is_some_and(|uid| gone.contains(uid))
                {
                    duplicator.path_layer_uid = Presence::Present(None);
                }
                if let Presence::Present(uids) = &mut duplicator.source_layer_uids {
                    uids.retain(|uid| !gone.contains(uid.as_str()));
                }
            }
        }
    }
    if let Presence::Present(store) = &mut candidate.tweens {
        store
            .layers
            .retain(|tweens| !gone.contains(tweens.layer_uid.as_str()));
    }
}
