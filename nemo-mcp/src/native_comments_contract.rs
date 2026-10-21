//! Original project team comments; ordered identity, raw attribution and pinned reads.
use super::revision;
use crate::contract::bounded_identifier;
use serde_json::{Map, Value};
pub(crate) const READ: &str = "query.document.object.project.comments.read";
pub(crate) const EDIT: &str = "command.document.object.project.comments.edit";
const FIELDS: [&str; 10] = [
    "id",
    "x",
    "y",
    "frame",
    "authorId",
    "authorName",
    "authorColor",
    "text",
    "createdAt",
    "resolved",
];
fn exact<'a>(
    value: &'a Value,
    required: &[&str],
    optional: &[&str],
) -> Option<&'a Map<String, Value>> {
    let object = value.as_object()?;
    (required.iter().all(|key| object.contains_key(*key))
        && object
            .keys()
            .all(|key| required.contains(&key.as_str()) || optional.contains(&key.as_str())))
    .then_some(object)
}
fn ordinal(value: &Value) -> Option<u64> {
    revision(Some(value)).filter(|v| *v > 0)
}
fn comment(value: &Value, append: bool) -> bool {
    let required: &[&str] = if append {
        &["id", "x", "y", "frame", "text", "createdAt", "resolved"]
    } else {
        &[]
    };
    let Some(object) = exact(value, required, &FIELDS) else {
        return false;
    };
    object.iter().all(|(key, value)| match key.as_str() {
        "x" | "y" | "frame" | "createdAt" => value.as_f64().is_some_and(f64::is_finite),
        "resolved" => value.is_boolean(),
        _ => value.is_string(),
    }) && (!append
        || value["text"]
            .as_str()
            .is_some_and(|text| !text.chars().all(js_space)))
}
fn js_space(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' |
        '\u{205f}' | '\u{3000}' | '\u{feff}')
}
fn selector(value: &Value) -> bool {
    match value["kind"].as_str() {
        Some("settings") => exact(value, &["kind"], &[]).is_some(),
        Some("record") => {
            exact(value, &["kind", "ordinal"], &[]).is_some()
                && ordinal(&value["ordinal"]).is_some()
        }
        Some("commentsPage") => {
            exact(value, &["kind", "cursor", "limit"], &[]).is_some()
                && revision(value.get("cursor")).is_some()
                && value["limit"]
                    .as_u64()
                    .is_some_and(|v| (1..=16).contains(&v))
        }
        _ => false,
    }
}
pub(super) fn request(operation: &str, value: &Value) -> bool {
    if operation == EDIT {
        if exact(value, &["command", "action"], &[]).is_none()
            || value["command"] != "object.project.comments.edit"
        {
            return false;
        }
        let action = &value["action"];
        return match action["kind"].as_str() {
            Some("append") => {
                exact(action, &["kind", "comment"], &[]).is_some()
                    && comment(&action["comment"], true)
            }
            Some("update") => {
                exact(action, &["kind", "ordinal", "text", "resolved"], &[]).is_some()
                    && ordinal(&action["ordinal"]).is_some()
                    && action["text"].is_string()
                    && action["resolved"].is_boolean()
            }
            Some("delete") => {
                exact(action, &["kind", "ordinal"], &[]).is_some()
                    && ordinal(&action["ordinal"]).is_some()
            }
            _ => false,
        };
    }
    operation == READ
        && exact(
            value,
            &["atRevision", "documentSnapshotId", "contextId", "selector"],
            &[],
        )
        .is_some()
        && revision(value.get("atRevision")).is_some()
        && value["documentSnapshotId"]
            .as_str()
            .is_some_and(bounded_identifier)
        && value["contextId"] == "scene-root"
        && selector(&value["selector"])
}
fn presence(value: &Value) -> Option<(bool, u64)> {
    let present = value["commentsPresent"].as_bool()?;
    let count = revision(value.get("commentCount"))?;
    (present || count == 0).then_some((present, count))
}
fn record(value: &Value, wanted: &Value) -> bool {
    match wanted["kind"].as_str() {
        Some("settings") => {
            exact(value, &["commentsPresent", "commentCount"], &[]).is_some()
                && presence(value).is_some()
        }
        Some("record") => {
            exact(value, &["kind", "ordinal", "comment"], &[]).is_some()
                && value["kind"] == "projectCommentRecord"
                && value["ordinal"] == wanted["ordinal"]
                && comment(&value["comment"], false)
        }
        Some("commentsPage") => {
            if exact(
                value,
                &[
                    "kind",
                    "commentsPresent",
                    "commentCount",
                    "cursor",
                    "items",
                    "nextCursor",
                ],
                &[],
            )
            .is_none()
                || value["kind"] != "projectCommentsPage"
                || value["cursor"] != wanted["cursor"]
            {
                return false;
            }
            let Some((_, count)) = presence(value) else {
                return false;
            };
            let Some(cursor) = revision(value.get("cursor")) else {
                return false;
            };
            let Some(items) = value["items"].as_array() else {
                return false;
            };
            if cursor > count || items.len() as u64 > wanted["limit"].as_u64().unwrap_or(0) {
                return false;
            }
            if !items.iter().enumerate().all(|(index, item)| {
                exact(item, &["ordinal", "comment"], &[]).is_some()
                    && item["ordinal"].as_u64() == cursor.checked_add(index as u64 + 1)
                    && comment(&item["comment"], false)
            }) {
                return false;
            }
            let Some(end) = cursor.checked_add(items.len() as u64) else {
                return false;
            };
            end <= count
                && if end < count {
                    !items.is_empty() && value["nextCursor"].as_u64() == Some(end)
                } else {
                    value["nextCursor"].is_null()
                }
        }
        _ => false,
    }
}
pub(super) fn result(operation: &str, document: &str, payload: &Value, value: &Value) -> bool {
    if operation == EDIT {
        return super::settings::result(value);
    }
    operation == READ
        && request(operation, payload)
        && exact(
            value,
            &[
                "instanceId",
                "documentId",
                "atRevision",
                "documentSnapshotId",
                "contextId",
                "result",
            ],
            &[],
        )
        .is_some()
        && value["instanceId"].as_str().is_some_and(bounded_identifier)
        && value["documentId"].as_str() == Some(document)
        && ["atRevision", "documentSnapshotId", "contextId"]
            .iter()
            .all(|key| value.get(*key) == payload.get(*key))
        && record(&value["result"], &payload["selector"])
}
#[cfg(test)]
#[path = "native_comments_contract_tests.rs"]
mod tests;
