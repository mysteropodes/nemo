//! #2372 Bring to front / Send to back of the Select selection as a step of `object.vector.batch`. An `order` step names one record
//! or stored item and the end of its layer's stack in the cell it lives in (`placement`: `front` or `back`). It moves that element
//! and what belongs to it together, in their current relative order: a pressure ribbon takes its linked fill, a texture anchor its
//! stamped copies (baseline select-bridge `onContext`: Paper `bringToFront` / `sendToBack`, which the Select menu runs over the
//! selection in selection order for the front and in reverse order for the back, so the selected items keep their order). The
//! stack is the composition order plan's natural positions when the context has one (every element of the layer in that frame
//! scope, vector or item), otherwise the record order of the cell (a component without a plan paints its records in that order and
//! its items above them, so an item has no place to move to there). Nothing else changes: not a style, not a geometry, not the
//! frame count. A layer that is locked or deformed by an active path effect is refused, as every drawing edit is.
use crate::commands::object_fill_owner::item_commands::check_layer as check_item_layer;
use crate::commands::object_fill_owner::vector_edit::order::OrderEditError;
use crate::commands::object_fill_owner::vector_edit::{
    check_drawing_layer, check_target, is_vector_record, order_code, Edit, VectorEditKind,
};
use crate::object_document::{ObjectDocument, ObjectRecord, ObjectTarget, Presence};
use crate::request_receipts::DispatchErrorCode;
use std::collections::HashSet;

/// Which end of the layer's stack the element goes to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    Front,
    Back,
}
impl Mode {
    /// The step's `placement`: `front` or `back` (a create's `behind` is not an order).
    pub(super) fn parse(value: &Presence<String>) -> Result<Self, DispatchErrorCode> {
        match value.value().map(String::as_str) {
            Some("front") => Ok(Self::Front),
            Some("back") => Ok(Self::Back),
            _ => Err(DispatchErrorCode::InvalidRequest),
        }
    }
}

fn same_cell(record: &ObjectTarget, target: &ObjectTarget) -> bool {
    record.context_id == target.context_id
        && record.layer_uid == target.layer_uid
        && record.frame_scope == target.frame_scope
}

/// The stroke ids that travel with `target`, the target first: the linked fill of a ribbon and the copies of a texture group, exactly
/// the companions a transform of the same record brings. A texture copy is never named on its own.
fn travelling(
    source: &ObjectDocument,
    existing: &ObjectRecord,
    target: &ObjectTarget,
) -> Result<HashSet<String>, DispatchErrorCode> {
    let ext = existing.vector_ext.value();
    let texture = ext.and_then(|e| e.brush_texture.value());
    if texture.is_some_and(|t| t.is_brush_texture_copy.value() == Some(&true)) {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let link = ext.and_then(|e| e.linked_fill_id.value());
    let group = texture.and_then(|t| t.brush_group_id.value());
    let mut ids = HashSet::from([target.stroke_id.clone()]);
    for record in source
        .objects
        .iter()
        .filter(|r| same_cell(&r.target, target) && r.target != *target && is_vector_record(r))
    {
        let other = record.vector_ext.value();
        let fill = link.is_some()
            && other.is_some_and(|e| {
                e.is_linked_fill_companion.value() == Some(&true)
                    && e.linked_fill_id.value() == link
            });
        let copy = group.is_some()
            && other
                .and_then(|e| e.brush_texture.value())
                .is_some_and(|t| {
                    t.is_brush_texture_copy.value() == Some(&true)
                        && t.brush_group_id.value() == group
                });
        if fill || copy {
            ids.insert(record.target.stroke_id.clone());
        }
    }
    Ok(ids)
}

/// `order` against a document being edited: `target` (a vector record or a stored item of the cell it names) goes to the `mode` end of
/// its layer's stack with its companions. `changed` is false when it was already there.
pub(super) fn order_in(
    source: &ObjectDocument,
    target: &ObjectTarget,
    mode: Mode,
) -> Result<Edit, DispatchErrorCode> {
    check_target(source, target)?;
    let record = source.objects.iter().find(|r| r.target == *target);
    let item = source
        .item_records()
        .is_some_and(|items| items.targets().any(|t| t == target));
    let moved = match record {
        Some(existing) if is_vector_record(existing) => travelling(source, existing, target)?,
        Some(_) => return Err(DispatchErrorCode::Unavailable),
        None if item => HashSet::from([target.stroke_id.clone()]),
        None => return Err(DispatchErrorCode::NotFound),
    };
    check_drawing_layer(source, target)?;
    if item {
        check_item_layer(source, target)?;
    }
    let mut document = source.clone();
    let planned = match &mut document.composition_order {
        Presence::Present(plan) => plan
            .contexts
            .iter_mut()
            .find(|c| c.context_id == target.context_id),
        Presence::Missing => None,
    };
    let changed = match planned {
        Some(context) => {
            let scope = context
                .scopes
                .iter_mut()
                .find(|s| s.frame_scope == target.frame_scope)
                .ok_or_else(|| order_code(OrderEditError::ElementAbsent))?;
            // The layer's stack, bottom to top; positions are 0..n without gaps (storage admission), so the sort is a permutation.
            let mut stack: Vec<usize> = (0..scope.elements.len())
                .filter(|&i| scope.elements[i].layer_uid == target.layer_uid)
                .collect();
            stack.sort_by_key(|&i| scope.elements[i].natural_position);
            let (carried, kept): (Vec<usize>, Vec<usize>) = stack
                .iter()
                .partition(|&&i| moved.contains(&scope.elements[i].stroke_id));
            if carried.len() != moved.len() {
                return Err(order_code(OrderEditError::ElementAbsent));
            }
            let order: Vec<usize> = if mode == Mode::Front {
                kept.into_iter().chain(carried).collect()
            } else {
                carried.into_iter().chain(kept).collect()
            };
            let changed = order != stack;
            for (position, &i) in order.iter().enumerate() {
                scope.elements[i].natural_position = u32::try_from(position)
                    .map_err(|_| order_code(OrderEditError::PositionOverflow))?;
            }
            changed
        }
        None if item => false,
        None => {
            let at: Vec<usize> = (0..document.objects.len())
                .filter(|&i| same_cell(&document.objects[i].target, target))
                .collect();
            let cell: Vec<ObjectRecord> = at.iter().map(|&i| document.objects[i].clone()).collect();
            let (carried, kept): (Vec<ObjectRecord>, Vec<ObjectRecord>) = cell
                .iter()
                .cloned()
                .partition(|r| moved.contains(&r.target.stroke_id));
            if carried.len() != moved.len() {
                return Err(DispatchErrorCode::NotFound);
            }
            let order: Vec<ObjectRecord> = if mode == Mode::Front {
                kept.into_iter().chain(carried).collect()
            } else {
                carried.into_iter().chain(kept).collect()
            };
            let changed = order
                .iter()
                .map(|r| &r.target)
                .ne(cell.iter().map(|r| &r.target));
            for (&slot, record) in at.iter().zip(order) {
                document.objects[slot] = record;
            }
            changed
        }
    };
    Ok(Edit {
        document,
        kind: VectorEditKind::Batch,
        stroke_id: target.stroke_id.clone(),
        stroke_ids: vec![target.stroke_id.clone()],
        changed,
        skipped: 0,
    })
}
