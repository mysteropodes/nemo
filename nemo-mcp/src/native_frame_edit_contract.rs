//! Public native frame-edit contract: closed input and operation-aware result DTOs.
//! The native owner retains documents, history, locks and receipts. Clipboard checks here validate
//! bounded DTO containers; native strict decoding validates the full record/order grammar atomically.
use crate::native_contract::{exact, frame, object};
use serde_json::{Map, Value};
use std::collections::HashSet;

pub(crate) const INSERT: &str = "command.document.object.frames.insert";
pub(crate) const EXTEND: &str = "command.document.object.frames.extend";
pub(crate) const DUPLICATE: &str = "command.document.object.frames.duplicate";
pub(crate) const REMOVE: &str = "command.document.object.frames.remove";
pub(crate) const KEY_PROMOTE: &str = "command.document.object.layer.key.promote";
pub(crate) const KEY_BLANK: &str = "command.document.object.layer.key.blank";
pub(crate) const KEY_CLEAR: &str = "command.document.object.layer.key.clear";
pub(crate) const KEY_CLEAR_CELLS: &str = "command.document.object.layer.key.clear_cells";
pub(crate) const TWEEN_REMOVE: &str = "command.document.object.layer.tween.remove";
pub(crate) const KEY_MOVE: &str = "command.document.object.layer.key.move";
pub(crate) const SELECTION_DUPLICATE: &str = "command.document.object.frames.clipboard.duplicate";
pub(crate) const REPEAT: &str = "command.document.object.frames.repeat";
pub(crate) const CLIPBOARD_COPY: &str = "command.document.object.frames.clipboard.copy";
pub(crate) const CLIPBOARD_CUT: &str = "command.document.object.frames.clipboard.cut";
pub(crate) const CLIPBOARD_PASTE: &str = "command.document.object.frames.clipboard.paste";
const OPERATIONS: [&str; 15] = [
    INSERT,
    EXTEND,
    DUPLICATE,
    REMOVE,
    KEY_PROMOTE,
    KEY_BLANK,
    KEY_CLEAR,
    KEY_CLEAR_CELLS,
    TWEEN_REMOVE,
    KEY_MOVE,
    SELECTION_DUPLICATE,
    CLIPBOARD_COPY,
    CLIPBOARD_CUT,
    CLIPBOARD_PASTE,
    REPEAT,
];
const MAX_LIST: usize = 4096;
const MAX_EXTEND: u64 = 999;
const MAX_SAFE: i64 = 9_007_199_254_740_991;

pub(crate) fn declares(operation: &str) -> bool {
    OPERATIONS.contains(&operation)
}

