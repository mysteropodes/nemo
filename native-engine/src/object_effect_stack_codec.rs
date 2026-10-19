//! Actual stored effect stacks use the existing scalar-track predicate before admission.
use crate::object_codec::order::validate_effect_track;
use crate::object_codec::{ObjectCodecError, ObjectCodecErrorKind};
use crate::object_document::{LayerEffect, ObjectLayer, ObjectRecord};

fn validate_stack(stack: &[LayerEffect]) -> Result<(), ObjectCodecError> {
    for effect in stack {
        if let Some(Some(tracks)) = effect.keys.value() {
            tracks.checked().map_err(|_| {
                ObjectCodecError::new(
                    ObjectCodecErrorKind::Invalid,
                    "effect parameter track map exceeds its bounds",
                )
            })?;
            for (_, track) in tracks.entries() {
                if let Some(track) = track {
                    validate_effect_track(track)?;
                }
            }
        }
    }
    Ok(())
}
pub(super) fn validate_fields(
    layers: &[ObjectLayer],
    objects: &[ObjectRecord],
) -> Result<(), ObjectCodecError> {
    for layer in layers {
        if let Some(stack) = layer.effects() {
            validate_stack(stack)?;
        }
    }
    for object in objects {
        if let Some(stack) = object.effects() {
            validate_stack(stack)?;
        }
    }
    Ok(())
}
