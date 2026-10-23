//! Closed bounded Mesh and presentation views for the existing common object query.
use super::{exact, frame, revision};
use serde_json::Value;

// The image-mesh branch carries one presented raster and at most sixteen field
// entries. It never admits a serialized document or an unbounded mesh object.
pub(super) fn image_mesh_view(value: &Value) -> bool {
    value
        .get("view")
        .and_then(|view| view.get("kind"))
        .and_then(Value::as_str)
        == Some("imageMesh")
}
fn opaque(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|id| !id.is_empty())
}
pub(super) fn image_mesh_request(value: &Value) -> bool {
    let Some(payload) = exact(value, &["atRevision", "documentSnapshotId", "view"], &[]) else {
        return false;
    };
    let Some(view) = exact(
        &payload["view"],
        &[
            "kind",
            "contextId",
            "layerUid",
            "frame",
            "workId",
            "viewGeneration",
            "page",
        ],
        &[],
    ) else {
        return false;
    };
    revision(payload.get("atRevision")).is_some()
        && opaque(payload.get("documentSnapshotId"))
        && view["kind"] == "imageMesh"
        && opaque(view.get("contextId"))
        && opaque(view.get("layerUid"))
        && frame(view.get("frame")).is_some()
        && view
            .get("workId")
            .and_then(Value::as_str)
            .is_some_and(crate::contract::bounded_identifier)
        && revision(view.get("viewGeneration")).is_some_and(|generation| generation > 0)
        && image_mesh_page_request(&view["page"])
}
fn image_mesh_page_request(value: &Value) -> bool {
    if value.get("kind").and_then(Value::as_str) == Some("selection") {
        return exact(value, &["kind"], &[]).is_some();
    }
    exact(value, &["kind", "meshId", "cursor", "limit"], &[]).is_some_and(|page| {
        matches!(
            page["kind"].as_str(),
            Some("outline" | "verts" | "tris" | "offsets")
        ) && opaque(page.get("meshId"))
            && frame(page.get("cursor")).is_some()
            && page
                .get("limit")
                .and_then(Value::as_u64)
                .is_some_and(|limit| (1..=16).contains(&limit))
    })
}
struct MeshCounts {
    outline: u64,
    vertices: u64,
    triangles: u64,
    offsets: bool,
}
fn image_mesh_counts(value: &Value) -> Option<MeshCounts> {
    let mesh = exact(
        value,
        &[
            "meshId",
            "outlineCount",
            "vertexCount",
            "triangleIndexCount",
            "offsetsPresent",
        ],
        &["cols", "rows"],
    )?;
    let counts = MeshCounts {
        outline: revision(mesh.get("outlineCount"))?,
        vertices: revision(mesh.get("vertexCount"))?,
        triangles: revision(mesh.get("triangleIndexCount"))?,
        offsets: mesh.get("offsetsPresent")?.as_bool()?,
    };
    (opaque(mesh.get("meshId"))
        && counts.outline <= counts.vertices
        && counts.triangles % 3 == 0
        && ["cols", "rows"].iter().all(|key| {
            mesh.get(*key).is_none_or(|value| {
                value
                    .as_u64()
                    .is_some_and(|density| (1..=64).contains(&density))
            })
        }))
    .then_some(counts)
}
fn image_mesh_target(value: &Value, view: &Value) -> bool {
    exact(
        value,
        &["contextId", "layerUid", "frameScope", "strokeId"],
        &[],
    )
    .is_some_and(|target| {
        target.get("contextId") == view.get("contextId")
            && target.get("layerUid") == view.get("layerUid")
            && opaque(target.get("strokeId"))
            && exact(&target["frameScope"], &["kind", "frame"], &[]).is_some_and(|scope| {
                matches!(scope["kind"].as_str(), Some("authored" | "reference"))
                    && frame(scope.get("frame"))
                        .is_some_and(|source| source <= view["frame"].as_u64().unwrap())
            })
    })
}
fn image_mesh_rect(value: &Value) -> bool {
    exact(value, &["x", "y", "width", "height", "rotation"], &[]).is_some_and(|rect| {
        rect.values()
            .all(|value| value.as_f64().is_some_and(f64::is_finite))
            && ["width", "height"]
                .iter()
                .all(|key| rect[*key].as_f64().is_some_and(|size| size > 0.0))
    })
}
fn image_mesh_page_result(
    requested: &Value,
    page: &Value,
    mesh: &Value,
    counts: &MeshCounts,
) -> bool {
    let Some(page) = exact(
        page,
        &["kind", "cursor", "total", "nextCursor", "items"],
        &[],
    ) else {
        return false;
    };
    let (Some(cursor), Some(total), Some(items)) = (
        frame(page.get("cursor")),
        revision(page.get("total")),
        page["items"].as_array(),
    ) else {
        return false;
    };
    if page.get("kind") != requested.get("kind")
        || page.get("cursor") != requested.get("cursor")
        || mesh.get("meshId") != requested.get("meshId")
        || items.len() > 16
        || items.len() as u64 > requested["limit"].as_u64().unwrap()
    {
        return false;
    }
    let expected = match page["kind"].as_str() {
        Some("outline") => counts.outline,
        Some("verts") => counts.vertices,
        Some("tris") => counts.triangles,
        Some("offsets") if counts.offsets => counts.vertices,
        _ => return false,
    };
    let end = cursor + items.len() as u64;
    let next_matches = if end == total {
        page["nextCursor"].is_null()
    } else {
        !items.is_empty() && frame(page.get("nextCursor")) == Some(end)
    };
    total == expected
        && end <= total
        && next_matches
        && items.iter().all(|value| {
            if page["kind"] == "tris" {
                frame(Some(value)).is_some_and(|index| index < counts.vertices)
            } else {
                value.as_array().is_some_and(|pair| {
                    pair.len() == 2
                        && pair
                            .iter()
                            .all(|number| number.as_f64().is_some_and(f64::is_finite))
                })
            }
        })
}
pub(super) fn image_mesh_result(document_id: &str, payload: &Value, result: &Value) -> bool {
    if !image_mesh_request(payload) {
        return false;
    }
    let view = &payload["view"];
    let Some(value) = exact(
        result,
        &[
            "kind",
            "atRevision",
            "documentSnapshotId",
            "contextId",
            "layerUid",
            "frame",
            "workId",
            "viewGeneration",
            "target",
            "rect",
            "mesh",
            "page",
        ],
        &[],
    ) else {
        return false;
    };
    if value["kind"] != "imageMesh"
        || ["atRevision", "documentSnapshotId"]
            .iter()
            .any(|key| value.get(*key) != payload.get(*key))
        || value["documentSnapshotId"].as_str()
            != Some(
                format!(
                    "native-object:{document_id}:{}",
                    payload["atRevision"].as_u64().unwrap()
                )
                .as_str(),
            )
        || ["contextId", "layerUid", "frame", "workId", "viewGeneration"]
            .iter()
            .any(|key| value.get(*key) != view.get(*key))
        || !image_mesh_target(&value["target"], view)
        || !image_mesh_rect(&value["rect"])
    {
        return false;
    }
    let counts = if value["mesh"].is_null() {
        None
    } else {
        match image_mesh_counts(&value["mesh"]) {
            Some(counts) => Some(counts),
            None => return false,
        }
    };
    if view["page"]["kind"] == "selection" {
        return exact(&value["page"], &["kind"], &[]).is_some()
            && value["page"]["kind"] == "selection";
    }
    counts.is_some_and(|counts| {
        image_mesh_page_result(&view["page"], &value["page"], &value["mesh"], &counts)
    })
}