fn command_is(map: &Map<String, Value>, expected: &str) -> bool {
    map.get("command").and_then(Value::as_str) == Some(expected)
}
/// An opaque layer UID: any nonempty string; no identifier grammar is invented.
fn name(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|text| !text.is_empty())
}
fn unique_names(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|items| {
        let mut seen = HashSet::new();
        (1..=MAX_LIST).contains(&items.len())
            && items.iter().all(|item| {
                item.as_str()
                    .is_some_and(|text| !text.is_empty() && seen.insert(text))
            })
    })
}
fn unique_frames(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|items| {
        let mut seen = HashSet::new();
        (1..=MAX_LIST).contains(&items.len())
            && items
                .iter()
                .all(|item| frame(Some(item)).is_some_and(|number| seen.insert(number)))
    })
}
fn unique_cells(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|items| {
        let mut seen = HashSet::new();
        (1..=MAX_LIST).contains(&items.len())
            && items.iter().all(|item| {
                exact(item, &["layerUid", "frame"], &[]).is_some_and(|cell| {
                    let layer = cell
                        .get("layerUid")
                        .and_then(Value::as_str)
                        .filter(|text| !text.is_empty());
                    match (layer, frame(cell.get("frame"))) {
                        (Some(layer), Some(number)) => seen.insert((layer, number)),
                        _ => false,
                    }
                })
            })
    })
}
// This is the bounded DTO shell. Native decoding validates full records/order and their relationships.
fn object_list(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_array)
        .is_some_and(|items| items.len() <= MAX_LIST && items.iter().all(Value::is_object))
}
fn clipboard_cells(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(|cells| {
        cells.len() <= MAX_LIST
            && cells.iter().all(|cell| {
                exact(
                    cell,
                    &["relLayer", "relFrame", "keyframe", "records"],
                    &["orderElements"],
                )
                .is_some_and(|cell| {
                    frame(cell.get("relLayer")).is_some()
                        && frame(cell.get("relFrame")).is_some()
                        && cell.get("keyframe").is_some_and(Value::is_boolean)
                        && object_list(cell.get("records"))
                        && cell
                            .get("orderElements")
                            .is_none_or(|v| object_list(Some(v)))
                })
            })
    })
}
fn clipboard_selection(value: Option<&Value>) -> bool {
    value.and_then(Value::as_array).is_some_and(Vec::is_empty) || unique_cells(value)
}
fn clipboard_packet(value: &Value) -> bool {
    exact(value, &["instanceId", "documentId", "cells"], &[]).is_some_and(|packet| {
        name(packet.get("instanceId"))
            && name(packet.get("documentId"))
            && clipboard_cells(packet.get("cells"))
    })
}
fn clipboard_result(operation: &str, value: &Value) -> bool {
    exact(value, &["applied", "historyEntriesAdded", "clipboard"], &[]).is_some_and(|value| {
        let publication = (
            value.get("applied").and_then(Value::as_bool),
            value.get("historyEntriesAdded").and_then(Value::as_u64),
        );
        (matches!(publication, (Some(false), Some(0)))
            || (operation == CLIPBOARD_CUT && matches!(publication, (Some(true), Some(1)))))
            && value.get("clipboard").is_some_and(|packet| {
                if packet.is_null() {
                    publication == (Some(false), Some(0))
                } else {
                    clipboard_packet(packet)
                }
            })
    })
}
/// The response header and detached packet both belong to the actual invoking owner.
pub(crate) fn clipboard_identity_matches(
    operation: &str,
    instance_id: &str,
    document_id: &str,
    value: &Value,
) -> bool {
    if !matches!(operation, CLIPBOARD_COPY | CLIPBOARD_CUT) {
        return true;
    }
    clipboard_result(operation, value)
        && value.get("clipboard").is_some_and(|packet| {
            packet.is_null()
                || (packet.get("instanceId").and_then(Value::as_str) == Some(instance_id)
                    && packet.get("documentId").and_then(Value::as_str) == Some(document_id))
        })
}
fn span(map: &Map<String, Value>) -> bool {
    name(map.get("layerUid")) && frame(map.get("from")).is_some() && frame(map.get("to")).is_some()
}

