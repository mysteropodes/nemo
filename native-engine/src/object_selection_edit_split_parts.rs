//! Building blocks of the split plan (F08, L02-H): the identities of the new
//! layers, the layer record of one piece and the holder a piece inherits.
use std::collections::HashSet;

use crate::object_document::object_group_kernels::object_selection_edit_plan::SelectionError;
use crate::object_document::{
    ObjectDocument, ObjectLayer, Presence, PropertyHolder, PropertyHolderTarget, PropertyRecord,
};

/// First `count` free `native-layer:{revision}:{n}` identities.
pub(crate) fn fresh_layer_uids(
    document: &ObjectDocument,
    revision: u64,
    count: usize,
) -> Result<Vec<String>, SelectionError> {
    let taken: HashSet<&str> = document
        .layers
        .iter()
        .map(|layer| layer.layer_uid.as_str())
        .collect();
    let mut fresh: Vec<String> = Vec::with_capacity(count);
    for suffix in 0..=(taken.len() + count) {
        if fresh.len() == count {
            break;
        }
        let candidate = format!("native-layer:{revision}:{suffix}");
        if !taken.contains(candidate.as_str()) {
            fresh.push(candidate);
        }
    }
    if fresh.len() < count {
        return Err(SelectionError::IdUnavailable);
    }
    Ok(fresh)
}

/// The holder a piece gets: the element's motion (static values and tracks, never
/// its expressions) plus the layer's expressions and controls.
pub(crate) fn piece_holder(
    layer_holder: Option<&PropertyHolder>,
    element_holder: Option<&PropertyHolder>,
    context_id: &str,
    piece_uid: &str,
) -> Option<PropertyHolder> {
    let mut properties: Vec<PropertyRecord> = Vec::new();
    if let Some(holder) = element_holder {
        for record in &holder.properties {
            if record.static_value.missing() && record.track.missing() {
                continue;
            }
            properties.push(PropertyRecord {
                property: record.property.clone(),
                static_value: record.static_value.clone(),
                track: record.track.clone(),
                expression: Presence::Missing,
            });
        }
    }
    if let Some(holder) = layer_holder {
        for record in &holder.properties {
            let Presence::Present(expression) = &record.expression else {
                continue;
            };
            match properties
                .iter_mut()
                .find(|kept| kept.property == record.property)
            {
                Some(kept) => kept.expression = Presence::Present(expression.clone()),
                None => properties.push(PropertyRecord {
                    property: record.property.clone(),
                    static_value: Presence::Missing,
                    track: Presence::Missing,
                    expression: Presence::Present(expression.clone()),
                }),
            }
        }
    }
    let controls = layer_holder
        .and_then(|holder| holder.controls.value())
        .filter(|controls| !controls.is_empty())
        .cloned();
    if properties.is_empty() && controls.is_none() {
        return None;
    }
    Some(PropertyHolder {
        target: PropertyHolderTarget {
            context_id: context_id.to_string(),
            layer_uid: piece_uid.to_string(),
            stroke_id: Presence::Missing,
        },
        controls: match controls {
            Some(controls) => Presence::Present(controls),
            None => Presence::Missing,
        },
        properties,
    })
}

pub(crate) fn piece_layer(
    source: &ObjectLayer,
    uid: String,
    name: String,
    color: &str,
) -> ObjectLayer {
    let mut piece = source.clone();
    piece.layer_uid = uid;
    piece.name = Presence::Present(name);
    piece.visible = Presence::Present(true);
    piece.locked = Presence::Present(false);
    piece.color = Presence::Present(color.to_string());
    piece.widget = Presence::Missing;
    piece.groups = Presence::Missing;
    // `folderId` is carried only when it names a folder (`ld[f] != null`).
    if matches!(piece.folder_id, Presence::Present(None)) {
        piece.folder_id = Presence::Missing;
    }
    piece
}
