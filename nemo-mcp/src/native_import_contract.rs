//! L10-J closed public contract of `command.document.object.import.batch` (payload command
//! `object.import.batch`, the provisional name of the accepted L10-H transaction). Pure shape and
//! receipt validation: execution, id allocation, storage and history stay in the native owner. The
//! table is mirrored by `src/js/adapters/native-import-contract.js`.
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use crate::native_contract::exact;
use serde_json::Value;
use std::collections::HashSet;

pub(crate) const OP: &str = "command.document.object.import.batch";
const COMMAND: &str = "object.import.batch";
/// Largest serialized `result` the dispatcher can echo inside its 4096-byte response (L10-H).
const MAX_RECEIPT_BYTES: usize = 3072;
/// `valid_id` of the owner id table: non-blank, at most 255 bytes, no NUL.
const MAX_ID_BYTES: usize = 255;
/// Anchor bounds of the vector family (`object_vector_codec.rs`).
const MIN_ANCHORS: usize = 2;
const MAX_ANCHORS: usize = 256;
const SKIP_KINDS: [&str; 3] = ["layer", "element", "asset"];
const PROVIDERS: [&str; 3] = ["raster-asset", "text", "font"];
const CAPS: [&str; 3] = ["butt", "round", "square"];
const JOINS: [&str; 3] = ["miter", "round", "bevel"];

pub(crate) fn admitted(operation: &str) -> bool {
    operation == OP
}

fn source_id(value: Option<&Value>) -> Option<&str> {
    value?
        .as_str()
        .filter(|id| !id.trim().is_empty() && id.len() <= MAX_ID_BYTES && !id.contains('\0'))
}

fn non_empty(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|text| !text.is_empty())
}

fn finite(value: Option<&Value>) -> bool {
    value.and_then(Value::as_f64).is_some_and(f64::is_finite)
}

fn point(value: Option<&Value>) -> bool {
    value.is_some_and(|p| {
        exact(p, &["x", "y"], &[]).is_some_and(|o| finite(o.get("x")) && finite(o.get("y")))
    })
}

fn solid_or_null(value: Option<&Value>) -> bool {
    match value {
        Some(Value::Null) => true,
        Some(fill) => exact(fill, &["kind", "r", "g", "b", "a"], &[]).is_some_and(|o| {
            o.get("kind").and_then(Value::as_str) == Some("solid")
                && ["r", "g", "b", "a"].iter().all(|key| {
                    o.get(*key)
                        .and_then(Value::as_f64)
                        .is_some_and(|channel| (0.0..=1.0).contains(&channel))
                })
        }),
        None => false,
    }
}

fn target(value: Option<&Value>) -> bool {
    value.is_some_and(|t| {
        exact(t, &["contextId", "frameScope", "layerUid", "strokeId"], &[]).is_some_and(|o| {
            non_empty(o.get("contextId"))
                && non_empty(o.get("layerUid"))
                && o.get("strokeId").and_then(Value::as_str) == Some("")
                && o.get("frameScope").is_some_and(|scope| {
                    exact(scope, &["kind", "frame"], &[]).is_some_and(|s| {
                        s.get("kind").and_then(Value::as_str) == Some("authored")
                            && s.get("frame")
                                .and_then(Value::as_u64)
                                .is_some_and(|frame| frame <= u64::from(u32::MAX))
                    })
                })
        })
    })
}

fn geometry(value: Option<&Value>) -> bool {
    value.is_some_and(|g| {
        exact(
            g,
            &[
                "kind",
                "closed",
                "coordinateSpace",
                "handleSpace",
                "segments",
            ],
            &[],
        )
        .is_some_and(|o| {
            o.get("kind").and_then(Value::as_str) == Some("cubic-path")
                && o.get("closed").is_some_and(Value::is_boolean)
                && o.get("coordinateSpace").and_then(Value::as_str) == Some("document-world")
                && o.get("handleSpace").and_then(Value::as_str) == Some("relative")
                && o.get("segments")
                    .and_then(Value::as_array)
                    .is_some_and(|segments| {
                        (MIN_ANCHORS..=MAX_ANCHORS).contains(&segments.len())
                            && segments.iter().all(|segment| {
                                exact(segment, &["point", "handleIn", "handleOut"], &[])
                                    .is_some_and(|s| {
                                        point(s.get("point"))
                                            && point(s.get("handleIn"))
                                            && point(s.get("handleOut"))
                                    })
                            })
                    })
        })
    })
}

