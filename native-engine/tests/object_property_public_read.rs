//! Literal public read oracles; use admitted documents and the existing snapshot.
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use serde_json::{json, Value};
const OP: &str = "query.document.object.property.read";
fn source(records: Value) -> Value {
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":16,
        "layers":[{"layerUid":"L0","locked":true}],"objects":[],
        "propertyHolders":[{"target":{"contextId":"scene-root","layerUid":"L0"},"properties":records}]})
}
fn pin(records: Value) -> ObjectSnapshot {
    ObjectSnapshot::new(
        "public-property-read",
        decode_storage_project(source(records).to_string().as_bytes()).unwrap(),
    )
    .unwrap()
}
fn keys() -> Value {
    json!([
        {"frame":0,"v":[0],"curvePoints":[],"hOut":[3,4]},
        {"frame":4,"v":[80],"hold":false,"curvePoints":null,"hIn":[0,0]},
        {"frame":12,"v":[0],"hold":true}
    ])
}
fn keyed() -> ObjectSnapshot {
    pin(
        json!([{"property":"opacity","static":[17],"expression":{"enabled":true,"code":"999"},"track":{"keys":keys()}}]),
    )
}
fn view(kind: &str) -> Value {
    let mut value = json!({"kind":kind,"target":{"contextId":"scene-root","layerUid":"L0"},"property":"opacity"});
    if kind == "rawSample" {
        value["frame"] = json!(2);
    } else {
        value["offset"] = json!(0);
        value["limit"] = json!(2);
    }
    value
}
fn selector(pin: &ObjectSnapshot, view: Value) -> Value {
    json!({"instanceId":pin.instance_id(),"documentId":pin.document_id(),"atRevision":pin.content_revision(),"documentSnapshotId":pin.snapshot_id(),"view":view})
}
fn read(pin: &ObjectSnapshot, view: Value) -> Result<Value, DispatchErrorCode> {
    pin.query_property_read_json(selector(pin, view).to_string().as_bytes())
}
fn public(pin: &ObjectSnapshot, view: Value) -> OpacityRequest {
    OpacityRequest::query(
        "public-read",
        pin.instance_id(),
        pin.document_id(),
        OP,
        json!({"atRevision":pin.content_revision(),"documentSnapshotId":pin.snapshot_id(),"view":view}),
    )
}
#[cfg(feature = "evaluation")]
#[test]
fn raw_public_samples_keep_literal_native_values_and_ignore_active_expression() {
    let pin = keyed();
    let saved = encode_storage_project(pin.document()).unwrap();
    for (frame, expected) in [
        (-0.5, 0.0),
        (0.5, 10.0),
        (2.0, 40.0),
        (4.0, 80.0),
        (20.0, 0.0),
    ] {
        let mut v = view("rawSample");
        v["frame"] = json!(frame);
        let response = pin.dispatch(public(&pin, v)).unwrap();
        assert!(response.ok);
        let r = response.result.unwrap();
        assert_eq!(r["frame"].as_f64(), Some(frame));
        assert!((r["value"][0].as_f64().unwrap() - expected).abs() < 1e-10);
        assert_eq!(r["dimension"], 1);
        assert_eq!(r["atRevision"], pin.content_revision());
        assert_eq!(r["documentSnapshotId"], pin.snapshot_id());
    }
    assert_eq!(encode_storage_project(pin.document()).unwrap(), saved);
}
#[test]
fn authored_pages_preserve_exact_keys_missing_null_empty_and_full_cursor() {
    let pin = keyed();
    let saved = encode_storage_project(pin.document()).unwrap();
    let first = read(&pin, view("authoredKeys")).unwrap();
    assert_eq!(first["items"], json!([keys()[0], keys()[1]]));
    assert_eq!(first["trackPresent"], true);
    assert_eq!(first["nextOffset"], 2);
    assert!(first["items"][0].get("hold").is_none());
    assert_eq!(first["items"][0]["curvePoints"], json!([]));
    assert!(first["items"][1]["curvePoints"].is_null());
    let mut v = view("authoredKeys");
    v["offset"] = json!(2);
    let last = read(&pin, v.clone()).unwrap();
    assert_eq!(last["items"], json!([keys()[2]]));
    assert!(last["nextOffset"].is_null());
    assert!(last["items"][0].get("curvePoints").is_none());
    v["offset"] = json!(3);
    assert_eq!(read(&pin, v.clone()).unwrap()["items"], json!([]));
    v["offset"] = json!(4);
    assert_eq!(read(&pin, v), Err(DispatchErrorCode::InvalidRequest));
    assert_eq!(encode_storage_project(pin.document()).unwrap(), saved);
}
#[test]
fn absent_and_present_empty_tracks_are_distinct_without_invented_keys() {
    for (records, present) in [
        (json!([]), false),
        (json!([{"property":"opacity","static":[17]}]), false),
        (json!([{"property":"opacity","track":{"keys":[]}}]), true),
    ] {
        let pin = pin(records);
        let r = read(&pin, view("authoredKeys")).unwrap();
        assert_eq!(r["items"], json!([]));
        assert_eq!(r["trackPresent"], present);
        assert!(r["nextOffset"].is_null());
    }
}
#[test]
fn original_bytes_reject_duplicate_unknown_positional_and_null_fields() {
    let pin = keyed();
    let q = selector(&pin, view("authoredKeys"));
    let text = q.to_string();
    for duplicate in [
        text.replace(
            "\"property\":\"opacity\"",
            "\"property\":\"opacity\",\"property\":\"opacity\"",
        ),
        text.replace("\"limit\":2", "\"limit\":2,\"limit\":2"),
        text.replace(
            "\"layerUid\":\"L0\"",
            "\"layerUid\":\"L0\",\"layerUid\":\"L0\"",
        ),
    ] {
        assert_ne!(duplicate, text);
        assert_eq!(
            pin.query_property_read_json(duplicate.as_bytes()),
            Err(DispatchErrorCode::InvalidRequest)
        );
    }
    let mut malformed = vec![];
    for pointer in ["", "/view", "/view/target"] {
        let mut v = q.clone();
        v.pointer_mut(pointer).unwrap()["unexpected"] = json!(1);
        malformed.push(v);
    }
    for (pointer, value) in [
        ("/view", json!([])),
        ("/view/target", json!([])),
        ("/view/target/strokeId", json!(null)),
        ("/view/limit", json!(0)),
        ("/view/limit", json!(9)),
        ("/view/offset", json!(-1)),
        ("/view/offset", json!(0.5)),
        ("/view/offset", json!(4294967296_u64)),
        ("/view/property", json!("")),
        ("/view/target/layerUid", json!("")),
    ] {
        let mut v = q.clone();
        if pointer == "/view/target/strokeId" {
            v["view"]["target"]["strokeId"] = value;
        } else {
            *v.pointer_mut(pointer).unwrap() = value;
        }
        malformed.push(v);
    }
    for v in malformed {
        assert_eq!(
            pin.query_property_read_json(v.to_string().as_bytes()),
            Err(DispatchErrorCode::InvalidRequest),
            "{v}"
        );
    }
}
#[test]
fn pins_targets_and_separate_property_owners_refuse_without_mutation() {
    let pin = keyed();
    let before = encode_storage_project(pin.document()).unwrap();
    for (field, value, code) in [
        (
            "instanceId",
            json!("other"),
            DispatchErrorCode::WrongInstance,
        ),
        (
            "documentId",
            json!("other"),
            DispatchErrorCode::WrongDocument,
        ),
        ("atRevision", json!(1), DispatchErrorCode::StaleRevision),
        (
            "documentSnapshotId",
            json!("other"),
            DispatchErrorCode::StaleRevision,
        ),
        (
            "atRevision",
            json!(9007199254740992_u64),
            DispatchErrorCode::InvalidRequest,
        ),
    ] {
        let mut q = selector(&pin, view("authoredKeys"));
        q[field] = value;
        assert_eq!(
            pin.query_property_read_json(q.to_string().as_bytes()),
            Err(code)
        );
    }
    let mut v = view("authoredKeys");
    v["target"]["layerUid"] = json!("absent");
    assert_eq!(read(&pin, v), Err(DispatchErrorCode::NotFound));
    for property in ["order", "timeRemap", "missing-property"] {
        let mut v = view("authoredKeys");
        v["property"] = json!(property);
        assert_eq!(read(&pin, v), Err(DispatchErrorCode::Unavailable));
    }
    assert_eq!(encode_storage_project(pin.document()).unwrap(), before);
}
#[test]
fn oversized_authored_key_refuses_instead_of_trimming_or_replacing_metadata() {
    let points: Vec<Value> = (0..200)
        .map(|i| json!({"x":i as f64/199.0,"y":i as f64/199.0}))
        .collect();
    let pin = pin(
        json!([{"property":"opacity","track":{"keys":[{"frame":0,"v":[17],"curvePoints":points}]}}]),
    );
    let before = encode_storage_project(pin.document()).unwrap();
    let mut v = view("authoredKeys");
    v["limit"] = json!(1);
    assert_eq!(read(&pin, v), Err(DispatchErrorCode::Unavailable));
    assert_eq!(encode_storage_project(pin.document()).unwrap(), before);
    let mut q = selector(&pin, view("authoredKeys"));
    q["view"]["property"] = json!("x".repeat(4096));
    assert!(q.to_string().len() > 4096);
    assert_eq!(
        pin.query_property_read_json(q.to_string().as_bytes()),
        Err(DispatchErrorCode::InvalidRequest)
    );
}
#[test]
fn common_envelope_preserves_identity_and_rejects_revision_cancellation_and_open_payload() {
    let pin = keyed();
    let q = public(&pin, view("authoredKeys"));
    let ok = pin.dispatch(q.clone()).unwrap();
    assert!(ok.ok);
    assert_eq!(ok.request_id, q.request_id);
    assert_eq!(ok.document_id, pin.document_id());
    assert_eq!(ok.content_revision, pin.content_revision());
    assert!(serde_json::to_vec(&ok).unwrap().len() <= 4096);
    let mut expected = q.clone();
    expected.expected_revision = Some(0);
    let mut cancelled = q.clone();
    cancelled.cancelled_before_dispatch = true;
    let mut open = q.clone();
    open.payload["unexpected"] = json!(true);
    for (q, code) in [
        (expected, DispatchErrorCode::InvalidRequest),
        (cancelled, DispatchErrorCode::CancelledBeforeDispatch),
        (open, DispatchErrorCode::InvalidRequest),
    ] {
        let r = pin.dispatch(q).unwrap();
        assert!(!r.ok);
        assert_eq!(r.error.unwrap().code(), code);
        assert!(r.result.is_none());
    }
}
#[cfg(all(feature = "history", feature = "evaluation"))]
#[test]
fn retained_read_survives_owner_edit_undo_redo_and_storage_roundtrip() {
    use crate::history::NativeObjectHistory;
    let mut source = source(json!([]));
    source["layers"][0]["locked"] = json!(false);
    let mut owner = NativeObjectHistory::new(
        "public-property-read",
        decode_storage_project(source.to_string().as_bytes()).unwrap(),
    )
    .unwrap();
    let old = owner.acquire_snapshot(0).unwrap();
    let old_bytes = encode_storage_project(old.document()).unwrap();
    let edit = json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L0"},"property":"opacity","action":{"kind":"setValue","frame":0,"values":[23.5]}});
    let candidate = owner
        .prepare_property_edit_json(edit.to_string().as_bytes())
        .unwrap();
    owner.commit_property_edit(candidate).unwrap();
    assert_eq!(
        read(&old, view("rawSample")).unwrap()["value"],
        json!([100.0])
    );
    assert_eq!(encode_storage_project(old.document()).unwrap(), old_bytes);
    let current = owner.acquire_snapshot(1).unwrap();
    assert_eq!(
        read(&current, view("rawSample")).unwrap()["value"],
        json!([23.5])
    );
    let depths = owner.history_depths();
    let saved = encode_storage_project(current.document()).unwrap();
    let reopened =
        ObjectSnapshot::new("reopened", decode_storage_project(&saved).unwrap()).unwrap();
    assert_eq!(
        read(&reopened, view("rawSample")).unwrap()["value"],
        json!([23.5])
    );
    assert_eq!(owner.history_depths(), depths);
    owner.undo(1).unwrap();
    assert_eq!(
        read(&owner.acquire_snapshot(2).unwrap(), view("rawSample")).unwrap()["value"],
        json!([100.0])
    );
    owner.redo(2).unwrap();
    assert_eq!(
        read(&owner.acquire_snapshot(3).unwrap(), view("rawSample")).unwrap()["value"],
        json!([23.5])
    );
}
#[cfg(not(feature = "evaluation"))]
#[test]
fn no_evaluation_build_reports_raw_sample_unavailable_but_authored_read_works() {
    let pin = keyed();
    assert_eq!(
        read(&pin, view("rawSample")),
        Err(DispatchErrorCode::Unavailable)
    );
    assert!(read(&pin, view("authoredKeys")).is_ok());
}

