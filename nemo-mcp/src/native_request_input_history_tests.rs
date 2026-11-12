use super::*;

fn history_request(command: &str) -> NativeApplicationRequest {
    let mut request = visibility_request();
    request.operation = format!("command.document.{command}");
    request.payload = json!({"command":command});
    request
}

#[test]
fn public_object_history_requires_registered_strict_revision_pinned_commands() {
    for command in ["object.undo", "object.redo"] {
        let base = history_request(command);
        let descriptor = crate::capabilities::native_catalog()
            .capability_for_operation(&base.operation)
            .expect("public native object history registration");
        assert_eq!(descriptor["id"], "native.object");
        assert_eq!(descriptor["availability"]["state"], "available");
        for revision in [0, 7, 9_007_199_254_740_991] {
            let mut request = base.clone();
            request.expected_revision = Some(revision);
            assert!(request.validate_input().is_ok());
            assert!(request.validate().is_ok());
        }
        let mut missing = base.clone();
        missing.expected_revision = None;
        same_failure(
            &missing,
            "invalid_request",
            "native command requires expectedRevision",
        );
        let mut unsafe_revision = base.clone();
        unsafe_revision.expected_revision = Some(9_007_199_254_740_992);
        same_failure(
            &unsafe_revision,
            "invalid_request",
            "expectedRevision exceeds the transport maximum",
        );
        let mut encoded = serde_json::to_value(&base).unwrap();
        encoded["expectedRevision"] = Value::Null;
        assert!(serde_json::from_value::<NativeApplicationRequest>(encoded).is_err());
        for payload in [
            json!({}),
            Value::Null,
            json!([]),
            json!({"command":null}),
            json!({"command":7}),
            json!({"command":"history.undo"}),
            json!({"command":if command == "object.undo" {"object.redo"} else {"object.undo"}}),
            json!({"command":command,"extra":true}),
        ] {
            let mut invalid = base.clone();
            invalid.payload = payload;
            same_failure(
                &invalid,
                "invalid_request",
                "native command does not match its declared contract",
            );
        }
    }
}

#[test]
fn public_object_history_success_is_exact_and_correlated_to_its_request() {
    for command in ["object.undo", "object.redo"] {
        let request = history_request(command);
        let good = json!({"applied":true,"historyEntriesAdded":0});
        assert!(crate::native_contract::validate_result(
            &request.operation,
            &request.document_id,
            &request.payload,
            &good
        ));
        for result in [
            Value::Null,
            json!({}),
            json!({"applied":false,"historyEntriesAdded":0}),
            json!({"applied":true,"historyEntriesAdded":1}),
            json!({"applied":true,"historyEntriesAdded":0.0}),
            json!({"applied":true,"historyEntriesAdded":null}),
            json!({"applied":true}),
            json!({"applied":null,"historyEntriesAdded":0}),
            json!({"applied":true,"historyEntriesAdded":0,"extra":true}),
        ] {
            assert!(!crate::native_contract::validate_result(
                &request.operation,
                &request.document_id,
                &request.payload,
                &result
            ));
        }
        let response = json!({"apiVersion":2,"requestId":request.request_id,
            "instanceId":request.instance_id,"documentId":request.document_id,
            "contentRevision":8,"ok":true,"result":good});
        assert!(crate::wire::validate_native_response(
            &serde_json::from_value(response.clone()).unwrap(),
            &request
        )
        .is_ok());
        for revision in [0_u64, 7, 9, 9_007_199_254_740_991] {
            let mut wrong_revision = response.clone();
            wrong_revision["contentRevision"] = json!(revision);
            assert!(crate::wire::validate_native_response(
                &serde_json::from_value(wrong_revision).unwrap(),
                &request
            )
            .is_err());
        }
        for key in ["requestId", "instanceId", "documentId"] {
            let mut foreign = response.clone();
            foreign[key] = json!("foreign");
            assert!(crate::wire::validate_native_response(
                &serde_json::from_value(foreign).unwrap(),
                &request
            )
            .is_err());
        }
        let mut mismatch = request.payload.clone();
        mismatch["command"] = json!(if command == "object.undo" {
            "object.redo"
        } else {
            "object.undo"
        });
        assert!(!crate::native_contract::validate_result(
            &request.operation,
            &request.document_id,
            &mismatch,
            &good
        ));
    }
}

