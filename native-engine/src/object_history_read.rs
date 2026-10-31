//! Current live history projection from the sole owner; no read receipts or writes. Also the
//! human labels of its entries and the multi-step move (one restore, one new revision) behind the
//! History panel's click-to-jump (#2376 G26).
use crate::history::NativeObjectHistory;
use crate::request_receipts::{
    DispatchError, DispatchErrorCode, OpacityRequest, ResponseEnvelope, APPLICATION_API_VERSION,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
const MAX_SAFE: u64 = 9_007_199_254_740_991;
/// Entries per page (the response stays far below its 4096-byte bound) and the share of a default
/// page placed before the cursor, so the cursor is always inside a default page.
const MAX_PAGE: u64 = 32;
const DEFAULT_BEFORE: u64 = 20;
/// Longest label kept (characters); a label is display text, never an identifier.
const MAX_LABEL: usize = 32;
const DEFAULT_LABEL: &str = "Edit";

/// Short display labels of history entries, keyed by the entry's `after_revision` (unique per entry,
/// and stable when the entry moves between the undo and redo stacks). Entries committed outside the
/// object command dispatcher have no label and read as "Edit".
#[derive(Debug, Default)]
pub(crate) struct HistoryLabels(BTreeMap<u64, String>);
impl HistoryLabels {
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }
    fn get(&self, after_revision: u64) -> &str {
        self.0
            .get(&after_revision)
            .map_or(DEFAULT_LABEL, String::as_str)
    }
}

fn capitalised(word: &str) -> String {
    let mut chars = word.chars();
    chars.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(chars).collect()
    })
}

/// The label of a command family or kind. `property.edit` names its property; the vector edit names
/// its kind (`payload.command`); every other command is named by its operation.
pub(crate) fn label_for(operation: &str, payload: &Value) -> String {
    let name = operation
        .strip_prefix("command.document.object.")
        .unwrap_or(operation);
    let text = |key: &str| payload.get(key).and_then(Value::as_str);
    let property_label;
    let label = match name {
        "property.edit" | "property.track.edit" => match text("property") {
            Some(p)
                if !p.is_empty()
                    && p.len() <= 24
                    && p.bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b)) =>
            {
                // "Property: " plus a 24-character id exceeds MAX_LABEL, so this path truncates like every other.
                property_label = format!("Property: {}", capitalised(p));
                property_label.as_str()
            }
            _ => "Edit Property",
        },
        "vector.edit" => match text("command") {
            Some("object.vector.create" | "object.vector.createMany") => "Draw",
            Some("object.vector.replace") => "Edit Shape",
            Some("object.vector.delete") => "Delete Shape",
            Some("object.vector.mask.edit") => "Edit Mask",
            _ => "Edit Shapes",
        },
        "property.preset.apply" => "Apply Motion Preset",
        "fill.set" => "Set Fill",
        "fill.brush" => "Brush Fill",
        "fill.trace" => "Trace Fill",
        "layer.visibility.set" => "Layer Visibility",
        "layer.create" => "New Layer",
        "layer.delete" => "Delete Layer",
        "layer.duplicate" => "Duplicate Layer",
        "layer.rename" => "Rename Layer",
        "layer.reorder" => "Reorder Layers",
        "layer.parent.set" => "Set Layer Parent",
        "layer.timeLink.set" => "Link Layer Time",
        "layer.followPath.set" => "Set Layer Follow Path",
        "layer.split" => "Split Layer",
        "layer.cut" => "Cut Layer",
        "layer.policy.edit" => "Layer Settings",
        "layer.compositing.edit" => "Layer Compositing",
        "layer.range.drag" => "Move Layer Range",
        "layer.key.promote" => "Insert Keyframe",
        "layer.key.blank" => "Blank Keyframe",
        "layer.key.clear" => "Clear Keyframe",
        "layer.key.clear_cells" => "Clear Cells",
        "layer.key.move" => "Move Keyframe",
        "layer.tween.remove" => "Remove Tween",
        "tween.generate" => "Generate Tween",
        "tween.regenerate" => "Regenerate Tween",
        "tween.reassign" => "Reassign Tween",
        "tween.split" => "Split Tween",
        "frames.insert" => "Insert Frame",
        "frames.extend" => "Extend Frame",
        "frames.duplicate" | "frames.clipboard.duplicate" => "Duplicate Frames",
        "frames.remove" => "Delete Frames",
        "frames.move" => "Move Frames",
        "frames.repeat" => "Repeat Frames",
        "frames.clipboard.cut" | "clipboard.cut" => "Cut",
        "frames.clipboard.paste" | "clipboard.paste" => "Paste",
        "selection.delete" => "Delete",
        "selection.duplicate" => "Duplicate",
        "boolean.apply" => "Boolean",
        "boolean.erase" => "Erase",
        "boolean.flatten" => "Flatten",
        "group.create" => "Group",
        "group.ungroup" => "Ungroup",
        "group.combine" => "Combine",
        "group.rename" => "Rename Group",
        "group.mode.set" => "Group Mode",
        "group.member.remove" => "Remove From Group",
        "parametric.edit" | "path.effects.edit" => "Edit Shape Parameters",
        "image.mesh.edit" => "Edit Image Mesh",
        "item.edit" => "Edit Item",
        "subselect.edit" => "Edit Points",
        "rig.edit" => "Edit Rig",
        "rig.pose" => "Rig Pose",
        "widget.create" | "widget.settings" | "widget.write" => "Edit Widget",
        "component.convert" | "component.expose" | "component.place" | "component.instance.set" => "Component",
        "duplicator.config" | "duplicator.objects" | "duplicator.path" | "duplicator.set" => "Duplicator",
        "brush.bitmap.stroke" | "brush.bitmap.publish" => "Brush Stroke",
        "asset.attach" => "Attach Asset",
        "asset.remove" => "Remove Asset",
        "asset.font.import" => "Import Font",
        "import.batch" => "Import",
        "audio.track.edit" => "Edit Audio Track",
        n if n.starts_with("labs.") => "Labs",
        n if n.starts_with("project.") => "Project Settings",
        _ => DEFAULT_LABEL,
    };
    label.chars().take(MAX_LABEL).collect()
}

