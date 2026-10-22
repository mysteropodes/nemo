//! Added import-family controls; all original vector contract tests remain in their unchanged child.
use super::*;
use serde_json::json;

fn examples() -> Vec<Value> {
    crate::capabilities::native_catalog()
        .capability_for_operation(OP)
        .unwrap()["examples"]
        .as_array()
        .unwrap()
        .iter()
        .skip(1)
        .take(3)
        .cloned()
        .collect()
}

fn mixed() -> Value {
    let examples = examples();
    let mut payload = examples[0]["request"]["payload"].clone();
    payload["text"] = examples[1]["request"]["payload"]["text"].clone();
    payload["assets"]
        .as_array_mut()
        .unwrap()
        .push(examples[1]["request"]["payload"]["assets"][0].clone());
    payload
}

fn mixed_receipt() -> Value {
    json!({"applied":true,"historyEntriesAdded":1,"layers":[],"skipped":[],
        "objects":[{"sourceId":"raster-source","strokeId":"native-raster"},
            {"sourceId":"text-source","strokeId":"native-text"}],
        "assets":[{"sourceId":"image-source","assetId":"native-image"},
            {"sourceId":"font-source","assetId":"native-font"}]})
}

#[test]
fn actual_retained_image_font_and_bundled_text_examples_use_the_public_wire() {
    let examples = examples();
    assert_eq!(examples.len(), 3);
    for example in examples {
        let command: NativeApplicationRequest =
            serde_json::from_value(example["request"].clone()).unwrap();
        let response: NativeApplicationResponse =
            serde_json::from_value(example["response"].clone()).unwrap();
        assert!(command.validate().is_ok());
        assert!(response_matches(&command, &response));
        assert!(crate::wire::validate_native_response(&response, &command).is_ok());
    }
    assert!(request(
        &json!({"command":"object.import.batch","layers":[],"records":[],
        "assets":[],"raster":[],"text":[]})
    ));
}

#[test]
fn descriptors_are_closed_scalar_retained_lookups_never_forged_resource_records() {
    for key in [
        "record",
        "source",
        "bytes",
        "path",
        "mediaType",
        "hasFontAdmission",
    ] {
        let mut payload = mixed();
        payload["assets"][0][key] = json!("forged");
        assert!(!request(&payload), "{key}");
    }
    for key in ["assets", "raster", "text"] {
        let mut payload = mixed();
        payload[key] = Value::Null;
        assert!(!request(&payload), "{key}");
    }
    for invalid in [
        json!(""),
        json!(" "),
        json!("\u{85}"),
        json!("a\0b"),
        json!("é".repeat(128)),
        json!({}),
    ] {
        let mut payload = mixed();
        payload["assets"][0]["assetId"] = invalid;
        assert!(!request(&payload));
    }
    let mut payload = mixed();
    payload["assets"][1]["sourceId"] = json!("image-source");
    assert!(!request(&payload));
    let mut payload = mixed();
    payload["text"][0]["sourceId"] = json!("raster-source");
    assert!(!request(&payload));
    for (kind, source) in [("asset", "image-source"), ("element", "text-source")] {
        let mut payload = mixed();
        payload["skipped"] = json!([{"kind":kind,"sourceId":source,"reason":"skip"}]);
        assert!(!request(&payload));
    }
}

#[test]
fn native_item_fields_presence_whitespace_and_authored_target_rules_are_preserved() {
    for (path, value) in [
        ("/raster/0/entry/item/width", json!(0)),
        ("/raster/0/entry/item/height", json!(-1)),
        ("/raster/0/entry/item/opacity", json!(1.1)),
        ("/text/0/entry/block/text", json!(" \u{feff}")),
        ("/text/0/entry/block/text", json!("a\0b")),
        ("/text/0/entry/block/size", json!(0)),
        ("/text/0/entry/block/fixedWidth", json!(0)),
        ("/text/0/entry/block/bold", Value::Null),
        (
            "/text/0/entry/block/font",
            json!({"kind":"asset","assetId":"font-source","bytes":[]}),
        ),
    ] {
        let mut payload = mixed();
        *payload.pointer_mut(path).unwrap() = value;
        assert!(!request(&payload), "{path}");
    }
    let mut payload = mixed();
    payload["text"][0]["entry"]["block"]["text"] = json!("\u{85}");
    payload["text"][0]["entry"]["block"]["wordSpacing"] = json!(0);
    payload["raster"][0]["entry"]["item"]["brushGroupId"] = json!(" ");
    assert!(request(&payload));
    for (field, value) in [
        ("lineHeightMult", json!(0)),
        ("fixedHeight", json!(-2)),
        ("textOverflow", json!(false)),
        ("textCase", json!("title")),
    ] {
        let mut payload = mixed();
        payload["text"][0]["entry"]["block"][field] = value;
        assert!(!request(&payload), "{field}");
    }
    for key in ["raster", "text"] {
        let mut payload = mixed();
        payload[key][0]["entry"]["target"]["contextId"] = json!("component:import");
        assert!(request(&payload));
        payload[key][0]["entry"]["target"]["strokeId"] = json!("chosen");
        assert!(!request(&payload));
        payload[key][0]["entry"]["target"]["strokeId"] = json!("");
        payload[key][0]["entry"]["target"]["frameScope"]["kind"] = json!("reference");
        assert!(!request(&payload));
    }
}

