//! Literal protocol controls for the existing query.document.object operation.
use super::{request, result};
use crate::contract::NativeApplicationRequest;
use serde_json::{json, Value};

const OP: &str = "query.document.object";

fn selection() -> (Value, Value) {
    let payload = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":{
        "kind":"imageMesh","contextId":"component:opaque/α","layerUid":"data:layer","frame":5,
        "workId":"native-work:9","viewGeneration":3,"page":{"kind":"selection"}}});
    let response = json!({"kind":"imageMesh","atRevision":7,"documentSnapshotId":"native-object:document:7",
        "contextId":"component:opaque/α","layerUid":"data:layer","frame":5,"workId":"native-work:9","viewGeneration":3,
        "target":{"contextId":"component:opaque/α","layerUid":"data:layer",
            "frameScope":{"kind":"authored","frame":2},"strokeId":"blob:stroke"},
        "rect":{"x":-12.5,"y":2,"width":100,"height":50,"rotation":-30},
        "mesh":{"meshId":"im_9","outlineCount":3,"vertexCount":4,"triangleIndexCount":6,"offsetsPresent":true},
        "page":{"kind":"selection"}});
    (payload, response)
}

fn field(kind: &str) -> (Value, Value) {
    let (mut payload, mut response) = selection();
    payload["view"]["page"] = json!({"kind":kind,"meshId":"im_9","cursor":0,"limit":2});
    let (total, items) = match kind {
        "outline" => (3, json!([[0, 0], [1, 0]])),
        "tris" => (6, json!([0, 3])),
        _ => (4, json!([[0, 0], [1, 0]])),
    };
    response["page"] = json!({"kind":kind,"cursor":0,"total":total,"nextCursor":2,"items":items});
    (payload, response)
}

fn changed(value: &Value, path: &str, replacement: Value) -> Value {
    let mut value = value.clone();
    *value.pointer_mut(path).unwrap() = replacement;
    value
}

#[test]
fn selection_keeps_opaque_component_identity_and_the_actual_held_target() {
    let (payload, mut response) = selection();
    assert!(request(&payload));
    assert!(result("document", &payload, &response));
    assert!(crate::native_contract::validate_request(OP, &payload));
    assert!(crate::native_contract::validate_result(
        OP, "document", &payload, &response
    ));
    for scope in ["authored", "reference"] {
        response["target"]["frameScope"]["kind"] = json!(scope);
        assert!(result("document", &payload, &response));
    }
    response["target"]["frameScope"]["frame"] = json!(5);
    response["mesh"]["cols"] = json!(1);
    response["mesh"]["rows"] = json!(64);
    assert!(result("document", &payload, &response));
    response["mesh"] = Value::Null;
    assert!(result("document", &payload, &response));
    assert!(!result("replacement", &payload, &response));
}

#[test]
fn every_field_page_correlates_its_metadata_and_progress() {
    for kind in ["outline", "verts", "tris", "offsets"] {
        let (payload, response) = field(kind);
        assert!(request(&payload), "{kind}");
        assert!(result("document", &payload, &response), "{kind}");
        assert!(crate::native_contract::validate_result(
            OP, "document", &payload, &response
        ));
        let mut unknown_request = payload.clone();
        unknown_request["view"]["page"]["unknown"] = json!(true);
        assert!(!request(&unknown_request));
        let mut unknown_response = response.clone();
        unknown_response["page"]["unknown"] = json!(true);
        assert!(!result("document", &payload, &unknown_response));
        for key in ["kind", "cursor", "total", "nextCursor", "items"] {
            let mut missing = response.clone();
            missing["page"].as_object_mut().unwrap().remove(key);
            assert!(
                !result("document", &payload, &missing),
                "{kind} missing {key}"
            );
        }
        for (path, value) in [
            ("/page/kind", json!("selection")),
            ("/page/cursor", json!(1)),
            ("/page/total", json!(99)),
            ("/page/nextCursor", Value::Null),
            ("/page/nextCursor", json!(1)),
            ("/page/nextCursor", json!(3)),
            ("/mesh/meshId", json!("another-mesh")),
            ("/mesh", Value::Null),
        ] {
            assert!(
                !result("document", &payload, &changed(&response, path, value)),
                "{kind} {path}"
            );
        }
        let mut stalled = response.clone();
        stalled["page"]["items"] = json!([]);
        stalled["page"]["nextCursor"] = json!(0);
        assert!(
            !result("document", &payload, &stalled),
            "{kind} zero progress"
        );
        let mut oversized = response.clone();
        oversized["page"]["items"] = if kind == "tris" {
            json!([0, 1, 2])
        } else {
            json!([[0, 0], [1, 0], [0, 1]])
        };
        oversized["page"]["nextCursor"] = if kind == "outline" {
            Value::Null
        } else {
            json!(3)
        };
        assert!(
            !result("document", &payload, &oversized),
            "{kind} request limit"
        );
    }
}

