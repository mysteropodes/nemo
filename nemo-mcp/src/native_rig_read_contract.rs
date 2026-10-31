//! #2255: closed, bounded rig presentation view of the existing common object query.
use super::{exact, frame, revision};
use serde_json::Value;

/// The part a request names and the most items one page of it may carry (the engine's table, restated).
const PARTS: [(&str, u64); 6] = [
    ("bones", 8),
    ("vertices", 16),
    ("chains", 8),
    ("binds", 8),
    ("bound", 64),
    ("weights", 8),
];
const REASONS: [&str; 4] = ["expression", "sample", "parent", "motion"];
const STATES: [&str; 4] = ["posed", "skipped", "unlinked", "unavailable"];
const MAX_LIST: u64 = 100_000;

pub(super) fn rig_view(value: &Value) -> bool {
    value["view"]["kind"] == "rig"
}
fn index(value: &Value) -> Option<u64> {
    value.as_u64().filter(|n| *n <= u64::from(u32::MAX))
}
fn text(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|s| !s.is_empty() && s.len() <= 128)
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
            "part",
            "bind",
            "vertex",
            "cursor",
            "limit",
        ],
        &[],
    ) else {
        return false;
    };
    let cap = view["part"]
        .as_str()
        .and_then(|part| PARTS.iter().find(|(name, _)| *name == part))
        .map(|(_, cap)| *cap);
    let (Some(bind), Some(vertex)) = (index(&view["bind"]), index(&view["vertex"])) else {
        return false;
    };
    revision(payload.get("atRevision")).is_some()
        && payload["documentSnapshotId"]
            .as_str()
            .is_some_and(|id| !id.is_empty())
        && view["kind"] == "rig"
        && view["contextId"] == "scene-root"
        && frame(view.get("frame")).is_some()
        && view["workId"]
            .as_str()
            .is_some_and(crate::contract::bounded_identifier)
        && revision(view.get("viewGeneration")).is_some_and(|generation| generation > 0)
        && text(&view["layerUid"])
        && (view["part"] == "weights" || (bind == 0 && vertex == 0))
        && view["cursor"]
            .as_u64()
            .is_some_and(|cursor| cursor <= MAX_LIST)
        && cap.is_some_and(|cap| {
            view["limit"]
                .as_u64()
                .is_some_and(|l| (1..=cap).contains(&l))
        })
}
fn number(value: &Value) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
}
fn pair(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|v| v.len() == 2 && v.iter().all(number))
}
fn nullable(value: &Value, check: fn(&Value) -> bool) -> bool {
    value.is_null() || check(value)
}
fn reference(value: &Value) -> bool {
    exact(value, &["boneId", "vi"], &[])
        .is_some_and(|r| text(&r["boneId"]) && index(&r["vi"]).is_some())
}
fn item(part: &str, value: &Value) -> bool {
    match part {
        "bones" => exact(
            value,
            &["boneId", "closed", "radius", "count", "center"],
            &[],
        )
        .is_some_and(|b| {
            text(&b["boneId"])
                && b["closed"].is_boolean()
                && nullable(&b["radius"], number)
                && b["count"].as_u64().is_some_and(|n| (2..=256).contains(&n))
                && pair(&b["center"])
        }),
        "vertices" => exact(
            value,
            &["boneId", "vi", "point", "handleIn", "handleOut"],
            &[],
        )
        .is_some_and(|v| {
            text(&v["boneId"])
                && index(&v["vi"]).is_some()
                && pair(&v["point"])
                && pair(&v["handleIn"])
                && pair(&v["handleOut"])
        }),
        "chains" => exact(
            value,
            &["endKey", "root", "joint", "end", "l1", "l2", "flip"],
            &[],
        )
        .is_some_and(|c| {
            c["endKey"]
                .as_str()
                .is_some_and(|k| !k.is_empty() && k.len() <= 160)
                && ["root", "joint", "end"]
                    .iter()
                    .all(|key| reference(&c[*key]))
                && number(&c["l1"])
                && number(&c["l2"])
                && c["flip"].is_boolean()
        }),
        "binds" => exact(
            value,
            &["index", "strokeId", "meshId", "points", "rotate", "state"],
            &[],
        )
        .is_some_and(|b| {
            index(&b["index"]).is_some()
                && (b["strokeId"].is_null() != b["meshId"].is_null())
                && nullable(&b["strokeId"], |v| text(v))
                && nullable(&b["meshId"], |v| text(v))
                && index(&b["points"]).is_some()
                && b["rotate"].is_boolean()
                && b["state"].as_str().is_some_and(|s| STATES.contains(&s))
        }),
        "bound" => value.as_array().is_some_and(|v| {
            v.len() == 4
                && index(&v[0]).is_some()
                && index(&v[1]).is_some()
                && number(&v[2])
                && number(&v[3])
        }),
        "weights" => {
            exact(value, &["entry", "boneId", "ptIndex", "offset", "w"], &[]).is_some_and(|w| {
                index(&w["entry"]).is_some()
                    && text(&w["boneId"])
                    && (w["ptIndex"].is_null() != w["offset"].is_null())
                    && nullable(&w["ptIndex"], |v| index(v).is_some())
                    && nullable(&w["offset"], number)
                    && number(&w["w"])
            })
        }
        _ => false,
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
            "layerUid",
            "part",
            "bind",
            "vertex",
            "cursor",
            "total",
            "nextCursor",
            "unavailable",
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
    let (Some(cursor), Some(limit), Some(total), Some(items), Some(part)) = (
        cursor,
        limit,
        answer["total"].as_u64(),
        answer["items"].as_array(),
        payload["view"]["part"].as_str(),
    ) else {
        return false;
    };
    let end = cursor + items.len() as u64;
    let reason = &answer["unavailable"];
    answer["kind"] == "rig"
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
            "part",
            "bind",
            "vertex",
        ]
        .iter()
        .all(|key| answer[*key] == view[*key])
        && answer["cursor"].as_u64() == Some(cursor)
        && items.len() as u64 <= limit
        && end <= total
        && total <= MAX_LIST
        && if end == total {
            answer["nextCursor"].is_null()
        } else {
            answer["nextCursor"].as_u64() == Some(end)
        }
        && (reason.is_null()
            || (reason.as_str().is_some_and(|r| REASONS.contains(&r)) && total == 0))
        && items.iter().all(|value| item(part, value))
        // The request pin is checked last: an item of an unlisted part is refused by `item` itself.
        && request(payload)
}
