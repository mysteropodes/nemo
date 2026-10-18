//! Exact placed-PNG response owner/caller correlation; authored UNRUN, no availability bypass.
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use crate::wire::{call_native, read_json, validate_native_response, write_json, Endpoint};
use serde_json::{json, Value};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
fn request() -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"png-wire-owner","instanceId":"requested-instance","documentId":"requested-document","expectedRevision":0,
        "operation":"command.document.object.asset.attach","payload":{"documentSnapshotId":"native-object:requested-document:0","lifecycleGeneration":1,
            "name":"still.png","mediaType":"image/png","source":{"kind":"file","path":"/picked.png"},"placement":{"kind":"newLayer"}}})).unwrap()
}
fn refusal(
    q: &NativeApplicationRequest,
    code: &str,
    instance: &str,
    document: &str,
) -> NativeApplicationResponse {
    serde_json::from_value(json!({"apiVersion":2,"requestId":q.request_id,"instanceId":instance,"documentId":document,"contentRevision":0,
        "ok":false,"error":{"code":code,"message":"Actual native image owner refusal"}})).unwrap()
}
#[test]
fn placed_png_owner_refusals_are_narrow_and_request_identity_and_other_errors_stay_strict() {
    let q = request();
    assert!(q.validate_input().is_ok());
    for (code, instance, document) in [
        ("wrong_instance", "actual-instance", "actual-document"),
        ("wrong_instance", "actual-instance", q.document_id.as_str()),
        ("wrong_document", q.instance_id.as_str(), "actual-document"),
    ] {
        let r = refusal(&q, code, instance, document);
        assert!(validate_native_response(&r, &q).is_ok());
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
    let mut catalog = q.clone();
    catalog.payload.as_object_mut().unwrap().remove("placement");
    catalog
        .payload
        .as_object_mut()
        .unwrap()
        .remove("lifecycleGeneration");
    let r = refusal(
        &catalog,
        "wrong_instance",
        "actual-instance",
        "actual-document",
    );
    assert!(!crate::wire::response_owner::reports_document(&catalog, &r));
    assert!(crate::native_contract::item::placed_png_owner_matches(&catalog, &r).is_none());
}
#[test]
fn placed_png_success_never_uses_owner_refusal_exceptions() {
    let q = request();
    let success:NativeApplicationResponse=serde_json::from_value(json!({"apiVersion":2,"requestId":q.request_id,"instanceId":q.instance_id,"documentId":q.document_id,
        "contentRevision":1,"ok":true,"result":{"applied":true,"historyEntriesAdded":1,"assetId":"asset-png","naturalWidth":2,"naturalHeight":1,"displayWidth":2,"displayHeight":1,
            "target":{"contextId":"scene-root","layerUid":"native-layer:1:0","frameScope":{"kind":"authored","frame":0},"strokeId":"native-raster:1:0"},"newLayer":true}})).unwrap();
    assert!(validate_native_response(&success, &q).is_ok());
    for field in ["instanceId", "documentId", "requestId"] {
        let mut changed = serde_json::to_value(&success).unwrap();
        changed[field] = json!("foreign");
        assert!(validate_native_response(&serde_json::from_value(changed).unwrap(), &q).is_err());
    }
}
#[tokio::test]
async fn actual_call_native_accepts_png_wrong_instance_actual_document_but_keeps_advertised_endpoint_instance_strict(
) {
    for forged_endpoint in [false, true] {
        let q = request();
        assert!(
            q.validate().is_ok(),
            "requires reviewed PNG capability activation before executing the full wire control"
        );
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let endpoint = Endpoint {
            instance_id: "actual-instance".into(),
            port: listener.local_addr().unwrap().port(),
            secret: "test-local-secret".into(),
            build_id: "fixture".into(),
        };
        let r = refusal(
            &q,
            "wrong_instance",
            if forged_endpoint {
                "foreign-endpoint"
            } else {
                "actual-instance"
            },
            "actual-document",
        );
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let input: Value = read_json(&mut stream).await.unwrap();
            assert_eq!(input["secret"], "test-local-secret");
            write_json(&mut stream, &r).await.unwrap();
        });
        let response = tokio::time::timeout(
            Duration::from_secs(5),
            call_native(&endpoint, q, CancellationToken::new()),
        )
        .await
        .unwrap();
        if forged_endpoint {
            assert!(response.is_err());
        } else {
            assert_eq!(response.unwrap().error.unwrap().code, "wrong_instance");
        }
        server.await.unwrap();
    }
}

#[test]
fn owner_refusal_correlation_never_waives_original_response_size_or_identifier_bounds() {
    let q = request();
    let mut r = refusal(&q, "wrong_instance", "actual-instance", "actual-document");
    r.error.as_mut().unwrap().message = "x".repeat(4096);
    assert!(validate_native_response(&r, &q).is_err());
    let mut r = refusal(&q, "wrong_instance", "bad instance", "actual-document");
    assert!(validate_native_response(&r, &q).is_err());
    r.instance_id = "actual-instance".into();
    r.document_id = "bad document".into();
    assert!(validate_native_response(&r, &q).is_err());
}