/// Presentation discovery shares the existing query operation, with no supplied
/// work token or caller-selected frame. Only the host can provide the result.
pub(super) fn presentation_view(value: &Value) -> bool {
    value.pointer("/view/kind").and_then(Value::as_str) == Some("presentation")
}
pub(super) fn presentation_request(value: &Value) -> bool {
    exact(value, &["atRevision", "view"], &[]).is_some_and(|payload| {
        revision(payload.get("atRevision")).is_some()
            && exact(&payload["view"], &["kind"], &[])
                .is_some_and(|view| view["kind"] == "presentation")
    })
}
pub(super) fn presentation_result(document_id: &str, payload: &Value, result: &Value) -> bool {
    if !presentation_request(payload) {
        return false;
    }
    exact(
        result,
        &[
            "kind",
            "atRevision",
            "documentSnapshotId",
            "contextId",
            "frame",
            "workId",
            "viewGeneration",
        ],
        &[],
    )
    .is_some_and(|value| {
        value["kind"] == "presentation"
            && revision(value.get("atRevision")) == revision(payload.get("atRevision"))
            && ["documentSnapshotId", "contextId", "workId"]
                .iter()
                .all(|key| {
                    value
                        .get(*key)
                        .and_then(Value::as_str)
                        .is_some_and(crate::contract::bounded_identifier)
                })
            && value["documentSnapshotId"].as_str()
                == Some(
                    format!(
                        "native-object:{document_id}:{}",
                        payload["atRevision"].as_u64().unwrap()
                    )
                    .as_str(),
                )
            && frame(value.get("frame")).is_some()
            && revision(value.get("viewGeneration")).is_some_and(|generation| generation > 0)
    })
}

