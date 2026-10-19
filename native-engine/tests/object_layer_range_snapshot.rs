//! Range projection through the actual immutable reader and existing timing-query dispatcher.
use super::*;
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::ObjectDocument;
#[path = "object_frame_edit_document_build.rs"]
#[allow(dead_code)]
mod build;
#[path = "object_layer_shift_build.rs"]
#[allow(dead_code)]
mod fixture;
#[path = "object_exposure_edit_key_rows.rs"]
#[allow(dead_code)]
mod rows;
fn selector(s: &ObjectSnapshot) -> Value {
    json!({"atRevision":s.content_revision(),"documentSnapshotId":s.snapshot_id(),
        "contextId":"scene-root","layerUid":"L0","frame":1,"view":"range"})
}
fn snapshot(d: ObjectDocument) -> ObjectSnapshot {
    ObjectSnapshot::from_admitted("range-read".into(), d).unwrap()
}
fn read(s: &ObjectSnapshot, q: &Value) -> Result<Value, DispatchErrorCode> {
    s.query_layer_range_json(q.to_string().as_bytes())
}
#[test]
fn no_timing_layer_has_exact_real_raw_and_clamped_edges_without_a_fabricated_exposure() {
    let mut v = fixture::value(&fixture::document());
    v["layers"][0].as_object_mut().unwrap().remove("timing");
    v["layers"][0]["range"] = json!({"inPoint":-2,"outPoint":9});
    let s = snapshot(decode_storage_project(v.to_string().as_bytes()).unwrap());
    let before = encode_storage_project(s.document()).unwrap();
    let q = selector(&s);
    let value = read(&s, &q).unwrap();
    assert_eq!(
        value,
        json!({"kind":"layerRange","instanceId":s.instance_id(),
        "documentId":s.document_id(),"atRevision":0,"documentSnapshotId":s.snapshot_id(),"contextId":"scene-root",
        "layerUid":"L0","frame":1,"range":{"rawInPoint":-2.0,"rawOutPoint":9.0,"inPoint":0.0,"outPoint":5.0,
            "hasCustomRange":true,"hasTimeLink":false}})
    );
    let mut old = q.clone();
    old.as_object_mut().unwrap().remove("view");
    assert!(matches!(
        s.query_layer_timing_json(old.to_string().as_bytes()),
        Err(DispatchErrorCode::Unavailable)
    ));
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
}
#[test]
fn original_timing_selector_and_range_view_are_closed_separate_reads_with_same_snapshot_identity() {
    let s = snapshot(fixture::document());
    let q = selector(&s);
    let mut old = q.clone();
    old.as_object_mut().unwrap().remove("view");
    let timing = serde_json::to_value(
        s.query_layer_timing_json(old.to_string().as_bytes())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(timing.as_object().unwrap().len(), 11);
    assert!(timing.get("range").is_none());
    assert!(timing.get("kind").is_none());
    assert!(s.query_layer_timing_json(q.to_string().as_bytes()).is_err());
    let range = read(&s, &q).unwrap();
    assert_eq!(range.as_object().unwrap().len(), 9);
    for k in [
        "instanceId",
        "documentId",
        "atRevision",
        "documentSnapshotId",
        "contextId",
        "layerUid",
        "frame",
    ] {
        assert_eq!(timing[k], range[k]);
    }
    let typed: OpacityRequest=serde_json::from_value(json!({"apiVersion":2,"requestId":"range-query",
        "instanceId":s.instance_id(),"documentId":s.document_id(),"operation":"query.document.object.layer.timing","payload":q})).unwrap();
    let response = s.dispatch_layer_timing(&typed).unwrap();
    assert!(response.ok);
    assert_eq!(response.result, Some(range));
    assert_eq!(response.content_revision, 0);
    assert_eq!(response.request_id, "range-query");
}
#[test]
fn range_query_rejects_stale_unknown_duplicate_extra_context_and_encoded_bounds_without_mutation() {
    let s = snapshot(fixture::document());
    let before = encode_storage_project(s.document()).unwrap();
    for (field, bad, expected) in [
        ("atRevision", json!(1), DispatchErrorCode::StaleRevision),
        (
            "documentSnapshotId",
            json!("other"),
            DispatchErrorCode::StaleRevision,
        ),
        ("view", json!("unknown"), DispatchErrorCode::InvalidRequest),
        ("extra", json!(1), DispatchErrorCode::InvalidRequest),
        (
            "contextId",
            json!("component"),
            DispatchErrorCode::Unavailable,
        ),
        ("layerUid", json!("missing"), DispatchErrorCode::NotFound),
        ("frame", json!(6), DispatchErrorCode::InvalidRequest),
    ] {
        let mut q = selector(&s);
        q[field] = bad;
        assert_eq!(read(&s, &q).unwrap_err(), expected);
    }
    let raw = selector(&s).to_string();
    let duplicate = raw.replace("\"view\":", "\"view\":\"range\",\"view\":");
    assert_ne!(duplicate, raw);
    assert_eq!(
        s.query_layer_range_json(duplicate.as_bytes()).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    let mut padded = raw.into_bytes();
    padded.resize(4097, b' ');
    assert_eq!(
        s.query_layer_range_json(&padded).unwrap_err(),
        DispatchErrorCode::InvalidRequest
    );
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
}
#[test]
fn range_query_preserves_inverted_edges_and_its_complete_reply_guard() {
    let mut v = fixture::value(&fixture::document());
    v["layers"][0]["range"] = json!({"inPoint":4,"outPoint":1,"timeLink":null});
    let s = snapshot(fixture::admit(v));
    let q = selector(&s);
    let value = read(&s, &q).unwrap();
    assert_eq!(
        value["range"],
        json!({"rawInPoint":4.0,"rawOutPoint":1.0,"inPoint":4.0,"outPoint":1.0,"hasCustomRange":true,"hasTimeLink":false})
    );
    let typed: OpacityRequest=serde_json::from_value(json!({"apiVersion":2,"requestId":"x".repeat(4096),
        "instanceId":s.instance_id(),"documentId":s.document_id(),"operation":"query.document.object.layer.timing","payload":q})).unwrap();
    let response = s.dispatch_layer_timing(&typed).unwrap();
    assert!(!response.ok);
    assert!(response.result.is_none());
}
