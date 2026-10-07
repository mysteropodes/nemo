//! Independent raw-byte admission and actual shared-owner/lifecycle controls.
use crate::application_mcp::ApplicationMcp;
use crate::native_application::{admit_release_request, complete_release};
use crate::native_application_contract::{NativeReleaseRequest, HOST_API_VERSION};
use crate::native_dispatch::{NativeDispatch, NativeReleaseProgress, ReleaseAdmission};
use crate::native_object_bootstrap::{bootstrap_raw, MAX_OBJECT_BOOTSTRAP_BYTES};
use native_engine::{
    commands::{NativeOpacityApplication, OpacityRequest, ResponseEnvelope},
    document::OpacityDocument,
    export_job::{JobReceipt, PendingFrame},
};
use serde_json::{json, Value};

fn document() -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases["records"]})
}
fn wrapper(state: &ApplicationMcp, bytes: &str) -> Value {
    json!({"apiVersion":2,"requestId":"bootstrap-object","instanceId":state.instance_id(),
        "documentJson":bytes})
}
fn raw(state: &ApplicationMcp) -> String {
    wrapper(state, &document().to_string()).to_string()
}
fn identity(state: &ApplicationMcp) -> Option<(u64, String, u64)> {
    let native = state.native_state();
    let guard = native.lock().unwrap();
    guard.active().map(|(generation, owner)| {
        (
            generation,
            owner.document_id().to_owned(),
            owner.content_revision(),
        )
    })
}
fn subscribe(state: &ApplicationMcp) -> Value {
    let binding = state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap();
    binding
}
fn require_retained(state: &ApplicationMcp, binding: &Value) {
    let error = state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap_err();
    assert!(
        error.contains("occupied"),
        "subscriber must survive refused admission"
    );
}

#[test]
fn raw_bootstrap_commits_actual_owner_and_resource_free_identity_receipt() {
    let state = ApplicationMcp::default();
    let receipt = bootstrap_raw(&state, &raw(&state)).unwrap();
    let (_, actual, revision) = identity(&state).unwrap();
    let result = serde_json::to_value(receipt).unwrap();
    assert_eq!(
        result,
        json!({"apiVersion":2,"requestId":"bootstrap-object",
        "instanceId":state.instance_id(),"documentId":actual,"contentRevision":0,
        "resourceCount":0,"viewportAvailable":false})
    );
    assert_eq!(revision, 0);
    assert!(actual.starts_with("native-object-document-"));
    assert!(uuid::Uuid::parse_str(state.instance_id()).is_ok());
    // Installing via this source port does not activate ordinary object dispatch.
    let request = json!({"apiVersion":2,"requestId":"unsupported","instanceId":state.instance_id(),
        "documentId":actual,"operation":"query.document.serialize","payload":{}});
    let before = identity(&state);
    assert!(state
        .dispatch_native(serde_json::from_value(request).unwrap())
        .is_err());
    assert_eq!(identity(&state), before);
}

#[test]
fn raw_wrapper_rejects_duplicates_unknown_fields_types_and_identity_before_reserve() {
    let state = ApplicationMcp::default();
    let valid = wrapper(&state, &document().to_string());
    let mut invalid = vec![
        String::new(),
        "null".into(),
        "{}".into(),
        "[2]".into(),
        format!("{} trailing", valid),
        format!("{} {{}}", valid),
        // Derived struct sequence decoding must not admit a non-object wrapper.
        json!([
            2,
            "bootstrap-object",
            state.instance_id(),
            document().to_string(),
            false
        ])
        .to_string(),
    ];
    for field in ["apiVersion", "requestId", "instanceId", "documentJson"] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(field);
        invalid.push(missing.to_string());
        let mut wrong = valid.clone();
        wrong[field] = Value::Null;
        invalid.push(wrong.to_string());
    }
    for (field, value) in [
        ("apiVersion", json!(1)),
        ("apiVersion", json!(2.0)),
        ("instanceId", json!("wrong-instance")),
        ("requestId", json!("")),
        ("requestId", json!("bad id")),
        ("requestId", json!("r".repeat(129))),
        ("documentId", json!("caller-id")),
        ("unknown", json!(true)),
        ("cancelledBeforeDispatch", json!(null)),
        ("cancelledBeforeDispatch", json!("false")),
    ] {
        let mut wrong = valid.clone();
        wrong[field] = value;
        invalid.push(wrong.to_string());
    }
    invalid.push(valid.to_string().replacen(
        "\"apiVersion\":2",
        "\"apiVersion\":2,\"apiVersion\":2",
        1,
    ));
    invalid.push(valid.to_string().replacen(
        "\"apiVersion\":2",
        "\"apiVersion\":2,\"api\\u0056ersion\":2",
        1,
    ));
    for bytes in invalid {
        assert!(
            bootstrap_raw(&state, &bytes).is_err(),
            "unexpected admission: {bytes}"
        );
        assert_eq!(identity(&state), None);
        // Neither successful installation nor an abandoned reservation remains.
        drop(state.reserve_native_install().unwrap());
    }
}

