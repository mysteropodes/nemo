//! Staged typed-Value object read contract. Raw duplicate keys require a later
//! raw-envelope gate before this capability can become available.
use super::{exact, frame, revision};
use serde_json::Value;

fn target(value: &Value) -> bool {
    exact(
        value,
        &["contextId", "frameScope", "layerUid", "strokeId"],
        &[],
    )
    .is_some_and(|value| {
        value.get("contextId").and_then(Value::as_str) == Some("scene-root")
            && ["layerUid", "strokeId"].iter().all(|key| {
                value
                    .get(*key)
                    .and_then(Value::as_str)
                    .is_some_and(|id| !id.is_empty())
            })
            && value.get("frameScope").is_some_and(|scope| {
                exact(scope, &["kind", "frame"], &[]).is_some_and(|scope| {
                    matches!(
                        scope.get("kind").and_then(Value::as_str),
                        Some("authored" | "reference")
                    ) && frame(scope.get("frame")).is_some()
                })
            })
    })
}

pub(super) fn request(value: &Value) -> bool {
    exact(value, &["atRevision", "stableTarget"], &[]).is_some_and(|value| {
        revision(value.get("atRevision")).is_some() && value.get("stableTarget").is_some_and(target)
    })
}

fn point(value: &Value) -> bool {
    exact(value, &["x", "y"], &[]).is_some_and(|value| {
        ["x", "y"].iter().all(|key| {
            value
                .get(*key)
                .and_then(Value::as_f64)
                .is_some_and(f64::is_finite)
        })
    })
}

fn geometry(value: &Value) -> bool {
    exact(
        value,
        &[
            "kind",
            "closed",
            "coordinateSpace",
            "handleSpace",
            "segments",
        ],
        &[],
    )
    .is_some_and(|value| {
        value.get("kind").and_then(Value::as_str) == Some("cubic-path")
            && value.get("closed").and_then(Value::as_bool) == Some(true)
            && value.get("coordinateSpace").and_then(Value::as_str) == Some("document-world")
            && value.get("handleSpace").and_then(Value::as_str) == Some("relative")
            && value
                .get("segments")
                .and_then(Value::as_array)
                .is_some_and(|segments| {
                    (2..=256).contains(&segments.len())
                        && segments.iter().all(|segment| {
                            exact(segment, &["point", "handleIn", "handleOut"], &[]).is_some_and(
                                |segment| {
                                    ["point", "handleIn", "handleOut"]
                                        .iter()
                                        .all(|key| segment.get(*key).is_some_and(point))
                                },
                            )
                        })
                })
    })
}

fn fill(value: &Value) -> bool {
    exact(value, &["kind", "r", "g", "b", "a"], &[]).is_some_and(|value| {
        value.get("kind").and_then(Value::as_str) == Some("solid")
            && ["r", "g", "b", "a"].iter().all(|key| {
                value
                    .get(*key)
                    .and_then(Value::as_f64)
                    .is_some_and(|channel| channel.is_finite() && (0.0..=1.0).contains(&channel))
            })
    })
}

fn record(value: &Value) -> bool {
    exact(
        value,
        &["schemaVersion", "family", "target", "geometry", "fill"],
        &[],
    )
    .is_some_and(|value| {
        value.get("schemaVersion").and_then(Value::as_u64) == Some(1)
            && value.get("family").and_then(Value::as_str) == Some("closed-solid-fill-cubic-path")
            && value.get("target").is_some_and(target)
            && value.get("geometry").is_some_and(geometry)
            && value.get("fill").is_some_and(fill)
    })
}

pub(super) fn result(document_id: &str, payload: &Value, result: &Value) -> bool {
    request(payload)
        && exact(result, &["atRevision", "documentSnapshotId", "object"], &[]).is_some_and(
            |value| {
                let selected = revision(payload.get("atRevision"));
                revision(value.get("atRevision")) == selected
                    && value.get("documentSnapshotId").and_then(Value::as_str)
                        == Some(
                            format!("native-object:{document_id}:{}", selected.unwrap()).as_str(),
                        )
                    && value.get("object").is_some_and(|object| {
                        record(object) && object.get("target") == payload.get("stableTarget")
                    })
            },
        )
}

#[cfg(test)]
#[path = "native_object_contract_tests.rs"]
mod tests;
