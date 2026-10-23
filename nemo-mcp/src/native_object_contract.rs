//! Staged typed-Value object read and layer-visibility contracts. Raw duplicate
//! keys require the shared original-byte admission gate before public availability.
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
    if image_mesh::presentation_view(value) {
        return image_mesh::presentation_request(value);
    }
    if image_mesh::image_mesh_view(value) {
        return image_mesh::image_mesh_request(value);
    }
    exact(value, &["atRevision", "stableTarget"], &[]).is_some_and(|value| {
        revision(value.get("atRevision")).is_some() && value.get("stableTarget").is_some_and(target)
    })
}

/// One closed selector parser supplies both admission and response correlation.
/// Carry its validated frame forward instead of reparsing an immutable payload.
fn timing_selector(value: &Value) -> Option<u64> {
    let value = exact(
        value,
        &[
            "atRevision",
            "documentSnapshotId",
            "contextId",
            "layerUid",
            "frame",
        ],
        &[],
    )?;
    let requested = frame(value.get("frame"))?;
    (revision(value.get("atRevision")).is_some()
        && ["documentSnapshotId", "contextId", "layerUid"]
            .iter()
            .all(|key| {
                value
                    .get(*key)
                    .and_then(Value::as_str)
                    .is_some_and(|text| !text.is_empty())
            }))
    .then_some(requested)
}

pub(super) fn timing_request(value: &Value) -> bool {
    if value.get("view").is_some_and(|view| view == "placement") {
        return layer_placement::request(value);
    }
    if value.get("view").is_some() {
        return layer_range::request(value);
    }
    timing_selector(value).is_some()
}

pub(super) fn timing_result(document_id: &str, payload: &Value, result: &Value) -> bool {
    if payload.get("view").is_some_and(|view| view == "placement") {
        return layer_placement::result(document_id, payload, result);
    }
    if payload.get("view").is_some() {
        return layer_range::result(document_id, payload, result);
    }
    let Some(requested) = timing_selector(payload) else {
        return false;
    };
    exact(
        result,
        &[
            "instanceId",
            "documentId",
            "atRevision",
            "documentSnapshotId",
            "contextId",
            "layerUid",
            "frame",
            "cellKind",
            "sourceFrame",
            "hasContent",
            "manualEdit",
        ],
        &[],
    )
    .is_some_and(|value| {
        let source = frame(value.get("sourceFrame"));
        let Some(content) = value.get("hasContent").and_then(Value::as_bool) else {
            return false;
        };
        let Some(manual) = value.get("manualEdit").and_then(Value::as_bool) else {
            return false;
        };
        value
            .get("instanceId")
            .and_then(Value::as_str)
            .is_some_and(crate::contract::bounded_identifier)
            && value.get("documentId").and_then(Value::as_str) == Some(document_id)
            && value.get("contextId").and_then(Value::as_str) == Some("scene-root")
            && [
                "atRevision",
                "documentSnapshotId",
                "contextId",
                "layerUid",
                "frame",
            ]
            .iter()
            .all(|key| value.get(*key) == payload.get(*key))
            && match value.get("cellKind").and_then(Value::as_str) {
                Some("key") => source == Some(requested) && !manual,
                Some("interpolated") => source == Some(requested),
                Some("held") => source.is_some_and(|source| source < requested) && !manual,
                Some("unexposed") => {
                    value.get("sourceFrame").is_some_and(Value::is_null) && !content && !manual
                }
                _ => false,
            }
    })
}

pub(super) fn visibility_request(value: &Value) -> bool {
    exact(value, &["command", "layerUid", "visible"], &[]).is_some_and(|value| {
        value.get("command").and_then(Value::as_str) == Some("object.layer.visibility.set")
            && value
                .get("layerUid")
                .and_then(Value::as_str)
                .is_some_and(|uid| !uid.is_empty())
            && value.get("visible").and_then(Value::as_bool).is_some()
    })
}

pub(super) fn visibility_result(value: &Value) -> bool {
    exact(value, &["applied", "historyEntriesAdded"], &[]).is_some_and(|value| {
        matches!(
            (
                value.get("applied").and_then(Value::as_bool),
                value.get("historyEntriesAdded").and_then(Value::as_u64)
            ),
            (Some(true), Some(1)) | (Some(false), Some(0))
        )
    })
}

