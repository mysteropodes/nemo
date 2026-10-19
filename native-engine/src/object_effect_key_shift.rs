//! Effect-track half of the same whole-layer Motion pass; no selected/standing effect lock route.
use super::retimed;
use crate::object_document::{FrameScopeKind, LayerEffect, ObjectDocument, Presence};
use crate::request_receipts::DispatchErrorCode;

fn present_mut<T>(slot: &mut Presence<T>) -> Option<&mut T> {
    match slot {
        Presence::Missing => None,
        Presence::Present(value) => Some(value),
    }
}
fn shift_stack(stack: &mut [LayerEffect], dx: i64, total: u32) -> Result<bool, DispatchErrorCode> {
    let mut touched = false;
    for effect in stack {
        if let Some(Some(tracks)) = present_mut(&mut effect.keys) {
            for (_, track) in tracks.entries_mut() {
                if let Some(track) = track {
                    if !track.keys.is_empty() {
                        *track = retimed(track, None, dx, total)?;
                        touched = true;
                    }
                }
            }
            // Clamp/rekey can change the encoded byte count; validate before publication.
            tracks
                .checked()
                .map_err(|_| DispatchErrorCode::Unavailable)?;
        }
    }
    Ok(touched)
}
fn shift_objects(
    objects: &mut [crate::object_document::ObjectRecord],
    context: &str,
    layer: &str,
    dx: i64,
    total: u32,
) -> Result<bool, DispatchErrorCode> {
    let mut touched = false;
    for object in objects {
        if object.target.context_id == context
            && object.target.layer_uid == layer
            && object.target.frame_scope.kind == FrameScopeKind::Authored
        {
            if let Some(Some(stack)) = present_mut(&mut object.effects) {
                // Each stored array is visited once. Held exposures do not materialize a new record.
                touched |= shift_stack(stack, dx, total)?;
            }
        }
    }
    Ok(touched)
}
pub(super) fn shift_layer(
    document: &mut ObjectDocument,
    context: &str,
    layer: &str,
    dx: i64,
    total: u32,
) -> Result<bool, DispatchErrorCode> {
    let mut touched = false;
    if context == "scene-root" {
        if let Some(layer) = document.layers.iter_mut().find(|l| l.layer_uid == layer) {
            if let Some(stack) = present_mut(&mut layer.effects) {
                touched |= shift_stack(stack, dx, total)?;
            }
        }
    }
    touched |= shift_objects(&mut document.objects, context, layer, dx, total)?;
    if let Some(tables) = present_mut(&mut document.component_tables) {
        if let Some(set) = present_mut(&mut tables.components) {
            if let Some(component) = set.components.iter_mut().find(|c| c.symbol_id == context) {
                if let Some(layer) = component.layers.iter_mut().find(|l| l.layer_uid == layer) {
                    if let Some(stack) = present_mut(&mut layer.effects) {
                        touched |= shift_stack(stack, dx, total)?;
                    }
                }
                touched |= shift_objects(&mut component.objects, context, layer, dx, total)?;
            }
        }
    }
    Ok(touched)
}
