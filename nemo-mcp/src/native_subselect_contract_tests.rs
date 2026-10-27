use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::json;
fn public_read(q: &Value, r: &Value) -> bool {
    crate::native_contract::validate_result("query.document.object", "document", q, r)
}
fn target() -> Value {
    json!({"contextId":"scene-root","layerUid":"layer-a","frameScope":{"kind":"authored","frame":0},"strokeId":"stroke-a"})
}
fn query() -> Value {
    json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","stableTarget":target(),"subselect":{"cursor":0,"limit":16}})
}
fn page() -> Value {
    json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","subselect":{"stableTarget":target(),"closed":false,"total":2,"cursor":0,"nextCursor":null,"preview":false,"segments":[{"point":[0,0],"handleIn":[0,0],"handleOut":[0,0]},{"point":[10,0],"handleIn":[0,0],"handleOut":[0,0]}]}})
}
#[test]
fn subselect_closed_request_and_geometry_correlation() {
    let q = query();
    let r = page();
    assert!(request(&q));
    assert!(public_read(&q, &r));
    assert!(crate::native_contract::validate_request(
        "query.document.object",
        &q
    ));
    for (path, value) in [
        ("/subselect/stableTarget/strokeId", json!("other")),
        ("/subselect/cursor", json!(1)),
        ("/subselect/total", json!(3)),
        ("/subselect/preview", json!(true)),
        ("/documentSnapshotId", json!("foreign")),
    ] {
        let mut bad = r.clone();
        *bad.pointer_mut(path).unwrap() = value;
        assert!(!public_read(&q, &bad), "{path}");
    }
    let mut mixed = q.clone();
    mixed["view"] = json!({"kind":"presentation"});
    assert!(!crate::native_contract::validate_request(
        "query.document.object",
        &mixed
    ));
    let mut null = q.clone();
    null["subselect"]["preview"] = Value::Null;
    assert!(!request(&null));
}
#[test]
fn subselect_command_uses_shared_owner_and_revision_correlator() {
    let mut q:NativeApplicationRequest=serde_json::from_value(json!({"apiVersion":2,"requestId":"edit-1","instanceId":"instance","documentId":"document","expectedRevision":7,"operation":EDIT,"payload":{"command":"object.subselect.edit","stableTarget":target(),"intent":{"kind":"handle","index":0,"handle":"handleOut","point":[5,4],"breakPair":false}}})).unwrap();
    assert!(q.validate_input().is_ok());
    assert!(q.validate().is_ok());
    q.payload["atFrame"] = json!(5);
    assert!(q.validate_input().is_ok());
    assert!(q.validate().is_ok());
    for value in [
        json!(-1),
        json!(1.5),
        json!(4294967296u64),
        Value::Null,
        json!("5"),
    ] {
        let mut bad = q.clone();
        bad.payload["atFrame"] = value;
        assert!(bad.validate().is_err());
    }
    q.payload["intent"]["breakPair"] = json!(true);
    q.payload["intent"]["pairedPoint"] = json!([3, 2]);
    assert!(q.validate_input().is_ok());
    assert!(q.validate().is_ok());
    for value in [Value::Null, json!([1]), json!(["x", 2])] {
        let mut bad = q.clone();
        bad.payload["intent"]["pairedPoint"] = value;
        assert!(bad.validate().is_err());
    }
    let mut bad = q.clone();
    bad.payload["intent"]["breakPair"] = json!(false);
    assert!(bad.validate().is_err());
    let r:NativeApplicationResponse=serde_json::from_value(json!({"apiVersion":2,"requestId":"edit-1","instanceId":"instance","documentId":"document","contentRevision":8,"ok":true,"result":{"applied":true,"historyEntriesAdded":1}})).unwrap();
    assert!(crate::native_contract::object_edit::response_matches(
        &q, &r
    ));
    for (key, value) in [
        ("instanceId", json!("foreign")),
        ("documentId", json!("foreign")),
        ("contentRevision", json!(9)),
    ] {
        let mut bad = serde_json::to_value(&r).unwrap();
        bad[key] = value;
        let bad = serde_json::from_value(bad).unwrap();
        assert!(!crate::native_contract::object_edit::response_matches(
            &q, &bad
        ));
    }
    q.expected_revision = None;
    assert!(q.validate_input().is_err());
}
#[test]
fn subselect_view_is_correlated_to_native_presentation_and_preview() {
    let view = json!({"kind":"subselect","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"limit":8});
    let mut q = json!({"atRevision":7,"documentSnapshotId":"native-object:document:7","view":view});
    let mut r = json!({"kind":"subselect","atRevision":7,"documentSnapshotId":"native-object:document:7","contextId":"scene-root","layerUid":"layer-a","frame":0,"workId":"native-work-1","viewGeneration":1,"cursor":0,"total":1,"nextCursor":null,"preview":false,"items":[{"stableTarget":target(),"closed":false,"count":2,"matrix":[1,0.2,0.3,1,8,9]}]});
    assert!(view_request(&q));
    assert!(public_read(&q, &r));
    for (key, value) in [
        ("workId", json!("native-work-2")),
        ("frame", json!(1)),
        ("preview", json!(true)),
        ("total", json!(2)),
    ] {
        let mut bad = r.clone();
        bad[key] = value;
        assert!(!public_read(&q, &bad), "{key}");
    }
    q["view"]["preview"] =
        json!({"stableTarget":target(),"intent":{"kind":"move","indices":[0],"delta":[1,2]}});
    r["preview"] = json!(true);
    assert!(view_request(&q));
    assert!(public_read(&q, &r));
    r["items"][0]["matrix"] = json!([1, 2, 2, 4, 0, 0]);
    assert!(!public_read(&q, &r));
}
