//! Common validator cases for the lane-6 object operations, authored UNRUN (no compiler by rule).
use super::*;
use crate::contract::RequestError;
use serde_json::json;

fn stable() -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"legacy layer:alpha","strokeId":"native-raster:3:0"})
}
fn created() -> Value {
    let mut t = stable();
    t["strokeId"] = json!("");
    t
}
fn block() -> Value {
    json!({"text":"Hello world","font":{"kind":"bundled","fontKey":"Roboto-Regular"},"size":17,"color":"#112233",
        "align":"left","fixedWidth":null,"fixedHeight":null,"bold":false,"italic":false,"underline":false,"strike":false,
        "letterSpacing":0,"wordSpacing":0,"lineHeightMult":1.25,"textCase":"none","anchorTopLeft":{"x":10,"y":20}})
}
fn item() -> Value {
    json!({"assetId":"asset-project-asset-1","x":10.5,"y":20,"width":64,"height":48,"opacity":1,"rotation":-12.5,
        "isBitmapBrush":false,"brushGroupId":"g1","isBrushTextureCopy":false,"meshId":"m1"})
}
fn commands() -> Vec<Value> {
    vec![
        json!({"command":"object.raster.create","target":created(),"item":item()}),
        json!({"command":"object.raster.move","stableTarget":stable(),"x":1,"y":2.5}),
        json!({"command":"object.raster.resize","stableTarget":stable(),"width":3,"height":4}),
        json!({"command":"object.raster.rotate","stableTarget":stable(),"rotation":190}),
        json!({"command":"object.raster.opacity","stableTarget":stable(),"opacity":0.5}),
        json!({"command":"object.raster.delete","stableTarget":stable()}),
        json!({"command":"object.text.create","target":created(),"block":block()}),
        json!({"command":"object.text.edit","stableTarget":stable(),"patch":{"fixedWidth":null,"text":"data:text kept opaque"}}),
        json!({"command":"object.text.delete","stableTarget":stable()}),
    ]
}
fn attach() -> Value {
    json!({"documentSnapshotId":"snapshot-8","name":"photo","mediaType":"image/png","source":{"kind":"file","path":"/Users/a/Pictures/photo.png"}})
}
fn remove() -> Value {
    json!({"documentSnapshotId":"snapshot-8","assetId":"asset-project-asset-1"})
}
fn envelope(operation: &str, payload: Value, expected: Option<u64>) -> NativeApplicationRequest {
    NativeApplicationRequest {
        api_version: 2,
        request_id: "item-7".into(),
        instance_id: "instance-A".into(),
        document_id: "document-A".into(),
        expected_revision: expected,
        operation: operation.into(),
        payload,
        cancelled_before_dispatch: false,
    }
}
fn response(
    request: &NativeApplicationRequest,
    revision: u64,
    result: Value,
) -> NativeApplicationResponse {
    NativeApplicationResponse {
        api_version: 2,
        request_id: request.request_id.clone(),
        instance_id: request.instance_id.clone(),
        document_id: request.document_id.clone(),
        content_revision: revision,
        ok: true,
        result: Some(result),
        error: None,
    }
}

#[test]
fn every_item_command_and_both_asset_commands_are_admitted_in_their_closed_shape() {
    for payload in commands() {
        assert!(request(ITEM, &payload), "{payload}");
    }
    assert!(request(ATTACH, &attach()));
    assert!(request(REMOVE, &remove()));
}

#[test]
fn a_missing_null_or_extra_member_refuses_every_command() {
    for payload in commands().into_iter().chain([attach(), remove()]) {
        let operation = if payload.get("command").is_some() {
            ITEM
        } else if payload.get("name").is_some() {
            ATTACH
        } else {
            REMOVE
        };
        for key in payload.as_object().unwrap().keys() {
            let mut missing = payload.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(!request(operation, &missing), "missing {key}: {payload}");
            let mut null = payload.clone();
            null[key] = Value::Null;
            assert!(!request(operation, &null), "null {key}: {payload}");
        }
        let mut extra = payload.clone();
        extra["extra"] = json!(1);
        assert!(!request(operation, &extra), "extra: {payload}");
    }
    assert!(!request(ITEM, &json!({"command":"object.vector.create"})));
    assert!(!request(ITEM, &json!(["object.raster.delete"])));
}