#[test]
fn terminal_empty_and_partial_pages_are_explicit_and_bounded() {
    let (mut payload, mut response) = field("verts");
    payload["view"]["page"]["cursor"] = json!(4);
    response["page"] = json!({"kind":"verts","cursor":4,"total":4,"nextCursor":null,"items":[]});
    assert!(result("document", &payload, &response));
    response["page"]["nextCursor"] = json!(4);
    assert!(!result("document", &payload, &response));
    payload["view"]["page"]["cursor"] = json!(3);
    response["page"] =
        json!({"kind":"verts","cursor":3,"total":4,"nextCursor":null,"items":[[0,1]]});
    assert!(result("document", &payload, &response));
    response["page"]["items"] = json!([[0, 1], [1, 1]]);
    assert!(!result("document", &payload, &response));
    let (mut empty_request, mut empty_response) = field("outline");
    empty_response["mesh"]["outlineCount"] = json!(0);
    empty_request["view"]["page"]["limit"] = json!(1);
    empty_response["page"] =
        json!({"kind":"outline","cursor":0,"total":0,"nextCursor":null,"items":[]});
    assert!(result("document", &empty_request, &empty_response));
}

#[test]
fn selectors_refuse_wrong_types_ranges_and_every_undeclared_member() {
    let (payload, _) = selection();
    for (path, value) in [
        ("/atRevision", json!(-1)),
        ("/atRevision", json!(1.5)),
        ("/atRevision", json!(9_007_199_254_740_992u64)),
        ("/documentSnapshotId", json!("")),
        ("/view/kind", json!("mesh")),
        ("/view/contextId", json!("")),
        ("/view/layerUid", json!("")),
        ("/view/frame", json!(4_294_967_296u64)),
        ("/view/frame", json!(-1)),
        ("/view/workId", json!("contains space")),
        ("/view/workId", json!("x".repeat(129))),
        ("/view/viewGeneration", json!(0)),
        ("/view/viewGeneration", json!(1.5)),
        ("/view/viewGeneration", json!(9_007_199_254_740_992u64)),
        ("/view/page/kind", json!("document")),
    ] {
        assert!(!request(&changed(&payload, path, value)), "{path}");
    }
    for path in ["", "/view", "/view/page"] {
        let mut unknown = payload.clone();
        unknown.pointer_mut(path).unwrap()["unknown"] = json!(true);
        assert!(!request(&unknown), "extra member at {path}");
        for key in payload.pointer(path).unwrap().as_object().unwrap().keys() {
            let mut missing = payload.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(!request(&missing), "missing {path}/{key}");
        }
    }
    let (page, _) = field("verts");
    for (path, value) in [
        ("/view/page/meshId", json!("")),
        ("/view/page/cursor", json!(-1)),
        ("/view/page/cursor", json!(4_294_967_296u64)),
        ("/view/page/cursor", json!(0.5)),
        ("/view/page/limit", json!(0)),
        ("/view/page/limit", json!(17)),
        ("/view/page/limit", json!(1.5)),
        ("/view/page/kind", json!("selection")),
    ] {
        assert!(!request(&changed(&page, path, value)), "{path}");
    }
    for key in ["kind", "meshId", "cursor", "limit"] {
        let mut missing = page.clone();
        missing["view"]["page"].as_object_mut().unwrap().remove(key);
        assert!(!request(&missing), "missing page {key}");
    }
}

