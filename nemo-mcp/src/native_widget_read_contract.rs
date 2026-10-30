//! #2255: closed, bounded widget presentation view of the existing common object query.
use super::{exact, frame, revision};
use serde_json::Value;

const REASONS: [&str; 5] = [
    "expression",
    "sample",
    "parent",
    "parent_geometry",
    "canvas",
];

pub(super) fn widget_view(value: &Value) -> bool {
    value["view"]["kind"] == "widget"
}
pub(super) fn request(value: &Value) -> bool {
    let Some(payload) = exact(value, &["atRevision", "documentSnapshotId", "view"], &[]) else {
        return false;
    };
    let Some(view) = exact(
        &payload["view"],
        &[
            "kind",
            "contextId",
            "frame",
            "workId",
            "viewGeneration",
            "cursor",
            "limit",
        ],
        &[],
    ) else {
        return false;
    };
    revision(payload.get("atRevision")).is_some()
        && payload["documentSnapshotId"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
        && view["kind"] == "widget"
        && view["contextId"] == "scene-root"
        && frame(view.get("frame")).is_some()
        && view["workId"]
            .as_str()
            .is_some_and(crate::contract::bounded_identifier)
        && revision(view.get("viewGeneration")).is_some_and(|generation| generation > 0)
        && view["cursor"].as_u64().is_some_and(|cursor| cursor <= 1024)
        && view["limit"]
            .as_u64()
            .is_some_and(|limit| (1..=4).contains(&limit))
}
fn number(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
}
fn pair(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|v| v.len() == 2 && v.iter().all(number))
}
fn quad(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|v| v.len() == 4 && v.iter().all(number))
}
fn axis(value: &Value) -> bool {
    exact(value, &["key", "min", "max", "rest", "label", "value"], &[]).is_some_and(|axis| {
        axis["key"].is_string()
            && axis["label"].is_string()
            && ["min", "max", "rest"].iter().all(|key| number(&axis[*key]))
            && (axis["value"].is_null() || number(&axis["value"]))
    })
}
fn item(value: &Value) -> bool {
    let Some(item) = exact(
        value,
        &[
            "layerUid",
            "visible",
            "locked",
            "kind",
            "x",
            "y",
            "center",
            "angle",
            "size",
            "puck",
            "rest",
            "anchor",
            "linear",
            "unavailable",
        ],
        &[],
    ) else {
        return false;
    };
    let slider = item["kind"] == "slider";
    let geometry = [
        &item["center"],
        &item["size"],
        &item["rest"],
        &item["anchor"],
    ];
    let placed = !item["center"].is_null();
    let basics = item["layerUid"].as_str().is_some_and(|uid| !uid.is_empty())
        && item["visible"].is_boolean()
        && item["locked"].is_boolean()
        && (slider || item["kind"] == "joystick")
        && axis(&item["x"])
        && (if slider {
            item["y"].is_null()
        } else {
            axis(&item["y"])
        });
    let reason = item["unavailable"].as_str();
    let consistent = match reason {
        None => item["unavailable"].is_null() && placed && pair(&item["puck"]),
        Some("control") => placed && item["puck"].is_null(),
        Some(reason) => REASONS.contains(&reason) && !placed && item["puck"].is_null(),
    };
    basics
        && consistent
        && if placed {
            geometry.iter().all(|v| pair(v)) && number(&item["angle"]) && quad(&item["linear"])
        } else {
            geometry.iter().all(|v| v.is_null())
                && item["angle"].is_null()
                && item["linear"].is_null()
        }
}
pub(super) fn result(document_id: &str, payload: &Value, result: &Value) -> bool {
    let Some(answer) = exact(
        result,
        &[
            "kind",
            "atRevision",
            "documentSnapshotId",
            "contextId",
            "frame",
            "workId",
            "viewGeneration",
            "cursor",
            "total",
            "nextCursor",
            "items",
        ],
        &[],
    ) else {
        return false;
    };
    let (view, cursor, limit) = (
        &payload["view"],
        payload["view"]["cursor"].as_u64(),
        payload["view"]["limit"].as_u64(),
    );
    let (Some(cursor), Some(limit), Some(total), Some(items)) = (
        cursor,
        limit,
        answer["total"].as_u64(),
        answer["items"].as_array(),
    ) else {
        return false;
    };
    let end = cursor + items.len() as u64;
    request(payload)
        && answer["kind"] == "widget"
        && answer["atRevision"] == payload["atRevision"]
        && answer["documentSnapshotId"] == payload["documentSnapshotId"]
        && answer["documentSnapshotId"].as_str()
            == Some(format!("native-object:{document_id}:{}", payload["atRevision"]).as_str())
        && ["contextId", "frame", "workId", "viewGeneration"]
            .iter()
            .all(|key| answer[*key] == view[*key])
        && answer["cursor"].as_u64() == Some(cursor)
        && items.len() as u64 <= limit
        && end <= total
        && total <= 1024
        && if end == total {
            answer["nextCursor"].is_null()
        } else {
            answer["nextCursor"].as_u64() == Some(end)
        }
        && items.iter().all(item)
}
