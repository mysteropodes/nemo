//! Split-layer-into-elements plan (F08, L02-H): `splitLayerIntoElementsCore`
//! (`src/js/app.js`, fbe4b8a3) over the native layer, object, holder, order and
//! group records.
//!
//! The elements are the shapes of the drawing the playhead shows (a hold shows
//! its key). Piece `e` of `n` takes the shape at position `e` of every authored
//! frame's z-order (`strokes[e]`) and the last piece takes everything from
//! position `n - 1` on (`strokes.slice(n - 1)`), the baseline's positional rule,
//! so hand-drawn frames with their own stroke identities stay together. Pieces
//! are named `<Layer> U+2014 <label> N`, all get the palette color of the layer count
//! before the split, the last piece keeps the layer uid (so references to the
//! layer still resolve) and the others get `native-layer:{revision}:{n}`.
//! Motion promotes the element holder to the piece's layer holder and the layer's
//! expressions and controls are copied to every piece; the layer's own motion
//! tracks and statics are dropped, exactly as the baseline does.
use std::collections::HashSet;
use crate::object_document::object_group_kernels::object_selection_compound as compound;

use crate::object_document::object_group_kernels::object_group_record::GroupTable;
use crate::object_document::object_group_kernels::object_selection_edit_frames::{
    default_order, effective_frame, in_cell, in_layer,
};
use crate::object_document::object_group_kernels::object_selection_edit_groups::settle;
use crate::object_document::object_group_kernels::object_selection_edit_plan::{
    layer_index, SelectionError, SelectionPlan, MAX_REVISION,
};
use crate::object_document::object_group_kernels::object_selection_edit_split_parts::{
    fresh_layer_uids, piece_holder, piece_layer, compound_piece, raster_piece,
};
use crate::object_document::{
    AuthoredOrder, FrameScopeKind, ObjectDocument, ObjectLayer, ObjectRecord, OrderElement,
    OrderLayer, Presence, PropertyHolder,
};

/// `LAYER_COLOR_PALETTE` of `src/js/app.js`.
const PALETTE: [&str; 8] = [
    "#5FA875", "#B49BE6", "#F0A585", "#E88BAE", "#E7C74C", "#EE7C58", "#7EA0FA", "#5FD1C4",
];

/// The two locale strings of `elementLabel` (`autoNameShape` / `autoNameStroke`).
#[derive(Debug, Clone, Copy)]
pub(crate) struct SplitLabels<'a> {
    pub(crate) shape: &'a str,
    pub(crate) stroke: &'a str,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct SplitRequest<'a> {
    pub(crate) context_id: &'a str,
    pub(crate) layer_uid: &'a str,
    /// The playhead frame whose shapes are the elements.
    pub(crate) frame: u32,
    pub(crate) labels: SplitLabels<'a>,
    /// The content revision the edit will publish (identity source).
    pub(crate) revision: u64,
}