fn identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(b))
}
impl NativeObjectHistory {
    /// Called under the common dispatch owner lock, before command/receipt dispatch.
    /// History is live state: even retained snapshots cannot select an older stack.
    pub fn dispatch_history_read(
        &self,
        request: &OpacityRequest,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        use DispatchErrorCode::*;
        let failure = |code| ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: false,
            result: None,
            error: Some(DispatchError {
                code,
                message: "Current native history read refused.".into(),
                details: (code == WrongDocument)
                    .then(|| json!({"requestedDocumentId":request.document_id})),
            }),
        };
        if !identifier(&request.request_id)
            || !identifier(&request.instance_id)
            || !identifier(&request.document_id)
            || !identifier(self.instance_id())
            || !identifier(self.document_id())
        {
            return Err(InvalidRequest);
        }
        let payload = request.payload.as_object();
        // Optional page: `offset` (absent = a window that keeps the cursor inside) and `limit` (1..=MAX_PAGE).
        let optional = |key: &str| payload.and_then(|p| p.get(key)).map(Value::as_u64);
        let (offset, limit) = (optional("offset"), optional("limit"));
        let revision = request.payload.get("atRevision").and_then(Value::as_u64);
        let snapshot_id = request
            .payload
            .get("documentSnapshotId")
            .and_then(Value::as_str);
        let code = if request.api_version != APPLICATION_API_VERSION
            || request.operation() != "query.document.object.history"
            || request.expected_revision.is_some()
            || !serde_json::to_vec(request).is_ok_and(|b| b.len() <= 4096)
            || payload.is_none_or(|p| {
                !p.contains_key("atRevision")
                    || !p.contains_key("documentSnapshotId")
                    || p.keys().any(|k| {
                        !["atRevision", "documentSnapshotId", "offset", "limit"]
                            .contains(&k.as_str())
                    })
            })
            || offset.is_some_and(|o| o.is_none_or(|o| o > MAX_SAFE))
            || limit.is_some_and(|l| l.is_none_or(|l| !(1..=MAX_PAGE).contains(&l)))
            || revision.is_none_or(|r| r > MAX_SAFE)
            || snapshot_id.is_none_or(|s| !identifier(s))
        {
            Some(InvalidRequest)
        } else if request.instance_id != self.instance_id() {
            Some(WrongInstance)
        } else if request.document_id != self.document_id() {
            Some(WrongDocument)
        } else if request.cancelled_before_dispatch {
            Some(CancelledBeforeDispatch)
        } else if revision != Some(self.content_revision()) {
            Some(StaleRevision)
        } else {
            None
        };
        if let Some(code) = code {
            return Ok(failure(code));
        }
        let Some(snapshot) = self.acquire_snapshot(self.content_revision()) else {
            return Ok(failure(Unavailable));
        };
        if snapshot_id != Some(snapshot.snapshot_id()) {
            return Ok(failure(StaleRevision));
        }
        let (undo, redo) = self.history_depths();
        let undo = u64::try_from(undo)
            .ok()
            .filter(|n| *n <= MAX_SAFE)
            .ok_or(Unavailable)?;
        let redo = u64::try_from(redo)
            .ok()
            .filter(|n| *n <= MAX_SAFE)
            .ok_or(Unavailable)?;
        // Chronological order: oldest done entry first, then the nearest redo; the cursor is `undo`.
        let limit = limit.flatten().unwrap_or(MAX_PAGE);
        let offset = offset
            .flatten()
            .unwrap_or_else(|| undo.saturating_sub(DEFAULT_BEFORE));
        let entries: Vec<Value> = (offset..offset.saturating_add(limit).min(undo + redo))
            .map(|index| {
                let entry = if index < undo {
                    &self.undo[index as usize]
                } else {
                    &self.redo[self.redo.len() - 1 - (index - undo) as usize]
                };
                json!({"index":index,"label":self.labels.get(entry.after_revision)})
            })
            .collect();
        let response = ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            error: None,
            result: Some(json!({"atRevision":self.content_revision(),
                "documentSnapshotId":snapshot.snapshot_id(),"undoDepth":undo,"redoDepth":redo,
                "offset":offset,"entries":entries})),
        };
        if !serde_json::to_vec(&response).is_ok_and(|b| b.len() <= 4096) {
            return Ok(failure(Unavailable));
        }
        Ok(response)
    }
}

