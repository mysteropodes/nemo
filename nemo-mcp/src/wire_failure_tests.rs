use super::*;
use serde::ser::Error as _;
use serde_json::Value;
use std::{
    future::pending,
    pin::Pin,
    task::{Context, Poll},
};

struct BrokenJson;
impl Serialize for BrokenJson {
    fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
        Err(S::Error::custom("fixture serialization failure"))
    }
}
struct BrokenWrite;
impl AsyncWrite for BrokenWrite {
    fn poll_write(self: Pin<&mut Self>, _: &mut Context<'_>, _: &[u8]) -> Poll<io::Result<usize>> {
        Poll::Ready(Err(io::Error::new(
            io::ErrorKind::BrokenPipe,
            "fixture closed writer",
        )))
    }
    fn poll_flush(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
    fn poll_shutdown(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }
}

#[tokio::test]
async fn json_writer_rejects_oversized_or_unserializable_input_before_writing() {
    let mut output = Vec::new();
    assert!(write_json(&mut output, &BrokenJson).await.is_err());
    assert!(output.is_empty());
    let oversized = "x".repeat(MAX_MESSAGE_BYTES - 2);
    assert_eq!(
        serde_json::to_vec(&oversized).unwrap().len(),
        MAX_MESSAGE_BYTES
    );
    assert_eq!(
        write_json(&mut output, &oversized)
            .await
            .unwrap_err()
            .to_string(),
        "message exceeds size limit"
    );
    assert!(output.is_empty());
    assert_eq!(
        write_json(&mut BrokenWrite, &serde_json::json!({}))
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::BrokenPipe
    );
}

#[tokio::test]
async fn deadline_and_precancel_keep_typed_uncertainty_without_repolling_or_retry() {
    let timeout = cancellable_for(
        pending::<io::Result<()>>(),
        CancellationToken::new(),
        Duration::from_millis(1),
    )
    .await
    .unwrap_err();
    assert_eq!(timeout.kind(), io::ErrorKind::TimedOut);
    assert_eq!(
        timeout.to_string(),
        "application timeout; query state before retrying a write"
    );
    let cancel = CancellationToken::new();
    cancel.cancel();
    let error = cancellable_for(
        async {
            panic!("pre-cancelled round trip was polled");
            #[allow(unreachable_code)]
            Ok::<(), io::Error>(())
        },
        cancel,
        Duration::ZERO,
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Interrupted);
    assert_eq!(
        error.to_string(),
        "request cancelled; query state before retrying a write"
    );
    assert_eq!(
        cancellable_for(
            async { Ok::<_, io::Error>(7) },
            CancellationToken::new(),
            Duration::from_secs(1)
        )
        .await
        .unwrap(),
        7
    );
}

#[tokio::test]
async fn bounded_reader_refuses_unrepresentable_limit_and_preserves_reader_failure() {
    assert_eq!(
        read_json_bounded::<Value>(&b"{}\n"[..], usize::MAX)
            .await
            .unwrap_err()
            .to_string(),
        "message limit overflow"
    );
    struct BrokenRead;
    impl AsyncRead for BrokenRead {
        fn poll_read(
            self: Pin<&mut Self>,
            _: &mut Context<'_>,
            _: &mut tokio::io::ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::ConnectionReset,
                "fixture read failure",
            )))
        }
    }
    assert_eq!(
        read_json_bounded::<Value>(BrokenRead, 4096)
            .await
            .unwrap_err()
            .kind(),
        io::ErrorKind::ConnectionReset
    );
}

#[tokio::test]
async fn native_status_transport_rejects_uncorrelated_or_invalid_host_status() {
    use tokio::net::TcpListener;
    for change in ["none", "request", "instance", "invalid"] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Endpoint {
            instance_id: "instance".into(),
            port: listener.local_addr().unwrap().port(),
            secret: "secret".into(),
            build_id: "fixture".into(),
        };
        let request = NativeStatusRequest {
            api_version: 2,
            request_id: "status".into(),
            instance_id: "instance".into(),
        };
        let mut response = NativeHostStatus::unavailable(&request, "fixture has no native owner");
        match change {
            "request" => response.request_id = "other".into(),
            "instance" => response.instance_id = "other".into(),
            "invalid" => response.api_version = 1,
            _ => {}
        }
        let peer = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let received: Value = read_json(&mut stream).await.unwrap();
            assert_eq!(
                received,
                serde_json::json!({"secret":"secret","nativeStatus":{"apiVersion":2,"requestId":"status","instanceId":"instance"}})
            );
            write_json(&mut stream, &response).await.unwrap();
        });
        let result = native_status(&endpoint, request, CancellationToken::new()).await;
        if change == "none" {
            assert_eq!(
                result.unwrap().reason.as_deref(),
                Some("fixture has no native owner")
            );
        } else {
            assert!(result.is_err(), "{change}");
        }
        peer.await.unwrap();
    }
}

#[tokio::test]
async fn metadata_replacement_error_is_explicit_and_response_dispositions_are_exclusive() {
    use tokio::net::TcpListener;
    let request:NativeApplicationRequest=serde_json::from_value(serde_json::json!({"apiVersion":2,"requestId":"metadata",
        "instanceId":"instance","documentId":"doc","operation":"query.document.object.property.metadata",
        "payload":{"atRevision":0,"documentSnapshotId":"native-object:doc:0","view":{"kind":"targets","offset":0,"limit":1}}})).unwrap();
    request.validate().unwrap();
    for change in ["replacement", "wrong-code", "both", "neither"] {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = Endpoint {
            instance_id: "instance".into(),
            port: listener.local_addr().unwrap().port(),
            secret: "secret".into(),
            build_id: "fixture".into(),
        };
        let mut response = NativeApplicationResponse {
            api_version: 2,
            request_id: "metadata".into(),
            instance_id: "instance".into(),
            document_id: "replacement".into(),
            content_revision: 0,
            ok: false,
            result: None,
            error: Some(crate::contract::NativeApplicationError {
                code: "wrong_document".into(),
                message: "owner replaced document".into(),
                details: None,
            }),
        };
        match change {
            "wrong-code" => response.error.as_mut().unwrap().code = "busy_conflict".into(),
            "both" => {
                response.document_id = "doc".into();
                response.result = Some(serde_json::json!({}));
            }
            "neither" => {
                response.document_id = "doc".into();
                response.error = None;
            }
            _ => {}
        }
        let peer = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let received: NativeWireRequest = read_json(&mut stream).await.unwrap();
            assert_eq!(received.secret, "secret");
            write_json(&mut stream, &response).await.unwrap();
        });
        let result = call_native(&endpoint, request.clone(), CancellationToken::new()).await;
        if change == "replacement" {
            assert_eq!(result.unwrap().document_id, "replacement");
        } else {
            assert!(result.is_err(), "{change}");
        }
        peer.await.unwrap();
    }
}
