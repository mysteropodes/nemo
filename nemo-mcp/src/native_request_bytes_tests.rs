//! Original spans and framing tests; no socket, owner or test-only budget substitution.
use crate::contract::MAX_MESSAGE_BYTES;
use crate::wire::{
    decode_json_bounded, decode_native_request, guarded_stdio_input, native_request_byte_limit,
    AuthenticatedWireRequest,
};
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
fn request(vector: bool) -> Value {
    json!({"apiVersion":2,"requestId":"raw","instanceId":"instance","documentId":"document","expectedRevision":7,
        "operation":if vector {"command.document.object.vector.edit"} else {"command.document.object.property.edit"},
        "payload":if vector {json!({"command":"object.vector.delete","stableTarget":{"contextId":"scene-root","frameScope":{"kind":"reference","frame":0},"layerUid":"blob:layer 雪","strokeId":"stroke"}})}else{json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":"layer"},"property":"opacity","action":{"kind":"removeKey","frame":-0.5}})}})
}
fn padded(value: &Value, size: usize) -> String {
    let mut raw = serde_json::to_string(value).unwrap();
    let padding = size.checked_sub(raw.len()).unwrap();
    raw.insert_str(raw.len() - 1, &" ".repeat(padding));
    assert_eq!(raw.len(), size);
    raw
}
fn tcp(request: &str) -> String {
    format!("{{\"secret\":\"test-secret\",\"nativeRequest\":{request}}}")
}
fn stdio(request: &str) -> String {
    format!("{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"tools/call\",\"params\":{{\"name\":\"nemo_command\",\"arguments\":{request}}}}}")
}
async fn guarded(bytes: Vec<u8>) -> Vec<u8> {
    let (mut writer, reader) = tokio::io::duplex(bytes.len() + 1);
    writer.write_all(&bytes).await.unwrap();
    drop(writer);
    let mut output = Vec::new();
    guarded_stdio_input(reader)
        .read_to_end(&mut output)
        .await
        .unwrap();
    output
}
#[test]
fn native_original_whitespace_utf8_limits_apply_at_direct_tcp_and_actual_stdio_argument_paths() {
    for (is_vector, limit) in [(true, 65536), (false, 4096)] {
        let value = request(is_vector);
        assert_eq!(
            native_request_byte_limit(value["operation"].as_str().unwrap()),
            limit
        );
        let raw = padded(&value, limit);
        assert!(raw.contains('雪') || !is_vector);
        assert!(decode_native_request(raw.as_bytes())
            .unwrap()
            .validate_input()
            .is_ok());
        assert!(decode_native_request(padded(&value, limit + 1).as_bytes()).is_err());
        for wrap in [tcp as fn(&str) -> String, stdio] {
            assert!(
                decode_json_bounded::<Value>(wrap(&raw).as_bytes(), MAX_MESSAGE_BYTES - 2).is_ok()
            );
            let bad = wrap(&padded(&value, limit + 1));
            let error =
                decode_json_bounded::<Value>(bad.as_bytes(), MAX_MESSAGE_BYTES - 2).unwrap_err();
            assert!(error
                .to_string()
                .contains(&format!("{limit} original bytes")));
        }
        let wire: AuthenticatedWireRequest =
            decode_json_bounded(tcp(&raw).as_bytes(), MAX_MESSAGE_BYTES - 2).unwrap();
        let AuthenticatedWireRequest::Native(wire) = wire else {
            panic!("native wrapper lost")
        };
        assert_eq!(serde_json::to_value(wire.native_request).unwrap(), value);
    }
    for operation in [
        "command.document.object.property.track.edit",
        "command.document.object.layer.policy.edit",
        "query.document.object",
        "unknown",
    ] {
        assert_eq!(native_request_byte_limit(operation), 4096);
    }
    let mut value = request(true);
    value["payload"]["stableTarget"]["layerUid"] = json!("雪".repeat(21846));
    assert!(serde_json::to_vec(&value).unwrap().len() > 65536);
    assert!(decode_native_request(serde_json::to_string(&value).unwrap().as_bytes()).is_err());
}
#[test]
fn native_original_duplicate_and_escaped_keys_precede_bound_or_value_collapse() {
    let raw = serde_json::to_string(&request(true)).unwrap();
    for (key, value) in [
        ("operation", json!("command.document.object.vector.edit")),
        ("command", json!("object.vector.delete")),
        ("contextId", json!("scene-root")),
        ("kind", json!("reference")),
        ("frame", json!(0)),
    ] {
        let needle = format!("\"{key}\":{value}");
        assert_eq!(raw.matches(&needle).count(), 1);
        let duplicated = raw.replacen(&needle, &format!("{needle},{needle}"), 1);
        let error = decode_native_request(duplicated.as_bytes()).unwrap_err();
        assert_eq!(error.to_string(), "duplicate JSON member");
    }
    // Outer framing remains bounded, but native request size must not preempt
    // complete syntax/duplicate admission before Value/typed collapse.
    let needle = "\"frame\":0";
    let mut oversized = padded(&request(true), 65537);
    oversized = oversized.replacen(needle, "\"frame\":0,\"frame\":0", 1);
    assert_eq!(
        decode_native_request(oversized.as_bytes())
            .unwrap_err()
            .to_string(),
        "duplicate JSON member"
    );
    let malformed = oversized.strip_suffix('}').unwrap();
    assert_ne!(
        decode_native_request(malformed.as_bytes())
            .unwrap_err()
            .to_string(),
        "duplicate JSON member"
    );
    let escaped = raw.replacen("\"operation\"", "\"oper\\u0061tion\"", 1);
    assert!(decode_native_request(escaped.as_bytes()).is_ok());
    let needle = "\"operation\":\"command.document.object.vector.edit\"";
    let duplicate = raw.replacen(
        needle,
        &format!("{needle},\"oper\\u0061tion\":\"command.document.object.vector.edit\""),
        1,
    );
    assert_eq!(
        decode_native_request(duplicate.as_bytes())
            .unwrap_err()
            .to_string(),
        "duplicate JSON member"
    );
    let positional = json!([
        2,
        "raw",
        "instance",
        "document",
        7,
        "command.document.object.vector.edit",
        request(true)["payload"],
        false
    ]);
    assert!(decode_native_request(serde_json::to_string(&positional).unwrap().as_bytes()).is_err());
    let unknown = raw.replacen("\"payload\":", "\"extra\":0,\"payload\":", 1);
    assert!(decode_native_request(unknown.as_bytes()).is_err());
    let mut missing = request(true);
    missing.as_object_mut().unwrap().remove("expectedRevision");
    assert!(decode_native_request(serde_json::to_string(&missing).unwrap().as_bytes()).is_err());
}
#[test]
fn nested_or_decoy_vector_operation_never_masks_an_overbudget_ordinary_request() {
    let mut ordinary = request(false);
    ordinary["payload"]["decoy"] = json!({"operation":"command.document.object.vector.edit"});
    let raw = padded(&ordinary, 4097);
    assert!(decode_json_bounded::<Value>(raw.as_bytes(), MAX_MESSAGE_BYTES - 2).is_err());
    let ordinary = padded(&request(false), 4097);
    let root_decoy=format!("{{\"apiVersion\":2,\"operation\":\"command.document.object.vector.edit\",\"method\":\"tools/call\",\"params\":{{\"name\":\"nemo_command\",\"arguments\":{ordinary}}}}}");
    assert!(decode_json_bounded::<Value>(root_decoy.as_bytes(), MAX_MESSAGE_BYTES - 2).is_err());
    let tcp_decoy=format!("{{\"nativeRequest\":{},\"method\":\"tools/call\",\"params\":{{\"name\":\"nemo_command\",\"arguments\":{ordinary}}}}}",request(true));
    assert!(decode_json_bounded::<Value>(tcp_decoy.as_bytes(), MAX_MESSAGE_BYTES - 2).is_err());
    let escaped_arg = stdio(&ordinary).replacen("\"arguments\"", "\"argu\\u006dents\"", 1);
    assert!(decode_json_bounded::<Value>(escaped_arg.as_bytes(), MAX_MESSAGE_BYTES - 2).is_err());
    // A legacy/non-command frame is not mistaken for a top-level native request.
    let legacy = json!({"method":"other","params":{"operation":"command.document.object.vector.edit","data":"x".repeat(70000)}});
    assert!(decode_json_bounded::<Value>(
        serde_json::to_string(&legacy).unwrap().as_bytes(),
        MAX_MESSAGE_BYTES - 2
    )
    .is_ok());
}
#[tokio::test]
async fn actual_guarded_stdio_keeps_wrapper_budget_and_rejects_native_spans_or_duplicates_first() {
    for (vector, limit) in [(true, 65536), (false, 4096)] {
        let raw = stdio(&padded(&request(vector), limit)) + "\n";
        assert_eq!(guarded(raw.as_bytes().to_vec()).await, raw.as_bytes());
        let raw = stdio(&padded(&request(vector), limit + 1)) + "\n";
        assert_eq!(guarded(raw.into_bytes()).await, b"null\n");
    }
    let native = serde_json::to_string(&request(true)).unwrap();
    let duplicate = native.replacen("\"frame\":0", "\"frame\":0,\"frame\":0", 1);
    assert_ne!(duplicate, native);
    assert_eq!(
        guarded((stdio(&duplicate) + "\n").into_bytes()).await,
        b"null\n"
    );
    let frame = stdio(&native);
    let prefix = "\"id\":1";
    assert_eq!(frame.matches(prefix).count(), 1);
    let overhead = frame.len() - prefix.len() + "\"id\":\"\"".len();
    let at_limit = frame.replacen(
        prefix,
        &format!(
            "\"id\":\"{}\"",
            "x".repeat(MAX_MESSAGE_BYTES - 2 - overhead)
        ),
        1,
    );
    assert_eq!(at_limit.len(), MAX_MESSAGE_BYTES - 2);
    assert_eq!(
        guarded((at_limit.clone() + "\n").into_bytes()).await,
        (at_limit + "\n").as_bytes()
    );
    let over = frame.replacen(
        prefix,
        &format!(
            "\"id\":\"{}\"",
            "x".repeat(MAX_MESSAGE_BYTES - 1 - overhead)
        ),
        1,
    );
    assert_eq!(over.len(), MAX_MESSAGE_BYTES - 1);
    assert_eq!(guarded((over + "\n").into_bytes()).await, b"null\n");
    assert_eq!(
        guarded(b"{\n".to_vec()).await,
        b"{\n",
        "legacy malformed syntax stays on the existing SDK error path"
    );
}

#[test]
fn a_fill_brush_outline_gets_the_vector_geometry_bound_and_nothing_else_widens() {
    assert_eq!(super::request_limit("command.document.object.fill.brush"), super::MAX_REQUEST_BYTES);
    assert_eq!(super::request_limit("command.document.object.vector.edit"), super::MAX_REQUEST_BYTES);
    for ordinary in ["command.document.object.fill.trace", "command.document.object.boolean.apply"] {
        assert_eq!(super::request_limit(ordinary), crate::contract::NATIVE_MAX_MESSAGE_BYTES, "{ordinary}");
    }
}
