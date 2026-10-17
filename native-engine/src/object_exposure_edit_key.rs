//! Sealed key-exposure transactions (F09/F10): the dense editor's F6 insert key (the layer-selection form, the
//! span-end `insertKeyframeAt` and `convertToKeyframes` are the same operation over a layer list and a cell list),
//! Clear Keyframe, Delete Frames (clear cells) and Remove Tween, prepared against one retained snapshot and
//! published through the existing revision owner (src/js/app.js `insertKeyframe`, `_insertKeyframeCore`,
//! `insertKeyframeAt`, `clearKeyframe`, `removeTweenSpan`, `convertToKeyframes` and src/js/timeline.js
//! `deleteSelectedFrames` at fbe4b8a3). The per-layer content edits are L05-B's document kernel; this file adds
//! the layer-selection and lock policy the baseline applies, the strict payload and the sealed prepare/commit
//! pair. History depth, request replay and dispatch belong to the history owner
//! (`object_exposure_edit_key_history.rs`) and are not duplicated here.
//!
//! Baseline rules kept on purpose: a locked layer is skipped by F6 and Delete Frames and refuses Clear Keyframe,
//! convert and Remove Tween; when nothing could change and a locked layer was in the way the baseline shows
//! "layer locked" instead of "already a keyframe", which is the typed `Unavailable` here; when nothing changed
//! and no lock was involved the operation is a semantic no-op (nothing published, redo kept). Targets are
//! always processed in document order so the bytes never depend on the order the caller listed them in.
//! Markers, in/out points, time links, Motion tracks, component layers (`blankOverride`, I14) and non-root
//! contexts have no native records yet; a layer timing of another family or context is refused by the kernel as
//! `Unavailable`, never invented. The drawing F7 blank-key core is not part of this file.
use crate::commands::object_fill_owner::object_frame_edit_document::{
    clear_cells, clear_key, demote_span, promote_cells, DocumentEdit, FrameEditError,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, ObjectDocument, ObjectLayer, Presence};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use std::collections::HashSet;

const MAX_PAYLOAD_BYTES: usize = 131_072;
/// Longest cell or target list one request may carry.
const MAX_LIST: usize = 4096;

#[derive(Debug)]
struct CellTarget {
    layer_uid: String,
    frame: u32,
}
object_deserialize!(CellTarget {
    layer_uid: String,
    frame: u32
});

#[derive(Debug)]
struct KeyPayload {
    command: String,
    layer_uids: Presence<Vec<String>>,
    cells: Presence<Vec<u32>>,
    layer_uid: Presence<String>,
    cell: Presence<u32>,
    targets: Presence<Vec<CellTarget>>,
    from: Presence<u32>,
    to: Presence<u32>,
}
object_deserialize!(KeyPayload {
    command: String,
    #[serde(default)] layer_uids: Presence<Vec<String>>,
    #[serde(default)] cells: Presence<Vec<u32>>,
    #[serde(default)] layer_uid: Presence<String>,
    #[serde(default)] cell: Presence<u32>,
    #[serde(default)] targets: Presence<Vec<CellTarget>>,
    #[serde(default)] from: Presence<u32>,
    #[serde(default)] to: Presence<u32>
});

/// One typed operation; every command admits exactly its own fields.
#[derive(Debug)]
enum KeyOperation {
    Promote {
        layers: Option<Vec<String>>,
        cells: Vec<u32>,
    },
    Clear {
        layer: String,
        cell: u32,
    },
    ClearCells {
        targets: Vec<CellTarget>,
    },
    RemoveTween {
        layer: String,
        from: u32,
        to: u32,
    },
}

fn require<T>(field: Presence<T>) -> Result<T, DispatchErrorCode> {
    match field {
        Presence::Present(value) => Ok(value),
        Presence::Missing => Err(DispatchErrorCode::InvalidRequest),
    }
}
fn forbid<T>(field: &Presence<T>) -> Result<(), DispatchErrorCode> {
    if field.missing() {
        Ok(())
    } else {
        Err(DispatchErrorCode::InvalidRequest)
    }
}
fn unique<T: std::hash::Hash + Eq>(mut items: impl Iterator<Item = T>) -> bool {
    let mut seen = HashSet::new();
    items.all(|item| seen.insert(item))
}
fn layer_name(field: Presence<String>) -> Result<String, DispatchErrorCode> {
    let name = require(field)?;
    if name.is_empty() {
        Err(DispatchErrorCode::InvalidRequest)
    } else {
        Ok(name)
    }
}

