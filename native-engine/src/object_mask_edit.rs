//! Original Path mask panel edits through the existing sealed vector owner.
//! Geometry, paint, order and unrelated frames remain authored exactly as before.
use crate::commands::object_fill_owner::vector_edit::{
    check_drawing_layer, check_target, is_vector_record, same_record, Edit, VectorEditKind,
};
use crate::object_document::{ObjectTarget, Presence};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;

/// The existing vector operation carries this bounded panel intent, not new authority.
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(super) enum MaskEdit {
    Mode { mode: String },
    Feather { value: Number },
    // An empty struct is intentional: a unit variant ignores trailing map fields.
    Unset {},
}

pub(super) fn apply(
    origin: &ObjectSnapshot,
    target: ObjectTarget,
    intent: MaskEdit,
) -> Result<Edit, DispatchErrorCode> {
    check_target(origin.document(), &target)?;
    let selected = origin
        .document()
        .objects
        .iter()
        .find(|record| record.target == target)
        .ok_or(DispatchErrorCode::NotFound)?;
    if !is_vector_record(selected) || !selected.is_mask() {
        return Err(DispatchErrorCode::Unavailable);
    }
    check_drawing_layer(origin.document(), &target)?;
    match &intent {
        MaskEdit::Mode { mode } if !["add", "subtract", "intersect"].contains(&mode.as_str()) => {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        MaskEdit::Feather { value } if !value.as_f64().is_some_and(|value| value.is_finite()) => {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        _ => {}
    }
    let shared = matches!(&intent, MaskEdit::Feather { .. });
    let mut document = origin.document().clone();
    let mut changed = false;
    for record in &mut document.objects {
        let in_scope = record.target.context_id == target.context_id
            && record.target.layer_uid == target.layer_uid
            && record.target.frame_scope == target.frame_scope;
        if !(record.target == target || shared && in_scope && record.is_mask()) {
            continue;
        }
        let before = record.clone();
        // is_mask() proved that this real vector extension is present.
        let Presence::Present(ext) = &mut record.vector_ext else {
            return Err(DispatchErrorCode::Unavailable);
        };
        match &intent {
            MaskEdit::Mode { mode } => ext.mask_mode = Presence::Present(Some(mode.clone())),
            MaskEdit::Feather { value } => {
                // Original panel Math.max(0, parsed): an authored negative or
                // negative zero input becomes zero; stored imported negatives
                // are otherwise preserved by the codec and ignored by rendering.
                let value = if value.as_f64().unwrap() <= 0.0 {
                    Number::from(0)
                } else {
                    value.clone()
                };
                ext.mask_feather = Presence::Present(Some(value));
            }
            MaskEdit::Unset {} => {
                ext.is_mask = Presence::Missing;
                ext.mask_mode = Presence::Missing;
                ext.mask_feather = Presence::Missing;
            }
        }
        if ext.is_empty() {
            record.vector_ext = Presence::Missing;
        }
        if same_record(&before, record) {
            // Equal numeric input cannot rewrite the original integer/float form.
            *record = before;
        } else {
            changed = true;
        }
    }
    Ok(Edit {
        document,
        kind: VectorEditKind::Mask,
        stroke_id: target.stroke_id,
        changed,
    })
}

#[cfg(test)]
#[path = "../tests/object_mask_storage.rs"]
mod tests;
