use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::json;
fn fields() -> Value {
    json!({"layerPanelWidth":null,"toolsPanelWidth":"","propsPanelWidth":"0300px",
        "propsPanelCollapsed":false,"timelineHeight":"90vh","panelOrder":["effects","effects","","雪\0"]})
}
fn query() -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"workspace-read","instanceId":"i","documentId":"d",
        "operation":READ,"payload":{"atRevision":7,"documentSnapshotId":"s7","contextId":"scene-root","selector":{"kind":"record"}}})).unwrap()
}
fn edit(value: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"workspace-edit","instanceId":"i","documentId":"d",
        "operation":EDIT,"expectedRevision":7,"payload":{"command":"object.project.workspace.edit","action":{"kind":"capture","workspace":value}}})).unwrap()
}
fn response(q: &NativeApplicationRequest, body: Value) -> NativeApplicationResponse {
    let result = if q.operation == READ {
        json!({"instanceId":"i","documentId":"d","atRevision":7,"documentSnapshotId":"s7","contextId":"scene-root","result":body})
    } else {
        body
    };
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":q.request_id,"instanceId":"i","documentId":"d",
        "contentRevision":7,"ok":true,"result":result}),
    )
    .unwrap()
}
fn wire(q: &NativeApplicationRequest, r: &NativeApplicationResponse) -> bool {
    crate::wire::validate_native_response(r, q).is_ok()
}
#[test]
fn original_capture_fields_are_mandatory_and_optional_motion_columns_preserves_raw_presence() {
    for value in [
        fields(),
        {
            let mut value = fields();
            value["motionColumns"] = Value::Null;
            value
        },
        {
            let mut value = fields();
            value["motionColumns"] = json!("data:raw\0unknown");
            value
        },
    ] {
        let q = edit(value);
        assert!(q.validate_input().is_ok());
        assert!(q.validate().is_ok());
    }
    for key in &FIELDS[..6] {
        let mut value = fields();
        value.as_object_mut().unwrap().remove(*key);
        assert!(edit(value).validate_input().is_err());
    }
    for (key, value) in [
        ("canvasW", json!(5)),
        ("preferences", json!({})),
        ("propsPanelCollapsed", Value::Null),
        ("panelOrder", json!([1])),
    ] {
        let mut fields = fields();
        fields[key] = value;
        assert!(edit(fields).validate_input().is_err());
    }
    let mut q = edit(fields());
    q.expected_revision = None;
    assert!(q.validate_input().is_err());
    let mut q = query();
    q.expected_revision = Some(7);
    assert!(q.validate_input().is_err());
    let mut q = query();
    q.payload["selector"] = json!({"kind":"record","limit":16});
    assert!(q.validate_input().is_err());
}
#[test]
fn pinned_raw_record_never_coerces_absent_partial_empty_or_null_fields() {
    let q = query();
    for record in [
        json!({"workspacePresent":false}),
        json!({"workspacePresent":true,"workspace":{}}),
        json!({"workspacePresent":true,"workspace":{"layerPanelWidth":null,"panelOrder":null,"propsPanelCollapsed":false,"motionColumns":""}}),
    ] {
        assert!(wire(&q, &response(&q, record)));
    }
    for record in [
        json!({"workspacePresent":true}),
        json!({"workspacePresent":false,"workspace":{}}),
        json!({"workspacePresent":true,"workspace":null}),
        json!({"workspacePresent":true,"workspace":{"extra":true}}),
    ] {
        assert!(!wire(&q, &response(&q, record)));
    }
    let valid = response(&q, json!({"workspacePresent":false}));
    for (key, wrong) in [
        ("atRevision", json!(8)),
        ("documentSnapshotId", json!("stale")),
        ("documentId", json!("other")),
        ("instanceId", json!("other")),
    ] {
        let mut r = serde_json::to_value(&valid).unwrap();
        r["result"][key] = wrong;
        assert!(!wire(&q, &serde_json::from_value(r).unwrap()));
    }
}
#[test]
fn complete_wire_budget_and_history_receipt_stay_closed() {
    let q = query();
    let r = response(
        &q,
        json!({"workspacePresent":true,"workspace":{"motionColumns":"x".repeat(4096)}}),
    );
    assert!(!wire(&q, &r));
    let q = edit(fields());
    assert!(wire(
        &q,
        &response(&q, json!({"applied":false,"historyEntriesAdded":0}))
    ));
    assert!(!wire(
        &q,
        &response(&q, json!({"applied":false,"historyEntriesAdded":1}))
    ));
    let mut q = edit(fields());
    q.payload["action"]["workspace"]["motionColumns"] = json!("x".repeat(4096));
    assert!(q.validate_input().is_err());
}
