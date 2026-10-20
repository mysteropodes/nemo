use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::json;
fn query(selector: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"q7","instanceId":"instance","documentId":"document",
    "operation":READ,"payload":{"atRevision":7,"documentSnapshotId":"snapshot-7","contextId":"scene-root","selector":selector}})).unwrap()
}
fn edit(a: Value) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"e7","instanceId":"instance","documentId":"document","expectedRevision":7,
    "operation":EDIT,"payload":{"command":"object.project.composition.edit","action":a}})).unwrap()
}
fn read_response(q: &NativeApplicationRequest, value: Value) -> NativeApplicationResponse {
    serde_json::from_value(json!({"apiVersion":2,"requestId":q.request_id,"instanceId":"instance","documentId":"document","contentRevision":7,"ok":true,
    "result":{"instanceId":"instance","documentId":"document","atRevision":7,"documentSnapshotId":"snapshot-7","contextId":"scene-root","result":value}})).unwrap()
}
fn settings() -> Value {
    json!({"markersPresent":false,"markerCount":0,"effectiveBpm":120,"effectiveBpmOffset":0,"effectiveBpmShow":false})
}

#[test]
fn registered_composition_actions_are_closed_and_preserve_raw_finite_storage_domain() {
    for a in [
        json!({"kind":"addMarker","frame":-0.5,"name":"","color":""}),
        json!({"kind":"updateMarker","ordinal":4096,"frame":3.5}),
        json!({"kind":"removeMarker","ordinal":1}),
        json!({"kind":"clearMarkers"}),
        json!({"kind":"setBpm"}),
        json!({"kind":"setBpm","bpm":-2.5,"bpmOffset":0,"bpmShow":false}),
    ] {
        assert!(edit(a).validate_input().is_ok());
    }
    let q = edit(json!({"kind":"setBpm","bpm":132}));
    let descriptor = crate::capabilities::native_catalog()
        .capability_for_operation(&q.operation)
        .unwrap();
    assert_eq!(descriptor["id"], "native.project-composition");
    assert_eq!(descriptor["authority"]["fallback"], "none");
    for a in [
        json!([]),
        json!({"kind":"setCompositionMetadata","markers":[]}),
        json!({"kind":"setBpm","bpm":null}),
        json!({"kind":"setBpm","bpmShow":0}),
        json!({"kind":"clearMarkers","extra":true}),
        json!({"kind":"removeMarker","ordinal":0}),
        json!({"kind":"addMarker","frame":0,"name":"é".repeat(513)}),
    ] {
        assert!(edit(a).validate_input().is_err());
    }
    let mut q = edit(json!({"kind":"setBpm"}));
    q.expected_revision = None;
    assert!(q.validate_input().is_err());
}

#[test]
fn registered_pinned_selectors_keep_original_forms_and_small_forward_pages() {
    for s in [
        json!({"kind":"settings"}),
        json!({"kind":"markerAt","index":5000}),
        json!({"kind":"markerNamed","name":""}),
        json!({"kind":"markerNearest","frame":-0.5}),
        json!({"kind":"markerPage","cursor":0,"limit":1}),
        json!({"kind":"markerPage","cursor":4096,"limit":16}),
    ] {
        assert!(query(s).validate_input().is_ok());
    }
    for s in [
        json!([]),
        json!({"kind":"settings","extra":1}),
        json!({"kind":"markerPage","cursor":0,"limit":0}),
        json!({"kind":"markerPage","cursor":4097,"limit":1}),
        json!({"kind":"markerNamed","name":null}),
        json!({"kind":"markerAt","index":0}),
    ] {
        assert!(query(s).validate_input().is_err());
    }
    let mut q = query(json!({"kind":"settings"}));
    q.expected_revision = Some(7);
    assert!(q.validate_input().is_err());
}

#[test]
fn actual_original_byte_decoder_refuses_nested_duplicates_and_complete_ingress_overflow() {
    let raw = serde_json::to_string(&edit(json!({"kind":"setBpm","bpm":120}))).unwrap();
    assert!(crate::wire::decode_native_request(raw.as_bytes()).is_ok());
    let duplicate = raw.replace("\"bpm\":120", "\"bpm\":120,\"bpm\":130");
    assert_ne!(raw, duplicate);
    assert!(crate::wire::decode_native_request(duplicate.as_bytes()).is_err());
    let raw = serde_json::to_vec(&edit(
        json!({"kind":"addMarker","frame":0,"name":"\u{1}".repeat(1024)}),
    ))
    .unwrap();
    assert!(raw.len() > 4096);
    assert!(crate::wire::decode_native_request(&raw).is_err());
}

