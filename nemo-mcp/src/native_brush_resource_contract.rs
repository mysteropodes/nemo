//! Lane-7 bitmap-brush resource operations: closed request and result shapes of the brush publication command and
//! of the brush reads (one store row, the brush part of one pinned scene). Shape only: the one native owner repeats
//! every rule (asset resolution, charged decode, dimensions, quota) on the original bytes and stays the only authority.
//! No pixel bytes travel through this contract: a request names ids and a size.
use super::{bounded_identifier, exact, frame, revision};
use crate::contract::{
    NativeApplicationRequest, NativeApplicationResponse, NATIVE_MAX_MESSAGE_BYTES,
};
use serde_json::Value;

pub(crate) const PUBLISH: &str = "command.document.object.brush.bitmap.publish";
pub(crate) const QUERY: &str = "query.document.object.brush.bitmap";
/// `bakeToCanvas` clamps every bake canvas to this many pixels per side (`MAX_TEX`).
const MAX_SIDE: u64 = 4096;
const MAX_ENTRIES: u64 = 4096;
const MAX_SOURCE_INDEX: u64 = MAX_ENTRIES * 64;

pub(crate) fn owns(operation: &str) -> bool {
    matches!(operation, PUBLISH | QUERY)
}

fn opaque(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|v| !v.trim().is_empty() && v.len() <= 255 && !v.contains('\0'))
}

fn identifier(value: &Value) -> bool {
    value.as_str().is_some_and(bounded_identifier)
}

fn side(value: &Value) -> bool {
    value.as_u64().is_some_and(|v| (1..=MAX_SIDE).contains(&v))
}

/// A decimal store version: a u64 without a leading zero (the owner's counter starts at one).
fn version(value: &Value) -> bool {
    value.as_str().is_some_and(|v| {
        !v.starts_with('0') && v.bytes().all(|b| b.is_ascii_digit()) && v.parse::<u64>().is_ok()
    })
}

/// One resident bitmap: version, size and the decoded RGBA cost `width * height * 4`.
fn bitmap(value: &Value) -> bool {
    // The sides are bounded first, so the product below cannot overflow.
    version(&value["version"])
        && side(&value["width"])
        && side(&value["height"])
        && value["bytes"].as_u64()
            == value["width"]
                .as_u64()
                .zip(value["height"].as_u64())
                .map(|(w, h)| w * h * 4)
}

pub(crate) fn request(operation: &str, payload: &Value) -> bool {
    match operation {
        PUBLISH => {
            exact(
                payload,
                &[
                    "command",
                    "documentSnapshotId",
                    "assetId",
                    "brushId",
                    "width",
                    "height",
                ],
                &[],
            )
            .is_some()
                && payload["command"] == "object.brush.bitmap.publish"
                && identifier(&payload["documentSnapshotId"])
                && opaque(&payload["assetId"])
                && opaque(&payload["brushId"])
                && side(&payload["width"])
                && side(&payload["height"])
        }
        QUERY => {
            let fields: &[&str] = match payload["action"].as_str() {
                Some("inspect") => &["action", "atRevision", "documentSnapshotId", "brushId"],
                Some("scene") => &[
                    "action",
                    "atRevision",
                    "documentSnapshotId",
                    "contextId",
                    "frame",
                    "offset",
                    "limit",
                ],
                _ => return false,
            };
            exact(payload, fields, &[]).is_some()
                && revision(payload.get("atRevision")).is_some()
                && identifier(&payload["documentSnapshotId"])
                && if payload["action"] == "inspect" {
                    opaque(&payload["brushId"])
                } else {
                    identifier(&payload["contextId"])
                        && frame(payload.get("frame")).is_some()
                        && payload["offset"].as_u64().is_some_and(|v| v <= MAX_ENTRIES)
                        && payload["limit"] == 1
                }
        }
        _ => false,
    }
}

fn row(value: &Value) -> bool {
    match value["resident"].as_bool() {
        Some(false) => {
            exact(value, &["brushId", "resident"], &[]).is_some() && opaque(&value["brushId"])
        }
        Some(true) => {
            exact(
                value,
                &["brushId", "resident", "version", "width", "height", "bytes"],
                &[],
            )
            .is_some()
                && opaque(&value["brushId"])
                && bitmap(value)
        }
        None => false,
    }
}

