//! Original-byte duplicate rejection and actual common read receipt correlation.
use crate::contract::{
    NativeApplicationRequest, NativeApplicationResponse, NATIVE_MAX_MESSAGE_BYTES,
};
use crate::wire::{decode_json_bounded, validate_native_response};
use serde_json::{json, Value};
fn pair(keys: bool) -> (NativeApplicationRequest, NativeApplicationResponse) {
    let mut view = json!({"kind":"rawSample","target":{"contextId":"scene-root","layerUid":"data:opaque 雪","strokeId":"blob:stroke"},"property":"opacity","frame":0.5});
    if keys {
        view.as_object_mut().unwrap().remove("frame");
        view["kind"] = json!("authoredKeys");
        view["offset"] = json!(0);
        view["limit"] = json!(2);
    }
    let request: NativeApplicationRequest = serde_json::from_value(json!({"apiVersion":2,"requestId":"read","instanceId":"instance","documentId":"document","operation":"query.document.object.property.read","payload":{"atRevision":7,"documentSnapshotId":"opaque:7","view":view}})).unwrap();
    let mut result = json!({"instanceId":"instance","documentId":"document","atRevision":7,"documentSnapshotId":"opaque:7","kind":view["kind"],"target":view["target"],"property":"opacity","dimension":1});
    if keys {
        result["trackPresent"] = json!(true);
        result["items"] =
            json!([{"frame":0,"v":[17],"curvePoints":null},{"frame":4,"v":[80],"curvePoints":[]}]);
        result["nextOffset"] = Value::Null;
    } else {
        result["frame"] = json!(0.5);
        result["value"] = json!([17]);
    }
    let response = serde_json::from_value(json!({"apiVersion":2,"requestId":"read","instanceId":"instance","documentId":"document","contentRevision":7,"ok":true,"result":result})).unwrap();
    (request, response)
}
#[test]
fn property_read_wire_correlates_all_pins_and_retains_typed_owner_refusals() {
    for keys in [false, true] {
        let (q, r) = pair(keys);
        assert!(q.validate().is_ok());
        assert!(validate_native_response(&r, &q).is_ok());
        for name in ["requestId", "instanceId", "documentId", "contentRevision"] {
            let mut bad = serde_json::to_value(&r).unwrap();
            bad[name] = if name == "contentRevision" {
                json!(8)
            } else {
                json!("foreign")
            };
            assert!(
                validate_native_response(&serde_json::from_value(bad).unwrap(), &q).is_err(),
                "{name}"
            );
        }
        for name in [
            "instanceId",
            "documentId",
            "atRevision",
            "documentSnapshotId",
            "kind",
            "target",
            "property",
            "dimension",
        ] {
            let mut bad = r.clone();
            bad.result.as_mut().unwrap()[name] = if name == "dimension" {
                json!(2)
            } else {
                json!("foreign")
            };
            assert!(validate_native_response(&bad, &q).is_err(), "inner {name}");
        }
        for code in [
            "invalid_request",
            "wrong_instance",
            "wrong_document",
            "stale_revision",
            "not_found",
            "unavailable",
            "cancelled_before_dispatch",
        ] {
            let mut failure = r.clone();
            failure.ok = false;
            failure.result = None;
            failure.error = Some(
                serde_json::from_value(json!({"code":code,"message":"Native read refused"}))
                    .unwrap(),
            );
            if code == "wrong_document" {
                failure.document_id = "replacement".into();
            }
            assert!(validate_native_response(&failure, &q).is_ok());
            failure.error.as_mut().unwrap().message = "x".repeat(4096);
            assert!(validate_native_response(&failure, &q).is_err());
        }
    }
}
#[test]
fn property_read_original_wire_refuses_every_nested_duplicate_and_nonfinite_number() {
    for keys in [false, true] {
        let (q, _) = pair(keys);
        let raw = serde_json::to_string(&q).unwrap();
        let mut members = Vec::new();
        for object in [&q.payload, &q.payload["view"], &q.payload["view"]["target"]] {
            members.extend(
                object
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.clone())),
            );
        }
        for (name, value) in members {
            let needle = format!("\"{name}\":{value}");
            assert_eq!(raw.matches(&needle).count(), 1);
            let duplicate = raw.replacen(&needle, &format!("{needle},{needle}"), 1);
            assert!(
                decode_json_bounded::<NativeApplicationRequest>(
                    duplicate.as_bytes(),
                    NATIVE_MAX_MESSAGE_BYTES
                )
                .is_err(),
                "{name}"
            );
        }
        assert!(decode_json_bounded::<NativeApplicationRequest>(
            raw.as_bytes(),
            NATIVE_MAX_MESSAGE_BYTES
        )
        .unwrap()
        .validate()
        .is_ok());
    }
    let (_, response) = pair(true);
    let response_raw = serde_json::to_string(&response).unwrap();
    for needle in ["\"frame\":0", "\"curvePoints\":null", "\"v\":[17]"] {
        assert_eq!(response_raw.matches(needle).count(), 1);
        let duplicate = response_raw.replacen(needle, &format!("{needle},{needle}"), 1);
        assert!(decode_json_bounded::<NativeApplicationResponse>(
            duplicate.as_bytes(),
            NATIVE_MAX_MESSAGE_BYTES
        )
        .is_err());
    }
    let (q, _) = pair(false);
    let raw = serde_json::to_string(&q).unwrap();
    let huge = raw.replacen("\"frame\":0.5", "\"frame\":1e309", 1);
    assert_ne!(huge, raw);
    assert!(decode_json_bounded::<NativeApplicationRequest>(
        huge.as_bytes(),
        NATIVE_MAX_MESSAGE_BYTES
    )
    .is_err());
}
#[test]
fn property_read_wire_counts_complete_utf8_bytes_and_separate_inner_result_bound() {
    let (mut q, _) = pair(false);
    let n = serde_json::to_vec(&q).unwrap().len();
    let uid = q.payload["view"]["target"]["layerUid"]
        .as_str()
        .unwrap()
        .to_owned();
    q.payload["view"]["target"]["layerUid"] = json!(uid + &"x".repeat(4096 - n));
    let mut raw = serde_json::to_vec(&q).unwrap();
    assert_eq!(raw.len(), 4096);
    assert!(decode_json_bounded::<NativeApplicationRequest>(&raw, 4096)
        .unwrap()
        .validate()
        .is_ok());
    raw.push(b' ');
    assert!(decode_json_bounded::<NativeApplicationRequest>(&raw, 4096).is_err());
    let (mut q, mut r) = pair(false);
    q.payload["view"]["property"] = json!("x".repeat(3072));
    r.result.as_mut().unwrap()["property"] = q.payload["view"]["property"].clone();
    assert!(serde_json::to_vec(&r).unwrap().len() < 4096);
    assert!(
        validate_native_response(&r, &q).is_err(),
        "3072 inner bound is independent of4096 envelope"
    );
}