fn vector_body(value: Option<&Value>) -> bool {
    value.is_some_and(|v| {
        exact(
            v,
            &[
                "stroke",
                "hasRealStroke",
                "strokeWidth",
                "strokeCap",
                "strokeJoin",
                "opacity",
            ],
            &["miterLimit", "dashArray", "dashOffset"],
        )
        .is_some_and(|o| {
            solid_or_null(o.get("stroke"))
                && o.get("hasRealStroke").and_then(Value::as_bool)
                    == Some(!o.get("stroke").is_some_and(Value::is_null))
                && finite(o.get("strokeWidth"))
                && finite(o.get("opacity"))
                && o.get("strokeCap")
                    .and_then(Value::as_str)
                    .is_some_and(|cap| CAPS.contains(&cap))
                && o.get("strokeJoin")
                    .and_then(Value::as_str)
                    .is_some_and(|join| JOINS.contains(&join))
                && o.get("miterLimit").is_none_or(|n| finite(Some(n)))
                && o.get("dashOffset").is_none_or(|n| finite(Some(n)))
                && o.get("dashArray").is_none_or(|a| {
                    a.as_array()
                        .is_some_and(|d| d.iter().all(|n| finite(Some(n))))
                })
        })
    })
}

/// Structural pre-check of the one record family the importers create; the owner re-validates it.
fn vector_record(value: Option<&Value>) -> bool {
    value.is_some_and(|r| {
        exact(
            r,
            &["schemaVersion", "family", "target", "geometry"],
            &["fill", "vector"],
        )
        .is_some_and(|o| {
            o.get("schemaVersion").and_then(Value::as_u64) == Some(1)
                && o.get("family").and_then(Value::as_str) == Some("editable-vector-path")
                && target(o.get("target"))
                && geometry(o.get("geometry"))
                && o.get("fill").is_none_or(|f| solid_or_null(Some(f)))
                && o.get("vector").is_none_or(|v| vector_body(Some(v)))
        })
    })
}

fn rows(value: Option<&Value>, ok: impl Fn(&Value) -> bool) -> Option<&Vec<Value>> {
    value?
        .as_array()
        .filter(|items| items.iter().all(|item| ok(item)))
}

fn layer_row(row: &Value) -> bool {
    exact(row, &["sourceId", "name"], &[])
        .is_some_and(|o| source_id(o.get("sourceId")).is_some() && non_empty(o.get("name")))
}

fn record_row(row: &Value) -> bool {
    exact(row, &["sourceId", "record"], &[])
        .is_some_and(|o| source_id(o.get("sourceId")).is_some() && vector_record(o.get("record")))
}

fn skip_row(row: &Value) -> bool {
    exact(row, &["kind", "sourceId", "reason"], &[]).is_some_and(|o| {
        o.get("kind")
            .and_then(Value::as_str)
            .is_some_and(|kind| SKIP_KINDS.contains(&kind))
            && source_id(o.get("sourceId")).is_some()
            && non_empty(o.get("reason"))
    })
}

fn unavailable_row(row: &Value) -> bool {
    exact(row, &["sourceId", "need"], &[]).is_some_and(|o| {
        source_id(o.get("sourceId")).is_some()
            && o.get("need")
                .and_then(Value::as_str)
                .is_some_and(|need| PROVIDERS.contains(&need))
    })
}

/// Ids of the entries that share one id space are unique.
fn distinct<'a>(mut ids: impl Iterator<Item = Option<&'a str>>) -> bool {
    let mut seen = HashSet::new();
    ids.all(|id| id.is_some_and(|id| seen.insert(id)))
}

fn source_ids<'a>(items: &'a [Value]) -> impl Iterator<Item = Option<&'a str>> + 'a {
    items
        .iter()
        .map(|row| row.get("sourceId").and_then(Value::as_str))
}

fn skip_ids<'a>(
    skipped: &'a [Value],
    kind: &'static str,
) -> impl Iterator<Item = Option<&'a str>> + 'a {
    skipped
        .iter()
        .filter(move |row| row.get("kind").and_then(Value::as_str) == Some(kind))
        .map(|row| row.get("sourceId").and_then(Value::as_str))
}

