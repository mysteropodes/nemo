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
