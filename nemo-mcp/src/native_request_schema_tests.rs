use super::*;
use crate::contract::NativeApplicationRequest;
use std::time::Instant;

#[test]
fn every_declared_example_compiles_and_matches_its_operation_schema() {
    for descriptor in crate::capabilities::native_catalog().descriptors() {
        for example in descriptor["examples"].as_array().unwrap() {
            let request: NativeApplicationRequest =
                serde_json::from_value(example["request"].clone()).unwrap();
            assert_eq!(
                payload(&request.operation, &request.payload),
                Some(true),
                "{} {}",
                descriptor["id"],
                request.operation
            );
            assert_eq!(
                envelope(&request),
                Some(true),
                "{} envelope",
                request.operation
            );
        }
    }
}

#[test]
fn formerly_forwarded_opacity_now_checks_its_schema_without_confusing_operation_and_command() {
    let base =
        json!({"command":"layer.opacity.set","stableTarget":{"layerUid":"layer"},"value":25});
    let operation = "command.document.apply";
    assert_eq!(payload(operation, &base), Some(true));
    for (key, value) in [
        ("extra", json!(true)),
        ("command", json!(operation)),
        ("value", json!(-0.01)),
        ("value", json!(100.01)),
        ("value", json!("25")),
        ("stableTarget", json!({"layerUid":"bad id"})),
    ] {
        let mut invalid = base.clone();
        invalid[key] = value;
        assert_eq!(
            payload(operation, &invalid),
            Some(false),
            "{key}: {invalid}"
        );
    }
    assert_eq!(
        payload(
            "query.document.opacity",
            &json!({"stableTarget":{"layerUid":"layer"}})
        ),
        Some(true)
    );
    assert_eq!(payload("query.document.opacity", &base), Some(false));
}

#[test]
fn request_envelope_uses_actual_branch_revision_policy_and_retains_cancellation() {
    let mut request: NativeApplicationRequest = serde_json::from_value(json!({"apiVersion":2,"requestId":"fixture","instanceId":"instance","documentId":"document","operation":"command.document.apply","expectedRevision":7,"payload":{"command":"layer.opacity.set","stableTarget":{"layerUid":"layer"},"value":25},"cancelledBeforeDispatch":true})).unwrap();
    assert_eq!(envelope(&request), Some(true));
    request.expected_revision = None;
    assert_eq!(envelope(&request), Some(false));
    request.operation = "query.document.opacity".into();
    assert_eq!(envelope(&request), Some(true));
    request.expected_revision = Some(0);
    assert_eq!(envelope(&request), Some(false));
}

#[test]
fn byte_semantic_limits_remain_stronger_than_json_schema_character_lengths() {
    let operation = "command.document.object.layer.cut";
    let value = json!({"command":"object.layer.cut","contextId":"scene-root","layerUid":"é".repeat(65),"frame":7});
    assert_eq!(
        payload(operation, &value),
        Some(true),
        "schema counts Unicode characters"
    );
    assert!(
        !crate::native_contract::validate_request(operation, &value),
        "retained owner predicate bounds encoded ID bytes"
    );
}

#[test]
fn used_operation_validators_are_cached_and_warm_validation_has_no_recompilation() {
    let operation = "query.document.revision";
    let start = Instant::now();
    let first = validator(operation, false).unwrap().unwrap();
    let cold = start.elapsed();
    let start = Instant::now();
    for _ in 0..10_000 {
        let cached = validator(operation, false).unwrap().unwrap();
        assert!(std::ptr::eq(first, cached));
        assert!(cached.is_valid(&json!({})));
    }
    eprintln!(
        "schema validation cost: first access {cold:?}; 10000 cached empty payloads {:?}",
        start.elapsed()
    );
}

#[test]
fn every_declared_operation_compiles_both_projections_before_shipping() {
    for descriptor in crate::capabilities::native_catalog().descriptors() {
        for operation in descriptor["operations"].as_array().unwrap() {
            let operation = operation.as_str().unwrap();
            for envelope in [false, true] {
                assert!(
                    validator(operation, envelope).unwrap().is_some(),
                    "{operation}"
                );
            }
        }
    }
}

