//! L05-K public contract of the native frame-edit operations (F09-F13): closed request shapes and the one compact
//! result, with Repeat-only cycle/skip counts. Storage, history, locks and receipts stay in the native owner; every shape mirrors the strict payload
//! parser of the owner of that operation (structure, key, retime, selection duplicate, repeat). F7 always publishes one checkpoint. Cut, copy and paste
//! have no declaration here: a clipboard packet is produced by the typed native API only.
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
const OPERATIONS: [&str; 12] = [
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
