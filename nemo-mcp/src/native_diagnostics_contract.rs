//! Existing diagnostics result validation, extracted without semantic changes.
use crate::contract::bounded_identifier;
use crate::native_contract::{exact, revision};
use serde_json::Value;

pub(super) fn diagnostics_result(value: &Value) -> bool {
    let Some(result) = exact(value, &["records", "truncated"], &[]) else {
        return false;
    };
    let Some(records) = result["records"].as_array() else {
        return false;
    };
    let mut prior = 0;
    result["truncated"].is_boolean()
        && (records.first().is_none_or(|record| record["sequence"] == 1)
            || result["truncated"] == true)
        && records.len() <= 32
        && records.iter().all(|record| {
            let Some(record) = exact(
                record,
                &["sequence", "operation", "contentRevision", "ok"],
                &["requestId", "requestIdRedacted", "targetId", "errorCode"],
            ) else {
                return false;
            };
            let Some(sequence) = revision(record.get("sequence"))
                .filter(|sequence| *sequence > 0 && (prior == 0 || *sequence == prior + 1))
            else {
                return false;
            };
            prior = sequence;
            let disposition = match record.get("ok").and_then(Value::as_bool) {
                Some(true) => !record.contains_key("errorCode"),
                Some(false) => {
                    record
                        .get("errorCode")
                        .and_then(Value::as_str)
                        .is_some_and(|code| {
                            matches!(
                                code,
                                "invalid_request"
                                    | "wrong_instance"
                                    | "wrong_document"
                                    | "stale_revision"
                                    | "busy_conflict"
                                    | "unavailable"
                                    | "not_found"
                                    | "cancelled_before_dispatch"
                                    | "internal"
                            )
                        })
                }
                None => false,
            };
            disposition
                && revision(record.get("contentRevision")).is_some()
                && match (record.get("requestId"), record.get("requestIdRedacted")) {
                    (Some(id), None) => id
                        .as_str()
                        .is_some_and(|id| bounded_identifier(id) && !id.contains([':', '/'])),
                    (None, Some(redacted)) => redacted == true,
                    _ => false,
                }
                && record.get("targetId").is_none_or(|value| {
                    value
                        .as_str()
                        .is_some_and(|id| bounded_identifier(id) && !id.contains([':', '/']))
                })
                && record
                    .get("operation")
                    .and_then(Value::as_str)
                    .is_some_and(|operation| {
                        matches!(
                            operation,
                            "command.document.apply"
                                | "transaction.begin"
                                | "transaction.update"
                                | "transaction.commit"
                                | "transaction.cancel"
                                | "history.undo"
                                | "history.redo"
                        )
                    })
        })
}
