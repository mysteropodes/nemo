//! Real original-byte, authenticated-wire and common receipt consumer controls.
use super::*;
use crate::contract::{NativeApplicationRequest, NativeApplicationResponse};
use crate::registry::Endpoint;
use crate::wire::{
    call_native, decode_native_request, guarded_stdio_input, validate_native_response,
    CancellationToken,
};
use serde_json::json;
use tokio::io::AsyncReadExt;
const COPY: &str = "command.document.object.frames.clipboard.copy";
const CUT: &str = "command.document.object.frames.clipboard.cut";
const PASTE: &str = "command.document.object.frames.clipboard.paste";
fn request(op: &str) -> NativeApplicationRequest {
    let command = op.strip_prefix("command.document.").unwrap();
    serde_json::from_value(json!({"apiVersion":2,"requestId":"clipboard-budget",
        "instanceId":"instance-A","documentId":"document-A","expectedRevision":7,
        "operation":op,"payload": if op == PASTE {
            json!({"command":command,"instanceId":"instance-A","documentId":"document-A",
                "layerUid":"L0","frame":0,"cells":[]})
        } else { json!({"command":command,"selection":[]}) }}))
    .unwrap()
}
fn response(q: &NativeApplicationRequest) -> NativeApplicationResponse {
    serde_json::from_value(
        json!({"apiVersion":2,"requestId":q.request_id,"instanceId":q.instance_id,
        "documentId":q.document_id,"contentRevision":7,"ok":true,
        "result":{"applied":false,"historyEntriesAdded":0,"clipboard":{
            "instanceId":q.instance_id,"documentId":q.document_id,"cells":[{
                "relLayer":0,"relFrame":0,"keyframe":true,"records":[{"padding":"x".repeat(8192)}]
            }]}}}),
    )
    .unwrap()
}
fn padded(q: &NativeApplicationRequest, length: usize) -> Vec<u8> {
    let mut raw = serde_json::to_vec(q).unwrap();
    assert!(raw.len() <= length);
    raw.resize(length, b' ');
    raw
}
#[test]
fn exact_full_original_request_frontiers_preserve_old_4096_and_unrelated_4097() {
    assert_eq!(MESSAGE_BYTES, 5_246_976);
    for op in [COPY, CUT, PASTE] {
        let q = request(op);
        for length in [4096, 4097, MESSAGE_BYTES - 1, MESSAGE_BYTES] {
            assert!(
                decode_native_request(&padded(&q, length)).is_ok(),
                "{op} {length}"
            );
        }
        assert!(decode_native_request(&padded(&q, MESSAGE_BYTES + 1)).is_err());
    }
    let mut q = request(COPY);
    q.operation = "command.document.object.frames.repeat".into();
    q.payload =
        json!({"command":"object.frames.repeat","layerUids":["L0"],"from":0,"to":0,"times":2});
    // Size check precedes any expectation about availability of this unrelated DTO.
    assert_eq!(crate::wire::native_request_byte_limit(&q.operation), 4096);
    assert!(decode_native_request(&padded(&q, 4097)).is_err());
    for op in [
        "command.document.object.layer.key.blank",
        "command.document.object.layer.key.move",
        "command.document.object.frames.clipboard.duplicate",
        "query.document.object",
    ] {
        assert_eq!(crate::wire::native_request_byte_limit(op), 4096);
        assert_eq!(native_response_byte_limit(op), 4096);
    }
    assert_eq!(native_response_byte_limit(PASTE), 4096);
}
#[test]
fn actual_typed_request_lengths_and_data_budget_are_independent() {
    let mut q = request(COPY);
    q.payload["selection"] = json!([{"layerUid":"","frame":0}]);
    let base = serde_json::to_vec(&q).unwrap().len();
    q.payload["selection"][0]["layerUid"] = json!("x".repeat(MESSAGE_BYTES - base));
    assert_eq!(serde_json::to_vec(&q).unwrap().len(), MESSAGE_BYTES);
    assert!(q.validate_input().is_ok());
    q.payload["selection"][0]["layerUid"].as_str().unwrap();
    q.payload["selection"][0]["layerUid"] = json!(format!(
        "{}x",
        q.payload["selection"][0]["layerUid"].as_str().unwrap()
    ));
    assert!(q.validate_input().is_err());
    let mut cells = json!([{"records":[{"padding":""}],"orderElements":[]}]);
    let overhead = serde_json::to_vec(&cells[0]["records"][0]).unwrap().len() + 2;
    cells[0]["records"][0]["padding"] = json!("x".repeat(DATA_BYTES - overhead));
    assert!(data_admitted(&cells));
    cells[0]["records"][0]["padding"] = json!("x".repeat(DATA_BYTES - overhead + 1));
    assert!(!data_admitted(&cells));
    // #2248: a cell's raster images and text blocks count against the same data bound.
    let mut items = json!([{"records":[],"textItems":[{"padding":""}]}]);
    let overhead = serde_json::to_vec(&items[0]["textItems"][0]).unwrap().len();
    items[0]["textItems"][0]["padding"] = json!("x".repeat(DATA_BYTES - overhead));
    assert!(data_admitted(&items));
    items[0]["textItems"][0]["padding"] = json!("x".repeat(DATA_BYTES - overhead + 1));
    assert!(!data_admitted(&items));
}
#[test]
fn larger_frames_never_mask_recursive_duplicate_malformed_depth_or_source_identity() {
    let q = request(COPY);
    let raw = String::from_utf8(padded(&q, 8192)).unwrap();
    assert!(decode_native_request(
        raw.replace("\"selection\":[]", "\"selection\":[],\"selection\":[]")
            .as_bytes()
    )
    .is_err());
    assert!(decode_native_request(format!("{raw}{{").as_bytes()).is_err());
    assert!(decode_native_request(
        format!("{}null{}", "[".repeat(200), "]".repeat(200)).as_bytes()
    )
    .is_err());
    let good = response(&q);
    assert!(validate_native_response(&good, &q).is_ok());
    for field in ["instanceId", "documentId"] {
        let mut bad = good.clone();
        bad.result.as_mut().unwrap()["clipboard"][field] = json!("foreign");
        assert!(validate_native_response(&bad, &q).is_err());
    }
    let mut bad = good.clone();
    bad.request_id = "different".into();
    assert!(validate_native_response(&bad, &q).is_err());
}
#[tokio::test]
async fn exact_full_encoded_response_reader_counts_whitespace_and_never_truncates() {
    let q = request(COPY);
    let good = response(&q);
    for length in [4096 + 8192, MESSAGE_BYTES - 1, MESSAGE_BYTES] {
        let mut raw = serde_json::to_vec(&good).unwrap();
        raw.resize(length, b' ');
        raw.push(b'\n');
        let decoded: NativeApplicationResponse =
            super::super::read_json_bounded(raw.as_slice(), MESSAGE_BYTES)
                .await
                .unwrap();
        assert_eq!(decoded, good);
        validate_native_response(&decoded, &q).unwrap();
    }
    let mut raw = serde_json::to_vec(&good).unwrap();
    raw.resize(MESSAGE_BYTES + 1, b' ');
    raw.push(b'\n');
    assert!(
        super::super::read_json_bounded::<NativeApplicationResponse>(raw.as_slice(), MESSAGE_BYTES)
            .await
            .is_err()
    );
}
#[tokio::test]
async fn actual_authenticated_tcp_carries_request_and_reply_above_legacy_one_mib() {
    let mut q = request(COPY);
    q.payload["selection"] = json!([{"layerUid":"x".repeat(1_100_000),"frame":0}]);
    let mut good = response(&q);
    good.result.as_mut().unwrap()["clipboard"]["cells"][0]["records"][0]["padding"] =
        json!("x".repeat(1_100_000));
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let endpoint = Endpoint {
        instance_id: q.instance_id.clone(),
        port: listener.local_addr().unwrap().port(),
        secret: "clipboard-test".into(),
        build_id: "test".into(),
    };
    let server = async {
        let (mut stream, _) = listener.accept().await.unwrap();
        let message = crate::wire::read_authenticated(&mut stream).await.unwrap();
        let AuthenticatedWireRequest::Native(message) = message else {
            panic!("native only")
        };
        assert_eq!(message.secret, endpoint.secret);
        assert_eq!(message.native_request, q);
        crate::wire::write_native_response(&mut stream, &good, &message.native_request)
            .await
            .unwrap();
    };
    let client = call_native(&endpoint, q.clone(), CancellationToken::new());
    let (_, actual) = tokio::time::timeout(std::time::Duration::from_secs(10), async {
        tokio::join!(server, client)
    })
    .await
    .unwrap();
    assert_eq!(actual.unwrap(), good);
}
#[tokio::test]
async fn actual_stdio_guard_admits_only_real_nemo_command_clipboard_path_and_preserves_next_frame()
{
    let mut q = request(COPY);
    q.payload["selection"] = json!([{"layerUid":"x".repeat(1_100_000),"frame":0}]);
    let frame = json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"nemo_command","arguments":q}});
    for (good, frame) in [
        (true, frame.clone()),
        (false, {
            let mut x = frame;
            x["params"]["name"] = json!("other");
            x
        }),
    ] {
        let raw = format!(
            "{}\n{{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}}\n",
            frame
        );
        let mut reader = guarded_stdio_input(std::io::Cursor::new(raw.into_bytes()));
        let mut output = String::new();
        reader.read_to_string(&mut output).await.unwrap();
        let first = output.lines().next().unwrap();
        assert_eq!(first == "null", !good);
        assert!(output.lines().last().unwrap().contains("ping"));
    }
}