#[test]
fn ordinary_gui_project_read_requests_cross_the_real_original_byte_boundary() {
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../tests/helpers/native-request-caller-fixture.cjs");
    let output = std::process::Command::new("node")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let requests: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(requests.len(), 5);
    assert!(requests
        .iter()
        .any(|r| r["payload"]["view"]["hitTest"] == true));
    assert!(requests
        .iter()
        .any(|r| r["payload"]["subselect"]["atFrame"] == 10));
    assert!(requests
        .iter()
        .any(|r| r["payload"]["view"]["contextId"] == "comp-a"));
    for request in requests {
        let bytes = serde_json::to_vec(&request).unwrap();
        assert!(
            crate::wire::decode_native_request(&bytes).is_ok(),
            "{request}"
        );
    }
}

#[test]
fn additive_operation_and_sibling_resource_compile_without_a_central_rust_registration() {
    let source = json!({"$id":"https://nemo.test/contracts/native-transport-v2.schema.json",
    "$defs":{"Envelope":{"type":"object","required":["operation","payload"]},"Request":{"oneOf":[{"allOf":[
      {"$ref":"#/$defs/Envelope"},{"properties":{"operation":{"const":"query.new_capability"},"payload":{"$ref":"new-capability.schema.json#/$defs/Input"}}}
    ]}]}}});
    let resource = r##"{"$defs":{"Input":{"type":"object","additionalProperties":false,"required":["limit"],"properties":{"limit":{"type":"integer","minimum":1,"maximum":3}}}}}"##;
    let catalog =
        build_catalog_from(source.clone(), &[("new-capability.schema.json", resource)]).unwrap();
    let entry = catalog.operations.get("query.new_capability").unwrap();
    let validator = compile(&catalog, entry, false).unwrap();
    assert!(validator.is_valid(&json!({"limit":2})));
    for bad in [
        json!({"limit":0}),
        json!({"limit":4}),
        json!({"limit":2,"extra":true}),
        json!({}),
    ] {
        assert!(!validator.is_valid(&bad));
    }
    assert!(compile(&catalog, entry, true)
        .unwrap()
        .is_valid(&json!({"operation":"query.new_capability","payload":{}})));
    // Bad embedded contracts return an error and cannot panic a traffic thread.
    assert!(build_catalog_from(source, &[]).is_err());
    assert!(build_catalog_from(json!({"$id":"https://nemo.test/invalid"}), &[]).is_err());
}

#[test]
fn build_resource_discovery_matches_all_sibling_contract_files_in_sorted_order() {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../engineering/application");
    let mut expected: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.ends_with(".schema.json") && name != "native-transport-v2.schema.json")
        .collect();
    expected.sort();
    assert_eq!(
        RESOURCES
            .iter()
            .map(|(name, _)| name.to_string())
            .collect::<Vec<_>>(),
        expected
    );
}

#[test]
fn projected_payload_and_envelope_agree_with_the_complete_request_schema() {
    let catalog = catalog().unwrap();
    let full = jsonschema::draft202012::options()
        .with_registry(&catalog.resources)
        .build(
            &json!({"$ref":format!("{}#/$defs/Request", catalog.schema["$id"].as_str().unwrap())}),
        )
        .unwrap();
    for descriptor in crate::capabilities::native_catalog().descriptors() {
        for example in descriptor["examples"].as_array().unwrap() {
            let value = &example["request"];
            let mut mutations = vec![value.clone()];
            for (key, changed) in [
                ("extra", json!(true)),
                ("apiVersion", json!(1)),
                ("expectedRevision", json!(null)),
                ("cancelledBeforeDispatch", json!(true)),
                ("expectedRevision", json!(0)),
                ("payload", json!({"unexpected":true})),
            ] {
                let mut altered = value.clone();
                altered[key] = changed;
                mutations.push(altered);
            }
            let mut missing = value.clone();
            missing.as_object_mut().unwrap().remove("expectedRevision");
            mutations.push(missing);
            for value in mutations {
                let projected = serde_json::from_value::<NativeApplicationRequest>(value.clone())
                    .is_ok_and(|request| {
                        payload(&request.operation, &request.payload) == Some(true)
                            && envelope(&request) == Some(true)
                    });
                assert_eq!(
                    projected,
                    full.is_valid(&value),
                    "projection drift: {value}"
                );
            }
        }
    }
}
