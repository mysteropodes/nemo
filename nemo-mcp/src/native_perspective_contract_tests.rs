use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::json;
fn query(selector: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"perspective-read","instanceId":"i","documentId":"d",
        "operation":READ,"payload":{"atRevision":7,"documentSnapshotId":"s7","contextId":"scene-root","selector":selector}})).unwrap()
}
fn edit(action: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"perspective-edit","instanceId":"i","documentId":"d",
        "operation":EDIT,"expectedRevision":7,"payload":{"command":"object.project.perspective.edit","action":action}})).unwrap()
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
fn settings() -> Value {
    json!({"perspectiveVPsPresent":false,"perspectiveVPsNull":false,"perspectiveVPCount":0,"effectiveVPCount":2,"vpSource":"default"})
}
fn page() -> Value {
    json!({"perspectiveVPsPresent":false,"perspectiveVPsNull":false,"perspectiveVPCount":0,"effectiveVPCount":2,"vpSource":"default",
        "kind":"perspectiveVPPage","cursor":0,"items":[{"ordinal":1,"x":-48,"y":90,"locked":false},
        {"ordinal":2,"x":368,"y":90,"locked":false}],"nextCursor":null})
}
#[test]
fn six_original_gestures_have_closed_shapes_and_exact_revision_admission() {
    for action in [
        json!({"kind":"setEnabled","enabled":true}),
        json!({"kind":"setMode","mode":"fisheye"}),
        json!({"kind":"reset"}),
        json!({"kind":"lockAll","locked":false}),
        json!({"kind":"move","ordinal":1,"x":-0.25,"y":2.5}),
        json!({"kind":"translate","dx":-2,"dy":1.25}),
    ] {
        let q = edit(action);
        assert!(q.validate_input().is_ok());
        assert!(q.validate().is_ok());
    }
    for action in [
        json!({"kind":"setDensity","density":24}),
        json!({"kind":"reset","canvasW":320}),
        json!({"kind":"reset","points":[]}),
        json!({"kind":"move","ordinal":0,"x":1,"y":2}),
        json!({"kind":"move","ordinal":1.5,"x":1,"y":2}),
        json!({"kind":"setMode","mode":"4pt"}),
        json!({"kind":"setEnabled","enabled":null}),
    ] {
        assert!(edit(action).validate_input().is_err());
    }
    let mut q = edit(json!({"kind":"reset"}));
    q.expected_revision = None;
    assert!(q.validate_input().is_err());
    let mut q = query(json!({"kind":"settings"}));
    q.expected_revision = Some(7);
    assert!(q.validate_input().is_err());
}
#[test]
fn raw_presence_null_and_unknown_inert_scalars_are_not_normalized_by_effective_defaults() {
    let q = query(json!({"kind":"settings"}));
    assert!(wire(&q, &response(&q, settings())));
    let mut raw = settings();
    raw["perspectiveVPsPresent"] = json!(true);
    raw["perspectiveVPsNull"] = json!(true);
    raw["perspectiveMode"] = json!("unknown-import");
    raw["perspectiveDensity"] = json!(-0.125);
    assert!(wire(&q, &response(&q, raw.clone())));
    for (key, value) in [
        ("perspectiveVPsPresent", json!(false)),
        ("vpSource", json!("stored")),
        ("effectiveVPCount", json!(3)),
    ] {
        let mut bad = raw.clone();
        bad[key] = value;
        assert!(!wire(&q, &response(&q, bad)));
    }
    let q = query(json!({"kind":"vpPage","cursor":0,"limit":16}));
    let body = page();
    assert!(wire(&q, &response(&q, body.clone())));
    let mut stored = body.clone();
    stored["vpSource"] = json!("stored");
    stored["perspectiveVPsPresent"] = json!(true);
    stored["perspectiveVPCount"] = json!(2);
    stored["items"][0].as_object_mut().unwrap().remove("locked");
    assert!(wire(&q, &response(&q, stored)));
    for mutate in [0, 1, 2, 3] {
        let mut bad = body.clone();
        match mutate {
            0 => bad["items"][0]["locked"] = json!(true),
            1 => bad["items"][1]["ordinal"] = json!(1),
            2 => bad["nextCursor"] = json!(0),
            _ => {
                bad["items"] = json!([]);
                bad["nextCursor"] = json!(0);
            }
        }
        assert!(!wire(&q, &response(&q, bad)));
    }
    assert!(query(json!({"kind":"vpPage","cursor":0,"limit":17}))
        .validate_input()
        .is_err());
}
#[test]
fn owner_snapshot_full_envelope_and_exact_history_receipt_are_correlated() {
    let q = query(json!({"kind":"settings"}));
    let r = response(&q, settings());
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
    let q = edit(json!({"kind":"reset"}));
    let mut r = response(&q, json!({"applied":true,"historyEntriesAdded":1}));
    r.content_revision = 8;
    assert!(wire(&q, &r));
    r.content_revision = 7;
    assert!(!wire(&q, &r));
    assert!(wire(
        &q,
        &response(&q, json!({"applied":false,"historyEntriesAdded":0}))
    ));
    let q = query(json!({"kind":"settings"}));
    let mut large = settings();
    large["perspectiveMode"] = json!("\u{1}".repeat(1024));
    assert!(!wire(&q, &response(&q, large)));
}
#[test]
fn public_examples_validate_without_dropping_guide_shadow_or_any_earlier_schema_branch() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .unwrap();
    let branches = schema["$defs"]["ResponseBase"]["properties"]["result"]["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(branches.len(), 37);
    assert_eq!(
        branches[30],
        json!({"$ref":"#/$defs/ObjectProjectGuideReadResult"})
    );
    assert_eq!(
        branches[31],
        json!({"$ref":"#/$defs/ObjectProjectShadowReadResult"})
    );
    assert_eq!(
        branches[32],
        json!({"$ref":"#/$defs/ObjectProjectPerspectiveReadResult"})
    );
    assert_eq!(
        branches[33],
        json!({"$ref":"#/$defs/ObjectProjectWorkspaceReadResult"})
    );
    assert_eq!(
        branches[34],
        json!({"$ref":"#/$defs/ObjectProjectCustomBrushPresetReadResult"})
    );
    assert_eq!(
        branches[35],
        json!({"$ref":"#/$defs/ObjectProjectCommentsReadResult"})
    );
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
    for example in descriptor["examples"].as_array().unwrap() {
        assert!(validator.is_valid(&example["request"]));
        assert!(validator.is_valid(&example["response"]));
    }
}
