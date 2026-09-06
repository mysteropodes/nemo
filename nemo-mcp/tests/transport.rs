use nemo_mcp::{
    contract::{ApplicationRequest, ApplicationResponse, Operation, MAX_MESSAGE_BYTES},
    registry::{read_endpoints, Endpoint, Registration},
    wire,
};
use serde_json::json;
use tokio::{io::AsyncWriteExt, net::TcpListener};
use tokio_util::sync::CancellationToken;

fn request() -> ApplicationRequest {
    ApplicationRequest {
        api_version: 1,
        request_id: "edit-1".into(),
        instance_id: Some("instance".into()),
        document_id: Some("document".into()),
        expected_revision: Some(4),
        operation: Operation::PropertySet,
        payload: json!({"layerId":"layer", "property":"opacity", "value":25}),
    }
}

#[test]
fn mutation_needs_identity_before_transport() {
    let mut req = request();
    assert!(req.validate().is_ok());
    req.expected_revision = None;
    assert!(req.validate().is_err());
    req.operation = Operation::Snapshot;
    assert!(req.validate().is_ok());
    req.api_version = 2;
    assert!(req.validate().is_err());
}

#[tokio::test]
async fn bounded_reader_rejects_incomplete_and_oversized_input() {
    assert!(wire::read_json::<serde_json::Value>(&b"{}"[..])
        .await
        .is_err());
    assert!(wire::read_json::<serde_json::Value>(&b"{}\n"[..])
        .await
        .is_ok());
    let mut bytes = vec![b' '; MAX_MESSAGE_BYTES];
    bytes.extend_from_slice(b"{}\n");
    assert!(wire::read_json::<serde_json::Value>(bytes.as_slice())
        .await
        .is_err());
}

#[tokio::test]
async fn call_preserves_command_and_checks_response_identity() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: "instance".into(),
        port: listener.local_addr().unwrap().port(),
        secret: "secret".into(),
        build_id: "candidate".into(),
    };
    let server = tokio::spawn(async move {
        for wrong_id in [false, true] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let msg: wire::WireRequest = wire::read_json(&mut stream).await.unwrap();
            assert_eq!(msg.secret, "secret");
            assert_eq!(
                msg.request.payload,
                json!({"layerId":"layer", "property":"opacity", "value":25})
            );
            assert_eq!(msg.request.expected_revision, Some(4));
            let response = ApplicationResponse {
                api_version: 1,
                request_id: if wrong_id {
                    "different".into()
                } else {
                    msg.request.request_id
                },
                instance_id: "instance".into(),
                document_id: "document".into(),
                revision: 5,
                ok: true,
                result: Some(json!({"value":25})),
                error: None,
            };
            wire::write_json(&mut stream, &response).await.unwrap();
        }
    });
    let response = wire::call(&endpoint, request(), CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(response.revision, 5);
    assert_eq!(response.result, Some(json!({"value":25})));
    assert!(wire::call(&endpoint, request(), CancellationToken::new())
        .await
        .is_err());
    server.await.unwrap();
}

#[tokio::test]
async fn pre_cancelled_write_never_connects() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: "instance".into(),
        port: listener.local_addr().unwrap().port(),
        secret: "secret".into(),
        build_id: "candidate".into(),
    };
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert_eq!(
        wire::call(&endpoint, request(), cancel)
            .await
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::Interrupted
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(20), listener.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn malformed_peer_cannot_supply_success() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: "instance".into(),
        port: listener.local_addr().unwrap().port(),
        secret: "secret".into(),
        build_id: "candidate".into(),
    };
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let _: wire::WireRequest = wire::read_json(&mut stream).await.unwrap();
        stream.write_all(b"{\"ok\":true}\n").await.unwrap();
    });
    assert!(wire::call(&endpoint, request(), CancellationToken::new())
        .await
        .is_err());
    server.await.unwrap();
}

#[test]
fn registrations_are_private_and_removed_when_released() {
    let root = tempfile::tempdir().unwrap();
    let endpoint = Endpoint {
        instance_id: uuid::Uuid::new_v4().to_string(),
        port: 1234,
        secret: uuid::Uuid::new_v4().to_string(),
        build_id: "candidate".into(),
    };
    let registration = Registration::create(root.path(), &endpoint).unwrap();
    assert_eq!(read_endpoints(root.path()).unwrap().len(), 1);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = root.path().join(format!("{}.json", endpoint.instance_id));
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(root.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    drop(registration);
    assert!(read_endpoints(root.path()).unwrap().is_empty());
}