#[test]
fn formerly_forwarded_envelopes_enforce_schema_revision_policy_at_the_original_byte_boundary() {
    for (operation, revision, payload, message) in [
        ("command.document.apply", None, json!({"command":"layer.opacity.set","stableTarget":{"layerUid":"layer"},"value":25}),
            "native command requires expectedRevision"),
        ("query.document.revision", Some(0), json!({}), "native pinned reads forbid expectedRevision"),
    ] {
        let request = NativeApplicationRequest {
            api_version: 2, request_id: "schema-envelope".into(), instance_id: "instance".into(), document_id: "document".into(),
            expected_revision: revision, operation: operation.into(), payload, cancelled_before_dispatch: false,
        };
        same_failure(&request, "invalid_request", message);
        let bytes = serde_json::to_vec(&request).unwrap();
        assert!(crate::wire::decode_native_request(&bytes).unwrap_err().to_string()
            .contains(message));
    }
}

#[test]
fn malformed_metadata_and_font_inputs_refuse_at_the_actual_public_byte_boundary() {
    for (operation, revision, payload, message) in [
        (crate::native_contract::comments::EDIT, Some(7), json!({"bogus":1}),
            "native command does not match its declared contract"),
        (crate::native_contract::custom_brush::EDIT, Some(7), json!({}),
            "native command does not match its declared contract"),
        (crate::native_contract::font::QUERY, None, json!({}),
            "native pinned-read payload does not match its declared contract"),
    ] {
        let request = NativeApplicationRequest {
            api_version: 2, request_id: "invalid-metadata".into(), instance_id: "instance".into(), document_id: "document".into(),
            expected_revision: revision, operation: operation.into(), payload, cancelled_before_dispatch: false,
        };
        let code = if revision.is_some() { "invalid_request" } else { "malformed_payload" };
        same_failure(&request,code,message);
        assert!(crate::wire::decode_native_request(&serde_json::to_vec(&request).unwrap()).unwrap_err().to_string().contains(message));
    }
    let request = NativeApplicationRequest {
        api_version: 2, request_id: "invalid-schema-shape".into(), instance_id: "instance".into(), document_id: "document".into(),
        expected_revision: None, operation: "query.document.opacity".into(), payload: json!({}), cancelled_before_dispatch: false,
    };
    same_failure(&request,"malformed_payload","native pinned-read payload does not match its declared contract");
    assert!(crate::wire::decode_native_request(&serde_json::to_vec(&request).unwrap()).is_err());
}

/// Add-one-capability, Rust side (#2363): validate_input holds no per-feature
/// operation list, so every declared operation obeys the revision policy and the
/// declared-contract gate that its own transport branch states.
#[test]
fn declared_operations_obey_their_own_branch_policy_through_the_mcp_input_gate() {
    let source = include_str!("native_request_input.rs");
    for central in ["::EDIT", "::OP", "::admitted(", "::declares(", "forbids_expected_revision"] {
        assert!(!source.contains(central), "validate_input must not list {central}");
    }
    let (mut required, mut forbidden) = (0, 0);
    for descriptor in crate::capabilities::native_catalog().descriptors() {
        for example in descriptor["examples"].as_array().unwrap() {
            let request: NativeApplicationRequest =
                serde_json::from_value(example["request"].clone()).unwrap();
            let policy = crate::native_contract::request_schema::expected_revision_policy(&request.operation);
            assert!(request.validate_input().is_ok(), "{}", request.operation);
            let mut probe = request.clone();
            match policy {
                Some(true) => {
                    required += 1;
                    probe.expected_revision = None;
                    same_failure(&probe, "invalid_request", "native command requires expectedRevision");
                    // A closed-shape violation is refused at the same declared-contract gate.
                    probe.expected_revision = Some(0);
                    probe.payload = json!({"command":"undeclared.shape","\u{0}":[]});
                    same_failure(&probe, "invalid_request", "native command does not match its declared contract");
                }
                Some(false) => {
                    forbidden += 1;
                    probe.expected_revision = Some(0);
                    same_failure(&probe, "invalid_request", "native pinned reads forbid expectedRevision");
                }
                None => panic!("{} is declared without a revision policy", request.operation),
            }
        }
    }
    assert!(required > 0 && forbidden > 0, "{required} command and {forbidden} query examples");
}