#[test]
fn actual_wire_validator_correlates_read_owner_revision_snapshot_and_selector() {
    let q = query(json!({"kind":"markerPage","cursor":0,"limit":1}));
    let page = json!({"kind":"markerPage","markersPresent":true,"markerCount":2,"cursor":0,"items":[{"ordinal":1,"frame":-0.5}],"nextCursor":1});
    let good = read_response(&q, page.clone());
    assert!(crate::wire::validate_native_response(&good, &q).is_ok());
    for (field, value) in [
        ("instanceId", json!("other")),
        ("documentId", json!("other")),
        ("atRevision", json!(8)),
        ("documentSnapshotId", json!("other")),
        ("contextId", json!("component")),
    ] {
        let mut r = good.clone();
        r.result.as_mut().unwrap()[field] = value;
        assert!(crate::wire::validate_native_response(&r, &q).is_err());
    }
    for body in [
        json!({"kind":"markerPage","markersPresent":true,"markerCount":2,"cursor":0,"items":[],"nextCursor":0}),
        json!({"kind":"markerPage","markersPresent":true,"markerCount":2,"cursor":0,"items":[{"ordinal":1,"frame":-0.5}],"nextCursor":null}),
        settings(),
    ] {
        assert!(crate::wire::validate_native_response(&read_response(&q, body), &q).is_err());
    }
    let mut r = good;
    r.content_revision = 8;
    assert!(crate::wire::validate_native_response(&r, &q).is_err());
}

#[test]
fn settings_raw_presence_defaults_and_named_marker_result_match_the_request() {
    let q = query(json!({"kind":"settings"}));
    assert!(crate::wire::validate_native_response(&read_response(&q, settings()), &q).is_ok());
    let mut raw = settings();
    raw["bpm"] = json!(-2.5);
    raw["effectiveBpm"] = json!(-2.5);
    assert!(crate::wire::validate_native_response(&read_response(&q, raw.clone()), &q).is_ok());
    raw["effectiveBpm"] = json!(120);
    assert!(crate::wire::validate_native_response(&read_response(&q, raw), &q).is_err());
    let q = query(json!({"kind":"markerNamed","name":"X"}));
    let value = json!({"index":1,"frame":2.0,"time":2.0/24.0,"name":"X","color":null});
    assert!(crate::wire::validate_native_response(&read_response(&q, value.clone()), &q).is_ok());
    let mut wrong = value;
    wrong["name"] = json!("Y");
    assert!(crate::wire::validate_native_response(&read_response(&q, wrong), &q).is_err());
}

#[test]
fn composition_edit_actual_wire_receipt_requires_one_or_zero_history_revision_exactly() {
    let q = edit(json!({"kind":"setBpm","bpm":132}));
    for applied in [false, true] {
        let r:NativeApplicationResponse=serde_json::from_value(json!({"apiVersion":2,"requestId":q.request_id,"instanceId":"instance","documentId":"document","contentRevision":if applied{8}else{7},"ok":true,
            "result":{"applied":applied,"historyEntriesAdded":if applied{1}else{0}}})).unwrap();
        assert!(crate::wire::validate_native_response(&r, &q).is_ok());
        for field in ["requestId", "instanceId", "documentId"] {
            let mut wire = serde_json::to_value(&r).unwrap();
            wire[field] = json!("foreign");
            let bad: NativeApplicationResponse = serde_json::from_value(wire).unwrap();
            assert!(crate::wire::validate_native_response(&bad, &q).is_err());
        }
        let mut bad = r;
        bad.content_revision += 1;
        assert!(crate::wire::validate_native_response(&bad, &q).is_err());
    }
}

#[test]
fn actual_wire_bound_refuses_a_structurally_valid_oversized_complete_page() {
    let q = query(json!({"kind":"markerPage","cursor":0,"limit":16}));
    let items: Vec<Value> = (0..7)
        .map(|i| json!({"ordinal":i+1,"frame":i,"name":"é".repeat(512)}))
        .collect();
    let r = read_response(
        &q,
        json!({"kind":"markerPage","markersPresent":true,"markerCount":7,"cursor":0,"items":items,"nextCursor":null}),
    );
    assert!(serde_json::to_vec(&r).unwrap().len() > 4096);
    assert!(crate::wire::validate_native_response(&r, &q).is_err());
}
