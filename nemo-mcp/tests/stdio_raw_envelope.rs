//! Raw protocol bytes through the compiled executable, before SDK Value parsing.
use nemo_mcp::{
    contract::{ApplicationResponse, MAX_MESSAGE_BYTES},
    registry::{Endpoint, Registration},
    wire,
};
use rmcp::model::{ClientCapabilities, Implementation, InitializeRequestParams};
use serde_json::{json, Value};
use std::{
    process::Stdio,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::TcpListener,
    process::{Child, ChildStdin, ChildStdout},
};

struct RawClient {
    _child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    sent_frames: usize,
}
impl RawClient {
    fn start(root: &std::path::Path) -> Self {
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"))
            .env("NEMO_MCP_REGISTRY", root)
            .env("NEMO_MCP_TRACE", "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .unwrap();
        Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            _child: child,
            sent_frames: 0,
        }
    }
    async fn send(&mut self, raw: &[u8]) {
        self.sent_frames += 1;
        tokio::time::timeout(Duration::from_secs(5), self.input.write_all(raw))
            .await
            .unwrap_or_else(|_| panic!("stdio write timed out at frame {}", self.sent_frames))
            .unwrap();
        self.input.flush().await.unwrap();
    }
    async fn receive(&mut self) -> Value {
        let mut line = String::new();
        let count = tokio::time::timeout(Duration::from_secs(5), self.output.read_line(&mut line))
            .await
            .unwrap_or_else(|_| panic!("stdio response timed out after frame {}", self.sent_frames))
            .unwrap();
        assert!(count > 0, "compiled stdio unexpectedly closed");
        serde_json::from_str(&line).unwrap()
    }
    async fn initialize(&mut self) {
        let params = InitializeRequestParams::new(
            ClientCapabilities::default(),
            Implementation::new("raw-fixture", "1"),
        );
        self.send(
            format!(
                "{}\n",
                json!({"jsonrpc":"2.0","id":0,"method":"initialize","params":params})
            )
            .as_bytes(),
        )
        .await;
        assert_eq!(self.receive().await["id"], 0);
        self.send(b"{\"jsonrpc\":\"2.0\",\"method\":\"notifications/initialized\"}\n")
            .await;
    }
    async fn ping(&mut self, id: u64) {
        self.send(format!("{}\n", json!({"jsonrpc":"2.0","id":id,"method":"ping"})).as_bytes())
            .await;
        let response = self.receive().await;
        assert_eq!(
            response["id"], id,
            "unexpected extra error or lost recovery: {response}"
        );
        assert_eq!(response["result"], json!({}));
    }
    async fn finish_and_stderr(self) -> String {
        let Self {
            _child: mut child,
            input,
            output: _,
            ..
        } = self;
        // ChildStdin shutdown alone does not close its pipe; EOF needs its drop.
        drop(input);
        tokio::time::timeout(Duration::from_secs(5), child.wait())
            .await
            .unwrap()
            .unwrap();
        let mut stderr = String::new();
        child
            .stderr
            .take()
            .unwrap()
            .read_to_string(&mut stderr)
            .await
            .unwrap();
        stderr
    }
}

fn assert_invalid(response: &Value) {
    assert_eq!(response["jsonrpc"], "2.0");
    assert!(
        response.get("id").is_none(),
        "SDK omits uncorrelatable id: {response}"
    );
    assert_eq!(response["error"]["code"], -32600);
    assert_eq!(response["error"]["message"], "Invalid request");
    assert!(response.get("result").is_none());
}

#[tokio::test]
async fn compiled_nested_duplicate_is_protocol_invalid_before_tool_dispatch() {
    let root = tempfile::tempdir().unwrap();
    let mut client = RawClient::start(root.path());
    client.initialize().await;
    let raw = br#"{"jsonrpc":"2.0","id":7,"method":"tools/call","params":{"name":"nemo_command","arguments":{"apiVersion":2,"requestId":"raw-duplicate","instanceId":"instance","documentId":"document","operation":"query.document.object","payload":{"atRevision":0,"stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},"layerUid":"legacy layer:alpha","strokeId":"ink/path_A","strokeId":"ink/path_A"}}}}}"#;
    let collapsed: Value = serde_json::from_slice(raw).unwrap();
    assert_eq!(
        collapsed["params"]["arguments"]["payload"]["stableTarget"]["strokeId"],
        "ink/path_A"
    );
    let mut frame = raw.to_vec();
    frame.push(b'\n');
    client.send(&frame).await;
    let response = client.receive().await;
    assert_eq!(
        response["error"]["code"], -32600,
        "raw duplicate reached tool dispatch: {response}"
    );
    assert_invalid(&response);
    client.ping(8).await;
}