#[test]
fn asset_and_item_success_accounts_for_every_declared_source_and_one_checkpoint() {
    let payload = mixed();
    let receipt = mixed_receipt();
    assert!(receipt_matches(&payload, &receipt));
    for mutate in [
        |r: &mut Value| {
            r.as_object_mut().unwrap().remove("assets");
        },
        |r: &mut Value| {
            r["assets"].as_array_mut().unwrap().pop();
        },
        |r: &mut Value| {
            r["assets"][1]["sourceId"] = json!("image-source");
        },
        |r: &mut Value| {
            r["assets"][0]["sourceId"] = json!("foreign");
        },
        |r: &mut Value| {
            r["assets"][1]["assetId"] = json!("native-image");
        },
        |r: &mut Value| {
            r["objects"].as_array_mut().unwrap().pop();
        },
        |r: &mut Value| {
            r["objects"][1]["strokeId"] = json!("native-raster");
        },
    ] {
        let mut bad = receipt.clone();
        mutate(&mut bad);
        assert!(result(&bad));
        assert!(!receipt_matches(&payload, &bad));
    }
    let assets_only = json!({"command":"object.import.batch","layers":[],"records":[],
        "assets":payload["assets"]});
    let mut receipt = receipt;
    receipt["objects"] = json!([]);
    assert!(receipt_matches(&assets_only, &receipt));
    assert!(revision_matches(Some(7), 8, &receipt));
    assert!(!revision_matches(Some(7), 7, &receipt));
    assert!(!receipt_matches(
        &assets_only,
        &json!({"applied":false,
        "historyEntriesAdded":0,"layers":[],"objects":[],"assets":[],"skipped":[]})
    ));
}

#[test]
fn full_request_and_optional_asset_receipt_still_have_the_original_byte_bounds() {
    let mut example = examples()[2].clone();
    example["request"]["payload"]["text"][0]["entry"]["block"]["text"] = json!("x".repeat(4096));
    let command: NativeApplicationRequest =
        serde_json::from_value(example["request"].clone()).unwrap();
    assert!(command.validate().is_err());
    let mut receipt = mixed_receipt();
    receipt["assets"] = Value::Null;
    assert!(!result(&receipt));
    receipt["assets"] = json!((0..70)
        .map(|i| { json!({"sourceId":format!("s{i}"),"assetId":format!("native-asset:{i}")}) })
        .collect::<Vec<_>>());
    assert!(!result(&receipt));
}

#[test]
fn new_examples_and_closed_resource_negatives_validate_against_the_actual_transport_schema() {
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
    for example in examples().into_iter().chain(source_examples()) {
        assert!(validator.is_valid(&example["request"]));
        assert!(validator.is_valid(&example["response"]));
        let mut bad = example["request"].clone();
        bad["payload"]["assets"] = json!([{"sourceId":"forged","assetId":"existing","bytes":[]}]);
        assert!(!validator.is_valid(&bad));
        let mut bad = example["response"].clone();
        bad["result"]["assets"] = json!([{"sourceId":"x","assetId":"native","record":{}}]);
        assert!(!validator.is_valid(&bad));
        let mut ordered = example["request"].clone();
        let mut order: Vec<_> = ["records", "raster", "text"]
            .iter()
            .flat_map(|key| ordered["payload"][*key].as_array().into_iter().flatten())
            .map(|row| row["sourceId"].clone())
            .collect();
        order.reverse();
        ordered["payload"]["objectOrder"] = json!(order);
        assert!(validator.is_valid(&ordered));
        assert!(request(&ordered["payload"]));
        for invalid in [
            Value::Null,
            json!({}),
            json!([null]),
            json!(["duplicate", "duplicate"]),
        ] {
            ordered["payload"]["objectOrder"] = invalid;
            assert!(!validator.is_valid(&ordered));
            assert!(!request(&ordered["payload"]));
        }
    }
}

fn source_examples() -> Vec<Value> {
    crate::capabilities::native_catalog()
        .capability_for_operation(OP)
        .unwrap()["examples"]
        .as_array()
        .unwrap()
        .iter()
        .skip(4)
        .cloned()
        .collect()
}