#[test]
fn targets_keep_a_created_item_unnamed_and_every_other_target_named() {
    let mut named = commands()[0].clone();
    named["target"]["strokeId"] = json!("native-raster:3:0");
    assert!(!request(ITEM, &named));
    let mut unnamed = commands()[1].clone();
    unnamed["stableTarget"]["strokeId"] = json!("");
    assert!(!request(ITEM, &unnamed));
    for (path, bad) in [
        ("contextId", json!("other")),
        ("layerUid", json!("")),
        ("frameScope", json!({"kind":"nope","frame":0})),
        (
            "frameScope",
            json!({"kind":"authored","frame":4294967296_u64}),
        ),
        ("frameScope", json!({"kind":"authored","frame":-1})),
    ] {
        let mut value = commands()[1].clone();
        value["stableTarget"][path] = bad;
        assert!(!request(ITEM, &value), "{path}");
    }
}

#[test]
fn raster_item_and_numeric_members_keep_their_json_types() {
    for (key, bad) in [
        ("x", json!("1")),
        ("opacity", Value::Null),
        ("rotation", json!(true)),
        ("isBitmapBrush", json!(0)),
        ("brushGroupId", json!("")),
        ("meshId", json!(1)),
        ("assetId", json!(" ")),
        ("note", json!(1)),
    ] {
        let mut payload = commands()[0].clone();
        payload["item"][key] = bad;
        assert!(!request(ITEM, &payload), "{key}");
    }
    let mut minimal = commands()[0].clone();
    minimal["item"] = json!({"assetId":"a","x":0,"y":0,"width":1,"height":1});
    assert!(request(ITEM, &minimal));
    for (index, key) in [(1, "x"), (2, "height"), (3, "rotation"), (4, "opacity")] {
        let mut payload = commands()[index].clone();
        payload[key] = json!("1");
        assert!(!request(ITEM, &payload), "{key}");
    }
}

#[test]
fn text_blocks_and_patches_are_closed_and_a_patch_never_carries_the_anchor() {
    for (key, bad) in [
        ("text", json!("")),
        ("text", json!("a\u{0}b")),
        ("font", json!({"kind":"google","family":"Roboto"})),
        ("font", json!({"kind":"bundled","fontKey":"Comic-Regular"})),
        ("font", json!({"kind":"asset"})),
        ("color", json!("#12")),
        ("color", json!("112233")),
        ("align", json!("justify")),
        ("textCase", json!("Upper")),
        ("fixedWidth", json!("120")),
        ("bold", json!(1)),
        ("anchorTopLeft", json!({"x":1})),
        ("isText", json!(true)),
        ("textOverflow", json!(false)),
    ] {
        let mut payload = commands()[6].clone();
        payload["block"][key] = bad.clone();
        assert!(!request(ITEM, &payload), "block {key} {bad}");
    }
    let mut asset_font = commands()[6].clone();
    asset_font["block"]["font"] = json!({"kind":"asset","assetId":"font-1"});
    assert!(request(ITEM, &asset_font));
    for patch in [
        json!({}),
        json!({"anchorTopLeft":{"x":1,"y":2}}),
        json!({"isText":true}),
        json!({"text":""}),
        json!({"size":"9"}),
    ] {
        let mut payload = commands()[7].clone();
        payload["patch"] = patch.clone();
        assert!(!request(ITEM, &payload), "patch {patch}");
    }
    let mut all_members = commands()[7].clone();
    all_members["patch"] = json!({"text":"x","font":{"kind":"bundled","fontKey":"Inter-Regular"},"size":2,"color":"#11223344","align":"right",
        "fixedWidth":3,"fixedHeight":null,"bold":true,"italic":true,"underline":true,"strike":true,"letterSpacing":-1,"wordSpacing":1,
        "lineHeightMult":2,"textCase":"capitalize"});
    assert!(request(ITEM, &all_members));
}