#[test]
fn response_correlation_preserves_all_pins_and_closed_nested_shapes() {
    let (payload, response) = selection();
    for (path, value) in [
        ("/kind", json!("object")),
        ("/atRevision", json!(8)),
        ("/documentSnapshotId", json!("native-object:document:8")),
        ("/contextId", json!("scene-root")),
        ("/layerUid", json!("another-layer")),
        ("/frame", json!(6)),
        ("/workId", json!("another-work")),
        ("/viewGeneration", json!(4)),
        ("/target/contextId", json!("scene-root")),
        ("/target/layerUid", json!("another-layer")),
        ("/target/strokeId", json!("")),
        ("/target/frameScope/kind", json!("all")),
        ("/target/frameScope/frame", json!(6)),
        ("/target/frameScope/frame", json!(-1)),
        ("/rect/width", json!(0)),
        ("/rect/height", json!(-1)),
        ("/rect/rotation", Value::Null),
        ("/mesh/meshId", json!("")),
        ("/mesh/outlineCount", json!(5)),
        ("/mesh/vertexCount", json!(-1)),
        ("/mesh/triangleIndexCount", json!(4)),
        ("/mesh/offsetsPresent", Value::Null),
        ("/mesh/vertexCount", json!(9_007_199_254_740_992u64)),
    ] {
        assert!(
            !result("document", &payload, &changed(&response, path, value)),
            "{path}"
        );
    }
    for density in [json!(0), json!(65), json!(1.5), Value::Null] {
        for key in ["cols", "rows"] {
            let mut invalid = response.clone();
            invalid["mesh"][key] = density.clone();
            assert!(!result("document", &payload, &invalid), "{key}");
        }
    }
    for path in [
        "",
        "/target",
        "/target/frameScope",
        "/rect",
        "/mesh",
        "/page",
    ] {
        let mut unknown = response.clone();
        unknown.pointer_mut(path).unwrap()["unknown"] = json!(true);
        assert!(
            !result("document", &payload, &unknown),
            "extra member at {path}"
        );
        for key in response.pointer(path).unwrap().as_object().unwrap().keys() {
            let mut missing = response.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(
                !result("document", &payload, &missing),
                "missing {path}/{key}"
            );
        }
    }
}

#[test]
fn points_indices_and_authored_offset_absence_are_not_interchangeable() {
    for kind in ["outline", "verts", "offsets"] {
        let (payload, response) = field(kind);
        for items in [
            json!([[0]]),
            json!([[0, 1, 2]]),
            json!([[0, null]]),
            json!([{"x":0,"y":1}]),
        ] {
            assert!(
                !result(
                    "document",
                    &payload,
                    &changed(&response, "/page/items", items)
                ),
                "{kind}"
            );
        }
    }
    let (payload, response) = field("offsets");
    assert!(!result(
        "document",
        &payload,
        &changed(&response, "/mesh/offsetsPresent", json!(false))
    ));
    let (payload, response) = field("tris");
    for items in [
        json!([4, 0]),
        json!([-1, 0]),
        json!([0.5, 0]),
        json!([4_294_967_296u64, 0]),
        json!([[0, 0]]),
    ] {
        assert!(!result(
            "document",
            &payload,
            &changed(&response, "/page/items", items)
        ));
    }
    let (mut payload, mut response) = field("verts");
    payload["view"]["page"]["limit"] = json!(16);
    response["mesh"]["vertexCount"] = json!(20);
    response["page"] =
        json!({"kind":"verts","cursor":0,"total":20,"nextCursor":17,"items":vec![[0,0];17]});
    assert!(!result("document", &payload, &response));
}

#[test]
fn original_operation_admits_the_new_variant_without_hybrid_or_write_fields() {
    let (payload, _) = selection();
    let envelope = json!({"apiVersion":2,"requestId":"read-mesh","instanceId":"instance","documentId":"document",
        "operation":OP,"payload":payload});
    let admitted: NativeApplicationRequest = serde_json::from_value(envelope.clone()).unwrap();
    assert!(admitted.validate_input().is_ok());
    let mut write_revision = envelope;
    write_revision["expectedRevision"] = json!(7);
    let refused: NativeApplicationRequest = serde_json::from_value(write_revision).unwrap();
    assert!(refused.validate_input().is_err());
    let legacy = json!({"atRevision":7,"stableTarget":{"contextId":"scene-root","layerUid":"L0",
        "frameScope":{"kind":"authored","frame":0},"strokeId":"a"}});
    assert!(request(&legacy));
    let mut hybrid = payload;
    hybrid["stableTarget"] = legacy["stableTarget"].clone();
    assert!(!request(&hybrid));
}

fn transport_schema() -> jsonschema::Validator {
    let schema: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .unwrap();
    // Complete canonical resources, matching the existing transport-schema tests;
    // no permissive replacement schemas and no external fetch are used.
    let mut registry = jsonschema::Registry::new();
    for text in [
        include_str!("../../engineering/application/native-bitmap-brush-resources-v1.schema.json"),
        include_str!("../../engineering/application/native-object-item-v1.schema.json"),
        include_str!("../../engineering/application/native-font-assets-v1.schema.json"),
        include_str!("../../engineering/application/native-property-metadata-v1.schema.json"),
    ] {
        let resource: Value = serde_json::from_str(text).unwrap();
        let id = resource["$id"].as_str().unwrap().to_owned();
        registry = registry
            .add(id, jsonschema::Resource::from_contents(resource))
            .unwrap();
    }
    jsonschema::draft202012::options()
        .with_registry(&registry.prepare().unwrap())
        .build(&schema)
        .unwrap()
}
fn schema_request(payload: Value) -> Value {
    json!({"apiVersion":2,"requestId":"mesh-schema","instanceId":"instance","documentId":"document",
        "operation":OP,"payload":payload})
}
fn schema_response(result: Value) -> Value {
    json!({"apiVersion":2,"requestId":"mesh-schema","instanceId":"instance","documentId":"document",
        "contentRevision":7,"ok":true,"result":result})
}