/// `splitLayerIntoElementsCore`.
pub(crate) fn plan_split(
    document: &ObjectDocument,
    request: &SplitRequest<'_>,
) -> Result<SelectionPlan, SelectionError> {
    let index = layer_index(document, request.layer_uid)?;
    let source = &document.layers[index];
    let (context, uid) = (request.context_id, request.layer_uid);
    if document.layer_is_locked(uid) {
        return Err(SelectionError::Locked(uid.to_string()));
    }
    if request.frame >= document.total_frames {
        return Err(SelectionError::FrameOutOfRange(request.frame));
    }
    if request.labels.shape.is_empty() || request.labels.stroke.is_empty() {
        return Err(SelectionError::EmptyLabel);
    }
    if request.revision > MAX_REVISION {
        return Err(SelectionError::RevisionOutOfRange);
    }
    if document.objects.iter().any(|record| {
        record.target.layer_uid == uid
            && record.target.context_id != context
            && record.target.frame_scope.kind == FrameScopeKind::Authored
    }) {
        return Err(SelectionError::MultiContext(uid.to_string()));
    }
    let shown = effective_frame(source, context, request.frame);
    let elements: Vec<&ObjectRecord> = match shown {
        Some(frame) => document
            .objects
            .iter()
            .filter(|record| in_cell(record, context, uid, frame) && !compound::is_companion(record))
            .collect(),
        None => Vec::new(),
    };
    compound::validate_document(document)?;
    let has_compound=document.objects.iter().filter(|r|in_layer(r,context,uid)).any(|r|r.vector_ext.value()
        .is_some_and(|x|!x.linked_fill_id.missing()||!x.bitmap_brush.missing()));
    let n = elements.len();
    if n < 2 {
        return Err(SelectionError::NeedTwoElements);
    }
    let mut uids = fresh_layer_uids(document, request.revision, n - 1)?;
    uids.push(uid.to_string());
    let base = match source.name.value() {
        Some(name) if !name.is_empty() => name.clone(),
        _ => "Layer".to_string(),
    };
    let color = PALETTE[document.layers.len() % PALETTE.len()];
    let mut pieces: Vec<ObjectLayer> = Vec::with_capacity(n);
    for (e, element) in elements.iter().enumerate() {
        let label = if element.solid_fill().is_some() {
            request.labels.shape
        } else {
            request.labels.stroke
        };
        let name = format!("{base} \u{2014} {label} {}", e + 1);
        pieces.push(piece_layer(source, uids[e].clone(), name, color));
    }

    // Objects: every authored record of the layer moves to its piece, in place.
    let mut objects: Vec<ObjectRecord> = Vec::with_capacity(document.objects.len());
    let mut piece_records: Vec<Vec<ObjectRecord>> = vec![Vec::new(); n];
    for record in &document.objects {
        if !in_layer(record, context, uid) {
            objects.push(record.clone());
            continue;
        }
        let piece = compound_piece(document, record, n)?;
        let mut moved = record.clone();
        moved.target.layer_uid = uids[piece].clone();
        piece_records[piece].push(moved.clone());
        objects.push(moved);
    }

    let mut candidate = document.clone();
    candidate.objects = objects;
    if let Presence::Present(items)=&mut candidate.items {
        if let Presence::Present(rasters)=&mut items.raster {
            for raster in rasters.iter_mut().filter(|r|r.target.context_id==context && r.target.layer_uid==uid && r.target.frame_scope.kind==FrameScopeKind::Authored) {
                raster.target.layer_uid=uids[raster_piece(document,raster,n)?].clone();
            }
        }
    }
    let piece_item_targets:Vec<Vec<_>>=uids.iter().map(|piece|candidate.items.value().map_or(vec![],|items|items.targets()
        .filter(|t|t.context_id==context && t.layer_uid==*piece).cloned().collect())).collect();
    let _replaced: Vec<ObjectLayer> = candidate.layers.splice(index..=index, pieces).collect();

    // Order plan: pieces take consecutive natural slots where the layer was.
    if let Presence::Present(plan) = &mut candidate.composition_order {
        for order_context in &mut plan.contexts {
            let Some(at) = order_context
                .layers
                .iter()
                .position(|entry| entry.layer_uid == uid)
            else {
                return Err(SelectionError::OrderPlanIncomplete(
                    order_context.context_id.clone(),
                ));
            };
            let is_context = order_context.context_id == context;
            let slot = order_context.layers[at].natural_position;
            let shifted = u32::try_from(n - 1).unwrap_or(u32::MAX);
            let mut element_orders: Vec<AuthoredOrder> = Vec::with_capacity(n);
            for element in &elements {
                let found = shown.and_then(|frame| {
                    order_context
                        .scopes
                        .iter()
                        .find(|scope| {
                            scope.frame_scope.kind == FrameScopeKind::Authored
                                && scope.frame_scope.frame == frame
                        })
                        .and_then(|scope| {
                            scope.elements.iter().find(|candidate| {
                                candidate.layer_uid == uid
                                    && candidate.stroke_id == element.target.stroke_id
                            })
                        })
                        .map(|found| found.order.clone())
                });
                element_orders.push(found.unwrap_or_else(default_order));
            }
            for entry in order_context.layers.iter_mut() {
                if entry.natural_position > slot {
                    entry.natural_position += shifted;
                }
            }
            let entries: Vec<OrderLayer> = (0..n)
                .map(|e| OrderLayer {
                    layer_uid: uids[e].clone(),
                    natural_position: slot + u32::try_from(e).unwrap_or(u32::MAX),
                    order: if is_context {
                        element_orders[e].clone()
                    } else {
                        default_order()
                    },
                })
                .collect();
            let _replaced: Vec<OrderLayer> =
                order_context.layers.splice(at..=at, entries).collect();
            if is_context {
                for scope in order_context
                    .scopes
                    .iter_mut()
                    .filter(|scope| scope.frame_scope.kind == FrameScopeKind::Authored)
                {
                    let original=scope.elements.clone();
                    scope.elements.retain(|element| element.layer_uid != uid);
                    let frame = scope.frame_scope.frame;
                    for (e, records) in piece_records.iter().enumerate() {
                        let mut targets:Vec<_>=records.iter().filter(|r|r.target.frame_scope.frame==frame).map(|r|&r.target)
                            .chain(piece_item_targets[e].iter().filter(|t|t.frame_scope.frame==frame)).collect();
                        if targets.iter().any(|t|piece_item_targets[e].iter().any(|item|item==*t)) || records.iter().any(compound::is_companion) {
                            targets.sort_by_key(|t|original.iter().find(|old|old.layer_uid==uid&&old.stroke_id==t.stroke_id).map_or(u32::MAX,|old|old.natural_position));
                        }
                        for (position,target) in targets.into_iter().enumerate() {
                            scope.elements.push(OrderElement{layer_uid:uids[e].clone(),stroke_id:target.stroke_id.clone(),
                                natural_position:u32::try_from(position).map_err(|_|SelectionError::ClipMismatch)?,
                                order:if has_compound {original.iter().find(|old|old.layer_uid==uid&&old.stroke_id==target.stroke_id)
                                    .map(|old|old.order.clone()).unwrap_or_else(default_order)}else{default_order()}});
                        }
                    }
                }
            }
        }
    }

    // Holders: the element's motion becomes the piece's layer motion.
    if let Presence::Present(holders) = &mut candidate.property_holders {
        let on_layer = |holder: &&PropertyHolder| {
            holder.target.context_id == context && holder.target.layer_uid == uid
        };
        let layer_holder = holders
            .iter()
            .filter(on_layer)
            .find(|holder| holder.target.stroke_id.missing())
            .cloned();
        let element_holders: Vec<Option<PropertyHolder>> = elements
            .iter()
            .map(|element| {
                holders
                    .iter()
                    .filter(on_layer)
                    .find(|holder| {
                        holder.target.stroke_id.value() == Some(&element.target.stroke_id)
                    })
                    .cloned()
            })
            .collect();
        holders.retain(|holder| {
            !(holder.target.context_id == context && holder.target.layer_uid == uid)
        });
        for e in 0..n {
            if let Some(holder) = piece_holder(
                layer_holder.as_ref(),
                element_holders[e].as_ref(),
                context,
                &uids[e],
            ) {
                holders.push(holder);
            }
        }
    }

    // Groups: only the last piece can combine; entries that tag its shapes stay.
    let last = candidate
        .layers
        .iter()
        .position(|layer| layer.layer_uid == uid);
    if let (Some(last), Some(table)) = (last, source.groups.value()) {
        if table.context_id != context {
            candidate.layers[last].groups = Presence::Present(table.clone());
        } else {
            let strokes: HashSet<String> = piece_records[n - 1]
                .iter()
                .map(|record| record.target.stroke_id.clone())
                .collect();
            let members: Vec<_> = table
                .members
                .iter()
                .filter(|member| strokes.contains(&member.stroke_id))
                .cloned()
                .collect();
            let keep: HashSet<&str> = members
                .iter()
                .map(|member| member.group_id.as_str())
                .collect();
            let mut kept_entries: Vec<_> = table
                .entries
                .iter()
                .filter(|entry| keep.contains(entry.gid.as_str()))
                .cloned()
                .collect();
            let mut kept_members = members.clone();
            settle(&mut kept_entries, &mut kept_members, &strokes);
            if !kept_entries.is_empty() {
                candidate.layers[last].groups = Presence::Present(GroupTable {
                    context_id: context.to_string(),
                    entries: kept_entries,
                    members: kept_members,
                });
            }
        }
    }

    compound::validate_document(&candidate)?;
    Ok(SelectionPlan::changed(document,candidate,shown.unwrap_or(request.frame),false,
        Vec::new(),Vec::new(),Vec::new(),Vec::new(),uids))
}
