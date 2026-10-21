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

fn expression_state_view(frame: f64) -> Value {
    json!({"kind":"expressionState","target":{"contextId":"scene-root","layerUid":"L0"},
        "property":"opacity","frame":frame})
}
#[test]
fn expression_state_reads_actual_presence_without_executing_or_claiming_success() {
    for (expression, present, enabled) in [
        (
            json!({"enabled":true,"code":"throw Error('must not run')"}),
            true,
            true,
        ),
        (json!({"enabled":false,"code":"0"}), true, false),
        (Value::Null, false, false),
    ] {
        let snapshot = pin(json!([{"property":"opacity","expression":expression}]));
        let saved = encode_storage_project(snapshot.document()).unwrap();
        let result = read(&snapshot, expression_state_view(2.5)).unwrap();
        assert_eq!(
            result["expression"],
            json!({"present":present,"enabled":enabled})
        );
        assert_eq!(
            result["evaluation"],
            json!({"state":"unavailable","reason":"not-evaluated"})
        );
        assert!(result.get("code").is_none());
        assert_eq!(encode_storage_project(snapshot.document()).unwrap(), saved);
    }
}
#[test]
fn real_expression_state_projection_preserves_typed_diagnostics_clear_and_zero_line() {
    let snapshot = keyed();
    let saved = encode_storage_project(snapshot.document()).unwrap();
    for diagnostic in [
        Value::Null,
        json!({"kind":"timeout"}),
        json!({"kind":"tooDeep"}),
        json!({"kind":"mustReturnNumber"}),
        json!({"kind":"mustReturnNumberOrXY"}),
        json!({"kind":"runtime","message":"literal <b>message</b>","line":0}),
        json!({"kind":"syntax","message":"syntax","line":null}),
    ] {
        let request = public(&snapshot, expression_state_view(2.));
        let response = snapshot.dispatch(request.clone()).unwrap();
        let result = snapshot
            .with_expression_state_evaluation(&request, response, diagnostic.clone())
            .unwrap();
        assert!(result.ok);
        assert_eq!(
            result.result.unwrap()["evaluation"],
            json!({"state":"evaluated","diagnostic":diagnostic})
        );
        assert_eq!(encode_storage_project(snapshot.document()).unwrap(), saved);
    }
}
#[test]
fn expression_state_rejects_stale_wrong_frame_or_malformed_receipts_without_mutation() {
    let snapshot = keyed();
    let saved = encode_storage_project(snapshot.document()).unwrap();
    for diagnostic in [
        json!({"kind":"timeout","extra":true}),
        json!({"kind":"unknown"}),
        json!({"kind":"runtime","message":"error","line":-1}),
        json!({"kind":"syntax","message":"error"}),
    ] {
        let request = public(&snapshot, expression_state_view(2.));
        let response = snapshot.dispatch(request.clone()).unwrap();
        assert_eq!(
            snapshot.with_expression_state_evaluation(&request, response, diagnostic),
            Err(DispatchErrorCode::InvalidRequest)
        );
    }
    let request = public(&snapshot, expression_state_view(2.));
    let response = snapshot.dispatch(request.clone()).unwrap();
    let wrong_frame = public(&snapshot, expression_state_view(3.));
    assert_eq!(
        snapshot.with_expression_state_evaluation(&wrong_frame, response, Value::Null),
        Err(DispatchErrorCode::InvalidRequest)
    );
    let mut stale = selector(&snapshot, expression_state_view(2.));
    stale["atRevision"] = json!(99);
    assert_eq!(
        snapshot.query_property_read_json(stale.to_string().as_bytes()),
        Err(DispatchErrorCode::StaleRevision)
    );
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), saved);
}
#[test]
fn oversized_expression_diagnostic_refuses_without_truncating_its_error() {
    let snapshot = keyed();
    let request = public(&snapshot, expression_state_view(2.));
    let response = snapshot.dispatch(request.clone()).unwrap();
    let result = snapshot
        .with_expression_state_evaluation(
            &request,
            response,
            json!({"kind":"runtime","message":"雪".repeat(2000),"line":1}),
        )
        .unwrap();
    assert!(!result.ok);
    assert_eq!(result.error.unwrap().code, DispatchErrorCode::Unavailable);
}

#[test]
fn expression_projection_rejects_cross_paired_envelopes_and_cancelled_queries() {
    let snapshot = keyed();
    for field in [
        "request",
        "instance",
        "document",
        "api",
        "expected",
        "cancelled",
    ] {
        let request = public(&snapshot, expression_state_view(2.));
        let response = snapshot.dispatch(request.clone()).unwrap();
        let mut foreign = request;
        match field {
            "request" => foreign.request_id = "another-request".into(),
            "instance" => foreign.instance_id = "another-instance".into(),
            "document" => foreign.document_id = "another-document".into(),
            "api" => foreign.api_version = 99,
            "expected" => foreign.expected_revision = Some(0),
            "cancelled" => foreign.cancelled_before_dispatch = true,
            _ => unreachable!(),
        }
        assert_eq!(
            snapshot.with_expression_state_evaluation(&foreign, response, Value::Null),
            Err(DispatchErrorCode::InvalidRequest),
            "{field}"
        );
    }
}

#[test]
fn expression_projection_matches_numeric_frames_without_accepting_wrong_or_malformed_frames() {
    let snapshot = keyed();
    let saved = encode_storage_project(snapshot.document()).unwrap();
    for frame in [json!(0), json!(2), json!(2.0), json!(2.5)] {
        let mut view = expression_state_view(2.);
        view["frame"] = frame;
        let request = public(&snapshot, view);
        let response = snapshot.dispatch(request.clone()).unwrap();
        let projected = snapshot
            .with_expression_state_evaluation(&request, response, Value::Null)
            .unwrap();
        assert!(projected.ok);
        assert_eq!(
            projected.result.unwrap()["evaluation"],
            json!({"state":"evaluated","diagnostic":null})
        );
    }
    for frame in [
        json!(3),
        json!(2.0001),
        json!("2"),
        Value::Null,
        json!(true),
        json!({}),
        json!([]),
    ] {
        let request = public(&snapshot, expression_state_view(2.));
        let response = snapshot.dispatch(request.clone()).unwrap();
        let mut mismatch = request;
        mismatch.payload["view"]["frame"] = frame;
        assert_eq!(
            snapshot.with_expression_state_evaluation(&mismatch, response, Value::Null),
            Err(DispatchErrorCode::InvalidRequest)
        );
    }
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), saved);
}
