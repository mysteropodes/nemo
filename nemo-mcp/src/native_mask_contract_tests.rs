//! Actual MCP contract/wire validation only; these controls do not execute a native owner.
use crate::native_contract::vector::tests::{created, envelope, record, response, stable};
use crate::native_contract::vector::{request, result, PARAMETRIC, VECTOR};
use crate::wire::validate_native_response;
use serde_json::{json, Value};

fn mask(intent: Value) -> Value {
    json!({"command":"object.vector.mask.edit","stableTarget":stable(),"maskEdit":intent})
}
fn receipt(applied: bool) -> Value {
    json!({"applied":applied,"historyEntriesAdded":u64::from(applied),
        "operation":"mask.edit","strokeId":"native-stroke:8:0"})
}

#[test]
fn stored_mask_members_preserve_nullable_defaults_and_arbitrary_imported_mode_spelling() {
    for tags in [
        json!({"isMask":null}),
        json!({"isMask":false}),
        json!({"isMask":true}),
        json!({"maskMode":null}),
        json!({"maskMode":""}),
        json!({"maskMode":"x".repeat(512)}),
        json!({"maskFeather":null}),
        json!({"maskFeather":0}),
        json!({"maskFeather":-3}),
        json!({"isMask":true,"maskMode":"intersect","maskFeather":2.5}),
    ] {
        let mut r = record(stable());
        r["vectorExt"] = tags.clone();
        let original = r.clone();
        assert!(crate::native_contract::vector::record(&r));
        assert!(request(
            VECTOR,
            &json!({"command":"object.vector.replace","record":r})
        ));
        assert_eq!(r, original);
        assert_eq!(r["vectorExt"], tags);
        let mut create = r;
        create["target"] = created();
        assert!(request(
            VECTOR,
            &json!({"command":"object.vector.create","record":create})
        ));
    }
    assert!(crate::native_contract::vector::record(&record(stable())));
}

#[test]
fn malformed_mask_record_members_are_refused_without_loosening_other_extension_checks() {
    for tags in [
        json!({"isMask":1}),
        json!({"isMask":"true"}),
        json!({"maskMode":7}),
        json!({"maskMode":[]}),
        json!({"maskFeather":"2"}),
        json!({"maskFeather":false}),
        json!({"isMask":true,"unownedMaskCache":[]}),
        json!({}),
    ] {
        let mut r = record(created());
        r["vectorExt"] = tags;
        assert!(!request(
            VECTOR,
            &json!({"command":"object.vector.create","record":r})
        ));
    }
    let mut r = record(created());
    r["vectorExt"] = json!({"isMask":true,"pressure":{}});
    assert!(
        !request(
            VECTOR,
            &json!({"command":"object.vector.create","record":r})
        ),
        "mask metadata cannot waive the existing pressure/real-stroke exclusion"
    );
}

#[test]
fn mask_panel_intents_use_the_exact_existing_vector_envelope_and_request_admission() {
    for intent in [
        json!({"kind":"mode","mode":"add"}),
        json!({"kind":"mode","mode":"subtract"}),
        json!({"kind":"mode","mode":"intersect"}),
        json!({"kind":"feather","value":0}),
        json!({"kind":"feather","value":-2}),
        json!({"kind":"unset"}),
    ] {
        let p = mask(intent);
        let original = p.clone();
        assert!(request(VECTOR, &p));
        assert!(crate::native_contract::validate_request(VECTOR, &p));
        assert!(envelope(VECTOR, p.clone(), Some(7))
            .validate_input()
            .is_ok());
        assert_eq!(p, original);
        for key in ["command", "stableTarget", "maskEdit"] {
            let mut missing = p.clone();
            missing.as_object_mut().unwrap().remove(key);
            assert!(!request(VECTOR, &missing));
        }
        let mut extra = p.clone();
        extra["layerMasks"] = json!([]);
        assert!(!request(VECTOR, &extra));
        assert!(!request(PARAMETRIC, &p));
    }
    let q = envelope(VECTOR, mask(json!({"kind":"unset"})), None);
    assert!(
        q.validate_input().is_err(),
        "mask edit requires expectedRevision like other vector edits"
    );
}

