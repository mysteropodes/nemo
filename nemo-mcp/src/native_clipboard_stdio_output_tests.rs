//! F12-E1: actual locked SDK constructor -> existing encoder and durable output owner.
//! No alternate DTO, output cap, retry or mutation; all original input bounds remain.
use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use crate::wire::{native_clipboard_transport as budget, validate_native_response};
use rmcp::model::{CallToolResult, RequestId, ServerResult};
use serde_json::{json, Value};
use tokio::io::AsyncBufReadExt;

// Actual rmcp3.2 structured response: native V plus JSON string(V.to_string()),
// encoded outer ID, and132 fixed JSONL bytes (131 for isError:true).
// Encoded text <=2*V_bytes+2: V is valid compact JSON with no raw controls.
// Legacy ingress can carry ID up to1048574 bytes; larger clipboard ingress
// requires the complete non-native wrapper (including ID) <=4096 instead.
const STRUCTURED_FIXED_JSONL_BYTES: usize = 132;
const OUTER_ID_BYTES_BOUND: usize = crate::contract::MAX_MESSAGE_BYTES - 2;
const MCP_CLIPBOARD_STDOUT_BYTES_BOUND: usize =
    3 * budget::MESSAGE_BYTES + OUTER_ID_BYTES_BOUND + STRUCTURED_FIXED_JSONL_BYTES + 2;

