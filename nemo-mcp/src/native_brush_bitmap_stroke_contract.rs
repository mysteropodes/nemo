//! #2361: the bitmap-brush Draw stroke DTO and its compact disposition; the native owner (anchor, companion and the
//! staged PNG in one revision) is authoritative and re-validates the whole record.
use crate::native_contract::exact;
use crate::contract::bounded_identifier;
use serde_json::Value;
pub(crate) const OP: &str = "command.document.object.brush.bitmap.stroke";
const MAX_REVISION: u64 = 9_007_199_254_740_991;
fn finite(value: &Value) -> bool {
    value
        .as_f64()
        .is_some_and(|n| n.is_finite() && n.abs() <= 1.0e12)
}
fn text(value: &Value, max: usize) -> bool {
    value
        .as_str()
        .is_some_and(|v| !v.is_empty() && v.len() <= max && !v.contains('\0'))
}
/// The anchor: a new editable vector path in an authored scene-root cell, 2..256 segments, with a bitmap brush.
fn anchor(value: &Value) -> bool {
    let target = &value["target"];
    let brush = &value["vectorExt"]["bitmapBrush"];
    value["family"] == "editable-vector-path"
        && target["strokeId"] == ""
        && target["contextId"] == "scene-root"
        && target["frameScope"]["kind"] == "authored"
        && target["frameScope"]["frame"]
            .as_u64()
            .is_some_and(|f| f <= u64::from(u32::MAX))
        && target["layerUid"].as_str().is_some_and(bounded_identifier)
        && value["geometry"]["segments"]
            .as_array()
            .is_some_and(|s| (2..=256).contains(&s.len()))
        && text(&brush["brushGroupId"], 255)
        && brush["bitmapBrushSpec"].is_object()
}
pub(super) fn request(payload: &Value) -> bool {
    exact(
        payload,
        &[
            "documentSnapshotId",
            "lifecycleGeneration",
            "record",
            "companion",
            "source",
        ],
        &[],
    )
    .is_some_and(|p| {
        let c = &p["companion"];
        p["documentSnapshotId"]
            .as_str()
            .is_some_and(bounded_identifier)
            && p["lifecycleGeneration"]
                .as_u64()
                .is_some_and(|n| (1..=MAX_REVISION).contains(&n))
            && anchor(&p["record"])
            && exact(c, &["x", "y", "width", "height", "opacity"], &[]).is_some()
            && ["x", "y", "width", "height"].iter().all(|k| finite(&c[*k]))
            && c["width"].as_f64().is_some_and(|w| w > 0.0)
            && c["height"].as_f64().is_some_and(|h| h > 0.0)
            && c["opacity"]
                .as_f64()
                .is_some_and(|o| (0.0..=1.0).contains(&o))
            && exact(&p["source"], &["kind", "path"], &[]).is_some()
            && p["source"]["kind"] == "file"
            && text(&p["source"]["path"], 4096)
    })
}
pub(super) fn result(value: &Value) -> bool {
    exact(value, &["applied", "historyEntriesAdded"], &[]).is_some_and(|v| {
        match v["applied"].as_bool() {
            Some(true) => v["historyEntriesAdded"].as_u64() == Some(1),
            Some(false) => v["historyEntriesAdded"].as_u64() == Some(0),
            None => false,
        }
    })
}

#[cfg(test)]
#[path = "native_brush_bitmap_stroke_contract_tests.rs"]
mod tests;