#[test]
fn native_sources_are_closed_pinned_descriptors_not_inline_asset_or_url_transport() {
    let examples = source_examples();
    assert_eq!(examples.len(), 3);
    for example in &examples {
        let command: NativeApplicationRequest =
            serde_json::from_value(example["request"].clone()).unwrap();
        let response: NativeApplicationResponse =
            serde_json::from_value(example["response"].clone()).unwrap();
        assert!(command.validate().is_ok());
        assert!(response_matches(&command, &response));
        for key in ["documentSnapshotId", "lifecycleGeneration"] {
            let mut bad = example["request"]["payload"].clone();
            bad.as_object_mut().unwrap().remove(key);
            assert!(!request(&bad), "{key}");
        }
    }
    let base = examples[0]["request"]["payload"].clone();
    for (key, value) in [
        ("sources", json!([])),
        ("sources", Value::Null),
        ("documentSnapshotId", json!("bad snapshot")),
        ("lifecycleGeneration", json!(0)),
        ("lifecycleGeneration", json!(1.5)),
    ] {
        let mut bad = base.clone();
        bad[key] = value;
        assert!(!request(&bad), "{key}");
    }
    for mutate in [
        |p: &mut Value| {
            p["sources"][0]["bytes"] = json!([]);
        },
        |p: &mut Value| {
            p["sources"][0]["source"]["bytes"] = json!([]);
        },
        |p: &mut Value| {
            p["sources"][0]["source"] = json!({"kind":"url","url":"https://example.invalid/a.png"});
        },
        |p: &mut Value| {
            p["sources"][0]["source"]["path"] = json!("relative.png");
        },
        |p: &mut Value| {
            p["sources"][0]["mediaType"] = json!("image/jpeg");
        },
        |p: &mut Value| {
            p["sources"][0]["name"] = json!(" ");
        },
    ] {
        let mut bad = base.clone();
        mutate(&mut bad);
        assert!(!request(&bad));
    }
    for key in ["documentSnapshotId", "lifecycleGeneration"] {
        let mut bad = json!({"command":"object.import.batch","layers":[],"records":[]});
        bad[key] = base[key].clone();
        assert!(!request(&bad));
    }
    for (key, value) in [
        ("weight", json!(0)),
        ("weight", json!(1001)),
        ("weight", json!(1.5)),
        ("italic", Value::Null),
        ("family", json!(" ")),
        ("url", json!("https://example.invalid")),
    ] {
        let mut bad = examples[2]["request"]["payload"].clone();
        bad["sources"][0]["source"][key] = value;
        assert!(!request(&bad), "{key}");
    }
    let mut large = base.clone();
    large["sources"] = json!((0..1025)
        .map(|i| {
            let mut row = base["sources"][0].clone();
            row["sourceId"] = json!(format!("source:{i}"));
            row
        })
        .collect::<Vec<_>>());
    assert!(!request(&large));
}

#[test]
fn source_assets_share_retained_and_skip_ids_and_asset_only_sources_are_changing_batches() {
    let mut example = source_examples()[0].clone();
    example["request"]["payload"]["assets"] =
        json!([{"sourceId":"retained-source","assetId":"retained-image"}]);
    example["response"]["result"]["assets"]
        .as_array_mut()
        .unwrap()
        .insert(
            0,
            json!({"sourceId":"retained-source","assetId":"native-asset:8:1"}),
        );
    let payload = &example["request"]["payload"];
    let receipt = &example["response"]["result"];
    assert!(receipt_matches(payload, receipt));
    let mut bad = payload.clone();
    bad["assets"][0]["sourceId"] = json!("image-source");
    assert!(!request(&bad));
    let mut bad = payload.clone();
    bad["skipped"] = json!([{"kind":"asset","sourceId":"image-source","reason":"skip"}]);
    assert!(!request(&bad));
    let mut bad = receipt.clone();
    bad["assets"].as_array_mut().unwrap().pop();
    assert!(!receipt_matches(payload, &bad));
    let mut bad = receipt.clone();
    bad["assets"][1]["assetId"] = bad["assets"][0]["assetId"].clone();
    assert!(!receipt_matches(payload, &bad));
    let mut bad = receipt.clone();
    bad["assets"].as_array_mut().unwrap().reverse();
    assert!(!receipt_matches(payload, &bad));
    let mut bad_items = mixed_receipt();
    bad_items["objects"].as_array_mut().unwrap().reverse();
    assert!(!receipt_matches(&mixed(), &bad_items));
    let asset_only = source_examples()[2].clone();
    assert!(receipt_matches(
        &asset_only["request"]["payload"],
        &asset_only["response"]["result"]
    ));
    assert!(revision_matches(
        Some(7),
        8,
        &asset_only["response"]["result"]
    ));
}

#[test]
fn object_order_is_closed_exact_and_independent_of_canonical_receipts() {
    let mut payload = mixed();
    assert!(request(&payload));
    payload["objectOrder"] = json!(["text-source", "raster-source"]);
    assert!(request(&payload));
    assert!(receipt_matches(&payload, &mixed_receipt()));
    for invalid in [
        Value::Null,
        json!({}),
        json!("text-source"),
        json!([]),
        json!([null]),
        json!(["text-source"]),
        json!(["text-source", "raster-source", "extra"]),
        json!(["text-source", "text-source"]),
        json!(["image-source", "raster-source"]),
        json!(["origin-object", "raster-source"]),
    ] {
        payload["objectOrder"] = invalid;
        assert!(!request(&payload), "{payload}");
    }
    assert!(request(
        &json!({"command":"object.import.batch","layers":[],"records":[],"objectOrder":[]})
    ));
    payload["objectOrder"] = json!(["raster-source", "text-source"]);
    let mut reordered = mixed_receipt();
    reordered["objects"].as_array_mut().unwrap().reverse();
    assert!(!receipt_matches(&payload, &reordered));
}
