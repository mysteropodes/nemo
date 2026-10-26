//! Live stack observation must neither consume receipts nor select historical stacks.
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::request_receipts::OpacityRequest;
use serde_json::{json, Value};
fn owner() -> NativeObjectHistory {
    NativeObjectHistory::new(
        "history-read",
        decode_storage_project(
            json!({
        "format":"nemo.native-object-document","formatVersion":1,"totalFrames":20,
        "layers":[{"layerUid":"L"}],"objects":[]})
            .to_string()
            .as_bytes(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn request(o: &NativeObjectHistory) -> OpacityRequest {
    OpacityRequest::query(
        "same-read-id",
        o.instance_id(),
        o.document_id(),
        "query.document.object.history",
        json!({"atRevision":o.content_revision(),
        "documentSnapshotId":o.acquire_snapshot(o.content_revision()).unwrap().snapshot_id()}),
    )
}
fn read(o: &NativeObjectHistory, r: &OpacityRequest) -> Value {
    serde_json::to_value(o.dispatch_history_read(r).unwrap()).unwrap()
}
fn edit(o: &mut NativeObjectHistory, value: f64) {
    let payload = json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"L"},
        "property":"opacity","action":{"kind":"setValue","frame":0,"values":[value]}});
    let candidate = o
        .prepare_property_edit_json(payload.to_string().as_bytes())
        .unwrap();
    o.commit_property_edit(candidate).unwrap();
}
#[test]
fn actual_nonzero_stacks_old_snapshots_noop_and_same_request_id_are_read_without_receipts() {
    let mut o = owner();
    let old = request(&o);
    assert_eq!(read(&o, &old)["result"]["undoDepth"], 0);
    edit(&mut o, 42.);
    let after_edit = request(&o);
    assert_eq!(read(&o, &after_edit)["result"]["undoDepth"], 1);
    edit(&mut o, 73.);
    o.undo(2).unwrap();
    assert_eq!(o.history_depths(), (1, 1));
    let pinned = request(&o);
    let before = format!("{o:?}");
    for _ in 0..4 {
        let result = read(&o, &pinned);
        assert_eq!(
            result["result"],
            json!({"atRevision":3,
            "documentSnapshotId":pinned.payload["documentSnapshotId"],"undoDepth":1,"redoDepth":1})
        );
    }
    assert_eq!(
        format!("{o:?}"),
        before,
        "all owner state including receipts stays unchanged"
    );
    for stale in [old, after_edit] {
        assert_eq!(read(&o, &stale)["error"]["code"], "stale_revision");
    }
    edit(&mut o, 42.);
    assert_eq!(o.history_depths(), (1, 1), "semantic no-op retains redo");
    assert_eq!(read(&o, &pinned)["result"]["redoDepth"], 1);
    o.redo(3).unwrap();
    assert_eq!(read(&o, &request(&o))["result"]["undoDepth"], 2);
    assert_eq!(read(&o, &request(&o))["result"]["redoDepth"], 0);
}
#[test]
fn foreign_stale_malformed_cancelled_and_oversize_requests_preserve_all_state() {
    let o = owner();
    let r = request(&o);
    let before = format!("{o:?}");
    let mut cases = vec![];
    for (key, value, code) in [
        ("instanceId", json!("foreign"), "wrong_instance"),
        ("documentId", json!("foreign"), "wrong_document"),
        ("expectedRevision", json!(0), "invalid_request"),
        ("apiVersion", json!(1), "invalid_request"),
        (
            "cancelledBeforeDispatch",
            json!(true),
            "cancelled_before_dispatch",
        ),
    ] {
        let mut bad = serde_json::to_value(&r).unwrap();
        bad[key] = value;
        cases.push((serde_json::from_value(bad).unwrap(), code));
    }
    for payload in [
        Value::Null,
        json!([]),
        json!({}),
        json!({"atRevision":0,"documentSnapshotId":"s","extra":1}),
        json!({"atRevision":-1,"documentSnapshotId":"s"}),
        json!({"atRevision":0.5,"documentSnapshotId":"s"}),
        json!({"atRevision":9007199254740992u64,"documentSnapshotId":"s"}),
        json!({"atRevision":0,"documentSnapshotId":"x".repeat(5000)}),
    ] {
        let mut bad = r.clone();
        bad.payload = payload;
        cases.push((bad, "invalid_request"));
    }
    for (key, value) in [
        ("atRevision", json!(1)),
        ("documentSnapshotId", json!("other-snapshot")),
    ] {
        let mut bad = r.clone();
        bad.payload[key] = value;
        cases.push((bad, "stale_revision"));
    }
    for (bad, code) in cases {
        assert_eq!(read(&o, &bad)["error"]["code"], code);
    }
    assert_eq!(format!("{o:?}"), before);
}