#[test]
fn raw_document_bytes_keep_nested_duplicate_and_semantic_refusals() {
    let state = ApplicationMcp::default();
    let valid = document();
    let mut invalid = vec![
        String::new(),
        "{}".into(),
        "[]".into(),
        format!("{} trailing", valid),
        valid.to_string().replacen(
            "\"formatVersion\":1",
            "\"formatVersion\":1,\"formatVersion\":1",
            1,
        ),
        valid.to_string().replacen(
            "\"formatVersion\":1",
            "\"formatVersion\":1,\"format\\u0056ersion\":1",
            1,
        ),
    ];
    // A decoded duplicate deep in an object record cannot be hidden by Value.
    let stroke = valid["objects"][0]["target"]["strokeId"].to_string();
    invalid.push(valid.to_string().replacen(
        &format!("\"strokeId\":{stroke}"),
        &format!("\"strokeId\":{stroke},\"stroke\\u0049d\":{stroke}"),
        1,
    ));
    for (path, value) in [
        ("/formatVersion", json!(2)),
        ("/totalFrames", json!(0)),
        ("/objects/0/family", json!("unsupported")),
        ("/objects/0/fill/r", json!(2)),
        ("/objects/0/geometry/closed", json!(false)),
        ("/objects/0/geometry/segments", json!([])),
        ("/objects/0/target/frameScope/frame", json!(21)),
        (
            "/objects/0/geometry/segments/0/point/x",
            json!("not-number"),
        ),
    ] {
        let mut wrong = valid.clone();
        *wrong.pointer_mut(path).unwrap() = value;
        invalid.push(wrong.to_string());
    }
    let mut unknown = valid.clone();
    unknown["objects"][0]["fill"]["extra"] = json!("never-ignore");
    invalid.push(unknown.to_string());
    let mut duplicate_layer = valid.clone();
    duplicate_layer["layers"][1] = duplicate_layer["layers"][0].clone();
    invalid.push(duplicate_layer.to_string());
    for bytes in invalid {
        let request = wrapper(&state, &bytes).to_string();
        let error = bootstrap_raw(&state, &request).unwrap_err();
        assert_eq!(error.code, "invalid_request");
        assert_eq!(identity(&state), None);
        assert!(!error.message.contains("never-ignore"));
        drop(state.reserve_native_install().unwrap());
    }
}

#[test]
fn raw_bootstrap_enforces_original_utf8_byte_limit_including_escaped_document() {
    let state = ApplicationMcp::default();
    let mut exact = raw(&state);
    exact.push_str(&" ".repeat(MAX_OBJECT_BOOTSTRAP_BYTES - exact.len()));
    assert_eq!(exact.len(), MAX_OBJECT_BOOTSTRAP_BYTES);
    bootstrap_raw(&state, &exact).unwrap();
    let empty = ApplicationMcp::default();
    let mut over = raw(&empty);
    over.push_str(&" ".repeat(MAX_OBJECT_BOOTSTRAP_BYTES - over.len()));
    over.push(' ');
    assert_eq!(
        bootstrap_raw(&empty, &over).unwrap_err().code,
        "invalid_request"
    );
    let mut multibyte = wrapper(&empty, &document().to_string());
    multibyte["documentJson"] = json!("é".repeat(MAX_OBJECT_BOOTSTRAP_BYTES / 2));
    let bytes = multibyte.to_string();
    assert!(
        bytes.chars().count() < MAX_OBJECT_BOOTSTRAP_BYTES
            && bytes.len() > MAX_OBJECT_BOOTSTRAP_BYTES
    );
    assert_eq!(
        bootstrap_raw(&empty, &bytes).unwrap_err().message,
        "object bootstrap exceeds byte limit"
    );
    assert_eq!(identity(&empty), None);
}