#[tokio::test]
async fn invalid_frames_before_initialize_recover_without_a_correlated_response() {
    let root = tempfile::tempdir().unwrap();
    let mut client = RawClient::start(root.path());
    client
        .send(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"id\":1,\"method\":\"initialize\"}\n")
        .await;
    assert_invalid(&client.receive().await);
    let mut oversized = vec![b' '; MAX_MESSAGE_BYTES];
    oversized.extend_from_slice(b"{}\n");
    client.send(&oversized).await;
    assert_invalid(&client.receive().await);
    client.initialize().await;
    client.ping(9).await;
}

#[tokio::test]
async fn rejected_input_never_echoes_or_logs_tainted_identity_body_or_secret() {
    let root = tempfile::tempdir().unwrap();
    let mut client = RawClient::start(root.path());
    client.initialize().await;
    let raw = b"{\"jsonrpc\":\"2.0\",\"id\":\"PRIVATE-ID-SENTINEL\",\"id\":\"PRIVATE-ID-SENTINEL\",\"method\":\"tools/call\",\"params\":{\"secret\":\"SECRET-SENTINEL\",\"body\":\"PRIVATE-BODY-SENTINEL\"}}\n";
    client.send(raw).await;
    let response = client.receive().await;
    assert_invalid(&response);
    client.ping(10).await;
    let stderr = client.finish_and_stderr().await;
    for marker in [
        "PRIVATE-ID-SENTINEL",
        "SECRET-SENTINEL",
        "PRIVATE-BODY-SENTINEL",
    ] {
        assert!(!response.to_string().contains(marker));
        assert!(!stderr.contains(marker), "tainted input entered stderr");
    }
}

#[tokio::test]
async fn original_stdio_byte_budget_accepts_exact_limit_and_rejects_one_more_byte() {
    let root = tempfile::tempdir().unwrap();
    let mut client = RawClient::start(root.path());
    client.initialize().await;
    let ping = b"{\"jsonrpc\":\"2.0\",\"id\":11,\"method\":\"ping\"}\n";
    let mut exact = vec![b' '; MAX_MESSAGE_BYTES - 1 - ping.len()];
    exact.extend_from_slice(ping);
    assert_eq!(exact.len(), MAX_MESSAGE_BYTES - 1);
    client.send(&exact).await;
    assert_eq!(client.receive().await["id"], 11);
    exact.insert(0, b' ');
    client.send(&exact).await;
    assert_invalid(&client.receive().await);
    client.ping(12).await;
}

#[tokio::test]
async fn malformed_duplicates_keep_sdk_syntax_handling_and_valid_bom_crlf_bytes_work() {
    let root = tempfile::tempdir().unwrap();
    let mut client = RawClient::start(root.path());
    client.initialize().await;
    let deep = format!(
        "{{\"a\":1,\"a\":2,\"deep\":{}0{}}}\n",
        "[".repeat(150),
        "]".repeat(150)
    );
    for (index, raw) in [
        "{\"jsonrpc\":\"2.0\",\"id\":19,\"method\":\"ping\",\"params\":{\"a\":1,\"a\":2,}}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":19,\"method\":\"ping\",\"params\":{\"a\":1,\"a\":2}} {}\n",
        "{\"jsonrpc\":\"2.0\",\"id\":19,\"method\":\"ping\",\"params\":{\"a\":1,\"a\":1e400}}\n",
        &deep,
    ]
    .into_iter()
    .enumerate()
    {
        client.send(raw.as_bytes()).await;
        client.ping(20 + index as u64).await;
    }
    client
        .send(
            "\n\r\n\u{feff}{\"jsonrpc\":\"2.0\",\"id\":\"🟠\",\"method\":\"ping\"}\r\n".as_bytes(),
        )
        .await;
    assert_eq!(client.receive().await["id"], "🟠");
    client.ping(24).await;
}