#[test]
fn record_arrays_above_4096_pair_with_native_data_bound_without_lifting_cell_count() {
    let source: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let records: Vec<Value> = (0..4100)
        .map(|index| {
            let mut record = source["records"][0].clone();
            record["target"]["strokeId"] = json!(format!("record-{index}"));
            record
        })
        .collect();
    let mut q = request(PASTE);
    q.payload["cells"] = json!([{"relLayer":0,"relFrame":0,"keyframe":true,"records":records}]);
    assert!(q.validate_input().is_ok());
    assert!(serde_json::to_vec(&q).unwrap().len() > 4096);
    let cell = json!({"relLayer":0,"relFrame":0,"keyframe":false,"records":[]});
    q.payload["cells"] = json!(vec![cell; 4097]);
    assert!(q.validate_input().is_err());
}
#[tokio::test]
async fn large_authenticated_headers_are_inclusive_bounded_and_cannot_mask_other_operations() {
    let mut q = request(COPY);
    q.payload["selection"] = json!([{"layerUid":"x".repeat(1_100_000),"frame":0}]);
    let inner = serde_json::to_vec(&q).unwrap().len();
    let mut frame = NativeWireRequest {
        secret: String::new(),
        native_request: q,
    };
    let fixed = serde_json::to_vec(&frame).unwrap().len() - inner;
    frame.secret = "s".repeat(WRAPPER_BYTES - fixed);
    let mut bytes = serde_json::to_vec(&frame).unwrap();
    bytes.push(b'\n');
    assert!(crate::wire::read_authenticated(bytes.as_slice())
        .await
        .is_ok());
    frame.secret.push('s');
    let mut bytes = serde_json::to_vec(&frame).unwrap();
    bytes.push(b'\n');
    assert!(crate::wire::read_authenticated(bytes.as_slice())
        .await
        .is_err());
    frame.native_request.operation = "query.document.revision".into();
    let mut bytes = serde_json::to_vec(&frame).unwrap();
    bytes.push(b'\n');
    assert!(crate::wire::read_authenticated(bytes.as_slice())
        .await
        .is_err());
}
#[tokio::test]
async fn unrelated_stdio_bom_and_cr_still_count_against_original_outer_limit() {
    let mut raw = serde_json::to_vec(&json!({"jsonrpc":"2.0","id":1,"method":"ping"})).unwrap();
    raw.resize(crate::contract::MAX_MESSAGE_BYTES - 2, b' ');
    let mut at = raw.clone();
    at.push(b'\n');
    let mut reader = guarded_stdio_input(std::io::Cursor::new(at));
    let mut output = String::new();
    reader.read_to_string(&mut output).await.unwrap();
    assert_ne!(output, "null\n");
    raw.insert(0, 0xbf);
    raw.insert(0, 0xbb);
    raw.insert(0, 0xef);
    raw.extend_from_slice(b"\r\n");
    let mut reader = guarded_stdio_input(std::io::Cursor::new(raw));
    let mut output = String::new();
    reader.read_to_string(&mut output).await.unwrap();
    assert_eq!(output, "null\n");
}
