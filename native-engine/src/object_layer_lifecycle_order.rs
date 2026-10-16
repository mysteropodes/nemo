//! Order-plan side of the layer lifecycle plans (L02-E): every
//! `OrderContext` is edited together with the layer array, never just the
//! first one (the reader-by-reader drift CLAUDE.md section 1 warns about).
use std::collections::HashSet;

use crate::object_document::object_group_kernels::object_layer_lifecycle_plan::LifecycleError;
use crate::object_document::{ObjectDocument, ObjectLayer, OrderElement, OrderLayer, Presence};

fn incomplete(context_id: &str, layer_uid: &str) -> LifecycleError {
    LifecycleError::OrderPlanIncomplete {
        context_id: context_id.to_string(),
        layer_uid: layer_uid.to_string(),
    }
}

/// After a reorder: each context keeps the natural slot of every array
/// position, and a layer takes the slot of the position it landed on. For a
/// context whose slots equal the array positions this is the new array index.
pub(crate) fn reseat_slots(
    candidate: &mut ObjectDocument,
    old_layers: &[ObjectLayer],
) -> Result<(), LifecycleError> {
    let new_uids: Vec<String> = candidate
        .layers
        .iter()
        .map(|layer| layer.layer_uid.clone())
        .collect();
    if let Presence::Present(plan) = &mut candidate.composition_order {
        for context in &mut plan.contexts {
            let context_id = context.context_id.clone();
            let mut old_slots: Vec<u32> = Vec::with_capacity(old_layers.len());
            for layer in old_layers {
                let slot = context
                    .layers
                    .iter()
                    .find(|entry| entry.layer_uid == layer.layer_uid)
                    .map(|entry| entry.natural_position)
                    .ok_or_else(|| incomplete(&context_id, &layer.layer_uid))?;
                old_slots.push(slot);
            }
            for entry in &mut context.layers {
                let position = new_uids
                    .iter()
                    .position(|uid| *uid == entry.layer_uid)
                    .ok_or_else(|| incomplete(&context_id, &entry.layer_uid))?;
                entry.natural_position = old_slots[position];
            }
        }
    }
    Ok(())
}

/// After a delete: the removed layers' entries and elements leave every
/// context, and the remaining natural slots are compacted to `0..n` keeping
/// their relative order. Scopes stay (an emptied scope still carries its frame).
pub(crate) fn remove_layers(candidate: &mut ObjectDocument, gone: &HashSet<&str>) {
    if let Presence::Present(plan) = &mut candidate.composition_order {
        for context in &mut plan.contexts {
            context
                .layers
                .retain(|entry| !gone.contains(entry.layer_uid.as_str()));
            let mut slots: Vec<u32> = context
                .layers
                .iter()
                .map(|entry| entry.natural_position)
                .collect();
            slots.sort_unstable();
            for entry in &mut context.layers {
                let rank = slots.partition_point(|slot| *slot < entry.natural_position);
                entry.natural_position = u32::try_from(rank).unwrap_or(u32::MAX);
            }
            for scope in &mut context.scopes {
                scope
                    .elements
                    .retain(|element| !gone.contains(element.layer_uid.as_str()));
            }
        }
    }
}

/// After a duplicate: in every context the copy joins the layer stack on top
/// (`slot`, the pre-insertion layer count, as layer-create does) with the
/// source's authored order, and each scope gains the copy of every source
/// element (same stroke id, position and authored order).
pub(crate) fn append_duplicate(
    candidate: &mut ObjectDocument,
    source: &str,
    fresh: &str,
    slot: u32,
) -> Result<(), LifecycleError> {
    if let Presence::Present(plan) = &mut candidate.composition_order {
        for context in &mut plan.contexts {
            let authored = context
                .layers
                .iter()
                .find(|entry| entry.layer_uid == source)
                .map(|entry| entry.order.clone())
                .ok_or_else(|| incomplete(&context.context_id, source))?;
            context.layers.push(OrderLayer {
                layer_uid: fresh.to_string(),
                natural_position: slot,
                order: authored,
            });
            for scope in &mut context.scopes {
                let copies: Vec<OrderElement> = scope
                    .elements
                    .iter()
                    .filter(|element| element.layer_uid == source)
                    .map(|element| OrderElement {
                        layer_uid: fresh.to_string(),
                        stroke_id: element.stroke_id.clone(),
                        natural_position: element.natural_position,
                        order: element.order.clone(),
                    })
                    .collect();
                scope.elements.extend(copies);
            }
        }
    }
    Ok(())
}