struct EndpointTask(tokio::task::JoinHandle<()>);
impl Drop for EndpointTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[tokio::test]
async fn rejected_raw_frames_never_reach_a_registered_endpoint_and_valid_legacy_does() {
    let root = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: "c19b3fa5-4f92-4afc-90df-fb9a2dbe9770".into(),
        port: listener.local_addr().unwrap().port(),
        secret: "135c2302-f4c7-4619-844a-d5a7017d1a9a".into(),
        build_id: "raw-fixture".into(),
    };
    let _registration = Registration::create(root.path(), &endpoint).unwrap();
    let observed = Arc::new(AtomicUsize::new(0));
    let accepted = observed.clone();
    let _task = EndpointTask(tokio::spawn(async move {
        loop {
            let (mut stream, _) = listener.accept().await.unwrap();
            accepted.fetch_add(1, Ordering::SeqCst);
            let message: wire::WireRequest = wire::read_json(&mut stream).await.unwrap();
            wire::write_json(
                &mut stream,
                &ApplicationResponse {
                    api_version: 1,
                    request_id: message.request.request_id,
                    instance_id: "c19b3fa5-4f92-4afc-90df-fb9a2dbe9770".into(),
                    document_id: "raw-document".into(),
                    revision: 0,
                    ok: true,
                    result: Some(json!({"ownerReached":true})),
                    error: None,
                },
            )
            .await
            .unwrap();
        }
    }));
    let mut client = RawClient::start(root.path());
    client.initialize().await;
    let control = json!({"jsonrpc":"2.0","id":30,"method":"tools/call","params":{"name":"nemo_command",
        "arguments":{"apiVersion":1,"requestId":"positive-raw","instanceId":"c19b3fa5-4f92-4afc-90df-fb9a2dbe9770","documentId":"raw-document",
        "expectedRevision":0,"operation":"property.set","payload":{"layerId":"layer","property":"opacity","value":25}}}});
    let encoded = serde_json::to_string(&control).unwrap();
    for (index, fragment) in [
        "\"id\":30",
        "\"method\":\"tools/call\"",
        "\"name\":\"nemo_command\"",
        "\"requestId\":\"positive-raw\"",
        "\"instanceId\":\"c19b3fa5-4f92-4afc-90df-fb9a2dbe9770\"",
        "\"documentId\":\"raw-document\"",
        "\"value\":25",
        "\"layerId\":\"layer\"",
    ]
    .into_iter()
    .enumerate()
    {
        assert!(encoded.contains(fragment));
        let duplicate = format!(
            "{}\n",
            encoded.replacen(fragment, &format!("{fragment},{fragment}"), 1)
        );
        client.send(duplicate.as_bytes()).await;
        assert_invalid(&client.receive().await);
        client.ping(100 + index as u64).await;
        assert_eq!(
            observed.load(Ordering::SeqCst),
            0,
            "{fragment} reached endpoint"
        );
    }
    let escaped = format!(
        "{}\n",
        encoded.replace("\"value\":25", "\"value\":25,\"\\u0076alue\":25")
    );
    client.send(escaped.as_bytes()).await;
    assert_invalid(&client.receive().await);
    assert_eq!(observed.load(Ordering::SeqCst), 0);
    let mut oversized = vec![b' '; MAX_MESSAGE_BYTES];
    oversized.extend_from_slice(encoded.as_bytes());
    oversized.push(b'\n');
    client.send(&oversized).await;
    assert_invalid(&client.receive().await);
    client.ping(32).await;
    assert_eq!(observed.load(Ordering::SeqCst), 0);
    client.send(format!("{encoded}\n").as_bytes()).await;
    let response = client.receive().await;
    assert_eq!(response["id"], 30);
    assert_eq!(
        response["result"]["structuredContent"]["result"]["ownerReached"], true,
        "positive endpoint control: {response}"
    );
    assert_eq!(observed.load(Ordering::SeqCst), 1);
}
