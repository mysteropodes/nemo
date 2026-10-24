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
        mesh_source: Presence::Missing,
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

/// A painted companion follows its independent anchor; per-frame anchor rank is the baseline split rule.
pub(crate) fn compound_piece(document:&ObjectDocument,record:&crate::object_document::ObjectRecord,count:usize)->Result<usize,SelectionError> {
    use crate::object_document::object_group_kernels::object_selection_compound::{is_companion,same_cell};
    let anchor=if is_companion(record) {
        let link=record.vector_ext.value().and_then(|x|x.linked_fill_id.value()).ok_or(SelectionError::ClipMismatch)?;
        document.objects.iter().find(|r|same_cell(&r.target,&record.target)&&!is_companion(r)
            &&r.vector_ext.value().and_then(|x|x.linked_fill_id.value())==Some(link)).ok_or(SelectionError::ClipMismatch)?
    }else{record};
    document.objects.iter().filter(|r|same_cell(&r.target,&anchor.target)&&!is_companion(r))
        .position(|r|r.target.stroke_id==anchor.target.stroke_id).map(|rank|rank.min(count-1)).ok_or(SelectionError::ClipMismatch)
}
pub(crate) fn raster_piece(document:&ObjectDocument,raster:&crate::object_document::RasterEntry,count:usize)->Result<usize,SelectionError> {
    use crate::object_document::object_group_kernels::object_selection_compound::same_cell;
    let Some(group)=raster.item.brush_group_id() else{return Ok(count-1);};
    let anchors:Vec<_>=document.objects.iter().filter(|r|same_cell(&r.target,&raster.target))
        .filter(|r|r.vector_ext.value().and_then(|x|x.bitmap_brush.value()).is_some_and(|b|b.brush_group_id==group)).collect();
    match anchors.as_slice(){[]=>Ok(count-1),[anchor]=>compound_piece(document,anchor,count),_=>Err(SelectionError::ClipMismatch)}
}