pub(super) fn request(operation: &str, value: &Value) -> bool {
    match operation {
        INSERT => exact(value, &["command", "cell"], &["layerUids"]).is_some_and(|p| {
            command_is(p, "object.frames.insert")
                && frame(p.get("cell")).is_some()
                && p.get("layerUids").is_none_or(|v| unique_names(Some(v)))
        }),
        EXTEND => exact(value, &["command", "cell", "count"], &[]).is_some_and(|p| {
            command_is(p, "object.frames.extend")
                && frame(p.get("cell")).is_some()
                && frame(p.get("count")).is_some_and(|count| count <= MAX_EXTEND)
        }),
        DUPLICATE => exact(value, &["command", "layerUid", "cell"], &[]).is_some_and(|p| {
            command_is(p, "object.frames.duplicate")
                && name(p.get("layerUid"))
                && frame(p.get("cell")).is_some()
        }),
        REMOVE => exact(value, &["command", "from", "to"], &[]).is_some_and(|p| {
            command_is(p, "object.frames.remove")
                && frame(p.get("from")).is_some()
                && frame(p.get("to")).is_some()
        }),
        KEY_PROMOTE => exact(value, &["command", "cells"], &["layerUids"]).is_some_and(|p| {
            command_is(p, "object.layer.key.promote")
                && unique_frames(p.get("cells"))
                && p.get("layerUids").is_none_or(|v| unique_names(Some(v)))
        }),
        KEY_BLANK => exact(value, &["command", "layerUid", "cell"], &[]).is_some_and(|p| {
            command_is(p, "object.layer.key.blank")
                && name(p.get("layerUid"))
                && frame(p.get("cell")).is_some()
        }),
        KEY_CLEAR => exact(value, &["command", "layerUid", "cell"], &[]).is_some_and(|p| {
            command_is(p, "object.layer.key.clear")
                && name(p.get("layerUid"))
                && frame(p.get("cell")).is_some()
        }),
        KEY_CLEAR_CELLS => exact(value, &["command", "targets"], &[]).is_some_and(|p| {
            command_is(p, "object.layer.key.clear_cells") && unique_cells(p.get("targets"))
        }),
        TWEEN_REMOVE => exact(value, &["command", "layerUid", "from", "to"], &[])
            .is_some_and(|p| command_is(p, "object.layer.tween.remove") && span(p)),
        KEY_MOVE => exact(value, &["command", "layerUid", "from", "to"], &[])
            .is_some_and(|p| command_is(p, "object.layer.key.move") && span(p)),
        SELECTION_DUPLICATE => exact(value, &["command", "layerUid", "selection"], &[])
            .is_some_and(|p| {
                command_is(p, "object.frames.clipboard.duplicate")
                    && name(p.get("layerUid"))
                    && unique_cells(p.get("selection"))
            }),
        CLIPBOARD_COPY | CLIPBOARD_CUT => {
            exact(value, &["command", "selection"], &[]).is_some_and(|p| {
                let command = if operation == CLIPBOARD_COPY {
                    "object.frames.clipboard.copy"
                } else {
                    "object.frames.clipboard.cut"
                };
                command_is(p, command) && clipboard_selection(p.get("selection"))
            })
        }
        CLIPBOARD_PASTE => exact(
            value,
            &[
                "command",
                "instanceId",
                "documentId",
                "layerUid",
                "frame",
                "cells",
            ],
            &[],
        )
        .is_some_and(|p| {
            command_is(p, "object.frames.clipboard.paste")
                && name(p.get("instanceId"))
                && name(p.get("documentId"))
                && name(p.get("layerUid"))
                && frame(p.get("frame")).is_some()
                && clipboard_cells(p.get("cells"))
        }),
        REPEAT => exact(
            value,
            &[
                "command",
                "fromLayerUid",
                "toLayerUid",
                "fromFrame",
                "toFrame",
                "times",
            ],
            &[],
        )
        .is_some_and(|p| {
            command_is(p, "object.frames.repeat")
                && name(p.get("fromLayerUid"))
                && name(p.get("toLayerUid"))
                && frame(p.get("fromFrame")).is_some()
                && frame(p.get("toFrame")).is_some()
                && p.get("times")
                    .and_then(Value::as_i64)
                    .is_some_and(|times| (-MAX_SAFE..=MAX_SAFE).contains(&times))
        }),
        _ => false,
    }
}
pub(super) fn result(value: &Value) -> bool {
    object::visibility_result(value)
}
/// Repeat alone exposes native cycle/skip counts; no other operation accepts them.
pub(crate) fn result_for_operation(operation: &str, value: &Value) -> bool {
    if matches!(operation, CLIPBOARD_COPY | CLIPBOARD_CUT) {
        return clipboard_result(operation, value);
    }
    if operation == KEY_BLANK {
        return result(value) && value["applied"] == true;
    }
    if operation != REPEAT {
        return declares(operation) && result(value);
    }
    exact(
        value,
        &[
            "applied",
            "historyEntriesAdded",
            "cycledLayers",
            "skippedComponentLayers",
        ],
        &[],
    )
    .is_some_and(|value| {
        matches!(
            (
                value.get("applied").and_then(Value::as_bool),
                value.get("historyEntriesAdded").and_then(Value::as_u64)
            ),
            (Some(true), Some(1)) | (Some(false), Some(0))
        ) && ["cycledLayers", "skippedComponentLayers"]
            .iter()
            .all(|key| {
                value
                    .get(*key)
                    .and_then(Value::as_u64)
                    .is_some_and(|count| count <= u64::from(u32::MAX))
            })
    })
}
pub(crate) fn revision_matches_for_operation(
    operation: &str,
    expected: Option<u64>,
    revision: u64,
    value: &Value,
) -> bool {
    result_for_operation(operation, value)
        && expected.and_then(|before| before.checked_add(u64::from(value["applied"] == true)))
            == Some(revision)
}
pub(crate) fn revision_matches(expected: Option<u64>, revision: u64, value: &Value) -> bool {
    result(value)
        && expected.and_then(|before| before.checked_add(u64::from(value["applied"] == true)))
            == Some(revision)
}

#[cfg(test)]
#[path = "native_frame_edit_contract_tests.rs"]
mod tests;
