//! Bounded shape clipboard payload and retained receipt validation.
//! Execution and reference validation remain in the native document owner.
use crate::contract::NativeApplicationRequest;
use crate::native_contract::{
    exact,
    object_edit::{frame, uid, MAX_ITEMS},
};
use serde_json::Value;

fn clip_record(value: &Value) -> bool {
    exact(
        value,
        &["schemaVersion", "family", "target", "geometry"],
        &["fill", "vector", "vectorExt", "effects"],
    )
    .is_some_and(|record| {
        record.get("target").is_some_and(|target| {
            exact(
                target,
                &["contextId", "frameScope", "layerUid", "strokeId"],
                &[],
            )
            .is_some_and(|target| {
                target
                    .get("frameScope")
                    .is_some_and(|scope| exact(scope, &["kind", "frame"], &[]).is_some())
            })
        }) && record.get("geometry").is_some_and(Value::is_object)
    })
}

fn raster(value: &Value) -> bool {
    exact(value, &["target", "item"], &[]).is_some_and(|entry| {
        exact(&entry["target"], &["contextId", "frameScope", "layerUid", "strokeId"], &[])
            .is_some_and(|target| exact(&target["frameScope"], &["kind", "frame"], &[]).is_some())
            && exact(&entry["item"], &["assetId", "x", "y", "width", "height"],
                &["opacity", "rotation", "isBitmapBrush", "brushGroupId", "isBrushTextureCopy", "meshId"]).is_some()
    })
}

fn source(value: &Value) -> bool {
    exact(value, &["instanceId", "documentId", "contentRevision", "strokeIds"], &[])
        .is_some_and(|source| uid(&source["instanceId"]) && uid(&source["documentId"])
            && source["contentRevision"].as_u64().is_some_and(|n| n <= 9_007_199_254_740_991)
            && source["strokeIds"].as_array().is_some_and(|ids| !ids.is_empty()
                && ids.len() <= MAX_ITEMS && ids.iter().all(uid)))
}

/// Bounded structural pre-check; the native owner re-validates every record.
pub(super) fn clip(value: &Value) -> bool {
    exact(
        value,
        &["contextId", "layerUid", "frame", "records"],
        &["groups", "rasters", "source"],
    )
    .is_some_and(|object| {
        object.get("contextId").is_some_and(uid)
            && object.get("layerUid").is_some_and(uid)
            && object.get("frame").is_some_and(frame)
            && object
                .get("records")
                .and_then(Value::as_array)
                .is_some_and(|records| {
                    records.len() <= MAX_ITEMS && records.iter().all(clip_record)
                })
            && object.get("groups").is_none_or(|groups| {
                exact(groups, &["contextId", "entries", "members"], &[]).is_some()
            })
            && match (object.get("rasters"), object.get("source")) {
                (None, None) => true,
                (Some(rasters), Some(origin)) => source(origin) && rasters.as_array()
                    .is_some_and(|items| !items.is_empty() && items.len() <= MAX_ITEMS && items.iter().all(raster)),
                _ => false,
            }
    })
}

pub(super) fn operation(operation: &str) -> bool {
    matches!(
        operation,
        "command.document.object.clipboard.copy" | "command.document.object.clipboard.cut"
    )
}

pub(super) fn result(operation: &str, value: &Value) -> bool {
    exact(value, &["applied", "historyEntriesAdded", "clipboard"], &[]).is_some_and(|members| {
        let Some(applied) = members["applied"].as_bool() else {
            return false;
        };
        if members["historyEntriesAdded"].as_u64() != Some(u64::from(applied)) {
            return false;
        }
        let packet = &members["clipboard"];
        (packet.is_null() || clip(packet))
            && if operation.ends_with(".copy") {
                !applied
            } else {
                applied != packet.is_null()
            }
    })
}

pub(super) fn correlated(request: &NativeApplicationRequest, value: &Value) -> bool {
    if !operation(&request.operation) {
        return true;
    }
    result(&request.operation, value)
        && (value["clipboard"].is_null()
            || ["contextId", "layerUid", "frame"]
                .iter()
                .all(|key| value["clipboard"][*key] == request.payload[*key]))
}
