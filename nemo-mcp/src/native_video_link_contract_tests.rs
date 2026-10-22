//! T12 registered wire/shape/correlation controls of the linked-video op.
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use crate::native_contract::video_link::{owner_matches, result, revision_matches, OP};
use crate::wire::{decode_native_request, validate_native_response};
use serde_json::{json, Value};

fn pair(intent: Value) -> (NativeApplicationRequest, NativeApplicationResponse) {
    let q: NativeApplicationRequest =
        serde_json::from_value(json!({"apiVersion":2,"requestId":"video-wire",
        "instanceId":"instance","documentId":"document","expectedRevision":7,"operation":OP,
        "payload":{"documentSnapshotId":"native-object:document:7","lifecycleGeneration":1,
            "source":{"kind":"file","path":"/admitted/clip.mov"},"intent":intent}}))
        .unwrap();
    let new_layer = q.payload["intent"]["kind"] == "newLayer";
    let placement = if new_layer {
        json!("placement-video-wire")
    } else {
        q.payload["intent"]["placementId"].clone()
    };
    let r: NativeApplicationResponse = serde_json::from_value(json!({"apiVersion":2,"requestId":q.request_id,
        "instanceId":q.instance_id,"documentId":q.document_id,"contentRevision":8,"ok":true,
        "result":{"applied":true,"historyEntriesAdded":1,"assetId":"asset-video-wire","placementId":placement,
            "target":{"contextId":"scene-root","layerUid":"native-layer:8:0"},"newLayer":new_layer,
            "sourceWidth":1920,"sourceHeight":1080,"sourceFrameCount":240}}))
    .unwrap();
    (q, r)
}
fn intents() -> [Value; 2] {
    [
        json!({"kind":"newLayer"}),
        json!({"kind":"replace","placementId":"legacy placement:a"}),
    ]
}
fn with(q: &NativeApplicationRequest, pointer: &str, value: Value) -> NativeApplicationRequest {
    let mut raw = serde_json::to_value(q).unwrap();
    let (parent, key) = pointer.rsplit_once('/').unwrap();
    raw.pointer_mut(parent).unwrap()[key] = value;
    serde_json::from_value(raw).unwrap()
}
fn refusal(
    q: &NativeApplicationRequest,
    code: &str,
    instance: &str,
    document: &str,
) -> NativeApplicationResponse {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":q.request_id,"instanceId":instance,
        "documentId":document,"contentRevision":7,"ok":false,
        "error":{"code":code,"message":"Native video link refused."}}),
    )
    .unwrap()
}

#[test]
fn both_intents_cross_original_byte_admission_and_registered_availability() {
    for intent in intents() {
        let (q, r) = pair(intent);
        let parsed = decode_native_request(&serde_json::to_vec(&q).unwrap()).unwrap();
        assert_eq!(parsed, q);
        assert!(q.validate().is_ok());
        assert!(validate_native_response(&r, &q).is_ok());
    }
}

#[test]
fn closed_payload_refuses_unknown_members_bad_paths_ids_and_a_missing_revision() {
    let (q, _) = pair(json!({"kind":"newLayer"}));
    for (pointer, value) in [
        ("/payload/bytes", json!("page bytes")),
        ("/payload/source/bytes", json!("page bytes")),
        ("/payload/source/kind", json!("url")),
        ("/payload/source/path", json!("")),
        ("/payload/source/path", json!("/clip\u{0}.mov")),
        ("/payload/lifecycleGeneration", json!(0)),
        ("/payload/documentSnapshotId", json!("bad snapshot")),
        ("/payload/intent", json!({"kind":"replace"})),
        (
            "/payload/intent",
            json!({"kind":"replace","placementId":"  "}),
        ),
        (
            "/payload/intent",
            json!({"kind":"newLayer","placementId":"p"}),
        ),
        ("/payload/intent", json!({"kind":"authoredFrame"})),
    ] {
        assert!(
            with(&q, pointer, value.clone()).validate_input().is_err(),
            "{pointer} = {value}"
        );
    }
    // The advertised bound is the last accepted length; one byte more is refused.
    let path = |n: usize| {
        json!({"documentSnapshotId":"s",
        "lifecycleGeneration":1,"source":{"kind":"file","path":format!("/{}", "a".repeat(n - 1))},
        "intent":{"kind":"newLayer"}})
    };
    assert!(crate::native_contract::video_link::request(&path(2048)));
    assert!(!crate::native_contract::video_link::request(&path(2049)));
    let mut missing = q;
    missing.expected_revision = None;
    assert!(missing.validate_input().is_err());
}

#[test]
fn duplicate_members_are_refused_from_the_original_bytes() {
    let (q, _) = pair(json!({"kind":"newLayer"}));
    let original = serde_json::to_string(&q).unwrap();
    for duplicated in [
        original.replace(
            "\"lifecycleGeneration\":1",
            "\"lifecycleGeneration\":1,\"lifecycleGeneration\":1",
        ),
        original.replace(
            "\"path\":\"/admitted/clip.mov\"",
            "\"path\":\"/admitted/clip.mov\",\"path\":\"/other.mov\"",
        ),
    ] {
        assert_ne!(duplicated, original);
        assert!(decode_native_request(duplicated.as_bytes()).is_err());
    }
}

