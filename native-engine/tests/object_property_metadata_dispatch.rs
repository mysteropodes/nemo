//! Public snapshot-envelope controls; raw duplicate transport admission is separate.
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use serde_json::{json, Value};

const OP: &str = "query.document.object.property.metadata";
fn corpus() -> Value {
    serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-property-metadata-v1/cases.json"
    ))
    .unwrap()
}
fn pin(fixed: &Value, case: &Value, instance: &str) -> ObjectSnapshot {
    let document = case["documentVariant"]
        .as_str()
        .map_or(&fixed["document"], |name| &fixed["documentVariants"][name]);
    ObjectSnapshot::new(
        instance,
        decode_storage_project(document.to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn payload(pin: &ObjectSnapshot, case: &Value) -> Value {
    json!({"atRevision":pin.content_revision(),"documentSnapshotId":pin.snapshot_id(),
        "view":case["request"]["view"]})
}
fn request(pin: &ObjectSnapshot, payload: Value, id: &str) -> OpacityRequest {
    OpacityRequest::query(id, pin.instance_id(), pin.document_id(), OP, payload)
}
// Replace only generated runtime identity fields in independently literal oracles.
fn expected(pin: &ObjectSnapshot, case: &Value, id: &str) -> Value {
    let mut result = case["expectedResult"].clone();
    result["instanceId"] = json!(pin.instance_id());
    result["documentId"] = json!(pin.document_id());
    result["atRevision"] = json!(pin.content_revision());
    result["documentSnapshotId"] = json!(pin.snapshot_id());
    json!({"apiVersion":2,"requestId":id,"instanceId":pin.instance_id(),
        "documentId":pin.document_id(),"contentRevision":pin.content_revision(),
        "ok":true,"result":result})
}
fn check_header(pin: &ObjectSnapshot, response: &ResponseEnvelope, id: &str) -> Value {
    let bytes = serde_json::to_vec(response).unwrap();
    assert!(bytes.len() <= 4096);
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(value["apiVersion"], 2);
    assert_eq!(value["requestId"], id);
    assert_eq!(value["instanceId"], pin.instance_id());
    assert_eq!(value["documentId"], pin.document_id());
    assert_eq!(value["contentRevision"], pin.content_revision());
    value
}
fn refusal(pin: &ObjectSnapshot, request: OpacityRequest, code: DispatchErrorCode) {
    let before = encode_storage_project(pin.document()).unwrap();
    let id = request.request_id.clone();
    let requested_document = request.document_id.clone();
    let response = pin.dispatch(request).unwrap();
    let value = check_header(pin, &response, &id);
    assert!(!response.ok);
    assert_eq!(response.error.as_ref().unwrap().code(), code);
    assert!(response.result.is_none());
    assert_eq!(value.as_object().unwrap().len(), 7);
    if code == DispatchErrorCode::WrongDocument {
        assert_eq!(value["error"].as_object().unwrap().len(), 3);
        assert_eq!(
            value["error"]["details"],
            json!({"requestedDocumentId":requested_document})
        );
    } else {
        assert_eq!(value["error"].as_object().unwrap().len(), 2);
    }
    assert!(value["error"]["message"]
        .as_str()
        .is_some_and(|s| !s.is_empty()));
    assert_eq!(encode_storage_project(pin.document()).unwrap(), before);
}

#[test]
fn seventeen_literal_successes_preserve_complete_correlated_envelopes_and_document() {
    let fixed = corpus();
    assert_eq!(fixed["cases"].as_array().unwrap().len(), 33);
    let mut successes = 0;
    for case in fixed["cases"].as_array().unwrap() {
        if case.get("expectedResult").is_none() {
            continue;
        }
        successes += 1;
        let pin = pin(&fixed, case, "d35-fixture");
        let before = encode_storage_project(pin.document()).unwrap();
        let response = pin
            .dispatch(request(&pin, payload(&pin, case), "metadata-literal"))
            .unwrap();
        let value = check_header(&pin, &response, "metadata-literal");
        assert_eq!(
            value,
            expected(&pin, case, "metadata-literal"),
            "{}",
            case["name"]
        );
        assert!(serde_json::to_vec(&value["result"]).unwrap().len() <= 3072);
        assert_eq!(pin.content_revision(), 0);
        assert_eq!(encode_storage_project(pin.document()).unwrap(), before);
        // Returned data is detached: caller mutations never write the owner.
        let mut detached = value;
        detached["result"]["items"] = json!([]);
        assert_eq!(encode_storage_project(pin.document()).unwrap(), before);
        assert_eq!(
            serde_json::to_value(
                pin.dispatch(request(&pin, payload(&pin, case), "again"))
                    .unwrap()
            )
            .unwrap(),
            expected(&pin, case, "again")
        );
    }
    assert_eq!(successes, 17);
}

#[test]
fn public_payload_has_exactly_three_required_nonnull_named_fields() {
    let fixed = corpus();
    let pin = pin(&fixed, &fixed["cases"][0], "d35-fixture");
    let valid = payload(&pin, &fixed["cases"][0]);
    for key in [
        "instanceId",
        "documentId",
        "expectedRevision",
        "stableTarget",
        "extra",
    ] {
        let mut bad = valid.clone();
        bad[key] = json!(0);
        refusal(
            &pin,
            request(&pin, bad, "extra"),
            DispatchErrorCode::InvalidRequest,
        );
    }
    for key in ["atRevision", "documentSnapshotId", "view"] {
        let mut missing = valid.clone();
        missing.as_object_mut().unwrap().remove(key);
        refusal(
            &pin,
            request(&pin, missing, "missing"),
            DispatchErrorCode::InvalidRequest,
        );
        let mut null = valid.clone();
        null[key] = Value::Null;
        refusal(
            &pin,
            request(&pin, null, "null"),
            DispatchErrorCode::InvalidRequest,
        );
    }
    for bad in [
        json!([0, pin.snapshot_id(), valid["view"]]),
        json!(false),
        json!(null),
    ] {
        refusal(
            &pin,
            request(&pin, bad, "positional"),
            DispatchErrorCode::InvalidRequest,
        );
    }
}

#[test]
fn nested_views_and_targets_keep_map_only_closed_admission() {
    let fixed = corpus();
    let pin = pin(&fixed, &fixed["cases"][0], "d35-fixture");
    for index in [14, 15, 16, 17, 21, 22, 23] {
        refusal(
            &pin,
            request(&pin, payload(&pin, &fixed["cases"][index]), "view"),
            DispatchErrorCode::InvalidRequest,
        );
    }
    let valid = payload(&pin, &fixed["cases"][0]);
    for view in [
        json!({"kind":"targets","offset":0,"limit":1,"extra":0}),
        json!({"offset":0,"limit":1}),
        json!({"kind":"targets","offset":0}),
        json!({"kind":"targets","offset":0,"limit":null}),
    ] {
        let mut bad = valid.clone();
        bad["view"] = view;
        refusal(
            &pin,
            request(&pin, bad, "closed-view"),
            DispatchErrorCode::InvalidRequest,
        );
    }
}

#[test]
fn outer_owner_cancellation_and_command_revision_precede_selector_admission() {
    let fixed = corpus();
    let pin = pin(&fixed, &fixed["cases"][0], "d35-fixture");
    let mut wrong_instance = request(&pin, json!({}), "wrong-instance");
    wrong_instance.instance_id = "different-instance".into();
    refusal(&pin, wrong_instance, DispatchErrorCode::WrongInstance);
    let mut wrong_document = request(&pin, json!({}), "wrong-document");
    wrong_document.document_id = "different-document".into();
    refusal(&pin, wrong_document, DispatchErrorCode::WrongDocument);
    refusal(
        &pin,
        request(&pin, json!({}), "cancelled").cancelled(),
        DispatchErrorCode::CancelledBeforeDispatch,
    );
    let mut write = request(&pin, payload(&pin, &fixed["cases"][0]), "write-revision");
    write.expected_revision = Some(0);
    refusal(&pin, write, DispatchErrorCode::InvalidRequest);
    let mut cancelled_write = request(&pin, json!({}), "cancelled-write").cancelled();
    cancelled_write.expected_revision = Some(0);
    refusal(
        &pin,
        cancelled_write,
        DispatchErrorCode::CancelledBeforeDispatch,
    );
}

#[test]
fn revision_snapshot_pins_are_exact_and_nonnumeric_inputs_are_not_coerced() {
    let fixed = corpus();
    let pin = pin(&fixed, &fixed["cases"][0], "d35-fixture");
    let valid = payload(&pin, &fixed["cases"][0]);
    for (key, value, code) in [
        ("atRevision", json!(1), DispatchErrorCode::StaleRevision),
        (
            "documentSnapshotId",
            json!("different-snapshot"),
            DispatchErrorCode::StaleRevision,
        ),
        (
            "atRevision",
            json!(9007199254740992u64),
            DispatchErrorCode::InvalidRequest,
        ),
        ("atRevision", json!(-1), DispatchErrorCode::InvalidRequest),
        ("atRevision", json!(0.5), DispatchErrorCode::InvalidRequest),
        ("atRevision", json!("0"), DispatchErrorCode::InvalidRequest),
        (
            "documentSnapshotId",
            json!(0),
            DispatchErrorCode::InvalidRequest,
        ),
    ] {
        let mut bad = valid.clone();
        bad[key] = value;
        refusal(&pin, request(&pin, bad, "pin"), code);
    }
}

#[test]
fn missing_targets_and_unrepresentable_inner_rows_return_correlated_failures() {
    let fixed = corpus();
    for (index, code) in [
        (8, DispatchErrorCode::NotFound),
        (9, DispatchErrorCode::NotFound),
        (10, DispatchErrorCode::NotFound),
        (11, DispatchErrorCode::NotFound),
        (20, DispatchErrorCode::Unavailable),
        (24, DispatchErrorCode::Unavailable),
    ] {
        let case = &fixed["cases"][index];
        let pin = pin(&fixed, case, "d35-fixture");
        refusal(
            &pin,
            request(&pin, payload(&pin, case), "missing-or-large"),
            code,
        );
    }
}

#[test]
fn largest_admitted_correlation_headers_fit_complete_response_bound() {
    let fixed = corpus();
    let instance = "i".repeat(128);
    let id = "r".repeat(128);
    let pin = pin(&fixed, &fixed["cases"][0], &instance);
    let response = pin
        .dispatch(request(&pin, payload(&pin, &fixed["cases"][0]), &id))
        .unwrap();
    assert_eq!(
        check_header(&pin, &response, &id),
        expected(&pin, &fixed["cases"][0], &id)
    );
    // Native document IDs are generated, not caller-settable. This independent
    // byte bound uses all three permitted maximum ASCII correlation IDs, the
    // largest u64 revision, and the full unchanged 3072-byte inner allowance.
    let headers = json!({"apiVersion":2,"requestId":"r".repeat(128),
        "instanceId":"i".repeat(128),"documentId":"d".repeat(128),
        "contentRevision":u64::MAX,"ok":true,"result":null});
    let overhead = serde_json::to_vec(&headers).unwrap().len() - 4;
    assert_eq!(overhead, 506);
    let maximum = overhead + 3072;
    assert_eq!(maximum, 3578);
    assert!(
        maximum <= 4096,
        "full inner allowance plus maximum headers: {maximum}"
    );
}

#[test]
fn unrepresentable_correlation_and_oversized_request_fail_without_mutation() {
    let fixed = corpus();
    let pin = pin(&fixed, &fixed["cases"][0], "d35-fixture");
    let before = encode_storage_project(pin.document()).unwrap();
    for key in ["request", "instance", "document"] {
        let mut bad = request(&pin, payload(&pin, &fixed["cases"][0]), "bad-header");
        match key {
            "request" => bad.request_id = "r".repeat(129),
            "instance" => bad.instance_id = "i".repeat(129),
            _ => bad.document_id = "d".repeat(129),
        }
        assert_eq!(pin.dispatch(bad), Err(DispatchErrorCode::InvalidRequest));
    }
    let mut large = payload(&pin, &fixed["cases"][0]);
    large["view"]["extra"] = json!("x".repeat(4096));
    refusal(
        &pin,
        request(&pin, large, "large-request"),
        DispatchErrorCode::InvalidRequest,
    );
    assert_eq!(encode_storage_project(pin.document()).unwrap(), before);
}

#[cfg(feature = "history")]
#[test]
fn wrapper_reads_keep_retained_pin_real_history_and_redo_unchanged() {
    use crate::history::NativeObjectHistory;
    let fixed = corpus();
    let case = &fixed["cases"][4];
    let mut owner = NativeObjectHistory::new(
        "d35-fixture",
        decode_storage_project(fixed["document"].to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    let old = owner.acquire_snapshot(0).unwrap();
    let before = encode_storage_project(old.document()).unwrap();
    let old_payload = payload(&old, case);
    let edit = json!({"command":"object.fill.set","stableTarget":fixed["document"]["objects"][0]["target"],
        "fill":{"kind":"solid","r":0.8,"g":0.1,"b":0.3,"a":1}});
    let candidate = owner
        .prepare_fill_json(edit.to_string().as_bytes())
        .unwrap();
    assert_eq!(owner.commit_fill(candidate), Ok(1));
    let current = owner.acquire_snapshot(1).unwrap();
    refusal(
        &current,
        request(&current, old_payload.clone(), "stale-pin"),
        DispatchErrorCode::StaleRevision,
    );
    assert_eq!(owner.history_depths(), (1, 0));
    assert_eq!(owner.undo(1), Ok(2));
    let undone = owner.acquire_snapshot(2).unwrap();
    let response = undone
        .dispatch(request(&undone, payload(&undone, case), "undone"))
        .unwrap();
    assert_eq!(
        check_header(&undone, &response, "undone"),
        expected(&undone, case, "undone")
    );
    refusal(
        &undone,
        request(&undone, payload(&undone, case), "cancelled").cancelled(),
        DispatchErrorCode::CancelledBeforeDispatch,
    );
    assert_eq!(owner.content_revision(), 2);
    assert_eq!(owner.history_depths(), (0, 1));
    assert_eq!(encode_storage_project(undone.document()).unwrap(), before);
    let retained = old
        .dispatch(request(&old, old_payload, "retained"))
        .unwrap();
    assert_eq!(
        check_header(&old, &retained, "retained"),
        expected(&old, case, "retained")
    );
    assert_eq!(encode_storage_project(old.document()).unwrap(), before);
    assert_eq!(owner.redo(2), Ok(3));
    assert_eq!(owner.history_depths(), (1, 0));
}