#[test]
fn malformed_or_cross_purpose_mask_intents_never_gain_contract_admission() {
    for intent in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"kind":"mode"}),
        json!({"kind":"mode","mode":"future"}),
        json!({"kind":"mode","mode":null}),
        json!({"kind":"feather","value":null}),
        json!({"kind":"feather","value":"2"}),
        json!({"kind":"unset","value":0}),
        json!({"kind":"feather","value":2,"mode":"add"}),
    ] {
        let p = mask(intent);
        assert!(!request(VECTOR, &p));
        assert!(envelope(VECTOR, p, Some(7)).validate_input().is_err());
    }
    for target in [json!({}), json!({"contextId":"foreign"}), Value::Null] {
        let mut p = mask(json!({"kind":"unset"}));
        p["stableTarget"] = target;
        assert!(!request(VECTOR, &p));
    }
    let mut p = mask(json!({"kind":"unset"}));
    p["record"] = record(created());
    assert!(!request(VECTOR, &p));
}

#[test]
fn actual_mcp_wire_accepts_only_correlated_mask_dispositions_owner_and_revision() {
    let q = envelope(
        VECTOR,
        mask(json!({"kind":"mode","mode":"subtract"})),
        Some(7),
    );
    for applied in [false, true] {
        let value = receipt(applied);
        assert!(result(VECTOR, &q.payload, &value));
        let r = response(&q, 7 + u64::from(applied), value);
        assert!(validate_native_response(&r, &q).is_ok());
        let mut stale = r.clone();
        stale.content_revision = if applied { 7 } else { 8 };
        assert!(validate_native_response(&stale, &q).is_err());
    }
    let valid = response(&q, 8, receipt(true));
    for member in ["request", "instance", "document"] {
        let mut foreign = valid.clone();
        match member {
            "request" => foreign.request_id = "foreign".into(),
            "instance" => foreign.instance_id = "foreign".into(),
            _ => foreign.document_id = "foreign".into(),
        }
        assert!(validate_native_response(&foreign, &q).is_err());
    }
    for (key, value) in [
        ("operation", json!("delete")),
        ("strokeId", json!("foreign")),
        ("historyEntriesAdded", json!(0)),
        ("applied", json!(false)),
        ("unownedMaskCache", json!([])),
    ] {
        let mut wrong = valid.clone();
        wrong.result.as_mut().unwrap()[key] = value;
        assert!(validate_native_response(&wrong, &q).is_err());
    }
}

// Same complete canonical loader as the existing frame/shadow schema controls.
// Resolve the real shipped external resources, never a permissive stub or DTO copy.
fn canonical_transport() -> jsonschema::Validator {
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
    jsonschema::draft202012::options()
        .with_registry(&registry.prepare().unwrap())
        .build(&schema)
        .unwrap()
}

