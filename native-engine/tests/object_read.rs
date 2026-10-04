use crate::object_codec::{decode_project, encode_project};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use serde_json::{json, Value};

fn source() -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases["records"]})
}
fn snapshot() -> ObjectSnapshot {
    ObjectSnapshot::new(
        "n25b-instance",
        decode_project(&serde_json::to_vec(&source()).unwrap()).unwrap(),
    )
    .unwrap()
}
fn request(snapshot: &ObjectSnapshot) -> Value {
    json!({"apiVersion":2,"requestId":"read-opaque","instanceId":snapshot.instance_id(),
        "documentId":snapshot.document_id(),"operation":"query.document.object",
        "payload":{"atRevision":0,"stableTarget":source()["objects"][0]["target"]}})
}
fn query(snapshot: &ObjectSnapshot, value: &Value) -> Result<Value, DispatchErrorCode> {
    snapshot
        .query_json(&serde_json::to_vec(value).unwrap())
        .map(|response| serde_json::to_value(response).unwrap())
}

#[test]
fn read_correlates_complete_identity_revision_and_target_without_writing() {
    let owner = snapshot();
    let before = encode_project(owner.document()).unwrap();
    let value = request(&owner);
    let response = query(&owner, &value).unwrap();
    assert_eq!(
        response,
        json!({"apiVersion":2,"requestId":"read-opaque","instanceId":"n25b-instance",
        "documentId":owner.document_id(),"contentRevision":0,"ok":true,
        "result":{"atRevision":0,"documentSnapshotId":owner.snapshot_id(),"object":source()["objects"][0]}})
    );
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(encode_project(owner.document()).unwrap(), before);
    assert_eq!(query(&owner, &value).unwrap(), response);
    let mut detached = response.clone();
    detached["result"]["object"]["fill"]["a"] = json!(0);
    detached["result"]["object"]["geometry"]["segments"][0]["point"]["x"] = json!(999);
    assert_eq!(query(&owner, &value).unwrap(), response);
    assert_eq!(encode_project(owner.document()).unwrap(), before);
    let mut other = value;
    other["payload"]["stableTarget"] = source()["objects"][1]["target"].clone();
    assert_eq!(
        query(&owner, &other).unwrap()["result"]["object"],
        source()["objects"][1]
    );
}

#[test]
fn wrong_identity_revision_or_scope_fails_and_retained_snapshot_survives_new_incarnation() {
    let old = snapshot();
    let leased = old.clone();
    let request = request(&old);
    let before = query(&old, &request).unwrap();
    let fresh = snapshot();
    assert_ne!(fresh.document_id(), old.document_id());
    assert_ne!(fresh.snapshot_id(), old.snapshot_id());
    assert_eq!(
        query(&fresh, &request).unwrap_err(),
        DispatchErrorCode::WrongDocument
    );
    drop(old);
    assert_eq!(query(&leased, &request).unwrap(), before);
    for (pointer, value, code) in [
        (
            "/instanceId",
            json!("another-instance"),
            DispatchErrorCode::WrongInstance,
        ),
        (
            "/documentId",
            json!("another-document"),
            DispatchErrorCode::WrongDocument,
        ),
        ("/payload/atRevision", json!(1), DispatchErrorCode::NotFound),
        (
            "/payload/stableTarget/strokeId",
            json!("missing"),
            DispatchErrorCode::NotFound,
        ),
        (
            "/payload/stableTarget/frameScope/frame",
            json!(8),
            DispatchErrorCode::NotFound,
        ),
        (
            "/payload/stableTarget/frameScope/kind",
            json!("reference"),
            DispatchErrorCode::NotFound,
        ),
        (
            "/payload/atRevision",
            json!(9007199254740992u64),
            DispatchErrorCode::InvalidRequest,
        ),
        ("/apiVersion", json!(1), DispatchErrorCode::InvalidRequest),
        (
            "/operation",
            json!("command.document.apply"),
            DispatchErrorCode::InvalidRequest,
        ),
        ("/requestId", json!(""), DispatchErrorCode::InvalidRequest),
    ] {
        let mut candidate = request.clone();
        *candidate.pointer_mut(pointer).unwrap() = value;
        assert_eq!(query(&leased, &candidate).unwrap_err(), code, "{pointer}");
    }
    assert_eq!(query(&leased, &request).unwrap(), before);
}

