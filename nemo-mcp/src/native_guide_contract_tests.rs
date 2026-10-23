use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::json;

fn request(op: &str, payload: Value) -> NativeApplicationRequest {
    let mut value = json!({"apiVersion":2,"requestId":"guide-q","instanceId":"instance-A",
        "documentId":"document-A","operation":op,"payload":payload});
    if op == EDIT {
        value["expectedRevision"] = json!(7);
    }
    serde_json::from_value(value).unwrap()
}
fn query(selector: Value) -> NativeApplicationRequest {
    request(
        READ,
        json!({"atRevision":7,"documentSnapshotId":"snapshot-7","contextId":"scene-root","selector":selector}),
    )
}
fn response(q: &NativeApplicationRequest, body: Value) -> NativeApplicationResponse {
    let value = if q.operation == READ {
        json!({"instanceId":"instance-A","documentId":"document-A","atRevision":7,
            "documentSnapshotId":"snapshot-7","contextId":"scene-root","result":body})
    } else {
        body
    };
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":q.request_id,"instanceId":"instance-A",
        "documentId":"document-A","contentRevision":7,"ok":true,"result":value}),
    )
    .unwrap()
}
fn wire(q: &NativeApplicationRequest, r: &NativeApplicationResponse) -> bool {
    crate::wire::validate_native_response(r, q).is_ok()
}
fn settings_value() -> Value {
    json!({"guidesPresent":true,"hPresent":false,"vPresent":true,"hCount":0,"vCount":0})
}
#[test]
fn three_original_guide_gestures_have_closed_shapes_and_revision_admission() {
    for action in [
        json!({"kind":"add","axis":"h","position":-12.25}),
        json!({"kind":"move","axis":"v","ordinal":1,"position":0.125}),
        json!({"kind":"remove","axis":"h","ordinal":2}),
    ] {
        let q = request(
            EDIT,
            json!({"command":"object.project.guide.edit","action":action}),
        );
        assert!(q.validate_input().is_ok());
        assert!(q.validate().is_ok());
    }
    for action in [
        json!({"kind":"clear","axis":"h"}),
        json!({"kind":"move","axis":"h","ordinal":0,"position":1}),
        json!({"kind":"move","axis":"h","ordinal":1.5,"position":1}),
        json!({"kind":"add","axis":"x","position":1}),
        json!({"kind":"add","axis":"h","position":null}),
        json!({"kind":"add","axis":"h","position":1,"guides":[]}),
    ] {
        assert!(request(
            EDIT,
            json!({"command":"object.project.guide.edit","action":action})
        )
        .validate_input()
        .is_err());
    }
    let mut q = request(
        EDIT,
        json!({"command":"object.project.guide.edit","action":{"kind":"add","axis":"h","position":1}}),
    );
    q.expected_revision = None;
    assert!(q.validate_input().is_err());
    let mut q = query(json!({"kind":"settings"}));
    q.expected_revision = Some(7);
    assert!(q.validate_input().is_err());
}
#[test]
fn guide_settings_and_pages_preserve_absence_duplicates_and_forward_progress() {
    let q = query(json!({"kind":"settings"}));
    assert!(q.validate().is_ok());
    assert!(wire(&q, &response(&q, settings_value())));
    for body in [
        json!({"guidesPresent":false,"hPresent":false,"vPresent":false,"hCount":0,"vCount":0}),
        json!({"guidesPresent":true,"hPresent":true,"vPresent":true,"hCount":0,"vCount":0}),
    ] {
        assert!(wire(&q, &response(&q, body)));
    }
    for body in [
        json!({"guidesPresent":false,"hPresent":true,"vPresent":false,"hCount":0,"vCount":0}),
        json!({"guidesPresent":true,"hPresent":false,"vPresent":true,"hCount":1,"vCount":0}),
        json!({"guidesPresent":true,"hPresent":true,"vPresent":true,"hCount":0.5,"vCount":0}),
    ] {
        assert!(!wire(&q, &response(&q, body)));
    }
    let q = query(json!({"kind":"axisPage","axis":"h","cursor":0,"limit":16}));
    let page = json!({"kind":"guideAxisPage","axis":"h","guidesPresent":true,"axisPresent":true,"guideCount":3,"cursor":0,
        "items":[{"ordinal":1,"position":-12.25},{"ordinal":2,"position":-12.25}],"nextCursor":2});
    assert!(wire(&q, &response(&q, page.clone())));
    for (key, value) in [
        ("nextCursor", json!(0)),
        ("axis", json!("v")),
        ("cursor", json!(1)),
        ("axisPresent", json!(false)),
    ] {
        let mut bad = page.clone();
        bad[key] = value;
        assert!(!wire(&q, &response(&q, bad)));
    }
    let mut bad = page.clone();
    bad["items"][1]["ordinal"] = json!(1);
    assert!(!wire(&q, &response(&q, bad)));
    let mut bad = page;
    bad["items"][0]["position"] = Value::Null;
    assert!(!wire(&q, &response(&q, bad)));
    for selector in [
        json!({"kind":"axisPage","axis":"h","cursor":0,"limit":17}),
        json!({"kind":"axisPage","axis":"h","cursor":-1,"limit":16}),
        json!({"kind":"axisPage","axis":"x","cursor":0,"limit":16}),
    ] {
        assert!(query(selector).validate_input().is_err());
    }
}
#[test]
fn guide_wire_correlates_native_owners_snapshots_and_one_history_receipt() {
    let q = query(json!({"kind":"settings"}));
    let r = response(&q, settings_value());
    let mut bad = r.clone();
    bad.instance_id = "other".into();
    assert!(!wire(&q, &bad));
    let mut bad = r.clone();
    bad.document_id = "other".into();
    assert!(!wire(&q, &bad));
    let mut bad = r.clone();
    bad.content_revision = 8;
    assert!(!wire(&q, &bad));
    for key in [
        "instanceId",
        "documentId",
        "documentSnapshotId",
        "contextId",
    ] {
        let mut bad = r.clone();
        bad.result.as_mut().unwrap()[key] = json!("other");
        assert!(!wire(&q, &bad));
    }
    let q = request(
        EDIT,
        json!({"command":"object.project.guide.edit","action":{"kind":"move","axis":"h","ordinal":1,"position":2.5}}),
    );
    let mut r = response(&q, json!({"applied":true,"historyEntriesAdded":1}));
    r.content_revision = 8;
    assert!(wire(&q, &r));
    r.content_revision = 7;
    assert!(!wire(&q, &r));
    let r = response(&q, json!({"applied":false,"historyEntriesAdded":0}));
    assert!(wire(&q, &r));
    let r = response(&q, json!({"applied":false,"historyEntriesAdded":1}));
    assert!(!wire(&q, &r));
}
#[test]
fn guide_public_descriptor_examples_validate_against_the_actual_complete_transport_schema() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .unwrap();
    let mut registry = jsonschema::Registry::new();
    for source in [
        include_str!("../../engineering/application/native-bitmap-brush-resources-v1.schema.json"),
        include_str!("../../engineering/application/native-object-item-v1.schema.json"),
        include_str!("../../engineering/application/native-font-assets-v1.schema.json"),
        include_str!("../../engineering/application/native-property-metadata-v1.schema.json"),
    ] {
        let document: Value = serde_json::from_str(source).unwrap();
        let id = document["$id"].as_str().unwrap().to_owned();
        registry = registry
            .add(id, jsonschema::Resource::from_contents(document))
            .unwrap();
    }
    let registry = registry.prepare().unwrap();
    let validator = jsonschema::draft202012::options()
        .with_registry(&registry)
        .build(&schema)
        .unwrap();
    let catalog = crate::capabilities::native_catalog();
    let descriptor = catalog.capability_for_operation(READ).unwrap();
    assert_eq!(descriptor["availability"]["state"], "available");
    for example in descriptor["examples"].as_array().unwrap() {
        assert!(validator.is_valid(&example["request"]));
        assert!(validator.is_valid(&example["response"]));
    }
}
