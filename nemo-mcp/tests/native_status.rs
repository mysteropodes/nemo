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
        "available":true,"documentId":"document-a","contentRevision":0,"lifecycleGeneration":1});
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
        ("lifecycleGeneration", json!(null)),
        ("lifecycleGeneration", json!(0)),
        ("lifecycleGeneration", json!(9_007_199_254_740_992u64)),
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
    boundary["lifecycleGeneration"] = json!(9_007_199_254_740_991u64);
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
    assert!(unavailable.document_id.is_none() && unavailable.content_revision.is_none());
    assert!(unavailable.lifecycle_generation.is_none());
    // Strict wire shape: the generation is camelCase and omitted when unavailable.
    let wire = serde_json::to_value(&unavailable).unwrap();
    assert!(wire.get("lifecycleGeneration").is_none());
    let mut active = unavailable.clone();
    active.available = true;
    active.reason = None;
    active.document_id = Some("document-a".into());
    active.content_revision = Some(3);
    active.lifecycle_generation = Some(7);
    assert!(active.validate().is_ok());
    assert_eq!(serde_json::to_value(&active).unwrap()["lifecycleGeneration"], 7);
    for reason in [None, Some(String::new())] {
        let mut invalid = unavailable.clone();
        invalid.reason = reason;
        assert!(
            matches!(invalid.validate(), Err(RequestError::InvalidRequest(message))
            if message == "invalid native host status")
        );
    }
}
