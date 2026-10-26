//! Existing presentation-read transport controls, separate from production coverage.
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use serde_json::json;

// Literal transport contracts establish no native surface presentation authority.
fn request() -> NativeApplicationRequest {
    serde_json::from_value(json!({
        "apiVersion":2,"requestId":"presentation-read","instanceId":"instance",
        "documentId":"document","operation":"query.document.object",
        "payload":{"atRevision":7,"view":{"kind":"presentation"}}
    }))
    .unwrap()
}

fn response() -> NativeApplicationResponse {
    serde_json::from_value(json!({
        "apiVersion":2,"requestId":"presentation-read","instanceId":"instance",
        "documentId":"document","contentRevision":7,"ok":true,
        "result":{"kind":"presentation","atRevision":7,
            "documentSnapshotId":"native-object:document:7","contextId":"scene-root",
            "frame":12,"workId":"native-work-9","viewGeneration":3}
    }))
    .unwrap()
}

#[tokio::test]
async fn presentation_wire_rejects_foreign_common_envelope_identity() {
    use crate::wire;
    for key in [
        None,
        Some("requestId"),
        Some("instanceId"),
        Some("documentId"),
    ] {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = crate::registry::Endpoint {
            instance_id: "instance".into(),
            port: listener.local_addr().unwrap().port(),
            secret: "presentation-contract".into(),
            build_id: "contract-test".into(),
        };
        let mut value = serde_json::to_value(response()).unwrap();
        if let Some(key) = key {
            value[key] = json!("foreign");
        }
        let server = async {
            let (mut stream, _) = listener.accept().await.unwrap();
            let envelope: wire::AuthenticatedWireRequest =
                wire::read_json(&mut stream).await.unwrap();
            let wire::AuthenticatedWireRequest::Native(message) = envelope else {
                panic!("non-native request")
            };
            assert_eq!(message.secret, endpoint.secret);
            assert_eq!(message.native_request, request());
            wire::write_json(&mut stream, &value).await.unwrap();
        };
        let client = wire::call_native(&endpoint, request(), wire::CancellationToken::new());
        let (_, result) = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(server, client)
        })
        .await
        .unwrap();
        assert_eq!(result.is_ok(), key.is_none());
    }
}