#[test]
fn duplicate_read_keys_and_write_fields_are_rejected_before_query() {
    let owner = snapshot();
    let mut value = request(&owner);
    value["expectedRevision"] = json!(0);
    assert_eq!(
        query(&owner, &value).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    let raw = serde_json::to_string(&request(&owner)).unwrap();
    for (needle, replacement) in [
        ("\"atRevision\":0", "\"atRevision\":0,\"atRevision\":0"),
        ("\"frame\":7", "\"frame\":7,\"frame\":7"),
        ("\"apiVersion\":2", "\"apiVersion\":2,\"apiVersion\":2"),
    ] {
        assert!(raw.contains(needle));
        assert_eq!(
            owner
                .query_json(raw.replacen(needle, replacement, 1).as_bytes())
                .unwrap_err(),
            DispatchErrorCode::InvalidRequest
        );
    }
}

#[test]
fn directly_deserialized_invalid_document_cannot_bypass_owner_admission() {
    let mut value = source();
    value["objects"][0]["target"]["layerUid"] = json!("unresolved");
    let document = serde_json::from_value(value).unwrap();
    assert!(ObjectSnapshot::new("instance", document).is_err());
    assert!(ObjectSnapshot::new("", snapshot().document().clone()).is_err());
}

#[cfg(feature = "application")]
mod active_dispatcher {
    use super::*;
    use crate::application::{ExportResourceResolver, NativeApplication, ResourceResolutionError};
    use crate::commands::OpacityRequest;
    use crate::export_job::{ExportArtifact, ExportCompositor, ExportReadback, StagedArtifactPort};
    use crate::protocol::OpaqueResourceHandle;
    use crate::render_scene::{GeometryPaintInput, RenderScene};
    struct Unused;
    impl StagedArtifactPort for Unused {
        fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
            panic!("read staged output")
        }
        fn write_frame(&mut self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
            panic!("read wrote output")
        }
        fn publish(&mut self, _: &str, _: &str, _: &[String]) -> Result<ExportArtifact, String> {
            panic!("read published output")
        }
        fn cleanup(&mut self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }
    impl ExportCompositor for Unused {
        type Composition = ();
        fn compose(&mut self, _: &RenderScene) -> Result<(), String> {
            panic!("read rendered")
        }
        fn readback_rgba8(&self, _: &()) -> Result<ExportReadback, String> {
            panic!("read pixels")
        }
    }
    impl ExportResourceResolver for Unused {
        fn resolve_geometry(
            &mut self,
            _: &OpaqueResourceHandle,
        ) -> Result<GeometryPaintInput, ResourceResolutionError> {
            panic!("read resolved geometry")
        }
    }

    #[test]
    fn staged_objects_are_unavailable_in_the_active_opacity_dispatcher() {
        let document =
            crate::codec::decode_project(include_bytes!("fixtures/opacity-v2/project.json"))
                .unwrap();
        let mut app =
            NativeApplication::new("active-opacity", document, Unused, Unused, Unused).unwrap();
        let initial = app.acquire_snapshot(0).unwrap();
        let before = crate::codec::encode_project(initial.document()).unwrap();
        let read = OpacityRequest::query(
            "object-read",
            app.instance_id(),
            app.document_id(),
            "query.document.object",
            json!({"atRevision":0,"stableTarget":source()["objects"][0]["target"]}),
        );
        let write = OpacityRequest::command(
            "object-write",
            app.instance_id(),
            app.document_id(),
            0,
            json!({"command":"object.fill.set","stableTarget":source()["objects"][0]["target"],
                "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}}),
        );
        for request in [read, write] {
            let response = app.dispatch(request);
            assert!(!response.is_ok());
            assert_eq!(
                response.error().unwrap().code(),
                DispatchErrorCode::InvalidRequest
            );
            assert_eq!(app.content_revision(), 0);
        }
        assert_eq!(
            crate::codec::encode_project(app.acquire_snapshot(0).unwrap().document()).unwrap(),
            before
        );
    }
}

#[test]
fn positional_read_envelopes_payloads_and_targets_fail_without_mutation() {
    let owner = snapshot();
    let original = request(&owner);
    let before = encode_project(owner.document()).unwrap();
    let expected = query(&owner, &original).unwrap();
    for (pointer, keys) in [
        (
            "",
            vec![
                "apiVersion",
                "requestId",
                "instanceId",
                "documentId",
                "operation",
                "payload",
            ],
        ),
        ("/payload", vec!["atRevision", "stableTarget"]),
        (
            "/payload/stableTarget",
            vec!["contextId", "frameScope", "layerUid", "strokeId"],
        ),
        ("/payload/stableTarget/frameScope", vec!["kind", "frame"]),
    ] {
        let mut candidate = original.clone();
        let object = candidate.pointer(pointer).unwrap();
        let positional = Value::Array(keys.iter().map(|key| object[*key].clone()).collect());
        *candidate.pointer_mut(pointer).unwrap() = positional;
        assert_eq!(
            query(&owner, &candidate).unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "{pointer}"
        );
    }
    assert_eq!(query(&owner, &original).unwrap(), expected);
    assert_eq!(encode_project(owner.document()).unwrap(), before);
}

#[test]
fn nonstring_scope_kinds_cannot_select_an_authored_or_reference_record() {
    let owner = snapshot();
    let original = request(&owner);
    let expected = query(&owner, &original).unwrap();
    let before = encode_project(owner.document()).unwrap();
    for invalid in [
        json!({"authored":null}),
        json!({"reference":null}),
        json!(["authored"]),
        Value::Null,
        json!(7),
        json!(true),
        json!("other"),
    ] {
        let mut candidate = original.clone();
        candidate["payload"]["stableTarget"]["frameScope"]["kind"] = invalid.clone();
        assert_eq!(
            query(&owner, &candidate).unwrap_err(),
            DispatchErrorCode::InvalidRequest,
            "kind {invalid}"
        );
    }
    assert_eq!(query(&owner, &original).unwrap(), expected);
    assert_eq!(encode_project(owner.document()).unwrap(), before);
}

fn common_read(owner: &ObjectSnapshot, value: Value) -> Value {
    let request: OpacityRequest = serde_json::from_value(value).unwrap();
    let response = owner.dispatch(request).unwrap();
    let bytes = serde_json::to_vec(&response).unwrap();
    assert!(bytes.len() <= 4096);
    serde_json::from_slice(&bytes).unwrap()
}

#[test]
fn common_envelope_preserves_frozen_records_and_old_incarnations() {
    let owner = snapshot();
    let retained = owner.clone();
    let before = encode_project(owner.document()).unwrap();
    for record in source()["objects"].as_array().unwrap() {
        let mut value = request(&owner);
        value["payload"]["stableTarget"] = record["target"].clone();
        let actual = common_read(&owner, value.clone());
        assert_eq!(actual, query(&owner, &value).unwrap());
        assert_eq!(actual["result"]["object"], *record);
        assert_eq!(common_read(&owner, value.clone()), actual);
        let mut detached = actual.clone();
        detached["result"]["object"]["fill"]["a"] = json!(0);
        detached["result"]["object"]["geometry"]["segments"][0]["point"]["x"] = json!(999);
        assert_eq!(common_read(&owner, value), actual);
    }
    let value = request(&owner);
    let expected = common_read(&owner, value.clone());
    let fresh = snapshot();
    let stale = common_read(&fresh, value.clone());
    assert_eq!(stale["error"]["code"], "wrong_document");
    assert_eq!(stale["documentId"], fresh.document_id());
    assert_eq!(
        stale["error"]["details"]["requestedDocumentId"],
        owner.document_id()
    );
    drop(owner);
    assert_eq!(common_read(&retained, value), expected);
    assert_eq!(encode_project(retained.document()).unwrap(), before);
}

#[test]
fn common_envelope_failures_are_correlated_and_never_mutate() {
    let owner = snapshot();
    let before = encode_project(owner.document()).unwrap();
    let original = request(&owner);
    for (pointer, value, code) in [
        ("/apiVersion", json!(1), "invalid_request"),
        ("/instanceId", json!("other-instance"), "wrong_instance"),
        ("/documentId", json!("other-document"), "wrong_document"),
        ("/payload/atRevision", json!(1), "not_found"),
        (
            "/payload/atRevision",
            json!(9007199254740992u64),
            "invalid_request",
        ),
        (
            "/payload/stableTarget/strokeId",
            json!("missing"),
            "not_found",
        ),
        (
            "/payload/stableTarget/frameScope/kind",
            json!("reference"),
            "not_found",
        ),
        (
            "/payload/stableTarget/frameScope/frame",
            json!(8),
            "not_found",
        ),
        (
            "/payload/stableTarget/frameScope/kind",
            json!({"authored":null}),
            "invalid_request",
        ),
        (
            "/payload/stableTarget",
            json!(["scene-root", 7, "0", "0"]),
            "invalid_request",
        ),
        ("/payload", Value::Null, "invalid_request"),
        (
            "/payload",
            json!({"atRevision":0,"stableTarget":original["payload"]["stableTarget"],"extra":true}),
            "invalid_request",
        ),
        (
            "/operation",
            json!("command.document.apply"),
            "invalid_request",
        ),
        ("/operation", json!("object.fill.set"), "invalid_request"),
    ] {
        let mut candidate = original.clone();
        *candidate.pointer_mut(pointer).unwrap() = value;
        let response = common_read(&owner, candidate);
        assert_eq!(response["error"]["code"], code, "{pointer}");
        assert_eq!(response["requestId"], original["requestId"]);
        assert_eq!(response["instanceId"], owner.instance_id());
        assert_eq!(response["documentId"], owner.document_id());
        assert_eq!(response["apiVersion"], 2);
        assert_eq!(response["contentRevision"], 0);
        assert_eq!(response["ok"], false);
        assert!(response.get("result").is_none());
    }
    let mut write_revision = original.clone();
    write_revision["expectedRevision"] = json!(0);
    assert_eq!(
        common_read(&owner, write_revision)["error"]["code"],
        "invalid_request"
    );
    let cancelled = serde_json::from_value::<OpacityRequest>(original.clone())
        .unwrap()
        .cancelled();
    let response = owner.dispatch(cancelled).unwrap();
    assert_eq!(
        response.error().unwrap().code(),
        DispatchErrorCode::CancelledBeforeDispatch
    );
    assert_eq!(
        common_read(&owner, original.clone()),
        query(&owner, &original).unwrap()
    );
    assert_eq!(encode_project(owner.document()).unwrap(), before);
}

#[test]
fn common_envelope_request_limit_includes_exact_threshold() {
    let owner = snapshot();
    let before = encode_project(owner.document()).unwrap();
    let mut value = request(&owner);
    value["payload"]["stableTarget"]["strokeId"] = json!("");
    let base_len = serde_json::to_vec(&value).unwrap().len();
    for (limit, code) in [
        (4095, "not_found"),
        (4096, "not_found"),
        (4097, "invalid_request"),
    ] {
        value["payload"]["stableTarget"]["strokeId"] = json!("x".repeat(limit - base_len));
        assert_eq!(serde_json::to_vec(&value).unwrap().len(), limit);
        assert_eq!(common_read(&owner, value.clone())["error"]["code"], code);
    }
    assert_eq!(encode_project(owner.document()).unwrap(), before);
}

#[test]
fn common_envelope_rejects_unrepresentable_identity_before_response() {
    let owner = snapshot();
    let before = encode_project(owner.document()).unwrap();
    for pointer in ["/requestId", "/instanceId", "/documentId"] {
        for invalid in [
            String::new(),
            "bad id".into(),
            "x".repeat(129),
            "x".repeat(5000),
        ] {
            let mut value = request(&owner);
            *value.pointer_mut(pointer).unwrap() = json!(invalid);
            let response = owner.dispatch(serde_json::from_value(value).unwrap());
            assert_eq!(response.unwrap_err(), DispatchErrorCode::InvalidRequest);
        }
    }
    let mut threshold = request(&owner);
    threshold["requestId"] = json!("x".repeat(128));
    assert_eq!(common_read(&owner, threshold)["ok"], true);
    let standalone =
        ObjectSnapshot::new("unbounded instance".repeat(500), owner.document().clone()).unwrap();
    let request = serde_json::from_value(request(&standalone)).unwrap();
    assert_eq!(
        standalone.dispatch(request).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(encode_project(owner.document()).unwrap(), before);
}

#[test]
fn common_envelope_never_truncates_an_oversized_codec_admitted_record() {
    let mut value = source();
    let segment = value["objects"][0]["geometry"]["segments"][0].clone();
    value["objects"][0]["geometry"]["segments"] = Value::Array(vec![segment; 256]);
    let owner = ObjectSnapshot::new(
        "bounded-instance",
        decode_project(&serde_json::to_vec(&value).unwrap()).unwrap(),
    )
    .unwrap();
    let before = encode_project(owner.document()).unwrap();
    let raw = request(&owner);
    let unbounded = query(&owner, &raw).unwrap();
    assert_eq!(unbounded["result"]["object"], value["objects"][0]);
    assert!(serde_json::to_vec(&unbounded).unwrap().len() > 4096);
    let response = common_read(&owner, raw);
    assert_eq!(response["error"]["code"], "unavailable");
    assert!(response.get("result").is_none());
    assert_eq!(encode_project(owner.document()).unwrap(), before);
}
