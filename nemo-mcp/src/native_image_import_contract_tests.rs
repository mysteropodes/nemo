//! L06-M2 actual registered wire/shape/correlation controls. Authored UNRUN until compiler slot.
use super::*;
use crate::contract::RequestError;
use crate::wire::decode_native_request;
use serde_json::json;
fn pair(placement: Value) -> (NativeApplicationRequest, NativeApplicationResponse) {
    let q:NativeApplicationRequest=serde_json::from_value(json!({"apiVersion":2,"requestId":"image-wire","instanceId":"instance","documentId":"document","expectedRevision":7,
        "operation":ATTACH,"payload":{"documentSnapshotId":"native-object:document:7","lifecycleGeneration":1,"name":"still.png","mediaType":"image/png",
            "source":{"kind":"file","path":"/admitted/still.png"},"placement":placement}})).unwrap();
    let new_layer = q.payload["placement"]["kind"] == "newLayer";
    let r:NativeApplicationResponse=serde_json::from_value(json!({"apiVersion":2,"requestId":q.request_id,"instanceId":q.instance_id,"documentId":q.document_id,"contentRevision":8,"ok":true,
        "result":{"applied":true,"historyEntriesAdded":1,"assetId":"canonical-existing-png","naturalWidth":640,"naturalHeight":480,"displayWidth":240,"displayHeight":180,
            "target":{"contextId":"scene-root","layerUid":if new_layer{json!("native-layer:8:0")}else{q.payload["placement"]["layerUid"].clone()},
                "frameScope":{"kind":"authored","frame":if new_layer{json!(0)}else{q.payload["placement"]["frame"].clone()}},"strokeId":"native-raster:8:0"},"newLayer":new_layer}})).unwrap();
    (q, r)
}
fn valid(q: &NativeApplicationRequest, r: &NativeApplicationResponse) -> bool {
    r.result
        .as_ref()
        .is_some_and(|value| result(&q.operation, &q.payload, value))
        && response_matches(q, r)
}
#[test]
fn both_registered_placement_forms_cross_original_byte_shape_admission_with_opaque_legacy_layer_ids(
) {
    for placement in [
        json!({"kind":"newLayer"}),
        json!({"kind":"authoredFrame","layerUid":"legacy layer:alpha","frame":3}),
    ] {
        let (q, r) = pair(placement);
        let bytes = serde_json::to_vec(&q).unwrap();
        let parsed = decode_native_request(&bytes).unwrap();
        assert_eq!(parsed, q);
        assert!(parsed.validate_input().is_ok());
        assert!(valid(&q, &r));
    }
}
#[test]
fn private_preparation_has_no_public_execution_activation() {
    let (q, _) = pair(json!({"kind":"newLayer"}));
    assert!(matches!(q.validate(), Err(RequestError::Unavailable(_))));
}
#[test]
fn duplicate_members_and_oversized_original_bytes_are_refused_before_shape_admission() {
    let (q, _) = pair(json!({"kind":"newLayer"}));
    let original = serde_json::to_string(&q).unwrap();
    for duplicated in [
        original.replace(
            "\"lifecycleGeneration\":1",
            "\"lifecycleGeneration\":1,\"lifecycleGeneration\":1",
        ),
        original.replace(
            "\"path\":\"/admitted/still.png\"",
            "\"path\":\"/admitted/still.png\",\"path\":\"/other.png\"",
        ),
        original.replace(
            "\"kind\":\"newLayer\"",
            "\"kind\":\"newLayer\",\"kind\":\"newLayer\"",
        ),
    ] {
        assert_ne!(duplicated, original);
        assert!(decode_native_request(duplicated.as_bytes()).is_err());
    }
    assert!(decode_native_request(format!("{}{}", " ".repeat(4096), original).as_bytes()).is_err());
}
#[test]
fn changed_owner_revision_target_disposition_and_dimensions_do_not_correlate() {
    for placement in [
        json!({"kind":"newLayer"}),
        json!({"kind":"authoredFrame","layerUid":"legacy layer:alpha","frame":3}),
    ] {
        let (q, r) = pair(placement);
        for field in ["requestId", "instanceId", "documentId", "contentRevision"] {
            let mut altered = serde_json::to_value(&r).unwrap();
            altered[field] = if field == "contentRevision" {
                json!(9)
            } else {
                json!("foreign")
            };
            assert!(
                !valid(&q, &serde_json::from_value(altered).unwrap()),
                "{field}"
            );
        }
        for (field, value) in [
            (
                "newLayer",
                json!(!r.result.as_ref().unwrap()["newLayer"].as_bool().unwrap()),
            ),
            ("historyEntriesAdded", json!(0)),
            ("naturalWidth", json!(0)),
            ("displayWidth", json!(641)),
            ("assetId", json!("")),
        ] {
            let mut altered = r.clone();
            altered.result.as_mut().unwrap()[field] = value;
            assert!(!valid(&q, &altered), "{field}");
        }
        for field in ["layerUid", "strokeId"] {
            let mut altered = r.clone();
            altered.result.as_mut().unwrap()["target"][field] = json!("foreign");
            assert!(!valid(&q, &altered), "{field}");
        }
    }
}
#[test]
fn catalog_only_compatibility_remains_closed_and_cannot_accept_a_placement_result() {
    let (mut q, r) = pair(json!({"kind":"newLayer"}));
    q.payload.as_object_mut().unwrap().remove("placement");
    q.payload
        .as_object_mut()
        .unwrap()
        .remove("lifecycleGeneration");
    assert!(q.validate_input().is_ok());
    assert!(!valid(&q, &r));
    assert!(result(
        ATTACH,
        &q.payload,
        &json!({"applied":false,"historyEntriesAdded":0})
    ));
    q.payload["placement"] = json!({"kind":"newLayer"});
    assert!(
        q.validate_input().is_err(),
        "placement requires lifecycle at the original registered boundary"
    );
}