#[cfg(all(feature = "evaluation", feature = "history"))]
#[test]
fn real_project_expression_context_flows_through_the_original_public_dispatch() {
    use crate::history::NativeObjectHistory;
    use crate::object_document::{ProjectEnvelope, ProjectMetadata};
    let drawing = decode_storage_project(
        source(json!([{"property":"opacity","static":[37]}]))
            .to_string()
            .as_bytes(),
    )
    .unwrap();
    let owner = NativeObjectHistory::from_admitted_project(
        "public-context",
        ProjectEnvelope::no_media(ProjectMetadata::new(640, 360, 30.).unwrap(), drawing),
    )
    .unwrap();
    let pin = owner.acquire_snapshot(0).unwrap();
    let view = json!({"kind":"expressionContext","target":{"contextId":"scene-root","layerUid":"L0"},"property":"opacity","frame":3.5});
    let response = pin.dispatch(public(&pin, view.clone())).unwrap();
    assert!(response.ok);
    let result = response.result.unwrap();
    assert_eq!(result["kind"], "expressionContext");
    assert_eq!(result["documentSnapshotId"], pin.snapshot_id());
    assert_eq!(result["comp"]["width"], 640);
    assert_eq!(result["comp"]["fps"], 30.);
    assert_eq!(result["self"]["index"], 0);
    assert_eq!(result["keyCount"], 0);
    assert!(result["compUnavailable"].is_null());
    let mut wrong = selector(&pin, view.clone());
    wrong["documentSnapshotId"] = json!("foreign");
    assert_eq!(
        pin.query_property_read_json(wrong.to_string().as_bytes()),
        Err(DispatchErrorCode::StaleRevision)
    );
    let mut missing = view;
    missing["target"]["layerUid"] = json!("deleted");
    assert_eq!(read(&pin, missing), Err(DispatchErrorCode::NotFound));
    assert_eq!(owner.history_depths(), (0, 0));
}
