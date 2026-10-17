//! Sealed frame-structure transactions (F09/F10): the dense editor's insert frame (F5), extend exposure,
//! duplicate key and remove frame / frame span, prepared against one retained snapshot and published through the
//! existing revision owner (src/js/app.js insertFrame, removeFrame, removeFrameSpan and src/js/timeline.js
//! duplicateKeyframe, extendExposure at fbe4b8a3). The document-level remap is L05-B's kernel; this file adds the
//! target and lock policy the baseline applies per operation, the extend loop, the duplicate source rule, the strict
//! payload and the sealed prepare/commit pair. History depth, request replay and dispatch belong to the history
//! owner (`object_frame_edit_structure_history.rs`) and are not duplicated here.
//!
//! Baseline quirks kept on purpose: F5 splices only the selected UNLOCKED layers and pads the others at the end
//! (an explicit selection of locked layers only refuses; with no selection and every layer locked every layer is
//! padded); extend and duplicate splice into ALL layers whether locked or not (duplicate refuses a locked source
//! layer); remove never looks at locks; the baseline `n || 1` default of extend is the caller's, here `count: 0` is
//! a no-op. Markers, in/out points, time links, Motion tracks, component layers and non-root contexts have no
//! native records yet and are not touched.
use crate::commands::object_fill_owner::object_frame_edit_document::{
    clone_cell, insert_frame, remove_frames, DocumentEdit, FrameEditError,
};
use crate::commands::ObjectFillOwner;
use crate::object_document::{
    object_deserialize, FrameScopeKind, ObjectDocument, ObjectLayer, ObjectRecord, Presence,
    TimingEdit,
};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use std::collections::HashSet;

// L05-G (#2111): layer range records follow the frame-structure edits (F5 and single-frame removal shift them).
#[path = "object_frame_edit_range_join.rs"]
#[allow(dead_code)]
mod object_frame_edit_range_join;
use object_frame_edit_range_join::{join_document_ranges, StructureEdit};

const MAX_PAYLOAD_BYTES: usize = 4096;
/// The baseline frame-count field (`#tl-total`) tops out at 999; one extend request never adds more.
const MAX_EXTEND: u32 = 999;

#[derive(Debug)]
struct FramePayload {
    command: String,
    cell: Presence<u32>,
    layer_uids: Presence<Vec<String>>,
    count: Presence<u32>,
    layer_uid: Presence<String>,
    from: Presence<u32>,
    to: Presence<u32>,
}
object_deserialize!(FramePayload {
    command: String,
    #[serde(default)] cell: Presence<u32>,
    #[serde(default)] layer_uids: Presence<Vec<String>>,
    #[serde(default)] count: Presence<u32>,
    #[serde(default)] layer_uid: Presence<String>,
    #[serde(default)] from: Presence<u32>,
    #[serde(default)] to: Presence<u32>
});

