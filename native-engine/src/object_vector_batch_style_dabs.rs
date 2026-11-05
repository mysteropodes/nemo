//! #2372 (A-bitmap-brush-edit): what a restyle or a transform of a brush anchor does to the pieces it derives its look from.
//! - A texture anchor whose dabs are STORED records (an imported project, a tween frame) paints those and the engine does not stamp it
//!   again. Baseline `regenerateBrushTexture` removes the old dabs and stamps new ones from the anchor after a width, a stroke colour or a
//!   preset edit; here the stored copies are deleted in the same revision and the anchor (seeded, as `applyBrushTexture` seeds a stamp)
//!   paints its dabs natively from its preset, so the one kernel (`brush_dab_stamp`) is the only stamper.
//! - A bitmap brush anchor keeps its look in a baked companion image; the edit changes the anchor's spec (`size`, `color`, `opacity`) and
//!   the batch re-bakes the image from it (`object_vector_batch_rebake.rs`; baseline `setBrushSize` writes `bitmapBrushSpec.size` then
//!   `SMBitmapBrush.regenerate`).
use crate::commands::object_fill_owner::vector_edit::batch_ops::detail;
use crate::commands::object_fill_owner::vector_edit::batch_ops::style::num;
use crate::commands::object_fill_owner::vector_edit::batch_ops::BatchOp;
use crate::object_document::{ObjectDocument, ObjectRecord, ObjectTarget, Presence, SolidFill};
use crate::request_receipts::DispatchErrorCode;
use serde_json::Number;

fn same_cell(a: &ObjectTarget, b: &ObjectTarget) -> bool {
    a.context_id == b.context_id && a.layer_uid == b.layer_uid && a.frame_scope == b.frame_scope
}
/// The texture group a record belongs to and whether it is one of its stored dabs.
fn group(record: &ObjectRecord) -> Option<(&str, bool)> {
    let t = record.vector_ext.value()?.brush_texture.value()?;
    Some((
        t.brush_group_id.value()?.as_str(),
        t.is_brush_texture_copy.value() == Some(&true),
    ))
}

/// True when the stored dab `copy` has its anchor (a record of its group that is not a dab) in the same cell.
pub(in crate::commands::object_fill_owner::vector_edit::batch_ops) fn has_anchor(
    source: &ObjectDocument,
    copy: &ObjectRecord,
) -> bool {
    let Some((id, _)) = group(copy) else {
        return false;
    };
    source
        .objects
        .iter()
        .any(|r| same_cell(&r.target, &copy.target) && group(r) == Some((id, false)))
}

/// The stored dabs of `anchor`'s group in its cell.
pub(super) fn copies(source: &ObjectDocument, anchor: &ObjectRecord) -> Vec<ObjectTarget> {
    let Some((id, false)) = group(anchor) else {
        return Vec::new();
    };
    source
        .objects
        .iter()
        .filter(|r| same_cell(&r.target, &anchor.target) && group(r) == Some((id, true)))
        .map(|r| r.target.clone())
        .collect()
}

/// Make `record` an anchor the engine stamps itself: it needs its preset (a group nothing can re-derive keeps its dabs) and a seed, which is
/// derived from the stroke and its group so a replay stamps the same dabs. False when the anchor cannot be re-stamped.
pub(super) fn seed(record: &mut ObjectRecord) -> bool {
    let id = record.target.stroke_id.clone();
    let Some(texture) = record
        .vector_ext
        .value_mut()
        .and_then(|e| e.brush_texture.value_mut())
    else {
        return false;
    };
    let (Some(preset), Some(group)) = (
        texture.brush_texture_preset.value().cloned(),
        texture.brush_group_id.value().cloned(),
    ) else {
        return false;
    };
    if texture.stroke_seed.missing() {
        texture.stroke_seed =
            Presence::Present(Number::from(detail::hash(&[&id, &preset, &group])));
    }
    true
}

/// A texture anchor whose own stroke is hidden by a fill remembers it for the strip; a new stroke colour is that memory.
pub(super) fn remember_stroke(record: &mut ObjectRecord, paint: &SolidFill) {
    if let (Some(texture), Some(hex)) = (
        record
            .vector_ext
            .value_mut()
            .and_then(|e| e.brush_texture.value_mut()),
        detail::hex8(paint),
    ) {
        texture.pre_texture_stroke = Presence::Present(Some(hex));
    }
}

/// The spec members a restyle edits on a bitmap anchor: the dab size (the Width field), the ink (the stroke colour, whose alpha byte is part
/// of it) and the stamp opacity (the Opacity field of an anchor with no fill of its own, which would otherwise un-hide the camouflaged
/// outline). A filled anchor also remembers the ink as the stroke it hides.
pub(super) fn bitmap_style(
    record: &mut ObjectRecord,
    width: Option<f64>,
    ink: Option<&SolidFill>,
    opacity: Option<f64>,
    filled: bool,
) -> Result<(), DispatchErrorCode> {
    let Some(fields) = record
        .vector_ext
        .value_mut()
        .and_then(|e| e.bitmap_brush.value_mut())
    else {
        return Ok(());
    };
    if let Some(width) = width {
        fields.bitmap_brush_spec.size = num(width)?;
    }
    if let Some(hex) = ink.and_then(detail::hex8) {
        fields.bitmap_brush_spec.color = hex.clone();
        if filled {
            fields.pre_texture_stroke = Presence::Present(Some(hex));
        }
    }
    if let Some(opacity) = opacity {
        fields.bitmap_brush_spec.opacity = num(opacity)?;
    }
    Ok(())
}

/// A step of the expanded batch that names only its op and the record or target it works on.
pub(in crate::commands::object_fill_owner::vector_edit::batch_ops) fn step(
    op: &str,
    record: Presence<ObjectRecord>,
    stable_target: Presence<ObjectTarget>,
) -> BatchOp {
    BatchOp {
        op: op.into(),
        record,
        stable_target,
        placement: Presence::Missing,
        keyframe: Presence::Missing,
        from_step: Presence::Missing,
        frame: Presence::Missing,
        geometry: Presence::Missing,
        offset: Presence::Missing,
        center: Presence::Missing,
        matrix: Presence::Missing,
        style: Presence::Missing,
    }
}