#[test]
fn asset_attach_names_a_file_source_and_never_carries_bytes() {
    for (path, bad) in [
        ("mediaType", json!("application/pdf")),
        ("name", json!("  ")),
        ("name", json!("a\u{0}")),
        ("documentSnapshotId", json!("")),
        ("documentSnapshotId", json!("-lead")),
        ("source", json!({"kind":"url","path":"/x"})),
        ("source", json!({"kind":"file","path":""})),
        ("source", json!({"kind":"file","path":"/x\u{0}"})),
        ("source", json!({"kind":"file","path":"/x","bytes":"QUJD"})),
        ("source", json!({"kind":"file","path":"x".repeat(4097)})),
        ("base64", json!("QUJD")),
    ] {
        let mut payload = attach();
        payload[path] = bad.clone();
        assert!(!request(ATTACH, &payload), "{path} {bad}");
    }
    let mut jpeg = attach();
    jpeg["mediaType"] = json!("image/jpeg");
    assert!(request(ATTACH, &jpeg));
    let mut bad_remove = remove();
    bad_remove["assetId"] = json!("");
    assert!(!request(REMOVE, &bad_remove));
}

#[test]
fn item_receipts_name_the_operation_and_the_stroke_they_ran_on() {
    let all = commands();
    let names = [
        "raster.create",
        "raster.move",
        "raster.resize",
        "raster.rotate",
        "raster.opacity",
        "raster.delete",
        "text.create",
        "text.edit",
        "text.delete",
    ];
    for (payload, name) in all.iter().zip(names) {
        let stroke = if name.ends_with(".create") {
            if name.starts_with("raster.") {
                "native-raster:8:0"
            } else {
                "native-text:8:0"
            }
        } else {
            "native-raster:3:0"
        };
        let good =
            json!({"applied":true,"historyEntriesAdded":1,"operation":name,"strokeId":stroke});
        assert!(result(ITEM, payload, &good), "{name}");
        for bad in [
            json!({"applied":true,"historyEntriesAdded":0,"operation":name,"strokeId":stroke}),
            json!({"applied":false,"historyEntriesAdded":1,"operation":name,"strokeId":stroke}),
            json!({"applied":true,"historyEntriesAdded":1,"operation":"raster.nudge","strokeId":stroke}),
            json!({"applied":true,"historyEntriesAdded":1,"operation":name,"strokeId":"other:9:9"}),
            json!({"applied":true,"historyEntriesAdded":1,"operation":name,"strokeId":stroke,"document":{}}),
        ] {
            assert!(!result(ITEM, payload, &bad), "{name} {bad}");
        }
        let other = if name == "raster.move" {
            "raster.resize"
        } else {
            "raster.move"
        };
        assert!(!result(
            ITEM,
            payload,
            &json!({"applied":true,"historyEntriesAdded":1,"operation":other,"strokeId":stroke})
        ));
    }
    // A created item reports an allocated id of its own family: a text id for a raster create is a different record kind.
    assert!(!result(
        ITEM,
        &all[0],
        &json!({"applied":true,"historyEntriesAdded":1,"operation":"raster.create","strokeId":"native-text:8:0"})
    ));
    assert!(result(
        ITEM,
        &all[1],
        &json!({"applied":false,"historyEntriesAdded":0,"operation":"raster.move","strokeId":"native-raster:3:0"})
    ));
    for operation in [ATTACH, REMOVE] {
        assert!(result(
            operation,
            &Value::Null,
            &json!({"applied":true,"historyEntriesAdded":1})
        ));
        assert!(result(
            operation,
            &Value::Null,
            &json!({"applied":false,"historyEntriesAdded":0})
        ));
        assert!(!result(
            operation,
            &Value::Null,
            &json!({"applied":true,"historyEntriesAdded":0})
        ));
        assert!(!result(
            operation,
            &Value::Null,
            &json!({"applied":true,"historyEntriesAdded":1,"assetId":"asset-x"})
        ));
    }
}