/// One typed operation; every command admits exactly its own fields.
#[derive(Debug)]
enum FrameOperation {
    Insert {
        cell: u32,
        layers: Option<Vec<String>>,
    },
    Extend {
        cell: u32,
        count: u32,
    },
    Duplicate {
        layer: String,
        cell: u32,
    },
    Remove {
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

impl FramePayload {
    fn operation(self) -> Result<FrameOperation, DispatchErrorCode> {
        let FramePayload {
            command,
            cell,
            layer_uids,
            count,
            layer_uid,
            from,
            to,
        } = self;
        match command.as_str() {
            "object.frames.insert" => {
                forbid(&count)?;
                forbid(&layer_uid)?;
                forbid(&from)?;
                forbid(&to)?;
                let layers = match layer_uids {
                    Presence::Missing => None,
                    Presence::Present(list) => {
                        let unique = {
                            let mut seen = HashSet::new();
                            list.iter()
                                .all(|uid| !uid.is_empty() && seen.insert(uid.as_str()))
                        };
                        if list.is_empty() || !unique {
                            return Err(DispatchErrorCode::InvalidRequest);
                        }
                        Some(list)
                    }
                };
                Ok(FrameOperation::Insert {
                    cell: require(cell)?,
                    layers,
                })
            }
            "object.frames.extend" => {
                forbid(&layer_uids)?;
                forbid(&layer_uid)?;
                forbid(&from)?;
                forbid(&to)?;
                Ok(FrameOperation::Extend {
                    cell: require(cell)?,
                    count: require(count)?,
                })
            }
            "object.frames.duplicate" => {
                forbid(&layer_uids)?;
                forbid(&count)?;
                forbid(&from)?;
                forbid(&to)?;
                let layer = require(layer_uid)?;
                if layer.is_empty() {
                    return Err(DispatchErrorCode::InvalidRequest);
                }
                Ok(FrameOperation::Duplicate {
                    layer,
                    cell: require(cell)?,
                })
            }
            "object.frames.remove" => {
                forbid(&cell)?;
                forbid(&layer_uids)?;
                forbid(&count)?;
                forbid(&layer_uid)?;
                Ok(FrameOperation::Remove {
                    from: require(from)?,
                    to: require(to)?,
                })
            }
            _ => Err(DispatchErrorCode::InvalidRequest),
        }
    }
}

impl FrameOperation {
    fn command(&self) -> &'static str {
        match self {
            FrameOperation::Insert { .. } => "object.frames.insert",
            FrameOperation::Extend { .. } => "object.frames.extend",
            FrameOperation::Duplicate { .. } => "object.frames.duplicate",
            FrameOperation::Remove { .. } => "object.frames.remove",
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
fn scene_authored(record: &ObjectRecord) -> bool {
    record.target.context_id == "scene-root"
        && record.target.frame_scope.kind == FrameScopeKind::Authored
}
fn all_uids(document: &ObjectDocument) -> Vec<&str> {
    document
        .layers
        .iter()
        .map(|layer| layer.layer_uid.as_str())
        .collect()
}
fn check_cell(document: &ObjectDocument, cell: u32) -> Result<(), DispatchErrorCode> {
    if cell < document.total_frames {
        Ok(())
    } else {
        Err(DispatchErrorCode::InvalidRequest)
    }
}
/// The one exit for a result built here: the shared strict drawing admission runs on the complete document.
fn finish(document: ObjectDocument) -> Result<DocumentEdit, DispatchErrorCode> {
    ObjectSnapshot::validate_drawing(&document).map_err(|_| DispatchErrorCode::InvalidRequest)?;
    Ok(DocumentEdit::Changed(document))
}

/// No selection and no unlocked layer: the baseline still grows the timeline by one blank hold on every layer.
fn pad_all(document: &ObjectDocument) -> Result<DocumentEdit, DispatchErrorCode> {
    let total = document.total_frames;
    let grown = total
        .checked_add(1)
        .ok_or(DispatchErrorCode::InvalidRequest)?;
    let mut next = document.clone();
    for layer in &mut next.layers {
        let edit = match layer.timing.value() {
            Some(timing) => timing
                .insert_pad(total)
                .map_err(|error| refusal(FrameEditError::from(error)))?,
            None => continue,
        };
        if let TimingEdit::Changed(timing) = edit {
            layer.timing = Presence::Present(timing);
        }
    }
    next.total_frames = grown;
    finish(next)
}

/// The layers an F5 really splices: the selected (or every) layer once all named layers are known, minus the
/// locked ones. The splice and the range join both use this one list.
fn insert_targets<'a>(
    document: &'a ObjectDocument,
    selected: Option<&'a [String]>,
) -> Result<Vec<&'a str>, DispatchErrorCode> {
    let mut targets: Vec<&str> = match selected {
        Some(list) => list.iter().map(String::as_str).collect(),
        None => all_uids(document),
    };
    for uid in &targets {
        if !document
            .layers
            .iter()
            .any(|layer| layer.layer_uid.as_str() == *uid)
        {
            return Err(DispatchErrorCode::NotFound);
        }
    }
    targets.retain(|uid| {
        document
            .layers
            .iter()
            .any(|layer| layer.layer_uid.as_str() == *uid && !is_locked(layer))
    });
    Ok(targets)
}

fn insert(
    document: &ObjectDocument,
    cell: u32,
    selected: Option<&[String]>,
) -> Result<DocumentEdit, DispatchErrorCode> {
    check_cell(document, cell)?;
    let targets = insert_targets(document, selected)?;
    if targets.is_empty() {
        return if selected.is_some() {
            Err(DispatchErrorCode::Unavailable)
        } else {
            pad_all(document)
        };
    }
    insert_frame(document, cell, &targets).map_err(refusal)
}

fn extend(
    document: &ObjectDocument,
    cell: u32,
    count: u32,
) -> Result<DocumentEdit, DispatchErrorCode> {
    check_cell(document, cell)?;
    if count > MAX_EXTEND {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    if count == 0 {
        return Ok(DocumentEdit::NoChange);
    }
    let targets = all_uids(document);
    let mut current = document.clone();
    for _ in 0..count {
        current = match insert_frame(&current, cell, &targets).map_err(refusal)? {
            DocumentEdit::Changed(next) => next,
            DocumentEdit::NoChange => return Err(DispatchErrorCode::Internal),
        };
    }
    Ok(DocumentEdit::Changed(current))
}

/// Every layer gets the blank slot at `cell + 1`; the source layer's slot becomes a key holding a copy of what
/// the source cell SHOWS (a key or an in-between its own records, a hold the key it holds), ids and order intact.
fn duplicate(
    document: &ObjectDocument,
    layer_uid: &str,
    cell: u32,
) -> Result<DocumentEdit, DispatchErrorCode> {
    check_cell(document, cell)?;
    let index = document
        .layers
        .iter()
        .position(|layer| layer.layer_uid.as_str() == layer_uid)
        .ok_or(DispatchErrorCode::NotFound)?;
    let layer = &document.layers[index];
    if is_locked(layer) {
        return Err(DispatchErrorCode::Unavailable);
    }
    let timing = layer.timing.value().ok_or(DispatchErrorCode::Unavailable)?;
    let (_, source, _) = timing.exposure(cell);
    let Some(source) = source else {
        return Ok(DocumentEdit::NoChange);
    };
    let has_content = document.objects.iter().any(|record| {
        scene_authored(record)
            && record.target.layer_uid == layer_uid
            && record.target.frame_scope.frame == source
    });
    if !has_content {
        return Ok(DocumentEdit::NoChange);
    }
    let DocumentEdit::Changed(mut next) =
        insert_frame(document, cell, &all_uids(document)).map_err(refusal)?
    else {
        return Err(DispatchErrorCode::Internal);
    };
    let at = cell
        .checked_add(1)
        .ok_or(DispatchErrorCode::InvalidRequest)?;
    let promoted = next.layers[index]
        .timing
        .value()
        .ok_or(DispatchErrorCode::Unavailable)?
        .promote_cell(at, next.total_frames)
        .map_err(|error| refusal(FrameEditError::from(error)))?;
    let TimingEdit::Changed(timing) = promoted else {
        return Err(DispatchErrorCode::Internal);
    };
    next.layers[index].timing = Presence::Present(timing);
    // The inserted slot is a hold, so nothing is stored there yet. Source cells never move: they sit at or before `cell`.
    clone_cell(&mut next, layer_uid, source, at);
    finish(next)
}

fn plan(
    document: &ObjectDocument,
    operation: &FrameOperation,
) -> Result<DocumentEdit, DispatchErrorCode> {
    match operation {
        FrameOperation::Insert { cell, layers } => insert(document, *cell, layers.as_deref()),
        FrameOperation::Extend { cell, count } => extend(document, *cell, *count),
        FrameOperation::Duplicate { layer, cell } => duplicate(document, layer.as_str(), *cell),
        FrameOperation::Remove { from, to } => remove_frames(document, *from, *to).map_err(refusal),
    }
}

/// F11 (L05-G): the layer range records ride along with a changed edit. `before` is the document the edit was
/// planned on, `after` its result; a layer without a `range` is left without one.
fn with_ranges(
    before: &ObjectDocument,
    after: ObjectDocument,
    operation: &FrameOperation,
) -> Result<ObjectDocument, DispatchErrorCode> {
    let total = before.total_frames;
    match operation {
        FrameOperation::Insert { cell, layers } => {
            let targeted = insert_targets(before, layers.as_deref())?;
            let edit = StructureEdit::Insert {
                cell: *cell,
                targeted: &targeted,
            };
            join_document_ranges(after, total, &edit)
        }
        FrameOperation::Extend { cell, count } => {
            let edit = StructureEdit::Extend {
                cell: *cell,
                count: *count,
            };
            join_document_ranges(after, total, &edit)
        }
        FrameOperation::Duplicate { cell, .. } => {
            join_document_ranges(after, total, &StructureEdit::Duplicate { cell: *cell })
        }
        FrameOperation::Remove { from, to } => {
            let edit = StructureEdit::Remove {
                from: *from,
                to: *to,
            };
            join_document_ranges(after, total, &edit)
        }
    }
}

/// Detached, non-Clone candidate sealed to the snapshot it was prepared from; only this file can build one.
/// `document` is `None` for a semantic no-op, which publishes nothing and keeps redo.
#[derive(Debug)]
pub(crate) struct PreparedFrameStructure {
    origin: ObjectSnapshot,
    document: Option<ObjectDocument>,
    command: &'static str,
}
impl PreparedFrameStructure {
    /// The command the payload named, so a dispatcher can refuse an operation/command mismatch.
    pub(crate) fn command(&self) -> &'static str {
        self.command
    }
}

impl ObjectFillOwner {
    pub(crate) fn prepare_frame_structure_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedFrameStructure, DispatchErrorCode> {
        if bytes.len() > MAX_PAYLOAD_BYTES {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: FramePayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let operation = payload.operation()?;
        let origin = self.current_snapshot();
        let document = match plan(origin.document(), &operation)? {
            DocumentEdit::Changed(next) => Some(with_ranges(origin.document(), next, &operation)?),
            DocumentEdit::NoChange => None,
        };
        Ok(PreparedFrameStructure {
            origin,
            document,
            command: operation.command(),
        })
    }

    pub(crate) fn commit_frame_structure(
        &mut self,
        candidate: PreparedFrameStructure,
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
#[path = "../tests/object_frame_edit_structure.rs"]
mod tests;