#[test]
fn complete_schema_keeps_legacy_and_all_image_mesh_pages_as_disjoint_shapes() {
    let schema = transport_schema();
    let corpus: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let object = &corpus["records"][0];
    let legacy = json!({"atRevision":7,"stableTarget":object["target"]});
    assert!(schema.is_valid(&schema_request(legacy.clone())));
    assert!(schema.is_valid(&schema_response(json!({"atRevision":7,
        "documentSnapshotId":"native-object:document:7","object":object}))));
    let (payload, response) = selection();
    assert!(schema.is_valid(&schema_request(payload.clone())));
    assert!(schema.is_valid(&schema_response(response.clone())));
    assert!(schema.is_valid(&schema_response(changed(&response, "/mesh", Value::Null))));
    let mut hybrid = payload;
    hybrid["stableTarget"] = legacy["stableTarget"].clone();
    assert!(!schema.is_valid(&schema_request(hybrid)));
    for kind in ["outline", "verts", "tris", "offsets"] {
        let (payload, response) = field(kind);
        assert!(schema.is_valid(&schema_request(payload.clone())), "{kind}");
        assert!(
            schema.is_valid(&schema_response(response.clone())),
            "{kind}"
        );
        for key in ["kind", "meshId", "cursor", "limit"] {
            let mut missing = payload.clone();
            missing["view"]["page"].as_object_mut().unwrap().remove(key);
            assert!(
                !schema.is_valid(&schema_request(missing)),
                "{kind} missing {key}"
            );
        }
        for path in ["", "/view", "/view/page"] {
            let mut extra = payload.clone();
            extra.pointer_mut(path).unwrap()["unknown"] = json!(true);
            assert!(
                !schema.is_valid(&schema_request(extra)),
                "{kind} extra {path}"
            );
        }
        for path in [
            "",
            "/target",
            "/target/frameScope",
            "/rect",
            "/mesh",
            "/page",
        ] {
            let mut extra = response.clone();
            extra.pointer_mut(path).unwrap()["unknown"] = json!(true);
            assert!(
                !schema.is_valid(&schema_response(extra)),
                "{kind} extra {path}"
            );
        }
        for key in ["kind", "cursor", "total", "nextCursor", "items"] {
            let mut missing = response.clone();
            missing["page"].as_object_mut().unwrap().remove(key);
            assert!(
                !schema.is_valid(&schema_response(missing)),
                "{kind} missing result {key}"
            );
        }
        assert!(!schema.is_valid(&schema_response(changed(&response, "/mesh", Value::Null))));
    }
}

#[test]
fn complete_schema_refuses_invalid_mesh_shapes_while_rust_retains_exact_correlation() {
    let schema = transport_schema();
    let (payload, response) = field("verts");
    for (path, value) in [
        ("/view/page/limit", json!(0)),
        ("/view/page/limit", json!(17)),
        ("/view/page/cursor", json!(4_294_967_296u64)),
        ("/view/page/kind", json!("selection")),
        ("/view/workId", json!("bad work id")),
        ("/view/viewGeneration", json!(0)),
    ] {
        assert!(
            !schema.is_valid(&schema_request(changed(&payload, path, value))),
            "{path}"
        );
    }
    for (path, value) in [
        ("/rect/width", json!(0)),
        ("/mesh/cols", json!(65)),
        ("/mesh/triangleIndexCount", json!(4)),
        ("/target/strokeId", json!("")),
        ("/page/items", json!([[0, 1, 2]])),
        ("/page/items", json!(vec![[0, 0]; 17])),
    ] {
        assert!(
            !schema.is_valid(&schema_response(changed(&response, path, value))),
            "{path}"
        );
    }
    let (_, offsets) = field("offsets");
    assert!(!schema.is_valid(&schema_response(changed(
        &offsets,
        "/mesh/offsetsPresent",
        json!(false)
    ))));
    let (_, triangles) = field("tris");
    assert!(!schema.is_valid(&schema_response(changed(
        &triangles,
        "/page/items",
        json!([-1])
    ))));
    // Shape schemas cannot compare independent fields or a request to its reply.
    // Preserve that distinction: the real Rust validator must refuse both cases.
    for (path, value) in [
        ("/workId", json!("foreign-work")),
        ("/page/total", json!(99)),
    ] {
        let uncorrelated = changed(&response, path, value);
        assert!(schema.is_valid(&schema_response(uncorrelated.clone())));
        assert!(!result("document", &payload, &uncorrelated));
    }
}
