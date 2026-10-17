//! Lane-6 object operations: closed request and result shapes of raster/text item edits and the
//! embedded-image asset catalog. Shape only: the one native owner repeats every rule (ranges, UTF-16
//! text bound, asset resolution, lock policy) on the original bytes and stays the only authority.
use super::{bounded_identifier, exact, frame};
use crate::contract::{
    NativeApplicationRequest, NativeApplicationResponse, NATIVE_MAX_MESSAGE_BYTES,
};
use serde_json::{Map, Value};

pub(crate) const ITEM: &str = "command.document.object.item.edit";
pub(crate) const ATTACH: &str = "command.document.object.asset.attach";
pub(crate) const REMOVE: &str = "command.document.object.asset.remove";
const FONTS: [&str; 8] = [
    "Roboto-Regular",
    "Roboto-Bold",
    "DejaVuSans-Regular",
    "DejaVuSans-Bold",
    "DejaVuSerif-Regular",
    "DejaVuSerif-Bold",
    "Manrope-Regular",
    "Inter-Regular",
];
const MEDIA: [&str; 7] = [
    "image/png",
    "image/jpeg",
    "image/gif",
    "image/webp",
    "image/bmp",
    "image/svg+xml",
    "image/avif",
];
const RESULT_OPERATIONS: [&str; 9] = [
    "raster.create",
    "raster.move",
    "raster.resize",
    "raster.rotate",
    "raster.opacity",
    "raster.delete",
    "text.create",
    "text.edit",
    "text.delete",
];

pub(crate) fn owns(operation: &str) -> bool {
    matches!(operation, ITEM | ATTACH | REMOVE)
}

fn opaque(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|v| !v.trim().is_empty() && v.len() <= 255 && !v.contains('\0'))
}

fn snapshot(value: &Value) -> bool {
    value.as_str().is_some_and(bounded_identifier)
}

fn target(value: &Value, create: bool) -> bool {
    // A created item names no stroke id (the owner allocates it); every other target names one.
    let stroke = if create {
        value["strokeId"] == ""
    } else {
        opaque(&value["strokeId"])
    };
    exact(
        value,
        &["contextId", "frameScope", "layerUid", "strokeId"],
        &[],
    )
    .is_some()
        && value["contextId"] == "scene-root"
        && opaque(&value["layerUid"])
        && stroke
        && exact(&value["frameScope"], &["kind", "frame"], &[]).is_some()
        && matches!(
            value["frameScope"]["kind"].as_str(),
            Some("authored" | "reference")
        )
        && frame(value["frameScope"].get("frame")).is_some()
}

fn raster_item(value: &Value) -> bool {
    let optional = [
        "opacity",
        "rotation",
        "isBitmapBrush",
        "brushGroupId",
        "isBrushTextureCopy",
        "meshId",
    ];
    exact(value, &["assetId", "x", "y", "width", "height"], &optional).is_some_and(|members| {
        opaque(&value["assetId"])
            && ["x", "y", "width", "height"]
                .iter()
                .all(|key| value[*key].is_number())
            && ["opacity", "rotation"]
                .iter()
                .all(|key| members.get(*key).is_none_or(Value::is_number))
            && ["isBitmapBrush", "isBrushTextureCopy"]
                .iter()
                .all(|key| members.get(*key).is_none_or(Value::is_boolean))
            && ["brushGroupId", "meshId"]
                .iter()
                .all(|key| members.get(*key).is_none_or(opaque))
    })
}

fn font(value: &Value) -> bool {
    match value["kind"].as_str() {
        Some("bundled") => {
            exact(value, &["kind", "fontKey"], &[]).is_some()
                && value["fontKey"]
                    .as_str()
                    .is_some_and(|k| FONTS.contains(&k))
        }
        Some("asset") => {
            exact(value, &["kind", "assetId"], &[]).is_some() && opaque(&value["assetId"])
        }
        _ => false,
    }
}