#[cfg(test)]
mod presentation_tests {
    use super::*;
    use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
    use serde_json::json;

    // Literal contract fixtures only; these tests do not establish native surface authority.
    fn payload() -> Value {
        json!({"atRevision":7,"view":{"kind":"presentation"}})
    }
    fn details() -> Value {
        json!({"kind":"presentation","atRevision":7,"documentSnapshotId":"native-object:document:7",
            "contextId":"scene-root","frame":12,"workId":"native-work-9","viewGeneration":3})
    }
    fn request() -> NativeApplicationRequest {
        serde_json::from_value(
            json!({"apiVersion":2,"requestId":"presentation-read","instanceId":"instance",
            "documentId":"document","operation":"query.document.object","payload":payload()}),
        )
        .unwrap()
    }
    fn response() -> NativeApplicationResponse {
        serde_json::from_value(
            json!({"apiVersion":2,"requestId":"presentation-read","instanceId":"instance",
            "documentId":"document","contentRevision":7,"ok":true,"result":details()}),
        )
        .unwrap()
    }
    fn accepts(value: &Value) -> bool {
        presentation_result("document", &payload(), value)
    }
    fn edited(original: &Value, path: &str, replacement: Value) -> Value {
        let mut value = original.clone();
        *value.pointer_mut(path).unwrap() = replacement;
        value
    }
    #[test]
    fn presentation_runtime_shapes_are_closed_pinned_and_bounded() {
        assert!(request().validate_input().is_ok());
        assert!(crate::wire::validate_native_response(&response(), &request()).is_ok());
        for bad in [
            Value::Null,
            json!([]),
            json!({"view":{"kind":"presentation"}}),
            json!({"atRevision":7,"view":null}),
            json!({"atRevision":7,"view":{"kind":"presentation","frame":0}}),
            json!({"atRevision":7,"view":{"kind":"presentation"},"documentSnapshotId":"caller-snapshot"}),
        ] {
            assert!(!crate::native_contract::validate_request(
                "query.document.object",
                &bad
            ));
        }
        for revision in [
            json!(-1),
            json!(0.5),
            json!(9_007_199_254_740_992u64),
            Value::Null,
        ] {
            assert!(!presentation_request(&edited(
                &payload(),
                "/atRevision",
                revision
            )));
        }
        for (path, value) in [
            ("/kind", json!("imageMesh")),
            ("/atRevision", json!(8)),
            ("/documentSnapshotId", json!("native-object:other:7")),
            ("/contextId", json!("bad context")),
            ("/contextId", json!("x".repeat(129))),
            ("/frame", json!(4_294_967_296u64)),
            ("/workId", json!("bad work")),
            ("/viewGeneration", json!(0)),
            ("/viewGeneration", json!(9_007_199_254_740_992u64)),
        ] {
            assert!(!accepts(&edited(&details(), path, value)), "{path}");
        }
        for key in details().as_object().unwrap().keys() {
            let mut missing = details();
            missing.as_object_mut().unwrap().remove(key);
            assert!(!accepts(&missing), "missing {key}");
        }
        let mut extra = details();
        extra["mesh"] = json!({});
        assert!(!accepts(&extra));
        let mut boundary = details();
        boundary["frame"] = json!(u32::MAX);
        boundary["viewGeneration"] = json!(9_007_199_254_740_991u64);
        assert!(accepts(&boundary));
        let mut mismatched = response();
        mismatched.content_revision = 8;
        assert!(crate::wire::validate_native_response(&mismatched, &request()).is_err());
        let mut write = request();
        write.expected_revision = Some(7);
        assert!(write.validate_input().is_err());
    }
    #[test]
    fn presentation_original_bytes_keep_duplicate_malformed_and_4096_guards() {
        let raw = serde_json::to_string(&request()).unwrap();
        assert!(crate::wire::decode_native_request(raw.as_bytes()).is_ok());
        for (needle, replacement) in [
            (
                "\"requestId\":",
                "\"requestId\":\"duplicate\",\"requestId\":",
            ),
            ("\"atRevision\":", "\"atRevision\":7,\"atRevision\":"),
            (
                "\"view\":",
                "\"view\":{\"kind\":\"presentation\"},\"view\":",
            ),
            (
                "\"kind\":\"presentation\"",
                "\"kind\":\"presentation\",\"kind\":\"presentation\"",
            ),
        ] {
            assert!(raw.contains(needle));
            assert!(crate::wire::decode_native_request(
                raw.replacen(needle, replacement, 1).as_bytes()
            )
            .is_err());
        }
        assert!(crate::wire::decode_native_request(raw[..raw.len() - 1].as_bytes()).is_err());
        assert!(crate::wire::decode_native_request(
            format!("{}{raw}", " ".repeat(4096)).as_bytes()
        )
        .is_err());
    }
    fn schema() -> jsonschema::Validator {
        let schema: Value = serde_json::from_str(include_str!(
            "../../engineering/application/native-transport-v2.schema.json"
        ))
        .unwrap();
        let mut resources = jsonschema::Registry::new();
        for text in [
            include_str!(
                "../../engineering/application/native-bitmap-brush-resources-v1.schema.json"
            ),
            include_str!("../../engineering/application/native-object-item-v1.schema.json"),
            include_str!("../../engineering/application/native-font-assets-v1.schema.json"),
            include_str!("../../engineering/application/native-property-metadata-v1.schema.json"),
        ] {
            let resource: Value = serde_json::from_str(text).unwrap();
            resources = resources
                .add(
                    resource["$id"].as_str().unwrap().to_owned(),
                    jsonschema::Resource::from_contents(resource),
                )
                .unwrap();
        }
        jsonschema::draft202012::options()
            .with_registry(&resources.prepare().unwrap())
            .build(&schema)
            .unwrap()
    }
    #[test]
    fn presentation_canonical_schema_preserves_legacy_and_mesh_disjoint_branches() {
        let validator = schema();
        let base = serde_json::to_value(request()).unwrap();
        let reply = serde_json::to_value(response()).unwrap();
        assert!(validator.is_valid(&base));
        assert!(validator.is_valid(&reply));
        let corpus: Value = serde_json::from_str(include_str!(
            "../../engineering/application/examples/native-object-v1/cases.json"
        ))
        .unwrap();
        let mut legacy = base.clone();
        legacy["payload"] = json!({"atRevision":7,"stableTarget":corpus["records"][0]["target"]});
        assert!(validator.is_valid(&legacy));
        let mut legacy_reply = reply.clone();
        legacy_reply["result"] = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","object":corpus["records"][0]});
        assert!(validator.is_valid(&legacy_reply));
        let mut mesh = base.clone();
        mesh["payload"] = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":{
            "kind":"imageMesh","contextId":"scene-root","layerUid":"L0","frame":12,"workId":"native-work-9",
            "viewGeneration":3,"page":{"kind":"selection"}}});
        assert!(validator.is_valid(&mesh));
        assert!(crate::native_contract::validate_request(
            "query.document.object",
            &mesh["payload"]
        ));
        for (path, value) in [
            ("/payload/view", Value::Null),
            ("/payload/view", json!({"kind":"presentation","page":{}})),
            (
                "/payload",
                json!({"atRevision":7,"view":{"kind":"presentation"},"stableTarget":corpus["records"][0]["target"]}),
            ),
        ] {
            assert!(!validator.is_valid(&edited(&base, path, value)));
        }
        for key in details().as_object().unwrap().keys() {
            let mut missing = reply.clone();
            missing["result"].as_object_mut().unwrap().remove(key);
            assert!(!validator.is_valid(&missing), "missing {key}");
        }
        let mut extra = reply;
        extra["result"]["target"] = json!({});
        assert!(!validator.is_valid(&extra));
    }
}
