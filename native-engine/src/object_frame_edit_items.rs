//! #2248: where the scene-root authored raster images and text blocks of a frame edit land. They are cell content as
//! much as the vector records are (baseline 1065e9d8: a cell is one stroke list), so every per-layer frame map the
//! kernel applies to records is applied to them too. A child of the kernel only so its context constant stays private.
use crate::commands::object_fill_owner::object_frame_edit_document::CONTEXT;
use crate::object_document::{FrameScopeKind, ItemRecords, ObjectDocument, ObjectTarget};

/// Move the scene-root authored raster images and text blocks like the records: `dests` names every frame a
/// layer's cell content lands on (none drops it, several copy it). Targets keep their stroke ids and the items
/// (and with them their mesh bindings) are untouched, so only the frame of the target changes.
pub(crate) fn remap_items(document: &mut ObjectDocument, dests: &dyn Fn(&str, u32) -> Vec<u32>) {
    let Some(items) = document.item_records() else {
        return;
    };
    let mut raster = Vec::with_capacity(items.raster_entries().len());
    for entry in items.raster_entries() {
        for to in item_dests(&entry.target, dests) {
            let mut copy = entry.clone();
            copy.target.frame_scope.frame = to;
            raster.push(copy);
        }
    }
    let mut text = Vec::with_capacity(items.text_entries().len());
    for entry in items.text_entries() {
        for to in item_dests(&entry.target, dests) {
            let mut copy = entry.clone();
            copy.target.frame_scope.frame = to;
            text.push(copy);
        }
    }
    document.items = ItemRecords::normalized(raster, text);
}
fn item_dests(target: &ObjectTarget, dests: &dyn Fn(&str, u32) -> Vec<u32>) -> Vec<u32> {
    if target.context_id == CONTEXT && target.frame_scope.kind == FrameScopeKind::Authored {
        dests(target.layer_uid.as_str(), target.frame_scope.frame)
    } else {
        vec![target.frame_scope.frame]
    }
}
