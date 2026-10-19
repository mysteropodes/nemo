//! F7: replace one active layer's cell with an empty, non-interpolated key.
//!
//! Baseline `insertBlankKeyframe` (app.js) ignores the layer-panel selection,
//! refuses a locked drawing but allows a component's outer row, and unconditionally
//! checkpoints history, including an already blank key. This detached transaction
//! therefore publishes one revision on every successful commit. The existing history
//! owner supplies that single checkpoint/replay; no history or queue lives here.
//! Only scene-root timing is admitted. Reference/other-context records, property
//! holders, assets, other layers and frames, total length and component interiors stay.
use crate::commands::object_fill_owner::object_frame_edit_document::drop_cells;
use crate::commands::ObjectFillOwner;
use crate::object_document::object_component_record_kernels::object_component_records::FrameKey;
use crate::object_document::{
    object_deserialize, FrameScopeKind, ItemRecords, LayerTiming, LayerTimingError, ObjectDocument,
    ObjectTarget, Presence, TimingEdit,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;

const COMMAND: &str = "object.layer.key.blank";
const CONTEXT: &str = "scene-root";
const MAX_PAYLOAD_BYTES: usize = 131_072;
const MAX_COMPONENT_KEYS: usize = 4096;

#[derive(Debug)]
struct BlankKeyPayload {
    command: String,
    layer_uid: String,
    cell: u32,
}
object_deserialize!(BlankKeyPayload {
    command: String,
    layer_uid: String,
    cell: u32
});

fn targeted(target: &ObjectTarget, layer: &str, cell: u32) -> bool {
    target.context_id == CONTEXT
        && target.frame_scope.kind == FrameScopeKind::Authored
        && target.frame_scope.frame == cell
        && target.layer_uid == layer
}

/// Use the existing canonical item container only when an item was removed;
/// otherwise preserve its exact optional-field presence and ordering.
fn drop_items(document: &mut ObjectDocument, layer: &str, cell: u32) {
    let Some(items) = document.items.value() else {
        return;
    };
    let raster = items.raster_entries();
    let text = items.text_entries();
    if !raster
        .iter()
        .any(|entry| targeted(&entry.target, layer, cell))
        && !text
            .iter()
            .any(|entry| targeted(&entry.target, layer, cell))
    {
        return;
    }
    document.items = ItemRecords::normalized(
        raster
            .iter()
            .filter(|entry| !targeted(&entry.target, layer, cell))
            .cloned()
            .collect(),
        text.iter()
            .filter(|entry| !targeted(&entry.target, layer, cell))
            .cloned()
            .collect(),
    );
}

/// Component blanking is the authored outer-key flag, never deletion of the
/// component definition or a rewrite of the instance's playback/transform fields.
fn blank_component(
    document: &mut ObjectDocument,
    layer: &str,
    cell: u32,
) -> Result<(), DispatchErrorCode> {
    let Presence::Present(tables) = &mut document.component_tables else {
        return Ok(());
    };
    let Presence::Present(set) = &mut tables.components else {
        return Ok(());
    };
    let Some(instance) = set
        .instances
        .iter_mut()
        .find(|instance| instance.context_id == CONTEXT && instance.layer_uid == layer)
    else {
        return Ok(());
    };
    if instance.frame_keys.missing() {
        instance.frame_keys = Presence::Present(Vec::new());
    }
    let Presence::Present(keys) = &mut instance.frame_keys else {
        return Err(DispatchErrorCode::Unavailable);
    };
    let blank = FrameKey {
        frame: cell,
        component_frame: Presence::Missing,
        blank_override: Presence::Present(Some(true)),
    };
    match keys.binary_search_by_key(&cell, |key| key.frame) {
        Ok(index) => keys[index] = blank,
        Err(index) => {
            if keys.len() >= MAX_COMPONENT_KEYS {
                return Err(DispatchErrorCode::InvalidRequest);
            }
            keys.insert(index, blank);
        }
    }
    Ok(())
}

fn plan(
    document: &ObjectDocument,
    layer: &str,
    cell: u32,
) -> Result<ObjectDocument, DispatchErrorCode> {
    let index = document
        .layers
        .iter()
        .position(|candidate| candidate.layer_uid == layer)
        .ok_or(DispatchErrorCode::NotFound)?;
    if cell >= document.total_frames {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let component = document.component_set().is_some_and(|set| {
        set.instances
            .iter()
            .any(|instance| instance.context_id == CONTEXT && instance.layer_uid == layer)
    });
    if !component
        && document.layers[index]
            .locked
            .value()
            .copied()
            .unwrap_or(false)
    {
        return Err(DispatchErrorCode::Unavailable);
    }
    // Real instances may be admitted without drawing timing or frameKeys. F7
    // authors their outer row using the existing component conversion's harmless
    // frame-zero seed; absence is not a reason to deny the component command.
    let timing = document.layers[index]
        .timing
        .value()
        .cloned()
        .or_else(|| component.then(LayerTiming::blank_drawing))
        .ok_or(DispatchErrorCode::Unavailable)?;
    let promoted =
        timing
            .promote_cell(cell, document.total_frames)
            .map_err(|error| match error {
                LayerTimingError::Invalid => DispatchErrorCode::InvalidRequest,
                LayerTimingError::Unsupported => DispatchErrorCode::Unavailable,
            })?;
    let mut next = document.clone();
    next.layers[index].timing = Presence::Present(match promoted {
        TimingEdit::Changed(timing) => timing,
        TimingEdit::NoChange => timing,
    });
    drop_cells(&mut next, layer, &|frame| frame == cell);
    drop_items(&mut next, layer, cell);
    blank_component(&mut next, layer, cell)?;
    // Existing admission requires an animated element to keep an authored record.
    // Do not erase its Motion holder or invent a replacement record to make F7 pass.
    let orphan = next.property_holders().into_iter().flatten().any(|holder| {
        holder.target.context_id == CONTEXT
            && holder.target.layer_uid == layer
            && holder.target.stroke_id.value().is_some_and(|id| {
                !next.objects.iter().any(|record| {
                    record.target.context_id == CONTEXT
                        && record.target.layer_uid == layer
                        && record.target.frame_scope.kind == FrameScopeKind::Authored
                        && record.target.stroke_id == *id
                })
            })
    });
    if orphan {
        return Err(DispatchErrorCode::Unavailable);
    }
    ObjectSnapshot::validate_drawing(&next).map_err(|_| DispatchErrorCode::InvalidRequest)?;
    Ok(next)
}

/// Detached, non-Clone candidate. Only this module constructs its pinned origin
/// and fully admitted replacement; a caller cannot substitute an unchecked DTO.
#[derive(Debug)]
pub(crate) struct PreparedBlankKey {
    origin: ObjectSnapshot,
    document: ObjectDocument,
}
impl PreparedBlankKey {
    pub(crate) fn command(&self) -> &'static str {
        COMMAND
    }
}

impl ObjectFillOwner {
    pub(crate) fn prepare_blank_key_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedBlankKey, DispatchErrorCode> {
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: BlankKeyPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if payload.command != COMMAND || payload.layer_uid.is_empty() {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let document = plan(origin.document(), &payload.layer_uid, payload.cell)?;
        Ok(PreparedBlankKey { origin, document })
    }

    pub(crate) fn commit_blank_key(
        &mut self,
        candidate: PreparedBlankKey,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        self.revisions
            .publish(candidate.document)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}

#[cfg(test)]
#[path = "../tests/object_blank_key_edit.rs"]
mod tests;