impl NativeObjectHistory {
    /// Labels the entries a dispatched command just added (everything above `undo_before`). Undo and
    /// redo move entries that already have their label.
    pub(super) fn label_new_entries(
        &mut self,
        undo_before: usize,
        operation: &str,
        payload: &Value,
    ) {
        if self.undo.len() <= undo_before
            || matches!(
                operation,
                "command.document.object.undo" | "command.document.object.redo"
            )
        {
            return;
        }
        let label = label_for(operation, payload);
        for entry in &self.undo[undo_before..] {
            self.labels.0.insert(entry.after_revision, label.clone());
        }
        if self.labels.0.len() > self.undo.len() + self.redo.len() + 64 {
            let live: std::collections::BTreeSet<u64> = self
                .undo
                .iter()
                .chain(&self.redo)
                .map(|e| e.after_revision)
                .collect();
            self.labels.0.retain(|revision, _| live.contains(revision));
        }
    }

    /// Optional `steps` of an undo/redo payload (absent = 1): the payload must be exactly the
    /// command, or the command plus a positive safe-integer `steps`.
    pub(super) fn history_steps(payload: &Value, command: &str) -> Option<usize> {
        let object = payload.as_object()?;
        if object.get("command").and_then(Value::as_str) != Some(command)
            || object.keys().any(|k| k != "command" && k != "steps")
        {
            return None;
        }
        match object.get("steps") {
            None => Some(1),
            Some(steps) => steps
                .as_u64()
                .filter(|n| (1..=MAX_SAFE).contains(n))
                .and_then(|n| usize::try_from(n).ok()),
        }
    }

    /// Moves `steps` entries as ONE restore: the content of the furthest entry's retained revision
    /// becomes one new revision, which is exactly what `steps` single moves end on. Nothing moves
    /// unless that restore succeeds.
    pub(super) fn move_history_steps(
        &mut self,
        expected_revision: u64,
        undo: bool,
        steps: usize,
    ) -> Result<u64, DispatchErrorCode> {
        if expected_revision != self.content_revision() {
            return Err(DispatchErrorCode::StaleRevision);
        }
        let source = if undo { &mut self.undo } else { &mut self.redo };
        if steps == 0 || steps > source.len() {
            return Err(DispatchErrorCode::Unavailable);
        }
        let furthest = &source[source.len() - steps];
        let revision = if undo {
            furthest.before_revision
        } else {
            furthest.after_revision
        };
        let restored = self.owner.restore(revision)?;
        let (from, to) = if undo {
            (&mut self.undo, &mut self.redo)
        } else {
            (&mut self.redo, &mut self.undo)
        };
        let at = from.len() - steps;
        to.extend(from.split_off(at).into_iter().rev());
        Ok(restored)
    }
}