pub(crate) fn request(payload: &Value) -> bool {
    let Some(o) = exact(
        payload,
        &["command", "layers", "records"],
        &["skipped", "unavailable"],
    ) else {
        return false;
    };
    let (Some(layers), Some(records)) = (
        rows(o.get("layers"), layer_row),
        rows(o.get("records"), record_row),
    ) else {
        return false;
    };
    let empty = Vec::new();
    let Some(skipped) = (match o.get("skipped") {
        None => Some(&empty),
        some => rows(some, skip_row),
    }) else {
        return false;
    };
    o.get("command").and_then(Value::as_str) == Some(COMMAND)
        && (o.get("unavailable").is_none() || rows(o.get("unavailable"), unavailable_row).is_some())
        && distinct(source_ids(layers))
        && distinct(source_ids(records).chain(skip_ids(skipped, "element")))
        && distinct(skip_ids(skipped, "layer"))
        && distinct(skip_ids(skipped, "asset"))
}

pub(crate) fn result(value: &Value) -> bool {
    exact(
        value,
        &[
            "applied",
            "historyEntriesAdded",
            "layers",
            "objects",
            "skipped",
        ],
        &[],
    )
    .is_some_and(|o| {
        let applied = o.get("applied").and_then(Value::as_bool);
        applied.is_some()
            && o.get("historyEntriesAdded").and_then(Value::as_u64)
                == Some(u64::from(applied == Some(true)))
            && rows(o.get("layers"), |row| {
                exact(row, &["sourceId", "layerUid"], &[]).is_some_and(|r| {
                    source_id(r.get("sourceId")).is_some() && non_empty(r.get("layerUid"))
                })
            })
            .is_some()
            && rows(o.get("objects"), |row| {
                exact(row, &["sourceId", "strokeId"], &[]).is_some_and(|r| {
                    source_id(r.get("sourceId")).is_some() && non_empty(r.get("strokeId"))
                })
            })
            .is_some()
            && rows(o.get("skipped"), skip_row).is_some()
            && serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= MAX_RECEIPT_BYTES)
    })
}

// Native planning maps every declared row or refuses the entire transaction.
fn mapped_rows_match(input: &[Value], mapped: &[Value], native_id: &str) -> bool {
    input.len() == mapped.len()
        && distinct(source_ids(mapped))
        && distinct(
            mapped
                .iter()
                .map(|row| row.get(native_id).and_then(Value::as_str)),
        )
        && input.iter().all(|row| {
            mapped
                .iter()
                .any(|item| item["sourceId"] == row["sourceId"])
        })
}
fn receipt_matches(payload: &Value, value: &Value) -> bool {
    if !request(payload) || !result(value) {
        return false;
    }
    let layers = payload["layers"].as_array().unwrap();
    let records = payload["records"].as_array().unwrap();
    let empty = Vec::new();
    let skipped = payload
        .get("skipped")
        .and_then(Value::as_array)
        .unwrap_or(&empty);
    payload
        .get("unavailable")
        .is_none_or(|v| v.as_array().is_some_and(|rows| rows.is_empty()))
        && distinct(source_ids(layers).chain(skip_ids(skipped, "layer")))
        && value["applied"].as_bool() == Some(!layers.is_empty() || !records.is_empty())
        && mapped_rows_match(layers, value["layers"].as_array().unwrap(), "layerUid")
        && mapped_rows_match(records, value["objects"].as_array().unwrap(), "strokeId")
        && value["skipped"].as_array() == Some(skipped)
}

/// A changing batch advances the revision by one, a batch that creates nothing by none.
pub(crate) fn revision_matches(expected: Option<u64>, revision: u64, value: &Value) -> bool {
    result(value)
        && expected.and_then(|before| before.checked_add(u64::from(value["applied"] == true)))
            == Some(revision)
}

pub(crate) fn response_matches(
    request: &NativeApplicationRequest,
    response: &NativeApplicationResponse,
) -> bool {
    if !admitted(&request.operation) {
        return true;
    }
    response.instance_id == request.instance_id
        && response.document_id == request.document_id
        && response.result.as_ref().is_some_and(|value| {
            receipt_matches(&request.payload, value)
                && revision_matches(request.expected_revision, response.content_revision, value)
        })
}

#[cfg(test)]
#[path = "native_import_contract_tests.rs"]
mod tests;
