//! PRIVATE coherent font resource contract; no source I/O or owner.
use super::{bounded_identifier, exact, revision};
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::Value;
pub(crate) const IMPORT: &str = "command.document.object.asset.font.import";
pub(crate) const QUERY: &str = "query.document.object.asset.font";
fn opaque(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|v| !v.trim().is_empty() && v.len() <= 255 && !v.contains('\0'))
}
fn identifier(value: &Value) -> bool {
    value.as_str().is_some_and(bounded_identifier)
}
fn digest(value: &Value) -> bool {
    value.as_str().is_some_and(|s| {
        s.len() == 64
            && s.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    })
}
fn count(value: &Value) -> bool {
    value
        .as_u64()
        .is_some_and(|n| (1..=16 * 1024 * 1024).contains(&n))
}
fn source(value: &Value) -> bool {
    match value["kind"].as_str() {
        Some("bundled") => {
            exact(value, &["kind", "fontKey"], &[]).is_some()
                && value["fontKey"].as_str().is_some_and(|s| {
                    [
                        "Roboto-Regular",
                        "Roboto-Bold",
                        "DejaVuSans-Regular",
                        "DejaVuSans-Bold",
                        "DejaVuSerif-Regular",
                        "DejaVuSerif-Bold",
                        "Manrope-Regular",
                        "Inter-Regular",
                    ]
                    .contains(&s)
                })
        }
        Some("google") => {
            exact(value, &["kind", "family", "weight", "italic"], &[]).is_some()
                && opaque(&value["family"])
                && value["weight"]
                    .as_u64()
                    .is_some_and(|v| (1..=1000).contains(&v))
                && value["italic"].is_boolean()
        }
        _ => false,
    }
}
pub(crate) fn request(operation: &str, payload: &Value) -> bool {
    if operation == IMPORT {
        return exact(
            payload,
            &[
                "command",
                "documentSnapshotId",
                "lifecycleGeneration",
                "source",
            ],
            &[],
        )
        .is_some()
            && payload["command"] == "object.asset.font.import"
            && identifier(&payload["documentSnapshotId"])
            && revision(payload.get("lifecycleGeneration")).is_some_and(|v| v > 0)
            && source(&payload["source"]);
    }
    if operation != QUERY {
        return false;
    }
    let fields: &[&str] = match payload["action"].as_str() {
        Some("list") => &[
            "action",
            "atRevision",
            "documentSnapshotId",
            "offset",
            "limit",
        ],
        Some("inspect") => &["action", "atRevision", "documentSnapshotId", "assetId"],
        Some("read") => &[
            "action",
            "atRevision",
            "documentSnapshotId",
            "assetId",
            "offset",
            "length",
        ],
        Some("receipt") => {
            return exact(payload, &["action", "forRequestId"], &[]).is_some()
                && identifier(&payload["forRequestId"])
        }
        _ => return false,
    };
    exact(payload, fields, &[]).is_some()
        && revision(payload.get("atRevision")).is_some()
        && identifier(&payload["documentSnapshotId"])
        && match payload["action"].as_str().unwrap() {
            "list" => {
                payload["limit"] == 1 && payload["offset"].as_u64().is_some_and(|v| v <= 1024)
            }
            "inspect" => opaque(&payload["assetId"]),
            _ => {
                opaque(&payload["assetId"])
                    && payload["offset"]
                        .as_u64()
                        .is_some_and(|v| v < 16 * 1024 * 1024)
                    && payload["length"]
                        .as_u64()
                        .is_some_and(|v| (1..=1536).contains(&v))
            }
        }
}
fn row(value: &Value) -> bool {
    if !opaque(&value["assetId"]) || !opaque(&value["name"]) {
        return false;
    }
    match value["sourceKind"].as_str() {
        Some("linked") => {
            exact(
                value,
                &["assetId", "name", "sourceKind", "available", "reason"],
                &[],
            )
            .is_some()
                && value["available"] == false
                && value["reason"] == "linked_font_unavailable"
        }
        Some("embedded") => {
            exact(
                value,
                &[
                    "assetId",
                    "name",
                    "sourceKind",
                    "available",
                    "mediaType",
                    "encodedBytes",
                    "encodedSha256",
                ],
                &[],
            )
            .is_some()
                && value["available"] == true
                && value["mediaType"] == "font/ttf"
                && count(&value["encodedBytes"])
                && digest(&value["encodedSha256"])
        }
        _ => false,
    }
}
fn base64_length(value: &Value) -> Option<usize> {
    let s = value.as_str()?.as_bytes();
    if s.is_empty() || s.len() > 2048 || s.len() % 4 != 0 {
        return None;
    }
    let padding = s.iter().rev().take_while(|b| **b == b'=').count();
    if padding > 2 {
        return None;
    }
    let alphabet = |b: u8| match b {
        b'A'..=b'Z' => Some(b - b'A'),
        b'a'..=b'z' => Some(b - b'a' + 26),
        b'0'..=b'9' => Some(b - b'0' + 52),
        b'+' => Some(62),
        b'/' => Some(63),
        _ => None,
    };
    if !s[..s.len() - padding]
        .iter()
        .all(|b| alphabet(*b).is_some())
    {
        return None;
    }
    let last = alphabet(s[s.len() - padding - 1])?;
    if padding == 1 && last & 3 != 0 || padding == 2 && last & 15 != 0 {
        return None;
    }
    Some(s.len() / 4 * 3 - padding)
}
pub(crate) fn result(operation: &str, payload: &Value, result: &Value) -> bool {
    if !request(operation, payload) {
        return false;
    }
    if operation == IMPORT {
        return exact(
            result,
            &[
                "applied",
                "historyEntriesAdded",
                "assetId",
                "mediaType",
                "encodedBytes",
                "encodedSha256",
            ],
            &[],
        )
        .is_some()
            && result["applied"] == true
            && result["historyEntriesAdded"] == 1
            && opaque(&result["assetId"])
            && result["mediaType"] == "font/ttf"
            && count(&result["encodedBytes"])
            && digest(&result["encodedSha256"]);
    }
    if payload["action"] == "receipt" {
        if result["forRequestId"] != payload["forRequestId"] {
            return false;
        }
        return match result["status"].as_str() {
            Some("not_found") => exact(result, &["status", "forRequestId"], &[]).is_some(),
            Some("committed") => {
                exact(
                    result,
                    &[
                        "status",
                        "forRequestId",
                        "requestFingerprintSha256",
                        "contentRevision",
                        "assetId",
                        "mediaType",
                        "encodedBytes",
                        "encodedSha256",
                    ],
                    &[],
                )
                .is_some()
                    && digest(&result["requestFingerprintSha256"])
                    && revision(result.get("contentRevision")).is_some()
                    && opaque(&result["assetId"])
                    && result["mediaType"] == "font/ttf"
                    && count(&result["encodedBytes"])
                    && digest(&result["encodedSha256"])
            }
            Some("refused") => {
                exact(
                    result,
                    &[
                        "status",
                        "forRequestId",
                        "requestFingerprintSha256",
                        "contentRevision",
                        "code",
                    ],
                    &[],
                )
                .is_some()
                    && digest(&result["requestFingerprintSha256"])
                    && revision(result.get("contentRevision")).is_some()
                    && result["code"].as_str().is_some_and(|v| {
                        [
                            "invalid_request",
                            "wrong_instance",
                            "wrong_document",
                            "stale_revision",
                            "busy_conflict",
                            "unavailable",
                            "not_found",
                            "cancelled_before_dispatch",
                            "internal",
                        ]
                        .contains(&v)
                    })
            }
            _ => false,
        };
    }
    if result["atRevision"] != payload["atRevision"]
        || result["documentSnapshotId"] != payload["documentSnapshotId"]
    {
        return false;
    }
    match payload["action"].as_str().unwrap() {
        "inspect" => {
            exact(result, &["atRevision", "documentSnapshotId", "font"], &[]).is_some()
                && row(&result["font"])
                && result["font"]["assetId"] == payload["assetId"]
        }
        "list" => {
            let Some(total) = result["totalFontRecords"].as_u64().filter(|v| *v <= 1024) else {
                return false;
            };
            let offset = payload["offset"].as_u64().unwrap();
            let Some(rows) = result["rows"].as_array() else {
                return false;
            };
            let next = offset + rows.len() as u64;
            exact(
                result,
                &[
                    "atRevision",
                    "documentSnapshotId",
                    "offset",
                    "nextOffset",
                    "totalFontRecords",
                    "rows",
                ],
                &[],
            )
            .is_some()
                && result["offset"] == offset
                && offset <= total
                && rows.len() == usize::from(offset < total)
                && rows.iter().all(row)
                && if next < total {
                    result["nextOffset"] == next
                } else {
                    result["nextOffset"].is_null()
                }
        }
        _ => {
            let Some(count) = result["returnedLength"]
                .as_u64()
                .filter(|v| *v > 0 && *v <= payload["length"].as_u64().unwrap())
            else {
                return false;
            };
            let Some(total) = result["totalLength"]
                .as_u64()
                .filter(|v| (1..=16 * 1024 * 1024).contains(v))
            else {
                return false;
            };
            exact(
                result,
                &[
                    "atRevision",
                    "documentSnapshotId",
                    "assetId",
                    "offset",
                    "returnedLength",
                    "totalLength",
                    "encodedSha256",
                    "base64",
                ],
                &[],
            )
            .is_some()
                && result["assetId"] == payload["assetId"]
                && result["offset"] == payload["offset"]
                && digest(&result["encodedSha256"])
                && payload["offset"]
                    .as_u64()
                    .unwrap()
                    .checked_add(count)
                    .is_some_and(|end| end <= total)
                && base64_length(&result["base64"]) == Some(count as usize)
        }
    }
}
pub(crate) fn response_matches(
    request: &NativeApplicationRequest,
    response: &NativeApplicationResponse,
) -> bool {
    if ![IMPORT, QUERY].contains(&request.operation.as_str()) {
        return true;
    }
    if response.instance_id != request.instance_id || response.document_id != request.document_id {
        return false;
    }
    let Some(value) = &response.result else {
        return false;
    };
    if request.operation == IMPORT {
        return request.expected_revision.and_then(|r| r.checked_add(1))
            == Some(response.content_revision);
    }
    if request.payload["action"] == "receipt" {
        return value
            .get("contentRevision")
            .is_none_or(|r| r.as_u64().is_some_and(|r| r <= response.content_revision));
    }
    request.payload["atRevision"].as_u64() == Some(response.content_revision)
}

#[cfg(test)]
#[path = "native_font_contract_tests.rs"]
mod tests;