fn request(operation: &str) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":"stdio-copy",
        "instanceId":"instance-A","documentId":"document-A","expectedRevision":0,
        "operation":operation,"payload":{"command":operation.strip_prefix("command.document.").unwrap(),
        "selection":[]}})).unwrap()
}
fn native_reply(q: &NativeApplicationRequest, error: bool) -> NativeApplicationResponse {
    serde_json::from_value(if error {
        json!({"apiVersion":2,"requestId":q.request_id,"instanceId":q.instance_id,
            "documentId":q.document_id,"contentRevision":0,"ok":false,
            "error":{"code":"invalid_request","message":"x"}})
    } else {
        json!({"apiVersion":2,"requestId":q.request_id,"instanceId":q.instance_id,
            "documentId":q.document_id,"contentRevision":0,"ok":true,
            "result":{"applied":false,"historyEntriesAdded":0,"clipboard":{
                "instanceId":q.instance_id,"documentId":q.document_id,"cells":[]}}})
    })
    .unwrap()
}
async fn emitted(value: Value, error: bool, id: RequestId) -> Vec<u8> {
    // These are exactly server.rs call_native's success/error constructors.
    let result = if error {
        CallToolResult::structured_error(value)
    } else {
        CallToolResult::structured(value)
    };
    let message =
        TxJsonRpcMessage::<RoleServer>::response(ServerResult::CallToolResult(result), id);
    let (writer, reader) = tokio::io::duplex(4096);
    let mut transport = super::new(tokio::io::empty(), writer);
    let send = transport.send(message);
    let read = async {
        let mut line = Vec::new();
        BufReader::new(reader)
            .read_until(b'\n', &mut line)
            .await
            .unwrap();
        line
    };
    let (sent, line) = tokio::join!(send, read);
    sent.unwrap();
    line
}
fn assert_complete(line: &[u8], value: &Value, error: bool, id: &RequestId) {
    assert_eq!(line.last(), Some(&b'\n'));
    assert_eq!(line.iter().filter(|b| **b == b'\n').count(), 1);
    let outer: Value = serde_json::from_slice(line).unwrap();
    assert_eq!(outer["id"], serde_json::to_value(id).unwrap());
    assert_eq!(outer["result"]["structuredContent"], *value);
    assert_eq!(outer["result"]["content"].as_array().unwrap().len(), 1);
    assert_eq!(outer["result"]["content"][0]["text"], value.to_string());
    assert_eq!(outer["result"]["isError"], error);
    let native_bytes = serde_json::to_vec(value).unwrap().len();
    let escaped_text_bytes = serde_json::to_vec(&value.to_string()).unwrap().len();
    let id_bytes = serde_json::to_vec(id).unwrap().len();
    assert!(escaped_text_bytes <= 2 * native_bytes + 2);
    assert!(id_bytes <= OUTER_ID_BYTES_BOUND);
    // Pin the real codec's entire frame, including SDK fields and LF, against
    // separate exact components. Changing SDK serialization must fail this test.
    assert_eq!(
        line.len(),
        native_bytes + escaped_text_bytes + id_bytes + STRUCTURED_FIXED_JSONL_BYTES
            - usize::from(error)
    );
    assert!(line.len() <= MCP_CLIPBOARD_STDOUT_BYTES_BOUND);
}
#[tokio::test]
async fn actual_structured_large_native_shaped_copy_cut_keep_both_complete_fields() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let records: Vec<Value> = (0..4100)
        .map(|index| {
            let mut record = cases["records"][0].clone();
            record["target"]["strokeId"] = json!(format!("opaque:\"\\雪-{index}"));
            record
        })
        .collect();
    for operation in [
        "command.document.object.frames.clipboard.copy",
        "command.document.object.frames.clipboard.cut",
    ] {
        let q = request(operation);
        let mut response = native_reply(&q, false);
        response.result.as_mut().unwrap()["clipboard"]["cells"] = json!([
            {"relLayer":0,"relFrame":0,"keyframe":true,"records":records}]);
        validate_native_response(&response, &q).unwrap();
        let value = serde_json::to_value(response).unwrap();
        let id = RequestId::String("outer:\"\\雪".into());
        let line = emitted(value.clone(), false, id.clone()).await;
        assert!(
            line.len() > budget::FRAME_BYTES,
            "actual structured stdout exceeds the ingress/TCP frame budget"
        );
        assert_complete(&line, &value, false, &id);
    }
}
#[tokio::test]
async fn actual_legacy_ingress_large_outer_id_is_retained_in_structured_output() {
    let q = request("command.document.object.frames.clipboard.copy");
    let mut raw = json!({"jsonrpc":"2.0","id":"","method":"tools/call",
        "params":{"name":"nemo_command","arguments":q}});
    let empty_size = serde_json::to_vec(&raw).unwrap().len();
    raw["id"] = json!("i".repeat(OUTER_ID_BYTES_BOUND - empty_size));
    let mut bytes = serde_json::to_vec(&raw).unwrap();
    assert_eq!(bytes.len(), OUTER_ID_BYTES_BOUND);
    bytes.push(b'\n');
    let mut transport = super::new(std::io::Cursor::new(bytes), tokio::io::sink());
    let parsed = transport.receive().await.unwrap();
    let parsed = serde_json::to_value(parsed).unwrap();
    assert_eq!(
        parsed["id"], raw["id"],
        "actual SDK receive retains the admitted ID"
    );
    let id: RequestId = serde_json::from_value(parsed["id"].clone()).unwrap();
    // Full canonical native reply frontier, with escape-heavy but valid error
    // messages, approaches the separately derived output bound. Native success
    // packet/data semantics are independently exercised above and by R1 tests.
    for size in [budget::MESSAGE_BYTES - 1, budget::MESSAGE_BYTES] {
        let mut response = native_reply(&q, true);
        response.error.as_mut().unwrap().message = String::new();
        let base = serde_json::to_vec(&response).unwrap().len();
        let remaining = size - base;
        response.error.as_mut().unwrap().message =
            "\\".repeat(remaining / 2) + &"x".repeat(remaining % 2);
        assert_eq!(serde_json::to_vec(&response).unwrap().len(), size);
        validate_native_response(&response, &q).unwrap();
        let value = serde_json::to_value(response).unwrap();
        let line = emitted(value.clone(), true, id.clone()).await;
        assert_complete(&line, &value, true, &id);
        assert!(MCP_CLIPBOARD_STDOUT_BYTES_BOUND - line.len() < 2048);
    }
    assert_eq!(MCP_CLIPBOARD_STDOUT_BYTES_BOUND, 16_789_636);
}