// Production opacity core in a private test adapter; no compositor or desktop claim.
struct OpacityFixture(NativeOpacityApplication);
impl NativeDispatch for OpacityFixture {
    fn instance_id(&self) -> &str {
        self.0.instance_id()
    }
    fn document_id(&self) -> &str {
        self.0.document_id()
    }
    fn content_revision(&self) -> u64 {
        self.0.content_revision()
    }
    fn dispatch(&mut self, request: OpacityRequest) -> Result<ResponseEnvelope, String> {
        Ok(self.0.handle(request))
    }
    fn replace_document(&mut self, _: OpacityDocument) -> Result<Vec<JobReceipt>, String> {
        Err("unused".into())
    }
    fn start_next_export_frame(&mut self, _: &str) -> Result<Option<PendingFrame>, String> {
        Err("unused".into())
    }
    fn finish_export_frame(&mut self, _: PendingFrame) -> Result<JobReceipt, String> {
        Err("unused".into())
    }
    fn release_project(&mut self) -> Result<NativeReleaseProgress, String> {
        Err("unused".into())
    }
    fn release_progress(&self) -> Option<NativeReleaseProgress> {
        None
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
fn install_opacity(state: &ApplicationMcp) {
    let document = native_engine::codec::decode_project(include_bytes!(
        "../../native-engine/tests/fixtures/opacity-v2/project.json"
    ))
    .unwrap();
    let opacity = NativeOpacityApplication::new(state.instance_id(), document).unwrap();
    let reservation = state.reserve_native_install().unwrap();
    state
        .install_dispatch(reservation.generation(), Box::new(OpacityFixture(opacity)))
        .unwrap();
}

#[test]
fn invalid_cancelled_and_duplicate_raw_bootstrap_keep_real_owners_and_subscribers() {
    for object in [false, true] {
        let state = ApplicationMcp::default();
        if object {
            bootstrap_raw(&state, &raw(&state)).unwrap();
        } else {
            install_opacity(&state);
        }
        let before = identity(&state);
        let binding = subscribe(&state);
        let mut cancelled = wrapper(&state, &document().to_string());
        cancelled["cancelledBeforeDispatch"] = json!(true);
        assert_eq!(
            bootstrap_raw(&state, &cancelled.to_string())
                .unwrap_err()
                .code,
            "cancelled_before_dispatch"
        );
        assert!(bootstrap_raw(&state, &wrapper(&state, "{}").to_string()).is_err());
        assert_eq!(
            bootstrap_raw(&state, &raw(&state)).unwrap_err().code,
            "unavailable"
        );
        assert_eq!(identity(&state), before);
        require_retained(&state, &binding);
        if !object {
            let native = state.native_state();
            let mut guard = native.lock().unwrap();
            let active = guard
                .active_mut(before.unwrap().0)
                .unwrap()
                .as_any_mut()
                .downcast_mut::<OpacityFixture>()
                .unwrap();
            assert_eq!(
                serde_json::to_value(active.0.acquire_snapshot(0).unwrap().document()).unwrap(),
                serde_json::from_slice::<Value>(include_bytes!(
                    "../../native-engine/tests/fixtures/opacity-v2/project.json"
                ))
                .unwrap()
            );
        }
    }
}

#[test]
fn raw_bootstrap_receipt_remains_a_commit_fact_after_real_release_and_reentry() {
    let state = ApplicationMcp::default();
    let first = bootstrap_raw(&state, &raw(&state)).unwrap();
    let committed = serde_json::to_value(first).unwrap();
    let generation = identity(&state).unwrap().0;
    let request = NativeReleaseRequest {
        api_version: HOST_API_VERSION,
        request_id: "release-first".into(),
        instance_id: state.instance_id().into(),
        document_id: committed["documentId"].as_str().unwrap().into(),
        expected_revision: 0,
        cancelled_before_dispatch: false,
    };
    assert!(
        matches!(admit_release_request(&state.native_state(), &request).unwrap(),
        ReleaseAdmission::Execute { generation: actual } if actual == generation)
    );
    let release = complete_release(&state.native_state(), generation, &request, || {
        Ok((vec![], "already_absent"))
    })
    .unwrap();
    assert_eq!(release.status, "succeeded");
    assert!(release.authority_removal_completed && release.reentry_available);
    assert_eq!(release.document_id, committed["documentId"]);
    assert_eq!(release.undo_depth + release.redo_depth, 0);
    let second = bootstrap_raw(&state, &raw(&state)).unwrap();
    let new_owner = identity(&state).unwrap();
    assert!(new_owner.0 > generation);
    assert_ne!(second.document_id, committed["documentId"]);
    assert_eq!(second.document_id, new_owner.1);
    assert_eq!(committed["contentRevision"], 0);
    let ReleaseAdmission::Retry { receipt, .. } =
        admit_release_request(&state.native_state(), &request).unwrap()
    else {
        panic!("terminal request must replay");
    };
    assert_eq!(receipt["documentId"], committed["documentId"]);
    let mut normalized = receipt;
    normalized["retrieved"] = json!(false);
    assert_eq!(normalized, serde_json::to_value(release).unwrap());
    assert_eq!(identity(&state), Some(new_owner));
}
