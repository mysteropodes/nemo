use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::json;

fn request(op: &str, payload: Value) -> NativeApplicationRequest {
    let mut value = json!({"apiVersion":2,"requestId":"palette-q","instanceId":"instance-A",
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
    json!({"palettesPresent":false,"paletteCount":0,"effectiveActivePaletteIdx":0})
}

#[test]
fn all_eight_palette_actions_are_bounded_field_intents_with_a_separate_closed_namespace() {
    for action in [
        json!({"kind":"create","id":"","name":"","color":"#AbC"}),
        json!({"kind":"rename","ordinal":1,"name":"Name"}),
        json!({"kind":"duplicate","ordinal":1,"id":"duplicate-id","name":"Copy"}),
        json!({"kind":"remove","ordinal":2}),
        json!({"kind":"select","ordinal":2}),
        json!({"kind":"appendColor","ordinal":1,"color":"#abc"}),
        json!({"kind":"removeColor","ordinal":1,"index":2}),
        json!({"kind":"moveColor","ordinal":1,"from":3,"to":1}),
    ] {
        let q = request(
            EDIT,
            json!({"command":"object.project.palette.edit","action":action}),
        );
        assert!(q.validate_input().is_ok(), "{}", q.payload);
        assert!(q.validate().is_ok());
    }
    for action in [
        json!({"kind":"replacePalettes","palettes":[]}),
        json!({"kind":"select","ordinal":0}),
        json!({"kind":"select","ordinal":1.5}),
        json!({"kind":"select","ordinal":9_007_199_254_740_992u64}),
        json!({"kind":"rename","ordinal":1,"name":"x","colors":[]}),
        json!({"kind":"create","id":"p","name":"P","color":null}),
        json!({"kind":"appendColor","ordinal":1,"color":"\0"}),
        json!({"kind":"appendColor","ordinal":1,"color":"😀".repeat(257)}),
    ] {
        assert!(request(
            EDIT,
            json!({"command":"object.project.palette.edit","action":action})
        )
        .validate_input()
        .is_err());
    }
    let mut wrong = request(
        EDIT,
        json!({"command":"object.project.settings.edit","action":{"kind":"select","ordinal":1}}),
    );
    assert!(wrong.validate_input().is_err());
    wrong.payload["command"] = json!("object.project.palette.edit");
    wrong.expected_revision = None;
    assert!(wrong.validate_input().is_err());
}

#[test]
fn settings_keep_absence_fractional_negative_and_upper_bound_selection_semantics() {
    let q = query(json!({"kind":"settings"}));
    assert!(q.validate_input().is_ok());
    assert!(q.validate().is_ok());
    for body in [
        settings_value(),
        json!({"palettesPresent":false,"paletteCount":0,"activePaletteIdx":0.5,"effectiveActivePaletteIdx":0.5}),
        json!({"palettesPresent":true,"paletteCount":0,"activePaletteIdx":-1.5,"effectiveActivePaletteIdx":-1.5}),
        json!({"palettesPresent":true,"paletteCount":2,"activePaletteIdx":8,"effectiveActivePaletteIdx":0}),
    ] {
        assert!(wire(&q, &response(&q, body)));
    }
    for body in [
        json!({"palettesPresent":false,"paletteCount":1,"effectiveActivePaletteIdx":0}),
        json!({"palettesPresent":false,"paletteCount":0,"activePaletteIdx":0.5,"effectiveActivePaletteIdx":0}),
        json!({"palettesPresent":true,"paletteCount":2,"activePaletteIdx":null,"effectiveActivePaletteIdx":0}),
        json!({"palettesPresent":false,"paletteCount":0,"effectiveActivePaletteIdx":0,"palettes":[]}),
    ] {
        assert!(!wire(&q, &response(&q, body)));
    }
    let mut revision = q;
    revision.expected_revision = Some(7);
    assert!(revision.validate_input().is_err());
}

#[test]
fn palette_and_color_pages_correlate_ordinals_cursor_counts_and_real_forward_progress() {
    let q = query(json!({"kind":"palettePage","cursor":0,"limit":16}));
    let body = json!({"kind":"palettePage","palettesPresent":true,"paletteCount":3,"cursor":0,
        "items":[{"ordinal":1,"id":"p","name":"Same","colorCount":3},
            {"ordinal":2,"id":"p","name":"Same","colorCount":0}],"nextCursor":2});
    assert!(wire(&q, &response(&q, body.clone())));
    for (key, value) in [
        ("nextCursor", json!(1)),
        ("cursor", json!(1)),
        ("paletteCount", json!(1)),
    ] {
        let mut bad = body.clone();
        bad[key] = value;
        assert!(!wire(&q, &response(&q, bad)));
    }
    let mut skipped = body.clone();
    skipped["items"][1]["ordinal"] = json!(3);
    assert!(!wire(&q, &response(&q, skipped)));
    let q = query(json!({"kind":"colorPage","ordinal":2,"cursor":1,"limit":16}));
    let body = json!({"kind":"paletteColorPage","paletteOrdinal":2,"colorCount":3,"cursor":1,
        "items":[{"index":2,"color":"#AbC"},{"index":3,"color":"#abc"}],"nextCursor":null});
    assert!(wire(&q, &response(&q, body.clone())));
    let mut wrong = body.clone();
    wrong["paletteOrdinal"] = json!(1);
    assert!(!wire(&q, &response(&q, wrong)));
    let mut stuck = body;
    stuck["items"] = json!([]);
    stuck["nextCursor"] = json!(1);
    assert!(!wire(&q, &response(&q, stuck)));
}

#[test]
fn public_read_wire_rejects_foreign_request_owner_snapshot_and_inner_revision() {
    let q = query(json!({"kind":"settings"}));
    let good = response(&q, settings_value());
    assert!(wire(&q, &good));
    for key in ["requestId", "instanceId", "documentId"] {
        let mut value = serde_json::to_value(&good).unwrap();
        value[key] = json!("foreign");
        assert!(!wire(&q, &serde_json::from_value(value).unwrap()));
    }
    for key in ["instanceId", "documentId", "documentSnapshotId"] {
        let mut value = serde_json::to_value(&good).unwrap();
        value["result"][key] = json!("foreign");
        assert!(!wire(&q, &serde_json::from_value(value).unwrap()));
    }
    let mut future = serde_json::to_value(&good).unwrap();
    future["result"]["atRevision"] = json!(8);
    assert!(!wire(&q, &serde_json::from_value(future).unwrap()));
}

#[test]
fn edit_revision_receipts_and_the_complete_read_response_stay_inside_existing_bounds() {
    let q = request(
        EDIT,
        json!({"command":"object.project.palette.edit","action":{"kind":"select","ordinal":1}}),
    );
    let mut applied = response(&q, json!({"applied":true,"historyEntriesAdded":1}));
    applied.content_revision = 8;
    assert!(wire(&q, &applied));
    applied.content_revision = 9;
    assert!(!wire(&q, &applied));
    let noop = response(&q, json!({"applied":false,"historyEntriesAdded":0}));
    assert!(wire(&q, &noop));
    let q = query(json!({"kind":"colorPage","ordinal":1,"cursor":0,"limit":16}));
    let items: Vec<Value> = (0..16)
        .map(|i| json!({"index":i+1,"color":"x".repeat(1024)}))
        .collect();
    assert!(!wire(
        &q,
        &response(
            &q,
            json!({"kind":"paletteColorPage","paletteOrdinal":1,
        "colorCount":16,"cursor":0,"items":items,"nextCursor":null})
        )
    ));
}

#[test]
fn shipped_schema_and_descriptor_include_palette_without_dropping_prior_result_branches() {
    let schema: Value = serde_json::from_str(include_str!(
        "../../engineering/application/native-transport-v2.schema.json"
    ))
    .unwrap();
    let branches = schema["$defs"]["ResponseBase"]["properties"]["result"]["oneOf"]
        .as_array()
        .unwrap();
    assert_eq!(branches.len(), 41);
    assert_eq!(
        branches[37],
        json!({"$ref":"#/$defs/ObjectHistoryReadResult"})
    );
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
    assert_eq!(
        branches[27],
        json!({"$ref":"#/$defs/ObjectFrameBulkMoveResult"})
    );
    assert_eq!(
        branches[28],
        json!({"$ref":"#/$defs/ObjectProjectCompositionReadResult"})
    );
    assert_eq!(
        branches[29],
        json!({"$ref":"#/$defs/ObjectProjectPaletteReadResult"})
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
    assert_eq!(descriptor["availability"]["state"], "available");
    for example in descriptor["examples"].as_array().unwrap() {
        assert!(validator.is_valid(&example["request"]));
        assert!(validator.is_valid(&example["response"]));
    }
}
