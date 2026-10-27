//! Preview is a detached read: no document/history write and no caller-provided geometry.
#[path = "object_vector_edit_support.rs"]
#[allow(dead_code)]
mod support;
use serde_json::{json, Value};
use support::{bytes, document, owner_of, record_at, target};
#[test]
fn subselect_read_pages_and_preview_never_publish() {
    let owner = owner_of(&document(2, vec![record_at("A", "L1", 0, "s-a")], None));
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let before = bytes(&owner);
    let mut request = json!({"apiVersion":2,"requestId":"read-subselect","instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "operation":"query.document.object","payload":{"atRevision":0,"documentSnapshotId":snapshot.snapshot_id(),
        "stableTarget":target("L1",0,"s-a"),"subselect":{"cursor":0,"limit":1}}});
    let read = |v: &Value| {
        serde_json::to_value(
            snapshot
                .dispatch(serde_json::from_value(v.clone()).unwrap())
                .unwrap(),
        )
        .unwrap()
    };
    let raw = read(&request);
    assert_eq!(raw["ok"], true, "{raw}");
    assert_eq!(raw["result"]["subselect"]["nextCursor"], 1);
    request["payload"]["subselect"]["preview"] = json!({"kind":"move","indices":[0],"delta":[4,7]});
    let preview = read(&request);
    assert_eq!(preview["ok"], true, "{preview}");
    assert_eq!(preview["result"]["subselect"]["preview"], true);
    for (axis, delta) in [(0, 4.0), (1, 7.0)] {
        assert_eq!(
            preview["result"]["subselect"]["segments"][0]["point"][axis]
                .as_f64()
                .unwrap(),
            raw["result"]["subselect"]["segments"][0]["point"][axis]
                .as_f64()
                .unwrap()
                + delta
        );
    }
    assert_eq!(bytes(&owner), before);
    assert_eq!(owner.history_depths(), (0, 0));
    request["payload"]["documentSnapshotId"] = "replaced".into();
    assert_eq!(read(&request)["error"]["code"], "stale_revision");
    request["documentId"] = "replacement-document".into();
    assert_eq!(read(&request)["error"]["code"], "wrong_document");
    assert_eq!(bytes(&owner), before);
}