#[test]
fn a_response_advances_the_revision_by_its_own_disposition_and_stays_in_the_envelope() {
    let q = envelope(ITEM, commands()[1].clone(), Some(7));
    let changed = json!({"applied":true,"historyEntriesAdded":1,"operation":"raster.move","strokeId":"native-raster:3:0"});
    let noop = json!({"applied":false,"historyEntriesAdded":0,"operation":"raster.move","strokeId":"native-raster:3:0"});
    assert!(response_matches(&q, &response(&q, 8, changed.clone())));
    assert!(response_matches(&q, &response(&q, 7, noop.clone())));
    assert!(!response_matches(&q, &response(&q, 7, changed.clone())));
    assert!(!response_matches(&q, &response(&q, 8, noop)));
    let mut other_request = response(&q, 8, changed.clone());
    other_request.request_id = "item-8".into();
    assert!(!response_matches(&q, &other_request));
    let mut foreign = response(&q, 8, changed.clone());
    foreign.instance_id = "instance-B".into();
    assert!(!response_matches(&q, &foreign));
    foreign = response(&q, 8, changed.clone());
    foreign.document_id = "document-B".into();
    assert!(!response_matches(&q, &foreign));
    let mut missing = response(&q, 8, changed.clone());
    missing.result = None;
    assert!(!response_matches(&q, &missing));
    let mut unbounded = changed;
    unbounded["pad"] = json!("x".repeat(NATIVE_MAX_MESSAGE_BYTES));
    assert!(!response_matches(&q, &response(&q, 8, unbounded)));
    let near_max = envelope(REMOVE, remove(), Some(OVERFLOWING_REVISION));
    assert!(!response_matches(
        &near_max,
        &response(
            &near_max,
            0,
            json!({"applied":true,"historyEntriesAdded":1})
        )
    ));
    let other = envelope("query.document.object", json!({}), None);
    assert!(response_matches(&other, &response(&other, 0, json!({}))));
}

const OVERFLOWING_REVISION: u64 = u64::MAX;

#[test]
fn registration_declares_item_and_png_attach_available_but_asset_remove_unavailable() {
    let catalog = crate::capabilities::native_catalog();
    assert_eq!(
        catalog.capability_for_operation(ITEM).unwrap()["id"],
        "native.object.item"
    );
    assert_eq!(
        catalog.capability_for_operation(ITEM).unwrap()["authority"]["fallback"],
        "none"
    );
    let attach_descriptor = catalog.capability_for_operation(ATTACH).unwrap();
    assert_eq!(attach_descriptor["id"], "native.object.asset");
    assert_eq!(attach_descriptor["availability"]["state"], "available");
    assert_eq!(attach_descriptor["operations"], json!([ATTACH]));
    assert_eq!(
        attach_descriptor["input"]["$ref"],
        "../native-object-item-v1.schema.json#/$defs/ImageImportAttach"
    );
    let remove_descriptor = catalog.capability_for_operation(REMOVE).unwrap();
    assert_eq!(remove_descriptor["id"], "native.object.asset.remove");
    assert_eq!(remove_descriptor["availability"]["state"], "unavailable");
    let remove_reason = remove_descriptor["availability"]["reason"]
        .as_str()
        .unwrap();
    assert!(!remove_reason.is_empty());
    assert!(crate::capabilities::validate_native_operation(ITEM).is_ok());
    assert!(crate::capabilities::validate_native_operation(ATTACH).is_ok());
    let refused = crate::capabilities::validate_native_operation(REMOVE).unwrap_err();
    assert!(matches!(refused, RequestError::Unavailable(reason) if reason == remove_reason));
}

#[test]
fn the_envelope_requires_a_revision_and_the_closed_payload() {
    for (operation, payload) in [
        (ITEM, commands()[1].clone()),
        (ATTACH, attach()),
        (REMOVE, remove()),
    ] {
        assert!(envelope(operation, payload.clone(), Some(7))
            .validate_input()
            .is_ok());
        assert!(envelope(operation, payload.clone(), None)
            .validate_input()
            .is_err());
        let mut open = payload;
        open["extra"] = json!(true);
        assert!(envelope(operation, open, Some(7)).validate_input().is_err());
    }
    assert!(envelope(ITEM, commands()[1].clone(), Some(7))
        .validate()
        .is_ok());
    // Common shape/declaration admission only; catalog-only host execution
    // remains unavailable and refuses before source I/O.
    assert!(envelope(ATTACH, attach(), Some(7)).validate().is_ok());
    let mut placed_png = attach();
    placed_png["placement"] = json!({"kind":"newLayer"});
    placed_png["lifecycleGeneration"] = json!(1);
    assert!(envelope(ATTACH, placed_png, Some(7)).validate().is_ok());
    assert!(matches!(
        envelope(REMOVE, remove(), Some(7)).validate(),
        Err(RequestError::Unavailable(_))
    ));
}