impl KeyPayload {
    fn operation(self) -> Result<KeyOperation, DispatchErrorCode> {
        let KeyPayload {
            command,
            layer_uids,
            cells,
            layer_uid,
            cell,
            targets,
            from,
            to,
        } = self;
        match command.as_str() {
            "object.layer.key.promote" => {
                forbid(&layer_uid)?;
                forbid(&cell)?;
                forbid(&targets)?;
                forbid(&from)?;
                forbid(&to)?;
                let cells = require(cells)?;
                if cells.is_empty() || cells.len() > MAX_LIST || !unique(cells.iter()) {
                    return Err(DispatchErrorCode::InvalidRequest);
                }
                let layers = match layer_uids {
                    Presence::Missing => None,
                    Presence::Present(list) => {
                        let named = list.iter().all(|name| !name.is_empty());
                        if list.is_empty()
                            || list.len() > MAX_LIST
                            || !named
                            || !unique(list.iter())
                        {
                            return Err(DispatchErrorCode::InvalidRequest);
                        }
                        Some(list)
                    }
                };
                Ok(KeyOperation::Promote { layers, cells })
            }
            "object.layer.key.clear" => {
                forbid(&layer_uids)?;
                forbid(&cells)?;
                forbid(&targets)?;
                forbid(&from)?;
                forbid(&to)?;
                Ok(KeyOperation::Clear {
                    layer: layer_name(layer_uid)?,
                    cell: require(cell)?,
                })
            }
            "object.layer.key.clear_cells" => {
                forbid(&layer_uids)?;
                forbid(&cells)?;
                forbid(&layer_uid)?;
                forbid(&cell)?;
                forbid(&from)?;
                forbid(&to)?;
                let targets = require(targets)?;
                let pairs = targets
                    .iter()
                    .map(|target| (target.layer_uid.as_str(), target.frame));
                if targets.is_empty()
                    || targets.len() > MAX_LIST
                    || targets.iter().any(|target| target.layer_uid.is_empty())
                    || !unique(pairs)
                {
                    return Err(DispatchErrorCode::InvalidRequest);
                }
                Ok(KeyOperation::ClearCells { targets })
            }
            "object.layer.tween.remove" => {
                forbid(&layer_uids)?;
                forbid(&cells)?;
                forbid(&targets)?;
                forbid(&cell)?;
                Ok(KeyOperation::RemoveTween {
                    layer: layer_name(layer_uid)?,
                    from: require(from)?,
                    to: require(to)?,
                })
            }
            _ => Err(DispatchErrorCode::InvalidRequest),
        }
    }
}

impl KeyOperation {
    fn command(&self) -> &'static str {
        match self {
            KeyOperation::Promote { .. } => "object.layer.key.promote",
            KeyOperation::Clear { .. } => "object.layer.key.clear",
            KeyOperation::ClearCells { .. } => "object.layer.key.clear_cells",
            KeyOperation::RemoveTween { .. } => "object.layer.tween.remove",
        }
    }
}

fn refusal(error: FrameEditError) -> DispatchErrorCode {
    match error {
        FrameEditError::Invalid => DispatchErrorCode::InvalidRequest,
        FrameEditError::UnknownLayer => DispatchErrorCode::NotFound,
        FrameEditError::Unsupported
        | FrameEditError::MissingTiming
        | FrameEditError::PreservedOutOfRange => DispatchErrorCode::Unavailable,
    }
}
/// Absent reads as the baseline `false`; presence is never authored by an edit that does not change it.
fn is_locked(layer: &ObjectLayer) -> bool {
    layer.locked.value().copied().unwrap_or(false)
}
fn index_of(document: &ObjectDocument, uid: &str) -> Result<usize, DispatchErrorCode> {
    document
        .layers
        .iter()
        .position(|layer| layer.layer_uid.as_str() == uid)
        .ok_or(DispatchErrorCode::NotFound)
}
fn check_cell(document: &ObjectDocument, cell: u32) -> Result<(), DispatchErrorCode> {
    if cell < document.total_frames {
        Ok(())
    } else {
        Err(DispatchErrorCode::InvalidRequest)
    }
}
/// The end of every selection edit. Changed wins; nothing changed with a locked layer in the way is the
/// baseline's "layer locked" refusal; nothing changed otherwise is a semantic no-op.
fn settle(
    current: ObjectDocument,
    changed: bool,
    locked_hit: bool,
) -> Result<DocumentEdit, DispatchErrorCode> {
    if changed {
        Ok(DocumentEdit::Changed(current))
    } else if locked_hit {
        Err(DispatchErrorCode::Unavailable)
    } else {
        Ok(DocumentEdit::NoChange)
    }
}

/// F6 / insertKeyframeAt / convertToKeyframes: every targeted unlocked layer, in document order, promotes the listed
/// cells (in the listed order, see the kernel) on the evolving document; a layer on which every cell already is a key
/// changes nothing. Each kernel step revalidates the complete document, and nothing is published unless all pass.
fn plan_promote(
    document: &ObjectDocument,
    selected: Option<&[String]>,
    cells: &[u32],
) -> Result<DocumentEdit, DispatchErrorCode> {
    for cell in cells {
        check_cell(document, *cell)?;
    }
    if let Some(list) = selected {
        for uid in list {
            index_of(document, uid)?;
        }
    }
    let targets: Vec<&str> = document
        .layers
        .iter()
        .map(|layer| layer.layer_uid.as_str())
        .filter(|uid| selected.is_none_or(|list| list.iter().any(|name| name.as_str() == *uid)))
        .collect();
    let mut current = document.clone();
    let (mut changed, mut locked_hit) = (false, false);
    for uid in targets {
        let index = index_of(&current, uid)?;
        if is_locked(&current.layers[index]) {
            locked_hit = true;
            continue;
        }
        if let DocumentEdit::Changed(next) = promote_cells(&current, uid, cells).map_err(refusal)? {
            current = next;
            changed = true;
        }
    }
    settle(current, changed, locked_hit)
}