pub(super) fn history_request(operation: &str, value: &Value) -> bool {
    let command = match operation {
        "command.document.object.undo" => "object.undo",
        "command.document.object.redo" => "object.redo",
        _ => return false,
    };
    exact(value, &["command"], &[])
        .is_some_and(|value| value.get("command").and_then(Value::as_str) == Some(command))
}

pub(super) fn history_result(value: &Value) -> bool {
    exact(value, &["applied", "historyEntriesAdded"], &[]).is_some_and(|value| {
        value.get("applied").and_then(Value::as_bool) == Some(true)
            && value.get("historyEntriesAdded").and_then(Value::as_u64) == Some(0)
    })
}

pub(super) fn create_request(value: &Value) -> bool {
    exact(value, &["command", "name"], &[]).is_some_and(|value| {
        value.get("command").and_then(Value::as_str) == Some("object.layer.create")
            && value.get("name").is_some_and(Value::is_string)
    })
}

pub(super) fn create_result(value: &Value) -> bool {
    exact(value, &["applied", "historyEntriesAdded", "layerUid"], &[]).is_some_and(|value| {
        value.get("applied").and_then(Value::as_bool) == Some(true)
            && value.get("historyEntriesAdded").and_then(Value::as_u64) == Some(1)
            && value
                .get("layerUid")
                .and_then(Value::as_str)
                .is_some_and(|uid| !uid.is_empty())
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
    if value.get("family").and_then(Value::as_str) == Some(crate::native_contract::vector::FAMILY) {
        return crate::native_contract::vector::record(value);
    }
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
    if image_mesh::presentation_view(payload) {
        return image_mesh::presentation_result(document_id, payload, result);
    }
    if image_mesh::image_mesh_view(payload) {
        return image_mesh::image_mesh_result(document_id, payload, result);
    }
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

#[path = "native_layer_placement_contract.rs"]
mod layer_placement;
#[path = "native_layer_range_contract.rs"]
mod layer_range;

#[path = "native_image_mesh_read_contract.rs"]
mod image_mesh;

#[cfg(test)]
#[path = "native_image_mesh_read_contract_tests.rs"]
mod image_mesh_tests;

#[cfg(test)]
mod presentation_transport_tests {
    use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
    use serde_json::json;

    // Literal transport contracts establish no native surface presentation authority.
    fn request() -> NativeApplicationRequest {
        serde_json::from_value(json!({
            "apiVersion":2,"requestId":"presentation-read","instanceId":"instance",
            "documentId":"document","operation":"query.document.object",
            "payload":{"atRevision":7,"view":{"kind":"presentation"}}
        }))
        .unwrap()
    }

    fn response() -> NativeApplicationResponse {
        serde_json::from_value(json!({
            "apiVersion":2,"requestId":"presentation-read","instanceId":"instance",
            "documentId":"document","contentRevision":7,"ok":true,
            "result":{"kind":"presentation","atRevision":7,
                "documentSnapshotId":"native-object:document:7","contextId":"scene-root",
                "frame":12,"workId":"native-work-9","viewGeneration":3}
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn presentation_wire_rejects_foreign_common_envelope_identity() {
        use crate::wire;
        for key in [
            None,
            Some("requestId"),
            Some("instanceId"),
            Some("documentId"),
        ] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = crate::registry::Endpoint {
                instance_id: "instance".into(),
                port: listener.local_addr().unwrap().port(),
                secret: "presentation-contract".into(),
                build_id: "contract-test".into(),
            };
            let mut value = serde_json::to_value(response()).unwrap();
            if let Some(key) = key {
                value[key] = json!("foreign");
            }
            let server = async {
                let (mut stream, _) = listener.accept().await.unwrap();
                let envelope: wire::AuthenticatedWireRequest =
                    wire::read_json(&mut stream).await.unwrap();
                let wire::AuthenticatedWireRequest::Native(message) = envelope else {
                    panic!("non-native request")
                };
                assert_eq!(message.secret, endpoint.secret);
                assert_eq!(message.native_request, request());
                wire::write_json(&mut stream, &value).await.unwrap();
            };
            let client = wire::call_native(&endpoint, request(), wire::CancellationToken::new());
            let (_, result) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
                tokio::join!(server, client)
            })
            .await
            .unwrap();
            assert_eq!(result.is_ok(), key.is_none());
        }
    }
}
