//! Ordered top-level storage admits static parent links only as a valid UID graph.
use crate::object_codec::{validate_content_inner, ObjectCodecError, ObjectCodecErrorKind};
use crate::object_document::{ObjectLayer, ObjectRecord};
use std::collections::HashMap;

/// Existing scene-content admission plus parent graph integrity; legacy/symbol callers opt out.
pub(crate) fn validate_scene_content(
    total_frames: u32,
    declared_layers: &[ObjectLayer],
    objects: &[ObjectRecord],
    complete_contexts: bool,
) -> Result<(), ObjectCodecError> {
    validate_content_inner(
        total_frames,
        declared_layers,
        objects,
        complete_contexts,
        true,
    )?;
    let invalid = |message| ObjectCodecError::new(ObjectCodecErrorKind::Invalid, message);
    // Existing admission above has already checked nonempty, unique layer UIDs.
    let indices: HashMap<&str, usize> = declared_layers
        .iter()
        .enumerate()
        .map(|(index, layer)| (layer.layer_uid(), index))
        .collect();
    let mut parents = Vec::with_capacity(declared_layers.len());
    for (index, layer) in declared_layers.iter().enumerate() {
        let parent = match layer.parent_layer_uid.value().and_then(Option::as_ref) {
            None => None,
            Some(uid) => {
                if uid.is_empty() {
                    return Err(invalid("empty parentLayerUid"));
                }
                let target = *indices
                    .get(uid.as_str())
                    .ok_or_else(|| invalid("unknown parentLayerUid"))?;
                if target == index {
                    return Err(invalid("layer cannot parent itself"));
                }
                Some(target)
            }
        };
        parents.push(parent);
    }
    // Three-state traversal is linear and does not recurse on an authored chain.
    let mut state = vec![0u8; declared_layers.len()];
    for start in 0..declared_layers.len() {
        let mut current = Some(start);
        while let Some(index) = current {
            match state[index] {
                2 => break,
                1 => return Err(invalid("cyclic layer parenting")),
                _ => {}
            }
            state[index] = 1;
            current = parents[index];
        }
        current = Some(start);
        while let Some(index) = current {
            if state[index] == 2 {
                break;
            }
            state[index] = 2;
            current = parents[index];
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/object_parent_links.rs"]
mod tests;
