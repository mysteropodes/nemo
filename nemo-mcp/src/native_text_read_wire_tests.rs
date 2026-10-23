//! Actual common wire admission/correlation controls, not a second query transport.
use super::*;
use serde_json::{json, Value};
fn pair() -> (NativeApplicationRequest, NativeApplicationResponse) {
    let target = json!({"contextId":"scene-root","layerUid":"native layer / 雪","frameScope":{"kind":"authored","frame":0},"strokeId":"native-text:1:0"});
    let q=serde_json::from_value(json!({"apiVersion":2,"requestId":"read-text","instanceId":"instance","documentId":"document","operation":"query.document.object.text",
        "payload":{"atRevision":7,"documentSnapshotId":"native-object:document:7","view":{"kind":"text","target":target,"offset":0,"limit":1024}}})).unwrap();
    let r=serde_json::from_value(json!({"apiVersion":2,"requestId":"read-text","instanceId":"instance","documentId":"document","contentRevision":7,"ok":true,
        "result":{"instanceId":"instance","documentId":"document","atRevision":7,"documentSnapshotId":"native-object:document:7","kind":"text","target":target,
        "textBytes":4,"offset":0,"text":"雪a","nextOffset":null}})).unwrap();
    (q, r)
}
#[test]
fn registered_text_query_uses_existing_common_original_byte_and_correlation_guards() {
    let (q, r) = pair();
    assert!(q.validate().is_ok());
    assert!(validate_native_response(&r, &q).is_ok());
    for field in ["requestId", "instanceId", "documentId", "contentRevision"] {
        let mut bad = serde_json::to_value(&r).unwrap();
        bad[field] = if field == "contentRevision" {
            json!(8)
        } else {
            json!("foreign")
        };
        assert!(
            validate_native_response(&serde_json::from_value(bad).unwrap(), &q).is_err(),
            "{field}"
        );
    }
    for field in [
        "instanceId",
        "documentId",
        "atRevision",
        "documentSnapshotId",
        "offset",
        "nextOffset",
    ] {
        let mut bad = serde_json::to_value(&r).unwrap();
        bad["result"][field] = if ["atRevision", "offset", "nextOffset"].contains(&field) {
            json!(8)
        } else {
            json!("foreign")
        };
        assert!(
            validate_native_response(&serde_json::from_value(bad).unwrap(), &q).is_err(),
            "{field}"
        );
    }
    let bytes = serde_json::to_vec(&q).unwrap();
    let parsed: NativeApplicationRequest = decode_json_bounded(&bytes, MAX_MESSAGE_BYTES).unwrap();
    assert!(parsed.validate().is_ok());
    let text = String::from_utf8(bytes)
        .unwrap()
        .replace("\"offset\":0", "\"offset\":0,\"offset\":0");
    assert!(
        decode_json_bounded::<NativeApplicationRequest>(text.as_bytes(), MAX_MESSAGE_BYTES)
            .is_err()
    );
    let text = text.replace(
        "\"offset\":0,\"offset\":0",
        "\"offset\":0,\"off\\u0073et\":0",
    );
    assert!(
        decode_json_bounded::<NativeApplicationRequest>(text.as_bytes(), MAX_MESSAGE_BYTES)
            .is_err()
    );
    let mut bad = serde_json::to_value(q).unwrap();
    bad["expectedRevision"] = json!(7);
    assert!(serde_json::from_value::<NativeApplicationRequest>(bad)
        .unwrap()
        .validate_input()
        .is_err());
}
#[test]
fn over_budget_or_unpaired_result_dispositions_never_become_success() {
    let (q, r) = pair();
    let mut bad = serde_json::to_value(&r).unwrap();
    bad["result"]["text"] = json!("a".repeat(4096));
    bad["result"]["textBytes"] = json!(4096);
    assert!(validate_native_response(&serde_json::from_value(bad).unwrap(), &q).is_err());
    let mut bad = serde_json::to_value(r).unwrap();
    bad["error"] = json!({"code":"unavailable","message":"no"});
    assert!(validate_native_response(&serde_json::from_value(bad).unwrap(), &q).is_err());
    let mut bad = serde_json::to_value(q).unwrap();
    bad["payload"]["view"]["rawFontBytes"] = Value::Array(vec![json!(1)]);
    assert!(serde_json::from_value::<NativeApplicationRequest>(bad)
        .unwrap()
        .validate_input()
        .is_err());
}
#[test]
fn genuine_text_owner_refusals_do_not_relax_success_or_unrelated_error_guards() {
    let (q, _) = pair();
    for code in ["wrong_instance", "wrong_document"] {
        let mut v = json!({"apiVersion":2,"requestId":q.request_id,"instanceId":if code=="wrong_instance"{"actual-instance"}else{q.instance_id.as_str()},"documentId":"actual-document","contentRevision":3,"ok":false,"error":{"code":code,"message":"Native read owner refusal"}});
        if code == "wrong_document" {
            v["error"]["details"] = json!({"requestedDocumentId":q.document_id});
        }
        let r: NativeApplicationResponse = serde_json::from_value(v.clone()).unwrap();
        assert!(validate_native_response(&r, &q).is_ok());
        v["requestId"] = json!("foreign");
        assert!(validate_native_response(&serde_json::from_value(v).unwrap(), &q).is_err());
    }
    for error in [
        json!({"code":"unavailable","message":"foreign"}),
        json!({"code":"wrong_document","message":"missing"}),
        json!({"code":"wrong_document","message":"bad","details":{"requestedDocumentId":"forged"}}),
    ] {
        let r=serde_json::from_value(json!({"apiVersion":2,"requestId":q.request_id,"instanceId":q.instance_id,"documentId":"actual-document","contentRevision":3,"ok":false,"error":error})).unwrap();
        assert!(validate_native_response(&r, &q).is_err());
    }
}
#[tokio::test]
async fn text_wrong_instance_may_report_actual_document_but_never_a_foreign_advertised_endpoint() {
    for forged in [false, true] {
        let (q, _) = pair();
        assert!(q.validate().is_ok());
        let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .await
            .unwrap();
        let endpoint = Endpoint {
            instance_id: "actual-instance".into(),
            port: listener.local_addr().unwrap().port(),
            secret: "test-local-secret".into(),
            build_id: "fixture".into(),
        };
        let response:NativeApplicationResponse=serde_json::from_value(json!({"apiVersion":2,"requestId":q.request_id,"instanceId":if forged{"foreign-endpoint"}else{"actual-instance"},"documentId":"actual-document","contentRevision":3,"ok":false,"error":{"code":"wrong_instance","message":"Actual owner"}})).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let request: Value = read_json(&mut stream).await.unwrap();
            assert_eq!(request["secret"], "test-local-secret");
            write_json(&mut stream, &response).await.unwrap();
        });
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            call_native(&endpoint, q, CancellationToken::new()),
        )
        .await
        .unwrap();
        if forged {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap().error.unwrap().code, "wrong_instance");
        }
        server.await.unwrap();
    }
}
