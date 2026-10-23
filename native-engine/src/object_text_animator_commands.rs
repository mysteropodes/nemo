//! Layer descriptor replacement/removal through the existing sealed item owner.
use crate::commands::object_fill_owner::item_commands::{ItemEdit, ItemPayload};
use crate::object_document::{object_text_layout_kernels::animator::TextAnimators, Presence};
use crate::request_receipts::DispatchErrorCode as E;
use crate::revision::ObjectSnapshot;

#[path = "object_text_animator_edit.rs"]
pub(crate) mod edit;

pub(super) fn apply(origin: &ObjectSnapshot, p: &ItemPayload) -> Result<ItemEdit, E> {
    let upsert = p.command == "object.text.animator.upsert";
    let editing = p.command == "object.text.animator.edit";
    p.expects(if editing {
        &["layerUid", "animatorId", "edit"]
    } else if upsert {
        &["layerUid", "animator"]
    } else {
        &["layerUid", "animatorId"]
    })?;
    let layer_uid = p
        .layer_uid
        .value()
        .filter(|s| !s.is_empty())
        .ok_or(E::InvalidRequest)?;
    let mut document = origin.document().clone();
    let layer = document
        .layers
        .iter_mut()
        .find(|l| l.layer_uid() == layer_uid)
        .ok_or(E::NotFound)?;
    if origin.document().layer_is_locked(layer_uid) {
        return Err(E::Unavailable);
    }
    if layer.widget.value().is_some() {
        return Err(E::InvalidRequest);
    }
    let mut records = layer
        .text_animators
        .value()
        .map(|v| v.0.clone())
        .unwrap_or_default();
    let id = if upsert {
        &p.animator.value().ok_or(E::InvalidRequest)?.id
    } else {
        p.animator_id
            .value()
            .filter(|s| !s.is_empty())
            .ok_or(E::InvalidRequest)?
    };
    let found = records.iter().position(|r| r.id == *id);
    let changed = if editing {
        let index = found.ok_or(E::NotFound)?;
        let wanted = edit::apply(&records[index], p.edit.value().ok_or(E::InvalidRequest)?)?;
        if wanted == records[index] {
            false
        } else {
            records[index] = wanted;
            true
        }
    } else if upsert {
        let wanted = p.animator.value().ok_or(E::InvalidRequest)?;
        if let Some(index) = found {
            if records[index] == *wanted {
                false
            } else {
                records[index] = wanted.clone();
                true
            }
        } else {
            if records.len() >= 256 {
                return Err(E::InvalidRequest);
            }
            records.push(wanted.clone());
            true
        }
    } else if let Some(index) = found {
        records.remove(index);
        true
    } else {
        false
    };
    if changed {
        layer.text_animators = Presence::Present(TextAnimators(records));
    }
    Ok(ItemEdit {
        document,
        kind: if editing {
            "text.animator.edit"
        } else if upsert {
            "text.animator.upsert"
        } else {
            "text.animator.remove"
        },
        stroke_id: id.clone(),
        changed,
    })
}
