//! Reply/status framing controls remain separate from operation-specific request limits.
use crate::contract::NativeApplicationResponse;
use crate::wire::decode_json_bounded;
use serde_json::json;
use tokio::io::AsyncWriteExt;
#[tokio::test]
async fn original_native_response_framing_keeps4096_content_bytes_and_its_newline_separate() {
    let mut response = json!({"apiVersion":2,"requestId":"reply","instanceId":"instance","documentId":"document","contentRevision":7,"ok":false,"error":{"code":"unavailable","message":""}});
    let size = serde_json::to_vec(&response).unwrap().len();
    response["error"]["message"] = json!("x".repeat(4096 - size));
    let raw = serde_json::to_vec(&response).unwrap();
    assert_eq!(raw.len(), 4096);
    let (mut writer, reader) = tokio::io::duplex(4098);
    writer.write_all(&raw).await.unwrap();
    writer.write_all(b"\n").await.unwrap();
    drop(writer);
    let actual: NativeApplicationResponse =
        crate::wire::read_json_bounded(reader, 4096).await.unwrap();
    assert_eq!(serde_json::to_value(actual).unwrap(), response);
    response["error"]["message"] = json!("x".repeat(4097 - size));
    let mut raw = serde_json::to_vec(&response).unwrap();
    assert_eq!(raw.len(), 4097);
    raw.push(b'\n');
    let (mut writer, reader) = tokio::io::duplex(raw.len() + 1);
    writer.write_all(&raw).await.unwrap();
    drop(writer);
    assert!(
        crate::wire::read_json_bounded::<NativeApplicationResponse>(reader, 4096)
            .await
            .is_err()
    );
}

#[test]
fn native_status_and_reply_objects_do_not_select_a_request_budget() {
    for body in [
        json!({"apiVersion":2,"requestId":"status","instanceId":"instance","available":false,"nativeContentRevision":null}),
        json!({"apiVersion":2,"requestId":"response","instanceId":"instance","documentId":"document","contentRevision":0,"ok":false,"error":{"code":"unavailable","message":"x".repeat(4100)}}),
    ] {
        let raw = serde_json::to_vec(&body).unwrap();
        assert!(decode_json_bounded::<serde_json::Value>(&raw, 8192).is_ok());
        if raw.len() > 4096 {
            assert!(decode_json_bounded::<NativeApplicationResponse>(&raw, 4096).is_err());
        }
    }
}