/// One brush-bearing record of the scene: where it sits, which stroke it is, and exactly one of a bitmap or a reason.
fn entry(value: &Value) -> bool {
    let common = ["sourceObjectIndex", "layerUid", "strokeId", "brushId"];
    let tail = if value.get("ink").is_some() {
        "ink"
    } else {
        "unavailable"
    };
    let mut keys = common.to_vec();
    keys.push(tail);
    exact(value, &keys, &[]).is_some()
        && value["sourceObjectIndex"]
            .as_u64()
            .is_some_and(|v| v <= MAX_SOURCE_INDEX)
        && opaque(&value["layerUid"])
        && opaque(&value["strokeId"])
        && opaque(&value["brushId"])
        && if tail == "ink" {
            exact(&value["ink"], &["version", "width", "height", "bytes"], &[]).is_some()
                && bitmap(&value["ink"])
        } else {
            exact(&value["unavailable"], &["code"], &[]).is_some()
                && opaque(&value["unavailable"]["code"])
        }
}

/// The result belongs to the request it answers: same snapshot, same selector, a well-formed page or row or receipt.
pub(crate) fn result(operation: &str, payload: &Value, value: &Value) -> bool {
    if operation == PUBLISH {
        return exact(
            value,
            &[
                "applied",
                "historyEntriesAdded",
                "brushId",
                "version",
                "width",
                "height",
                "bytes",
            ],
            &[],
        )
        .is_some()
            && value["applied"].is_boolean()
            && value["historyEntriesAdded"] == 0
            && value["brushId"] == payload["brushId"]
            && value["width"] == payload["width"]
            && value["height"] == payload["height"]
            && bitmap(value);
    }
    if operation != QUERY
        || value["atRevision"] != payload["atRevision"]
        || value["documentSnapshotId"] != payload["documentSnapshotId"]
    {
        return false;
    }
    if payload["action"] == "inspect" {
        return exact(value, &["atRevision", "documentSnapshotId", "brush"], &[]).is_some()
            && row(&value["brush"])
            && value["brush"]["brushId"] == payload["brushId"];
    }
    let (Some(total), Some(offset), Some(entries)) = (
        value["totalEntries"].as_u64().filter(|v| *v <= MAX_ENTRIES),
        payload["offset"].as_u64(),
        value["entries"].as_array(),
    ) else {
        return false;
    };
    let next = offset + entries.len() as u64;
    exact(
        value,
        &[
            "atRevision",
            "documentSnapshotId",
            "contextId",
            "frame",
            "offset",
            "nextOffset",
            "totalEntries",
            "entries",
        ],
        &[],
    )
    .is_some()
        && value["contextId"] == payload["contextId"]
        && value["frame"] == payload["frame"]
        && value["offset"] == offset
        && offset <= total
        && entries.len() == usize::from(offset < total)
        && entries.iter().all(entry)
        && if next < total {
            value["nextOffset"] == next
        } else {
            value["nextOffset"].is_null()
        }
}

/// A successful answer carries a result and belongs to the request owner. A publication moves no revision; a read
/// answers at the revision it asked for. Complete responses stay within 4096 bytes.
pub(crate) fn response_matches(
    request: &NativeApplicationRequest,
    response: &NativeApplicationResponse,
) -> bool {
    if !owns(&request.operation) {
        return true;
    }
    if response.request_id != request.request_id
        || response.instance_id != request.instance_id
        || response.document_id != request.document_id
        || response.result.is_none()
        || !serde_json::to_vec(response).is_ok_and(|bytes| bytes.len() <= NATIVE_MAX_MESSAGE_BYTES)
    {
        return false;
    }
    if request.operation == PUBLISH {
        return request.expected_revision == Some(response.content_revision);
    }
    request.payload["atRevision"].as_u64() == Some(response.content_revision)
}

#[cfg(test)]
#[path = "native_brush_resource_contract_tests.rs"]
mod tests;
