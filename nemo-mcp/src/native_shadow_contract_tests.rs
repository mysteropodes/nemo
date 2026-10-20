use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::json;

fn query(selector: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"shadow-read","instanceId":"i","documentId":"d",
        "operation":READ,"payload":{"atRevision":7,"documentSnapshotId":"s7","contextId":"scene-root","selector":selector}})).unwrap()
}
fn edit(action: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"shadow-edit","instanceId":"i","documentId":"d",
        "operation":EDIT,"expectedRevision":7,"payload":{"command":"object.project.shadow.edit","action":action}})).unwrap()
}
fn response(q: &NativeApplicationRequest, body: Value) -> NativeApplicationResponse {
    let result = if q.operation == READ {
        json!({"instanceId":"i","documentId":"d","atRevision":7,"documentSnapshotId":"s7","contextId":"scene-root","result":body})
    } else {
        body
    };
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":q.request_id,"instanceId":"i","documentId":"d",
        "contentRevision":7,"ok":true,"result":result}),
    )
    .unwrap()
}
fn wire(q: &NativeApplicationRequest, r: &NativeApplicationResponse) -> bool {
    crate::wire::validate_native_response(r, q).is_ok()
}
#[test]
fn only_original_add_and_select_are_admitted_with_one_native_revision() {
    for action in [
        json!({"kind":"add","id":"duplicate","color":"#aBc"}),
        json!({"kind":"select","ordinal":2}),
        json!({"kind":"add","id":"","color":""}),
    ] {
        let q = edit(action);
        assert!(q.validate_input().is_ok());
        assert!(q.validate().is_ok());
    }
    for action in [
        json!({"kind":"remove","ordinal":1}),
        json!({"kind":"select","ordinal":0}),
        json!({"kind":"select","ordinal":1.5}),
        json!({"kind":"add","id":"x","color":"#abc","shadowMode":true}),
        json!({"kind":"add","id":"x\u{0}","color":"#abc"}),
        json!({"kind":"add","id":"x","color":"é".repeat(513)}),
    ] {
        assert!(edit(action).validate_input().is_err());
    }
    let mut q = edit(json!({"kind":"select","ordinal":2}));
    q.expected_revision = None;
    assert!(q.validate_input().is_err());
    let q = edit(json!({"kind":"select","ordinal":2}));
    let mut r = response(&q, json!({"applied":true,"historyEntriesAdded":1}));
    r.content_revision = 8;
    assert!(wire(&q, &r));
    r.content_revision = 7;
    assert!(!wire(&q, &r));
    assert!(wire(
        &q,
        &response(&q, json!({"applied":false,"historyEntriesAdded":0}))
    ));
    assert!(!wire(
        &q,
        &response(&q, json!({"applied":false,"historyEntriesAdded":1}))
    ));
}
#[test]
fn absence_empty_and_exact_duplicate_records_are_closed_and_pinned() {
    let q = query(json!({"kind":"settings"}));
    for body in [
        json!({"shadowPalettePresent":false,"shadowColorCount":0}),
        json!({"shadowPalettePresent":true,"shadowColorCount":0,"shadowActiveId":""}),
        json!({"shadowPalettePresent":true,"shadowColorCount":2,"shadowActiveId":"duplicate"}),
    ] {
        assert!(wire(&q, &response(&q, body)));
    }
    assert!(!wire(
        &q,
        &response(
            &q,
            json!({"shadowPalettePresent":false,"shadowColorCount":1})
        )
    ));
    let q = query(json!({"kind":"palettePage","cursor":0,"limit":16}));
    let page = json!({"kind":"shadowPalettePage","shadowPalettePresent":true,"shadowColorCount":3,"cursor":0,
        "items":[{"ordinal":1,"id":"duplicate","color":"#aBc"},{"ordinal":2,"id":"duplicate","color":"#ABC"}],"nextCursor":2});
    assert!(wire(&q, &response(&q, page.clone())));
    for (key, value) in [
        ("nextCursor", json!(0)),
        ("cursor", json!(1)),
        ("shadowPalettePresent", json!(false)),
    ] {
        let mut bad = page.clone();
        bad[key] = value;
        assert!(!wire(&q, &response(&q, bad)));
    }
    let mut bad = page.clone();
    bad["items"][1]["ordinal"] = json!(1);
    assert!(!wire(&q, &response(&q, bad)));
    let mut bad = page;
    bad["items"] = json!([]);
    bad["nextCursor"] = json!(0);
    assert!(!wire(&q, &response(&q, bad)));
    for selector in [
        json!({"kind":"palettePage","cursor":0,"limit":17}),
        json!({"kind":"palettePage","cursor":-1,"limit":16}),
    ] {
        assert!(query(selector).validate_input().is_err());
    }
    let q = query(json!({"kind":"settings"}));
    let r = response(
        &q,
        json!({"shadowPalettePresent":false,"shadowColorCount":0}),
    );
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
    let mut bad = r;
    bad.content_revision = 8;
    assert!(!wire(&q, &bad));
}
#[test]
fn complete_envelope_limit_refuses_large_records_without_truncation() {
    let q = query(json!({"kind":"palettePage","cursor":0,"limit":16}));
    let body = json!({"kind":"shadowPalettePage","shadowPalettePresent":true,"shadowColorCount":3,"cursor":0,
        "items":[{"ordinal":1,"id":"x".repeat(1024),"color":"x".repeat(1024)},
        {"ordinal":2,"id":"x".repeat(1024),"color":"x".repeat(1024)},
        {"ordinal":3,"id":"x".repeat(1024),"color":"x".repeat(1024)}],"nextCursor":null});
    assert!(!wire(&q, &response(&q, body)));
}
#[test]
fn public_descriptor_examples_pass_the_actual_complete_transport_schema() {
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
    let validator = jsonschema::draft202012::options()
        .with_registry(&registry.prepare().unwrap())
        .build(&schema)
        .unwrap();
    let catalog = crate::capabilities::native_catalog();
    let descriptor = catalog.capability_for_operation(READ).unwrap();
    for example in descriptor["examples"].as_array().unwrap() {
        assert!(validator.is_valid(&example["request"]));
        assert!(validator.is_valid(&example["response"]));
    }
}