#[test]
fn advertised_canonical_schema_accepts_nullable_mask_tags_in_full_create_and_read_envelopes() {
    let validator = canonical_transport();
    let descriptor: Value = serde_json::from_str(include_str!(
        "../../engineering/application/capabilities-v2/native-object-vector.json"
    ))
    .unwrap();
    assert_eq!(
        descriptor["input"]["oneOf"][0]["$ref"],
        "../native-transport-v2.schema.json#/$defs/ObjectVectorEditPayload"
    );
    for tags in [
        json!({"isMask":null}),
        json!({"isMask":false}),
        json!({"isMask":true}),
        json!({"maskMode":null}),
        json!({"maskMode":""}),
        json!({"maskMode":"x".repeat(512)}),
        json!({"maskFeather":null}),
        json!({"maskFeather":0}),
        json!({"maskFeather":-3}),
        json!({"isMask":true,"maskMode":"intersect","maskFeather":2.5}),
    ] {
        let mut r = record(created());
        r["vectorExt"] = tags;
        let create = envelope(
            VECTOR,
            json!({"command":"object.vector.create","record":r}),
            Some(7),
        );
        assert!(validator.is_valid(&serde_json::to_value(&create).unwrap()));
        r["target"] = stable();
        let query = envelope(
            "query.document.object",
            json!({"atRevision":7,"stableTarget":stable()}),
            None,
        );
        assert!(validator.is_valid(&serde_json::to_value(&query).unwrap()));
        let read = response(
            &query,
            7,
            json!({"atRevision":7,
            "documentSnapshotId":"native-object:document-A:7","object":r}),
        );
        assert!(validator.is_valid(&serde_json::to_value(&read).unwrap()));
    }
    for tags in [
        json!({"isMask":1}),
        json!({"maskMode":false}),
        json!({"maskFeather":"3"}),
        json!({"isMask":true,"foreign":1}),
    ] {
        let mut r = record(created());
        r["vectorExt"] = tags;
        let create = envelope(
            VECTOR,
            json!({"command":"object.vector.create","record":r}),
            Some(7),
        );
        assert!(!validator.is_valid(&serde_json::to_value(&create).unwrap()));
        r["target"] = stable();
        let query = envelope(
            "query.document.object",
            json!({"atRevision":7,"stableTarget":stable()}),
            None,
        );
        let read = response(
            &query,
            7,
            json!({"atRevision":7,
            "documentSnapshotId":"native-object:document-A:7","object":r}),
        );
        assert!(!validator.is_valid(&serde_json::to_value(&read).unwrap()));
    }
}

#[test]
fn advertised_canonical_schema_closes_each_original_mask_intent_in_the_full_vector_request() {
    let validator = canonical_transport();
    for intent in [
        json!({"kind":"mode","mode":"add"}),
        json!({"kind":"mode","mode":"subtract"}),
        json!({"kind":"mode","mode":"intersect"}),
        json!({"kind":"feather","value":0}),
        json!({"kind":"feather","value":-2}),
        json!({"kind":"unset"}),
    ] {
        let q = envelope(VECTOR, mask(intent), Some(7));
        let value = serde_json::to_value(&q).unwrap();
        assert!(validator.is_valid(&value));
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove("expectedRevision");
        assert!(!validator.is_valid(&missing));
        let mut foreign = value;
        foreign["payload"]["foreign"] = json!(1);
        assert!(!validator.is_valid(&foreign));
    }
    for intent in [
        json!({"kind":"unset","value":4}),
        json!({"kind":"mode","mode":"future"}),
        json!({"kind":"mode"}),
        json!({"kind":"feather","value":null}),
        json!({"kind":"feather","value":"3"}),
        json!({"kind":"feather","value":2,"mode":"add"}),
        json!(["unset"]),
    ] {
        let q = envelope(VECTOR, mask(intent), Some(7));
        assert!(!validator.is_valid(&serde_json::to_value(&q).unwrap()));
    }
}

#[test]
fn advertised_canonical_schema_reaches_mask_edit_receipts_without_weakening_wire_correlation() {
    let validator = canonical_transport();
    let q = envelope(
        VECTOR,
        mask(json!({"kind":"mode","mode":"subtract"})),
        Some(7),
    );
    for applied in [false, true] {
        let r = response(&q, 7 + u64::from(applied), receipt(applied));
        assert!(validator.is_valid(&serde_json::to_value(&r).unwrap()));
        assert!(validate_native_response(&r, &q).is_ok());
    }
    for operation in ["mask", "mask.edit.extra", "foreign"] {
        let mut r = response(&q, 8, receipt(true));
        r.result.as_mut().unwrap()["operation"] = json!(operation);
        assert!(!validator.is_valid(&serde_json::to_value(&r).unwrap()));
    }
    let mut foreign = response(&q, 8, receipt(true));
    foreign.result.as_mut().unwrap()["foreign"] = json!(1);
    assert!(!validator.is_valid(&serde_json::to_value(&foreign).unwrap()));
    // The schema describes every vector receipt. Only the actual wire can
    // correlate another valid vector operation with this mask request.
    let mut other_operation = response(&q, 8, receipt(true));
    other_operation.result.as_mut().unwrap()["operation"] = json!("delete");
    assert!(validator.is_valid(&serde_json::to_value(&other_operation).unwrap()));
    assert!(validate_native_response(&other_operation, &q).is_err());
}