fn color(value: &str) -> bool {
    value.starts_with('#')
        && matches!(value.len(), 7 | 9)
        && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

/// Each PRESENT member of a text block or patch, by name; required-ness is the caller's.
fn text_fields(members: &Map<String, Value>) -> bool {
    members.iter().all(|(key, value)| match key.as_str() {
        "text" => value
            .as_str()
            .is_some_and(|t| !t.is_empty() && !t.contains('\0')),
        "font" => font(value),
        "size" | "letterSpacing" | "wordSpacing" | "lineHeightMult" => value.is_number(),
        "color" => value.as_str().is_some_and(color),
        "align" => matches!(value.as_str(), Some("left" | "center" | "right")),
        "fixedWidth" | "fixedHeight" => value.is_null() || value.is_number(),
        "bold" | "italic" | "underline" | "strike" => value.is_boolean(),
        "textCase" => matches!(
            value.as_str(),
            Some("none" | "upper" | "lower" | "capitalize")
        ),
        "anchorTopLeft" => {
            exact(value, &["x", "y"], &[]).is_some()
                && value["x"].is_number()
                && value["y"].is_number()
        }
        _ => false,
    })
}

const TEXT_OPTIONAL: [&str; 11] = [
    "fixedWidth",
    "fixedHeight",
    "bold",
    "italic",
    "underline",
    "strike",
    "letterSpacing",
    "wordSpacing",
    "lineHeightMult",
    "textCase",
    "anchorTopLeft",
];

fn text_block(value: &Value) -> bool {
    exact(
        value,
        &["text", "font", "size", "color", "align", "anchorTopLeft"],
        &TEXT_OPTIONAL,
    )
    .is_some_and(text_fields)
}

/// A patch names members to replace; it never carries `anchorTopLeft` (an edit cannot re-anchor).
fn text_patch(value: &Value) -> bool {
    value.as_object().is_some_and(|members| {
        !members.is_empty() && !members.contains_key("anchorTopLeft") && text_fields(members)
    })
}

fn item_request(payload: &Value) -> bool {
    let Some(command) = payload["command"].as_str() else {
        return false;
    };
    let stable = |extra: &[&str]| {
        let mut keys = vec!["command", "stableTarget"];
        keys.extend_from_slice(extra);
        exact(payload, &keys, &[]).is_some()
            && target(&payload["stableTarget"], false)
            && extra.iter().all(|key| payload[*key].is_number())
    };
    match command {
        "object.raster.create" => {
            exact(payload, &["command", "target", "item"], &[]).is_some()
                && target(&payload["target"], true)
                && raster_item(&payload["item"])
        }
        "object.raster.move" => stable(&["x", "y"]),
        "object.raster.resize" => stable(&["width", "height"]),
        "object.raster.rotate" => stable(&["rotation"]),
        "object.raster.opacity" => stable(&["opacity"]),
        "object.raster.delete" | "object.text.delete" => stable(&[]),
        "object.text.create" => {
            exact(payload, &["command", "target", "block"], &[]).is_some()
                && target(&payload["target"], true)
                && text_block(&payload["block"])
        }
        "object.text.edit" => {
            exact(payload, &["command", "stableTarget", "patch"], &[]).is_some()
                && target(&payload["stableTarget"], false)
                && text_patch(&payload["patch"])
        }
        _ => false,
    }
}

pub(crate) fn request(operation: &str, payload: &Value) -> bool {
    match operation {
        ITEM => item_request(payload),
        ATTACH => {
            exact(
                payload,
                &["documentSnapshotId", "name", "mediaType", "source"],
                &[],
            )
            .is_some()
                && snapshot(&payload["documentSnapshotId"])
                && opaque(&payload["name"])
                && payload["mediaType"]
                    .as_str()
                    .is_some_and(|m| MEDIA.contains(&m))
                && exact(&payload["source"], &["kind", "path"], &[]).is_some()
                && payload["source"]["kind"] == "file"
                && payload["source"]["path"]
                    .as_str()
                    .is_some_and(|p| !p.is_empty() && p.len() <= 4096 && !p.contains('\0'))
        }
        REMOVE => {
            exact(payload, &["documentSnapshotId", "assetId"], &[]).is_some()
                && snapshot(&payload["documentSnapshotId"])
                && opaque(&payload["assetId"])
        }
        _ => false,
    }
}

fn command_receipt(value: &Value) -> bool {
    value["applied"].is_boolean()
        && value["historyEntriesAdded"].as_u64() == Some(u64::from(value["applied"] == true))
}

/// The receipt of an item edit names the operation it ran and the stroke it touched: the stable target for
/// an edit or delete, an owner-allocated id (`native-raster:` / `native-text:`) for a create.
pub(crate) fn result(operation: &str, payload: &Value, value: &Value) -> bool {
    match operation {
        ITEM => {
            let Some(command) = payload["command"].as_str() else {
                return false;
            };
            let Some(ran) = command.strip_prefix("object.") else {
                return false;
            };
            let Some(stroke) = value["strokeId"].as_str() else {
                return false;
            };
            let allocated = if ran.starts_with("raster.") {
                stroke.starts_with("native-raster:")
            } else {
                stroke.starts_with("native-text:")
            };
            let stroke_matches = if ran.ends_with(".create") {
                allocated
            } else {
                payload["stableTarget"]["strokeId"].as_str() == Some(stroke)
            };
            exact(
                value,
                &["applied", "historyEntriesAdded", "operation", "strokeId"],
                &[],
            )
            .is_some()
                && command_receipt(value)
                && RESULT_OPERATIONS.contains(&ran)
                && value["operation"].as_str() == Some(ran)
                && stroke_matches
        }
        ATTACH | REMOVE => {
            exact(value, &["applied", "historyEntriesAdded"], &[]).is_some()
                && command_receipt(value)
        }
        _ => true,
    }
}

/// A successful receipt belongs to the request owner and advances the revision by exactly its own
/// disposition: one when applied, none for an exact no-op. Complete responses stay within 4096 bytes.
pub(crate) fn response_matches(
    request: &NativeApplicationRequest,
    response: &NativeApplicationResponse,
) -> bool {
    if !owns(&request.operation) {
        return true;
    }
    let Some(value) = &response.result else {
        return false;
    };
    response.request_id == request.request_id
        && response.instance_id == request.instance_id
        && response.document_id == request.document_id
        && request
            .expected_revision
            .and_then(|before| before.checked_add(u64::from(value["applied"] == true)))
            == Some(response.content_revision)
        && serde_json::to_vec(response).is_ok_and(|bytes| bytes.len() <= NATIVE_MAX_MESSAGE_BYTES)
}

#[cfg(test)]
#[path = "native_item_contract_tests.rs"]
mod tests;