/// Clear Keyframe: only a key is cleared. A locked layer refuses before the key test, as the baseline does.
fn plan_clear(
    document: &ObjectDocument,
    layer: &str,
    cell: u32,
) -> Result<DocumentEdit, DispatchErrorCode> {
    let index = index_of(document, layer)?;
    check_cell(document, cell)?;
    if is_locked(&document.layers[index]) {
        return Err(DispatchErrorCode::Unavailable);
    }
    clear_key(document, layer, cell).map_err(refusal)
}

/// Delete Frames over a selection that may span layers: locked layers are skipped, every other named layer has
/// its listed cells (key or in-between alike) turned into plain holds.
fn plan_clear_cells(
    document: &ObjectDocument,
    targets: &[CellTarget],
) -> Result<DocumentEdit, DispatchErrorCode> {
    for target in targets {
        index_of(document, target.layer_uid.as_str())?;
        check_cell(document, target.frame)?;
    }
    let mut current = document.clone();
    let (mut changed, mut locked_hit) = (false, false);
    for layer in &document.layers {
        let frames: Vec<u32> = targets
            .iter()
            .filter(|target| target.layer_uid == layer.layer_uid)
            .map(|target| target.frame)
            .collect();
        if frames.is_empty() {
            continue;
        }
        if is_locked(layer) {
            locked_hit = true;
            continue;
        }
        let uid = layer.layer_uid.as_str();
        if let DocumentEdit::Changed(next) = clear_cells(&current, uid, &frames).map_err(refusal)? {
            current = next;
            changed = true;
        }
    }
    settle(current, changed, locked_hit)
}

/// Remove Tween: everything strictly between the pair becomes a hold of the start key; both ends and the frame
/// count stay. The baseline loops silently when `from > to`; here the span must be ordered and inside the timeline.
fn plan_remove_tween(
    document: &ObjectDocument,
    layer: &str,
    from: u32,
    to: u32,
) -> Result<DocumentEdit, DispatchErrorCode> {
    let index = index_of(document, layer)?;
    if from > to {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    check_cell(document, to)?;
    if is_locked(&document.layers[index]) {
        return Err(DispatchErrorCode::Unavailable);
    }
    demote_span(document, layer, from, to).map_err(refusal)
}

fn plan(
    document: &ObjectDocument,
    operation: &KeyOperation,
) -> Result<DocumentEdit, DispatchErrorCode> {
    match operation {
        KeyOperation::Promote { layers, cells } => {
            plan_promote(document, layers.as_deref(), cells.as_slice())
        }
        KeyOperation::Clear { layer, cell } => plan_clear(document, layer.as_str(), *cell),
        KeyOperation::ClearCells { targets } => plan_clear_cells(document, targets.as_slice()),
        KeyOperation::RemoveTween { layer, from, to } => {
            plan_remove_tween(document, layer.as_str(), *from, *to)
        }
    }
}

/// Detached, non-Clone candidate sealed to the snapshot it was prepared from; only this file can build one.
/// `document` is `None` for a semantic no-op, which publishes nothing and keeps redo.
#[derive(Debug)]
pub(crate) struct PreparedExposureKey {
    origin: ObjectSnapshot,
    document: Option<ObjectDocument>,
    command: &'static str,
}
impl PreparedExposureKey {
    /// The command the payload named, so a dispatcher can refuse an operation/command mismatch.
    pub(crate) fn command(&self) -> &'static str {
        self.command
    }
}

impl ObjectFillOwner {
    pub(crate) fn prepare_exposure_key_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedExposureKey, DispatchErrorCode> {
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: KeyPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let operation = payload.operation()?;
        let origin = self.current_snapshot();
        let document = match plan(origin.document(), &operation)? {
            DocumentEdit::Changed(next) => Some(next),
            DocumentEdit::NoChange => None,
        };
        Ok(PreparedExposureKey {
            origin,
            document,
            command: operation.command(),
        })
    }

    pub(crate) fn commit_exposure_key(
        &mut self,
        candidate: PreparedExposureKey,
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
        match candidate.document {
            None => Ok(self.content_revision()),
            Some(document) => self
                .revisions
                .publish(document)
                .map_err(|_| DispatchErrorCode::Unavailable),
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_exposure_edit_key.rs"]
mod tests;
