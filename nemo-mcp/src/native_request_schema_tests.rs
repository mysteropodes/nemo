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
    let fixture: Value = serde_json::from_slice(&output.stdout).unwrap();
    let requests = fixture["requests"].as_array().unwrap();
    assert_eq!(requests.len(), 12);
    assert!(requests
        .iter()
        .any(|r| r["payload"]["view"]["hitTest"] == true));
    assert!(requests
        .iter()
        .any(|r| r["payload"]["subselect"]["atFrame"] == 10));
    assert!(requests
        .iter()
        .any(|r| r["payload"]["view"]["contextId"] == "comp-a"));
    assert!(requests
        .iter()
        .any(|r| r["payload"]["subselect"]["center"] == true
            && r["payload"]["subselect"]["atFrame"] == 10));
    for (index, request) in requests.iter().enumerate() {
        let bytes = serde_json::to_vec(&request).unwrap();
        assert!(
            crate::wire::decode_native_request(&bytes).is_ok(),
            "{request}"
        );
        let typed = crate::wire::decode_native_request(&bytes).unwrap();
        let response: crate::contract::NativeApplicationResponse =
            serde_json::from_value(fixture["responses"][index].clone()).unwrap();
        let admitted_response = crate::wire::validate_native_response(&response, &typed);
        assert!(
            admitted_response.is_ok(),
            "{request}: {admitted_response:?}"
        );
        if request["payload"]["view"]["hitTest"] == true {
            for bad in [
                json!([[0, 0]]),
                json!([[0, 0], [0, 0], [1, "bad"]]),
                Value::Null,
            ] {
                let mut invalid = response.clone();
                invalid.result.as_mut().unwrap()["items"][0]["vertexOffsets"] = bad;
                assert!(crate::wire::validate_native_response(&invalid, &typed).is_err());
            }
        }
        if request["payload"]["subselect"].get("atFrame").is_some() {
            let mut mismatch = response;
            mismatch.result.as_mut().unwrap()["subselect"]["atFrame"] = json!(9);
            assert!(crate::wire::validate_native_response(&mismatch, &typed).is_err());
        }
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

#[test]
fn unknown_operations_have_no_projected_validator_and_still_use_registration_diagnostics() {
    let operation = "query.not_registered";
    assert!(validator(operation, false).unwrap().is_none());
    assert!(!declares(operation));
    let request: NativeApplicationRequest = serde_json::from_value(json!({"apiVersion":2,"requestId":"unknown","instanceId":"instance","documentId":"document","operation":operation,"payload":{}})).unwrap();
    assert_eq!(envelope(&request), None);
    let error = request.validate_input().unwrap_err();
    assert_eq!(error.code(), "invalid_request");
    assert!(error
        .message()
        .contains("is not declared by a registered native capability"));
}

#[test]
fn malformed_embedded_contracts_refuse_setup_or_compilation_without_panicking() {
    let base = json!({"$id":"https://nemo.test/contracts/native-transport-v2.schema.json", "$defs":{
      "Envelope":{"type":"object"}, "Request":{"oneOf":[{"allOf":[{"$ref":"#/$defs/Envelope"},
        {"properties":{"operation":{"const":"query.fixture"},"payload":{"type":"object"}}}]}]}}});
    let mut cases = Vec::new();
    let mut missing_id = base.clone();
    missing_id.as_object_mut().unwrap().remove("$id");
    cases.push((missing_id, "missing transport id"));
    let mut invalid_id = base.clone();
    invalid_id["$id"] = json!("no-slash");
    cases.push((invalid_id, "invalid transport id"));
    let mut missing_allof = base.clone();
    missing_allof["$defs"]["Request"]["oneOf"][0] = json!({});
    cases.push((missing_allof, "missing request allOf"));
    let mut short = base.clone();
    short["$defs"]["Request"]["oneOf"][0]["allOf"] = json!([]);
    cases.push((short, "unsupported request branch"));
    let mut shape = base.clone();
    shape["$defs"]["Request"]["oneOf"][0]["allOf"][1] = Value::Null;
    cases.push((shape, "missing request shape"));
    let mut properties = base.clone();
    properties["$defs"]["Request"]["oneOf"][0]["allOf"][1] = json!({});
    cases.push((properties, "missing request properties"));
    let mut condition = base.clone();
    condition["$defs"]["Request"]["oneOf"][0]["allOf"][1]["if"] = json!({});
    cases.push((condition, "unsupported request property projection"));
    let mut ref_missing = base.clone();
    ref_missing["$defs"]["Request"]["oneOf"][0]["allOf"][0] = json!({});
    cases.push((ref_missing, "missing request envelope ref"));
    let mut ref_external = base.clone();
    ref_external["$defs"]["Request"]["oneOf"][0]["allOf"][0]["$ref"] = json!("other.json");
    cases.push((ref_external, "nonlocal request envelope"));
    let mut no_operation = base.clone();
    no_operation["$defs"]["Request"]["oneOf"][0]["allOf"][1]["properties"]["operation"] = json!({});
    cases.push((no_operation, "missing operation const/enum"));
    let mut wrong_name = base.clone();
    wrong_name["$defs"]["Request"]["oneOf"][0]["allOf"][1]["properties"]["operation"] =
        json!({"const":7});
    cases.push((wrong_name, "non-string operation"));
    let mut duplicate = base.clone();
    let branch = duplicate["$defs"]["Request"]["oneOf"][0].clone();
    duplicate["$defs"]["Request"]["oneOf"]
        .as_array_mut()
        .unwrap()
        .push(branch);
    cases.push((duplicate, "duplicate operation"));
    for properties in [
        json!({"operation":{"const":"query.fixture"},"unrelated":{}}),
        json!({"payload":{},"unrelated":{}}),
    ] {
        let mut missing_projection_member = base.clone();
        missing_projection_member["$defs"]["Request"]["oneOf"][0]["allOf"][1]["properties"] =
            properties;
        cases.push((
            missing_projection_member,
            "unsupported request property projection",
        ));
    }
    let mut invalid_uri = base.clone();
    invalid_uri["$id"] = json!("https://[invalid/schema.json");
    assert!(build_catalog_from(invalid_uri, &[]).is_err());
    assert!(build_catalog_from(
        base.clone(),
        &[(
            "invalid-id.schema.json",
            r#"{"$id":"https://[invalid/schema.json"}"#
        )]
    )
    .is_err());
    for (source, expected) in cases {
        let result = std::panic::catch_unwind(|| build_catalog_from(source, &[]));
        assert!(
            result.is_ok(),
            "setup must refuse rather than panic: {expected}"
        );
        assert!(result.unwrap().err().unwrap().contains(expected));
    }
    assert!(
        build_catalog_from(base.clone(), &[("bad.schema.json", "{broken")])
            .err()
            .unwrap()
            .contains("bad.schema.json")
    );
    let mut unsupported_keyword = base;
    unsupported_keyword["$defs"]["Request"]["oneOf"][0]["allOf"][1]["properties"]["payload"] =
        json!({"type":"not-a-json-type"});
    let catalog = build_catalog_from(unsupported_keyword, &[]).unwrap();
    assert!(compile(
        &catalog,
        catalog.operations.get("query.fixture").unwrap(),
        false
    )
    .is_err());
}