#[test]
fn a_blank_opaque_string_is_refused_and_a_host_path_is_only_non_empty() {
    for blank in ["", " ", "  ", "\t", "\n"] {
        let mut layer = commands()[1].clone();
        layer["stableTarget"]["layerUid"] = json!(blank);
        assert!(!request(ITEM, &layer), "layerUid {blank:?}");
        let mut stroke = commands()[1].clone();
        stroke["stableTarget"]["strokeId"] = json!(blank);
        assert!(!request(ITEM, &stroke), "strokeId {blank:?}");
        let mut brush = commands()[0].clone();
        brush["item"]["brushGroupId"] = json!(blank);
        assert!(!request(ITEM, &brush), "brushGroupId {blank:?}");
        let mut name = attach();
        name["name"] = json!(blank);
        assert!(!request(ATTACH, &name), "name {blank:?}");
        let mut asset = remove();
        asset["assetId"] = json!(blank);
        assert!(!request(REMOVE, &asset), "assetId {blank:?}");
    }
    let mut path = attach();
    path["source"]["path"] = json!(" ");
    assert!(request(ATTACH, &path), "the host path is not trimmed");
    path["source"]["path"] = json!("");
    assert!(!request(ATTACH, &path));
    let mut edge = commands()[1].clone();
    edge["stableTarget"]["layerUid"] = json!("a".repeat(255));
    assert!(request(ITEM, &edge), "255 bytes");
    edge["stableTarget"]["layerUid"] = json!("a".repeat(256));
    assert!(!request(ITEM, &edge), "256 bytes");
    edge["stableTarget"]["layerUid"] = json!("\u{e9}".repeat(128));
    assert!(
        !request(ITEM, &edge),
        "128 two-byte characters are 256 bytes"
    );
}

#[test]
fn the_shared_result_check_routes_the_item_operations_through_this_module() {
    let payload = commands()[1].clone();
    let good = json!({"applied":true,"historyEntriesAdded":1,"operation":"raster.move","strokeId":"native-raster:3:0"});
    assert!(crate::native_contract::validate_result(
        ITEM,
        "document-A",
        &payload,
        &good
    ));
    for flawed in [
        json!({"applied":true,"historyEntriesAdded":1,"operation":"raster.resize","strokeId":"native-raster:3:0"}),
        json!({"applied":true,"historyEntriesAdded":1,"operation":"raster.move","strokeId":"native-raster:9:9"}),
        json!({"applied":true,"historyEntriesAdded":0,"operation":"raster.move","strokeId":"native-raster:3:0"}),
    ] {
        assert!(
            !crate::native_contract::validate_result(ITEM, "document-A", &payload, &flawed),
            "{flawed}"
        );
    }
    // The request is validated first: a payload outside the closed shape never reaches the receipt check.
    let open = json!({"command":"object.vector.create"});
    assert!(!crate::native_contract::validate_result(
        ITEM,
        "document-A",
        &open,
        &good
    ));
    let receipt = json!({"applied":true,"historyEntriesAdded":1});
    assert!(crate::native_contract::validate_result(
        ATTACH,
        "document-A",
        &attach(),
        &receipt
    ));
    assert!(crate::native_contract::validate_result(
        REMOVE,
        "document-A",
        &remove(),
        &receipt
    ));
    assert!(!crate::native_contract::validate_result(
        ATTACH,
        "document-A",
        &attach(),
        &json!({"applied":true,"historyEntriesAdded":1,"assetId":"asset-x"})
    ));
}
