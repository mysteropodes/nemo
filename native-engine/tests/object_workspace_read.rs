//! Raw workspace presence and bounded complete read envelopes on pinned snapshots.
use super::{capture, captured, owner, pin, read, request, saved};
use crate::history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode as Error;
use serde_json::{json, Value};

fn raw_query(p: &crate::revision::ObjectSnapshot) -> Value {
    json!({"command":"object.project.workspace.read","instanceId":p.instance_id(),
        "documentId":p.document_id(),"atRevision":p.content_revision(),
        "documentSnapshotId":p.snapshot_id(),"contextId":"scene-root","selector":{"kind":"record"}})
}
#[test]
fn missing_empty_partial_null_false_and_order_are_returned_without_default_materialization() {
    for fields in [
        json!({}),
        json!({"workspace":{}}),
        json!({"workspace":{"layerPanelWidth":null,"propsPanelCollapsed":false,"panelOrder":null}}),
        json!({"workspace":{"panelOrder":[],"motionColumns":""}}),
        json!({"workspace":{"panelOrder":["same","same",""],"timelineHeight":"auto"}}),
    ] {
        let o = owner(fields.clone());
        let before = saved(&o);
        let p = pin(&o);
        let response = read(&p, request(&p, "raw-record"));
        assert_eq!(response["ok"], true);
        assert_eq!(response["result"]["documentSnapshotId"], p.snapshot_id());
        let result = &response["result"]["result"];
        assert_eq!(
            result["workspacePresent"],
            fields.get("workspace").is_some()
        );
        assert_eq!(result.get("workspace"), fields.get("workspace"));
        assert_eq!(saved(&o), before);
        assert_eq!(o.history_depths(), (0, 0));
    }
    // Existing top-level null refusal is unchanged; only the typed inner fields are nullable.
    let o = owner(json!({}));
    let mut wire: Value = serde_json::from_slice(&saved(&o)).unwrap();
    wire["project"]["workspace"] = Value::Null;
    assert!(
        NativeObjectHistory::from_project_file("root-null", wire.to_string().as_bytes()).is_err()
    );
}
#[test]
fn workspace_pins_and_public_or_raw_read_fences_refuse_without_mutation() {
    let mut o = owner(json!({}));
    let old = pin(&o);
    let before = saved(&o);
    for (field, value, code) in [
        ("instanceId", json!("foreign"), Error::WrongInstance),
        ("documentId", json!("foreign"), Error::WrongDocument),
        ("atRevision", json!(1), Error::StaleRevision),
        ("documentSnapshotId", json!("wrong"), Error::StaleRevision),
        ("contextId", json!("nested"), Error::Unavailable),
        (
            "command",
            json!("object.project.shadow.read"),
            Error::InvalidRequest,
        ),
        (
            "selector",
            json!({"kind":"record","extra":true}),
            Error::InvalidRequest,
        ),
    ] {
        let mut query = raw_query(&old);
        query[field] = value;
        assert_eq!(
            old.query_workspace_metadata_json(&serde_json::to_vec(&query).unwrap()),
            Err(code)
        );
    }
    let duplicate = raw_query(&old).to_string().replace(
        "\"kind\":\"record\"",
        "\"kind\":\"record\",\"kind\":\"record\"",
    );
    assert_eq!(
        old.query_workspace_metadata_json(duplicate.as_bytes()),
        Err(Error::InvalidRequest)
    );
    let oversized = vec![b' '; 4097];
    for bytes in [b"[]".as_slice(), b"{".as_slice(), oversized.as_slice()] {
        assert_eq!(
            old.query_workspace_metadata_json(bytes),
            Err(Error::InvalidRequest)
        );
    }
    for (field, value, code) in [
        ("atRevision", json!(1), "stale_revision"),
        ("documentSnapshotId", json!("wrong"), "stale_revision"),
        ("contextId", json!("nested"), "unavailable"),
        ("extra", json!(true), "invalid_request"),
    ] {
        let mut q = request(&old, field);
        q.payload[field] = value;
        assert_eq!(read(&old, q)["error"]["code"], code);
    }
    let mut q = request(&old, "read-revision");
    q.expected_revision = Some(0);
    assert_eq!(read(&old, q)["error"]["code"], "invalid_request");
    let mut q = request(&old, "cancelled-read");
    q.cancelled_before_dispatch = true;
    assert_eq!(read(&old, q)["error"]["code"], "cancelled_before_dispatch");
    assert_eq!(saved(&o), before);
    capture(&mut o, "advance", captured());
    let current = pin(&o);
    assert_eq!(
        read(&current, request(&old, "old-pin"))["error"]["code"],
        "stale_revision"
    );
    assert_eq!(
        read(&old, request(&old, "retained-pin"))["result"]["result"],
        json!({"workspacePresent":false})
    );
}
#[test]
fn complete_envelope_budget_refuses_an_otherwise_bounded_record_without_truncation() {
    let o = owner(json!({"workspace":{"layerPanelWidth":""}}));
    let p = pin(&o);
    let base = p
        .query_workspace_metadata_json(&serde_json::to_vec(&raw_query(&p)).unwrap())
        .unwrap();
    let padding = 4096 - serde_json::to_vec(&base).unwrap().len() - 32;
    let o = owner(json!({"workspace":{"layerPanelWidth":"x".repeat(padding)}}));
    let before = saved(&o);
    let p = pin(&o);
    let inner = p
        .query_workspace_metadata_json(&serde_json::to_vec(&raw_query(&p)).unwrap())
        .unwrap();
    assert!(serde_json::to_vec(&inner).unwrap().len() <= 4096);
    assert!(serde_json::to_vec(&inner).unwrap().len() > 4096 - 64);
    let mut q = request(&p, "bounded-record");
    q.request_id = "r".repeat(128);
    let result = read(&p, q);
    assert_eq!(result["ok"], false);
    assert_eq!(result["error"]["code"], "unavailable");
    assert_eq!(saved(&o), before);
    assert_eq!(o.history_depths(), (0, 0));
    let mut o = owner(json!({}));
    let before = saved(&o);
    let mut workspace = captured();
    workspace["motionColumns"] = json!("\u{0001}".repeat(1024));
    assert_eq!(
        capture(&mut o, "oversize-capture", workspace)["error"]["code"],
        "invalid_request"
    );
    assert_eq!(saved(&o), before);
}
