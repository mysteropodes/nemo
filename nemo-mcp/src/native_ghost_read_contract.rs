//! #2358 D23: closed, bounded onion / Ghost All view of the existing common query.document.object operation.
use super::{exact, frame, revision};
use serde_json::Value;

pub(super) fn ghost_view(value: &Value) -> bool {
    value["view"]["kind"] == "ghosts"
}
fn number(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
}
fn percent(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_f64)
        .is_some_and(|v| (0. ..=100.).contains(&v))
}
fn tint(value: &Value) -> bool {
    value.as_str().is_some_and(|text| {
        text.len() == 7 && text.starts_with('#') && text[1..].bytes().all(|b| b.is_ascii_hexdigit())
    })
}
fn shift(value: &Value) -> bool {
    exact(value, &["x", "y", "next"], &[])
        .is_some_and(|s| number(&s["x"]) && number(&s["y"]) && s["next"].is_boolean())
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
            "layerUid",
            "onion",
            "ghostAll",
            "from",
            "to",
            "previousOpacity",
            "nextOpacity",
            "mode",
            "ghost",
            "cursor",
        ],
        &["previousTint", "nextTint", "shift"],
    ) else {
        return false;
    };
    revision(payload.get("atRevision")).is_some()
        && payload["documentSnapshotId"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
        && view["kind"] == "ghosts"
        && view["contextId"] == "scene-root"
        && frame(view.get("frame")).is_some()
        && frame(view.get("from")).is_some()
        && frame(view.get("to")).is_some()
        && view["workId"]
            .as_str()
            .is_some_and(crate::contract::bounded_identifier)
        && revision(view.get("viewGeneration")).is_some_and(|generation| generation > 0)
        && view["layerUid"].as_str().is_some_and(|uid| !uid.is_empty())
        && view["onion"].is_boolean()
        && view["ghostAll"].is_boolean()
        && percent(view.get("previousOpacity"))
        && percent(view.get("nextOpacity"))
        && matches!(
            view["mode"].as_str(),
            Some("default" | "tinted" | "outline")
        )
        && view["ghost"].as_u64().is_some_and(|ghost| ghost < 4096)
        && view["cursor"]
            .as_u64()
            .is_some_and(|cursor| cursor <= 262_144)
        && view.get("previousTint").is_none_or(tint)
        && view.get("nextTint").is_none_or(tint)
        && view.get("shift").is_none_or(shift)
}
fn segment(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|s| (s.len() == 2 || s.len() == 6) && s.iter().all(number))
}
fn paint(value: &Value) -> bool {
    value.is_null()
        || value.as_str().is_some_and(|text| {
            text.len() == 9
                && text.starts_with('#')
                && text[1..].bytes().all(|b| b.is_ascii_hexdigit())
        })
}
fn item(value: &Value) -> bool {
    exact(
        value,
        &[
            "path", "from", "count", "sourceId", "closed", "width", "fill", "stroke", "opacity",
            "segments",
        ],
        &[],
    )
    .is_some_and(|item| {
        let (from, count) = (item["from"].as_u64(), item["count"].as_u64());
        let len = item["segments"].as_array().map(|s| s.len() as u64);
        item["path"].as_u64().is_some_and(|path| path < 262_144)
            && matches!((from, count, len), (Some(from), Some(count), Some(len))
                if len >= 1 && from + len <= count && count <= 262_144)
            && item["sourceId"].as_str().is_some()
            && item["closed"].is_boolean()
            && number(&item["width"])
            && paint(&item["fill"])
            && paint(&item["stroke"])
            && item["opacity"]
                .as_f64()
                .is_some_and(|v| (0. ..=1.).contains(&v))
            && item["segments"]
                .as_array()
                .is_some_and(|s| s.iter().all(segment))
    })
}
fn descriptor(value: &Value, count: u64) -> bool {
    exact(
        value,
        &["index", "count", "frame", "kind", "opacity", "translation"],
        &[],
    )
    .is_some_and(|d| {
        d["count"].as_u64() == Some(count)
            && d["index"].as_u64().is_some_and(|i| i < count)
            && frame(d.get("frame")).is_some()
            && matches!(d["kind"].as_str(), Some("previous" | "next" | "allKeys"))
            && d["opacity"]
                .as_f64()
                .is_some_and(|v| (0. ..=1.).contains(&v))
            && d["translation"]
                .as_array()
                .is_some_and(|t| t.len() == 2 && t.iter().all(number))
    })
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
            "layerUid",
            "ghost",
            "cursor",
            "total",
            "next",
            "items",
        ],
        &[],
    ) else {
        return false;
    };
    let view = &payload["view"];
    let (Some(cursor), Some(ghost), Some(total), Some(items)) = (
        view["cursor"].as_u64(),
        view["ghost"].as_u64(),
        answer["total"].as_u64(),
        answer["items"].as_array(),
    ) else {
        return false;
    };
    let covered: u64 = items
        .iter()
        .filter_map(|i| i["segments"].as_array().map(|s| s.len() as u64))
        .sum();
    let end = cursor + covered;
    let count = answer["ghost"]["count"].as_u64().unwrap_or(0);
    let expected_next = if answer["ghost"].is_null() {
        Value::Null
    } else if end < total {
        serde_json::json!({"ghost":ghost,"cursor":end})
    } else if ghost + 1 < count {
        serde_json::json!({"ghost":ghost + 1,"cursor":0})
    } else {
        Value::Null
    };
    request(payload)
        && answer["kind"] == "ghosts"
        && answer["atRevision"] == payload["atRevision"]
        && answer["documentSnapshotId"] == payload["documentSnapshotId"]
        && answer["documentSnapshotId"].as_str()
            == Some(format!("native-object:{document_id}:{}", payload["atRevision"]).as_str())
        && [
            "contextId",
            "frame",
            "workId",
            "viewGeneration",
            "layerUid",
            "cursor",
        ]
        .iter()
        .all(|key| answer[*key] == view[*key])
        && total <= 262_144
        && end <= total
        && answer["next"] == expected_next
        && if answer["ghost"].is_null() {
            ghost == 0 && cursor == 0 && total == 0 && items.is_empty()
        } else {
            descriptor(&answer["ghost"], count) && answer["ghost"]["index"].as_u64() == Some(ghost)
        }
        && items.iter().all(item)
}
