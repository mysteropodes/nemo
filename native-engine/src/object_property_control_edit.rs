//! Expression-control declaration edits (D18-B): add, rename, remove and move on one holder's
//! `ExpressionControl` list, inside the existing property transaction. Baseline: `addExprControl`,
//! `renameExprControl`, `removeExprControl` and `moveExprControl` of motion.js @1065e9d8.
//!
//! - The control key is the request's `property`: the caller proposes a fresh `xc_` key for an add
//!   (the engine only checks its shape and that it is unused) and names an existing one otherwise.
//! - A name is the JS `trim` of the input, never empty, unique per holder under the JS trim and
//!   lower-case rule (a clash is `invalid_request`, the baseline refuses with a toast).
//! - A removal drops the control's track, static value and expression record in the same candidate,
//!   so there is one revision and one history entry. A widget axis that points at the control makes
//!   the removal `unavailable`: the axis must never be left dangling.
//! - A move past either end changes nothing (no revision, no history, redo preserved).
//! - No saved-format change: the `controls` list and the property records already exist.
use crate::commands::{ObjectFillOwner, PreparedPropertyEdit};
use crate::object_document::{
    ExpressionControl, ObjectDocument, Presence, PropertyHolder, PropertyHolderTarget,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde_json::Number;

/// Baseline `CONTROL_TYPES`, in menu order.
const TYPES: [&str; 5] = ["number", "checkbox", "angle", "point", "color"];
const MAX_NAME_CHARS: usize = 128;
const MAX_KEY_BYTES: usize = 128;

/// The closed control intents; the key travels as the request's `property`.
#[derive(Debug, Clone, Copy)]
pub(super) enum ControlOp<'a> {
    Add { kind: &'a str, name: &'a str },
    Rename { name: &'a str },
    Remove,
    Move { direction: i64 },
}

/// Baseline `CONTROL_DEFAULT`.
fn default_for(kind: &str) -> Vec<Number> {
    let values: &[i64] = match kind {
        "point" => &[0, 0],
        "color" => &[255, 255, 255, 255],
        _ => &[0],
    };
    values.iter().map(|v| Number::from(*v)).collect()
}

fn key_shape(key: &str) -> bool {
    key.len() <= MAX_KEY_BYTES
        && key.len() > 3
        && key.starts_with("xc_")
        && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn authored_name(name: &str) -> Result<String, DispatchErrorCode> {
    let trimmed = PropertyHolder::trimmed_control_name(name);
    if trimmed.is_empty() || trimmed.chars().count() > MAX_NAME_CHARS {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    Ok(trimmed.to_owned())
}

/// A widget axis binding this key on the layer-level holder blocks its removal.
fn widget_axis_uses(document: &ObjectDocument, target: &PropertyHolderTarget, key: &str) -> bool {
    target.stroke_id.missing()
        && document
            .layers
            .iter()
            .filter(|layer| layer.layer_uid == target.layer_uid)
            .filter_map(|layer| layer.widget.value())
            .any(|widget| {
                widget.x.key == key || widget.y.value().is_some_and(|axis| axis.key == key)
            })
}

/// Another control already carries this name under the JS trim and lower-case rule.
fn clash(controls: &[ExpressionControl], name: &str, except: Option<usize>) -> bool {
    let wanted = PropertyHolder::normalized_control_name(name);
    controls.iter().enumerate().any(|(i, c)| {
        Some(i) != except && PropertyHolder::normalized_control_name(&c.name) == wanted
    })
}

impl ObjectFillOwner {
    pub(super) fn apply_control_edit(
        origin: ObjectSnapshot,
        mut document: ObjectDocument,
        target: &PropertyHolderTarget,
        key: &str,
        op: ControlOp<'_>,
    ) -> Result<PreparedPropertyEdit, DispatchErrorCode> {
        let mut holders = document
            .property_holders
            .value()
            .cloned()
            .unwrap_or_default();
        let index = holders.iter().position(|h| h.target() == target);
        let mut holder = index
            .map(|i| holders[i].clone())
            .unwrap_or_else(|| PropertyHolder {
                mesh_source: Presence::Missing,
                target: target.clone(),
                controls: Presence::Missing,
                properties: vec![],
            });
        // An absent target cannot invent a holder, exactly as for the value edits.
        PropertyHolder::validate_with_items(
            &Presence::Present(vec![holder.clone()]),
            &document.layers,
            &document.objects,
            document.composition_order.value(),
            document.component_tables(),
            document.items.value(),
            document.image_meshes.value(),
        )
        .map_err(|_| DispatchErrorCode::NotFound)?;
        let mut controls = holder.controls.value().cloned().unwrap_or_default();
        let at = controls.iter().position(|c| c.key == key);
        match op {
            ControlOp::Add { kind, name } => {
                if !TYPES.contains(&kind) || !key_shape(key) || at.is_some() {
                    return Err(DispatchErrorCode::InvalidRequest);
                }
                let name = authored_name(name)?;
                if clash(&controls, &name, None) {
                    return Err(DispatchErrorCode::InvalidRequest);
                }
                controls.push(ExpressionControl {
                    key: key.to_owned(),
                    name,
                    kind: kind.to_owned(),
                    default_value: Presence::Present(default_for(kind)),
                });
            }
            ControlOp::Rename { name } => {
                let at = at.ok_or(DispatchErrorCode::NotFound)?;
                let name = authored_name(name)?;
                if clash(&controls, &name, Some(at)) {
                    return Err(DispatchErrorCode::InvalidRequest);
                }
                controls[at].name = name;
            }
            ControlOp::Remove => {
                let at = at.ok_or(DispatchErrorCode::NotFound)?;
                if widget_axis_uses(&document, target, key) {
                    return Err(DispatchErrorCode::Unavailable);
                }
                controls.remove(at);
                holder.properties.retain(|record| record.property != key);
            }
            ControlOp::Move { direction } => {
                let at = at.ok_or(DispatchErrorCode::NotFound)?;
                let to = match direction {
                    -1 => at.checked_sub(1),
                    1 => Some(at + 1).filter(|to| *to < controls.len()),
                    _ => return Err(DispatchErrorCode::InvalidRequest),
                };
                if let Some(to) = to {
                    controls.swap(at, to);
                }
            }
        }
        holder.controls = if controls.is_empty() {
            Presence::Missing
        } else {
            Presence::Present(controls)
        };
        let empty = holder.controls.missing()
            && holder.properties.is_empty()
            && holder.mesh_source.missing();
        match (index, empty) {
            (Some(i), true) => {
                holders.remove(i);
            }
            (Some(i), false) => holders[i] = holder,
            (None, false) => holders.push(holder),
            (None, true) => {}
        }
        let before = document.property_holders.clone();
        document.property_holders = if holders.is_empty() {
            Presence::Missing
        } else {
            Presence::Present(holders)
        };
        if document.property_holders != before {
            ObjectSnapshot::validate_drawing(&document)
                .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        }
        Ok(PreparedPropertyEdit::sealed(origin, document))
    }
}
