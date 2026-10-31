use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::json;

fn query(selector: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"camera-read","instanceId":"i","documentId":"d",
        "operation":READ,"payload":{"atRevision":7,"documentSnapshotId":"s7","contextId":"scene-root","selector":selector}})).unwrap()
}
fn response(q: &NativeApplicationRequest, body: Value) -> NativeApplicationResponse {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":q.request_id,"instanceId":"i","documentId":"d",
        "contentRevision":7,"ok":true,"result":{"instanceId":"i","documentId":"d","atRevision":7,
        "documentSnapshotId":"s7","contextId":"scene-root","result":body}}),
    )
    .unwrap()
}
fn wire(q: &NativeApplicationRequest, r: &NativeApplicationResponse) -> bool {
    crate::wire::validate_native_response(r, q).is_ok()
}
fn key(ordinal: u64) -> Value {
    json!({"ordinal":ordinal,"frame":(ordinal-1)*4,"x":1.5,"y":-2,"w":320,"rot":0.5,"ease":[0.25,0.1,0.25,1]})
}

#[test]
fn the_read_is_a_closed_pinned_selector_without_a_revision_pin() {
    let q = query(json!({"kind":"settings"}));
    assert!(crate::native_contract::validate_request(READ, &q.payload));
    for bad in [
        json!({"kind":"settings","extra":1}),
        json!({"kind":"keyPage","cursor":0,"limit":17}),
        json!({"kind":"keyPage","cursor":0,"limit":0}),
        json!({"kind":"keyPage","cursor":-1,"limit":1}),
        json!({"kind":"all"}),
    ] {
        assert!(!request(READ, &query(bad).payload), "{}", q.request_id);
    }
    assert!(crate::native_contract::forbids_expected_revision(READ));
}

#[test]
fn settings_answers_are_closed_and_an_absent_set_has_no_keys() {
    let q = query(json!({"kind":"settings"}));
    for ok in [
        json!({"keysPresent":false,"keyCount":0}),
        json!({"keysPresent":true,"keyCount":4,"layerOn":true}),
    ] {
        assert!(wire(&q, &response(&q, ok)));
    }
    for bad in [
        json!({"keysPresent":false,"keyCount":2}),
        json!({"keysPresent":true,"keyCount":1,"layerOn":"yes"}),
        json!({"keysPresent":true,"keyCount":1,"cameraView":{}}),
        json!({"keyCount":1}),
    ] {
        assert!(!wire(&q, &response(&q, bad)));
    }
}

#[test]
fn key_pages_are_dense_ordinal_bounded_and_chain_by_next_cursor() {
    let q = query(json!({"kind":"keyPage","cursor":2,"limit":2}));
    let page = |items: Vec<Value>, next: Value| json!({"kind":"cameraKeyPage","keysPresent":true,"keyCount":5,"cursor":2,"items":items,"nextCursor":next});
    assert!(wire(
        &q,
        &response(&q, page(vec![key(3), key(4)], json!(4)))
    ));
    assert!(wire(&q, &response(&q, page(vec![key(3)], json!(3)))));
    assert!(!wire(
        &q,
        &response(&q, page(vec![key(3), key(5)], json!(4)))
    ));
    assert!(!wire(
        &q,
        &response(&q, page(vec![key(3), key(4)], Value::Null))
    ));
    assert!(!wire(
        &q,
        &response(&q, page(vec![key(3), key(4), key(5)], Value::Null))
    ));
    assert!(!wire(&q, &response(&q, page(vec![], json!(2)))));
    let mut bad = key(3);
    bad["ease"] = json!([0, 1, 2]);
    assert!(!wire(&q, &response(&q, page(vec![bad], json!(3)))));
    let last = query(json!({"kind":"keyPage","cursor":4,"limit":16}));
    let end = json!({"kind":"cameraKeyPage","keysPresent":true,"keyCount":5,"cursor":4,"items":[key(5)],"nextCursor":null});
    assert!(wire(&last, &response(&last, end)));
}
