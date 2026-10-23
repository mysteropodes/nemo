use nemo_mcp::contract::{NativeHostStatus, NativeStatusRequest, RequestError};
use serde_json::json;

#[test]
fn native_status_request_rejects_invalid_version_and_bounded_identities() {
    let valid = json!({"apiVersion":2,"requestId":"status-1","instanceId":"instance-a"});
    let request: NativeStatusRequest = serde_json::from_value(valid.clone()).unwrap();
    assert!(request.validate().is_ok());
    let mut wrong_version = request.clone();
    wrong_version.api_version = 1;
    assert!(matches!(
        wrong_version.validate(),
        Err(RequestError::InvalidRequest(_))
    ));
    for field in ["requestId", "instanceId"] {
        for value in [
            "".to_owned(),
            "x".repeat(129),
            "-bad".into(),
            "bad id".into(),
            "bad\n".into(),
        ] {
            let mut invalid = valid.clone();
            invalid[field] = json!(value);
            let request: NativeStatusRequest = serde_json::from_value(invalid).unwrap();
            assert!(
                matches!(request.validate(), Err(RequestError::InvalidRequest(message))
                if message == "invalid native status request identity")
            );
        }
        let mut boundary = valid.clone();
        boundary[field] = json!("x".repeat(128));
        assert!(serde_json::from_value::<NativeStatusRequest>(boundary)
            .unwrap()
            .validate()
            .is_ok());
    }
}

#[test]
fn native_host_status_requires_valid_identity_and_a_complete_availability_state() {
    let valid = json!({"apiVersion":2,"requestId":"status-1","instanceId":"instance-a",
        "available":true,"documentId":"document-a","contentRevision":0,"lifecycleGeneration":17});
    assert!(serde_json::from_value::<NativeHostStatus>(valid.clone())
        .unwrap()
        .validate()
        .is_ok());
    for (field, value) in [
        ("apiVersion", json!(1)),
        ("requestId", json!("")),
        ("instanceId", json!("")),
        ("documentId", json!("bad id")),
        ("contentRevision", json!(9_007_199_254_740_992u64)),
        ("documentId", json!(null)),
        ("contentRevision", json!(null)),
    ] {
        let mut invalid = valid.clone();
        invalid[field] = value;
        let status: NativeHostStatus = serde_json::from_value(invalid).unwrap();
        assert!(
            matches!(status.validate(), Err(RequestError::InvalidRequest(message))
            if message == "invalid native host status")
        );
    }
    let mut boundary = valid;
    boundary["contentRevision"] = json!(9_007_199_254_740_991u64);
    assert!(serde_json::from_value::<NativeHostStatus>(boundary)
        .unwrap()
        .validate()
        .is_ok());
    let request = NativeStatusRequest {
        api_version: 2,
        request_id: "status-1".into(),
        instance_id: "instance-a".into(),
    };
    let unavailable = NativeHostStatus::unavailable(&request, "no active document");
    assert!(unavailable.validate().is_ok());
    assert_eq!(
        (&unavailable.request_id, &unavailable.instance_id),
        (&request.request_id, &request.instance_id)
    );
    assert!(!unavailable.available);
    assert!(unavailable.lifecycle_generation.is_none());
    assert!(serde_json::to_value(&unavailable).unwrap().get("lifecycleGeneration").is_none());
    assert!(unavailable.document_id.is_none() && unavailable.content_revision.is_none());
    for reason in [None, Some(String::new())] {
        let mut invalid = unavailable.clone();
        invalid.reason = reason;
        assert!(
            matches!(invalid.validate(), Err(RequestError::InvalidRequest(message))
            if message == "invalid native host status")
        );
    }
}

fn refused_status(value: serde_json::Value) -> bool {
    match serde_json::from_value::<NativeHostStatus>(value) {
        Ok(status) => status.validate().is_err(),
        Err(_) => true,
    }
}

#[test]
fn active_generation_is_required_positive_safe_integer_not_a_default_or_null() {
    let valid = json!({"apiVersion":2,"requestId":"generation","instanceId":"instance-a",
        "available":true,"documentId":"document-a","contentRevision":23,"lifecycleGeneration":37});
    for generation in [1, 37, 9_007_199_254_740_991u64] {
        let mut value = valid.clone();
        value["lifecycleGeneration"] = json!(generation);
        let status: NativeHostStatus = serde_json::from_value(value.clone()).unwrap();
        status.validate().unwrap();
        assert_eq!(status.lifecycle_generation, Some(generation));
        assert_eq!(serde_json::to_value(status).unwrap(), value);
    }
    let mut missing = valid.clone();
    missing.as_object_mut().unwrap().remove("lifecycleGeneration");
    assert!(refused_status(missing), "missing generation must not become 1");
    for generation in [
        json!(null), json!(0), json!(-1), json!(1.5), json!(1.0), json!(true),
        json!("37"), json!([]), json!({}), json!(9_007_199_254_740_992u64),
        json!(u64::MAX),
    ] {
        let mut invalid = valid.clone();
        invalid["lifecycleGeneration"] = generation;
        assert!(refused_status(invalid.clone()), "accepted {invalid}");
    }
    let mut unknown = valid;
    unknown["generation"] = json!(37);
    assert!(refused_status(unknown));
    let duplicate = r#"{"apiVersion":2,"requestId":"generation","instanceId":"instance-a","available":true,"documentId":"document-a","contentRevision":23,"lifecycleGeneration":37,"lifecycleGeneration":38}"#;
    assert!(serde_json::from_str::<NativeHostStatus>(duplicate).is_err());
}

#[test]
fn unavailable_status_cannot_carry_generation_and_request_cannot_supply_it() {
    let unavailable = json!({"apiVersion":2,"requestId":"generation","instanceId":"instance-a",
        "available":false,"reason":"native authority inactive"});
    assert!(!refused_status(unavailable.clone()));
    for generation in [json!(null), json!(0), json!(1), json!(37), json!(9_007_199_254_740_991u64)] {
        let mut invalid = unavailable.clone();
        invalid["lifecycleGeneration"] = generation;
        assert!(refused_status(invalid.clone()), "accepted {invalid}");
    }
    let injected_request = json!({"apiVersion":2,"requestId":"generation",
        "instanceId":"instance-a","lifecycleGeneration":37});
    assert!(serde_json::from_value::<NativeStatusRequest>(injected_request).is_err());
}