#[test]
fn changed_owner_revision_intent_and_facts_do_not_correlate() {
    for intent in intents() {
        let (q, r) = pair(intent);
        for field in ["requestId", "instanceId", "documentId", "contentRevision"] {
            let mut altered = serde_json::to_value(&r).unwrap();
            altered[field] = if field == "contentRevision" {
                json!(9)
            } else {
                json!("foreign")
            };
            let altered: NativeApplicationResponse = serde_json::from_value(altered).unwrap();
            assert!(validate_native_response(&altered, &q).is_err(), "{field}");
        }
        for (field, value) in [
            (
                "newLayer",
                json!(!r.result.as_ref().unwrap()["newLayer"].as_bool().unwrap()),
            ),
            ("placementId", json!("another-placement")),
            ("historyEntriesAdded", json!(0)),
            ("applied", json!(false)),
            ("sourceFrameCount", json!(0)),
            ("sourceWidth", json!(4_294_967_296u64)),
            ("assetId", json!("")),
            ("decoder", json!("leaked")),
        ] {
            let mut altered = r.clone();
            altered.result.as_mut().unwrap()[field] = value;
            assert!(validate_native_response(&altered, &q).is_err(), "{field}");
        }
        for context in [
            json!(""),
            json!("  "),
            json!("a\u{0}b"),
            json!("c".repeat(256)),
            json!(7),
        ] {
            let mut altered = r.clone();
            altered.result.as_mut().unwrap()["target"]["contextId"] = context;
            assert!(!result(altered.result.as_ref().unwrap()));
        }
        let value = r.result.as_ref().unwrap();
        assert!(result(value) && revision_matches(&q, 8, value));
        assert!(!revision_matches(&q, 7, value));
    }
}

#[test]
fn owner_refusals_are_narrow_and_other_errors_stay_strict() {
    let (q, _) = pair(json!({"kind":"newLayer"}));
    for (code, instance, document) in [
        ("wrong_instance", "actual-instance", "actual-document"),
        ("wrong_instance", "actual-instance", q.document_id.as_str()),
        ("wrong_document", q.instance_id.as_str(), "actual-document"),
    ] {
        let r = refusal(&q, code, instance, document);
        assert_eq!(owner_matches(&q, &r), Some(true));
        assert!(
            validate_native_response(&r, &q).is_ok(),
            "{code}/{instance}"
        );
        let mut changed = r.clone();
        changed.request_id = "foreign".into();
        assert!(validate_native_response(&changed, &q).is_err());
    }
    for (code, instance, document) in [
        ("stale_revision", "actual-instance", "actual-document"),
        ("unavailable", q.instance_id.as_str(), "actual-document"),
        ("wrong_document", "actual-instance", "actual-document"),
        ("wrong_instance", q.instance_id.as_str(), "actual-document"),
    ] {
        assert!(
            validate_native_response(&refusal(&q, code, instance, document), &q).is_err(),
            "{code}/{instance}"
        );
    }
    let mut other = q.clone();
    other.operation = crate::native_contract::item::ATTACH.into();
    assert!(owner_matches(&other, &refusal(&q, "wrong_instance", "a", "b")).is_none());
}

#[test]
fn ordinary_refusals_from_the_owner_correlate_without_a_result() {
    let (q, _) = pair(json!({"kind":"newLayer"}));
    for code in [
        "stale_revision",
        "not_found",
        "unavailable",
        "invalid_request",
    ] {
        let r = refusal(&q, code, &q.instance_id, &q.document_id);
        assert!(validate_native_response(&r, &q).is_ok(), "{code}");
    }
    // The host's probe timeout: typed notDispatched, relayed as an ordinary retryable refusal.
    let mut timeout = refusal(&q, "unavailable", &q.instance_id, &q.document_id);
    timeout.error.as_mut().unwrap().details =
        Some(json!({"disposition":"notDispatched","committed":false,"retryExecution":false}));
    assert!(validate_native_response(&timeout, &q).is_ok());
}

#[test]
fn a_replace_inside_a_component_correlates_but_a_new_layer_stays_in_the_scene_root() {
    let (q, r) = pair(json!({"kind":"replace","placementId":"p-comp"}));
    let mut inside = r.clone();
    inside.result.as_mut().unwrap()["target"]["contextId"] = json!("sym_comp");
    assert!(result(inside.result.as_ref().unwrap()));
    assert!(revision_matches(&q, 8, inside.result.as_ref().unwrap()));
    assert!(validate_native_response(&inside, &q).is_ok());
    let (q, r) = pair(json!({"kind":"newLayer"}));
    let mut elsewhere = r.clone();
    elsewhere.result.as_mut().unwrap()["target"]["contextId"] = json!("sym_comp");
    assert!(result(elsewhere.result.as_ref().unwrap()));
    assert!(!revision_matches(&q, 8, elsewhere.result.as_ref().unwrap()));
    assert!(validate_native_response(&elsewhere, &q).is_err());
}

#[test]
fn the_path_bound_fits_the_whole_request_with_every_other_field_at_its_maximum() {
    let id = |n: usize| "a".repeat(n);
    let at = |n: usize| -> NativeApplicationRequest {
        serde_json::from_value(
            json!({"apiVersion":2,"requestId":id(128),"instanceId":id(128),
            "documentId":id(128),"expectedRevision":9_007_199_254_740_991_u64,"operation":OP,
            "payload":{"documentSnapshotId":id(128),"lifecycleGeneration":9_007_199_254_740_991_u64,
                "source":{"kind":"file","path":format!("/{}", id(n - 1))},
                "intent":{"kind":"replace","placementId":id(255)}}}),
        )
        .unwrap()
    };
    let limit = serde_json::to_vec(&at(2048)).unwrap();
    assert!(limit.len() <= 4096, "{}", limit.len());
    let parsed = decode_native_request(&limit).unwrap();
    assert!(parsed.validate().is_ok());
    assert!(at(2049).validate_input().is_err());
}
