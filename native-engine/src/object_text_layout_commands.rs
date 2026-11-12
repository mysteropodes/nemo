//! L06-L (G38a): the text-block transactions of `object_raster_asset_commands.rs` -- create, edit and delete -- as pure
//! functions from the sealed origin to the edited document. The block is the strict L06-F `TextBlockRecord`; the
//! target is the stable native identity, and the stable holder name of its glyph pins (`text_holder_id`) is derived
//! from that target only, so an edit never re-keys an existing glyph (`<holder>/glyph/<ordinal>`, ordinal = baseline
//! `charIndex`) and never moves `anchorTopLeft`. Layout itself is derived output: nothing is laid out or stored
//! here, and a font asset that the retained font provider cannot supply is reported by that provider at evaluation.
use crate::commands::object_fill_owner::item_commands::{
    allocate_id, asset_code, check_layer, check_target, inventory_of, lists, order_insert,
    order_remove, other_family, scope_of, store, ItemEdit, ItemPayload,
};
use crate::commands::object_fill_owner::object_frame_edit_document::DocumentEdit;
use crate::commands::object_fill_owner::vector_edit::{cells, context};
use crate::object_document::object_text_layout_kernels::object_text_layout_record::pin::GlyphPin;
use crate::object_document::{
    font_asset_issue, text_holder_id, ObjectDocument, ObjectTarget, TextEntry,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;

#[path = "object_text_layout_patch.rs"]
pub(crate) mod patch;

/// Prefix of the native ids the owner allocates for text blocks.
#[path = "object_text_animator_commands.rs"]
pub(crate) mod animator;

const ID_PREFIX: &str = "native-text";

pub(crate) fn apply(
    origin: &ObjectSnapshot,
    payload: &ItemPayload,
) -> Result<ItemEdit, DispatchErrorCode> {
    match payload.command.as_str() {
        "object.text.animator.upsert"
        | "object.text.animator.remove"
        | "object.text.animator.edit" => animator::apply(origin, payload),
        "object.text.create" => {
            payload.expects_with(&["target", "block"], &["keyframe"])?;
            create(origin, payload)
        }
        "object.text.edit" => {
            payload.expects_with(&["stableTarget", "patch"], &["frame", "keyframe"])?;
            edit(origin, payload)
        }
        "object.text.delete" => {
            payload.expects_with(&["stableTarget"], &["frame", "keyframe"])?;
            delete(origin, payload)
        }
        _ => Err(DispatchErrorCode::InvalidRequest),
    }
}

fn stable(payload: &ItemPayload) -> Result<&ObjectTarget, DispatchErrorCode> {
    payload
        .stable_target
        .value()
        .ok_or(DispatchErrorCode::InvalidRequest)
}

/// The document an edit works on and the target it lands on. With `keyframe: "copy"` (#2267: the baseline's ensureKeyframe before a
/// text commit) a HELD or unexposed cell is first keyed with a copy of what it showed -- its records, text blocks and images under their
/// own ids -- in the same revision, and the target moves to that cell (`frame`, the playhead cell; a create already names it), so the key
/// every held frame carries is never edited. Without it the document and target are the origin's.
fn landing(
    origin: &ObjectSnapshot,
    payload: &ItemPayload,
    target: &ObjectTarget,
) -> Result<(ObjectDocument, ObjectTarget, bool), DispatchErrorCode> {
    let mut document = origin.document().clone();
    let copy = match payload.keyframe.value() {
        None => false,
        Some(word) if word == "copy" => true,
        Some(_) => return Err(DispatchErrorCode::InvalidRequest),
    };
    let frame = match payload.frame.value() {
        None => target.frame(),
        Some(number) => number.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or(DispatchErrorCode::InvalidRequest)?,
    };
    if !copy {
        if payload.frame.value().is_some() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        return Ok((document, target.clone(), false));
    }
    let mut landed = target.clone();
    landed.frame_scope.frame = frame;
    let scope = scope_of(&document, &landed).ok_or(DispatchErrorCode::Unavailable)?;
    if frame >= scope.1 {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let held = Some(scope.0)
        .and_then(|layers| layers.iter().find(|layer| layer.layer_uid() == landed.layer_uid()))
        .and_then(|layer| layer.timing.value())
        .is_some_and(|timing| matches!(timing.exposure(frame).0, "held" | "unexposed"));
    let mut promoted = false;
    if held {
        // A component's held cell is keyed through a view of the document (its layers and frame count are the component's), as its vector edits are.
        let context = landed.context_id().to_string();
        let view = context::view(&document, &context)?;
        let base = view.as_ref().unwrap_or(&document);
        if let DocumentEdit::Changed(next) = cells::promote(base, &context, landed.layer_uid(), &[frame]).map_err(|_| DispatchErrorCode::Unavailable)? {
            document = if view.is_some() { context::restore(next, &document, &context)? } else { next };
            promoted = true;
        }
    }
    Ok((document, landed, promoted))
}

fn index_of(document: &ObjectDocument, target: &ObjectTarget) -> Result<usize, DispatchErrorCode> {
    document
        .item_records()
        .and_then(|items| {
            items
                .text_entries()
                .iter()
                .position(|entry| &entry.target == target)
        })
        .ok_or_else(|| other_family(document, target))
}

fn create(origin: &ObjectSnapshot, payload: &ItemPayload) -> Result<ItemEdit, DispatchErrorCode> {
    let (Some(target), Some(block)) = (payload.target.value(), payload.block.value()) else {
        return Err(DispatchErrorCode::InvalidRequest);
    };
    if !target.stroke_id().is_empty() {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    if let Some(issue) = font_asset_issue(block.font(), inventory_of(origin)) {
        return Err(asset_code(issue));
    }
    let (mut document, target, _) = landing(origin, payload, target)?;
    let stroke_id = allocate_id(&document, origin.content_revision(), ID_PREFIX)?;
    let mut target = target;
    target.stroke_id = stroke_id.clone();
    check_target(&document, &target)?;
    check_layer(&document, &target)?;
    order_insert(&mut document, &target)?;
    if GlyphPin::new(&text_holder_id(&target), 0).is_err() {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let (raster, mut text) = lists(&document);
    text.push(TextEntry {
        target,
        block: block.clone(),
    });
    store(&mut document, raster, text);
    Ok(ItemEdit {
        document,
        kind: "text.create",
        stroke_id,
        changed: true,
    })
}

fn edit(origin: &ObjectSnapshot, payload: &ItemPayload) -> Result<ItemEdit, DispatchErrorCode> {
    let target = stable(payload)?;
    let wanted = payload
        .patch
        .value()
        .ok_or(DispatchErrorCode::InvalidRequest)?;
    let (document, target, promoted) = landing(origin, payload, target)?;
    let (document, target) = (&document, &target);
    check_target(document, target)?;
    let index = index_of(document, target)?;
    check_layer(document, target)?;
    let stroke_id = target.stroke_id().to_string();
    let current = &document
        .item_records()
        .ok_or(DispatchErrorCode::Internal)?
        .text_entries()[index]
        .block;
    let Some(edited) = patch::apply_patch(current, wanted)? else {
        return Ok(ItemEdit {
            document: document.clone(),
            kind: "text.edit",
            stroke_id,
            changed: promoted,
        });
    };
    if let Some(issue) = font_asset_issue(edited.font(), inventory_of(origin)) {
        return Err(asset_code(issue));
    }
    let mut next = document.clone();
    let (raster, mut text) = lists(&next);
    text[index].block = edited;
    store(&mut next, raster, text);
    Ok(ItemEdit {
        document: next,
        kind: "text.edit",
        stroke_id,
        changed: true,
    })
}

fn delete(origin: &ObjectSnapshot, payload: &ItemPayload) -> Result<ItemEdit, DispatchErrorCode> {
    let target = stable(payload)?;
    let (document, target, _) = landing(origin, payload, target)?;
    let (document, target) = (&document, &target);
    check_target(document, target)?;
    let index = index_of(document, target)?;
    check_layer(document, target)?;
    let mut next = document.clone();
    order_remove(&mut next, target)?;
    let (raster, mut text) = lists(&next);
    text.remove(index);
    store(&mut next, raster, text);
    Ok(ItemEdit {
        document: next,
        kind: "text.delete",
        stroke_id: target.stroke_id().to_string(),
        changed: true,
    })
}
