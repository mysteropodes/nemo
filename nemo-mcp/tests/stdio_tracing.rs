//! Opt-in command tracing stays on bounded stderr and omits request secrets.
use nemo_mcp::{
    contract::{ApplicationResponse, Operation},
    registry::{Endpoint, Registration},
    wire,
};
use rmcp::{
    model::{CallToolRequest, CallToolRequestParams, ClientRequest},
    service::PeerRequestOptions,
    transport::TokioChildProcess,
    ServiceExt,
};
use serde_json::{json, Value};
use std::{future::Future, path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    net::TcpListener,
    sync::oneshot,
    task::JoinHandle,
};

const MAX_TRACE_EVENT_BYTES: usize = 512;
const MAX_TRACE_TOTAL_BYTES: usize = 4096;
const SECRET_SENTINEL: &str = "SECRET-SENTINEL-12345678901234567890";
const PRIVATE_PATH_SENTINEL: &str = "PRIVATE_PATH_SENTINEL";

async fn bounded<T>(future: impl Future<Output = T>) -> T {
    tokio::time::timeout(Duration::from_secs(5), future)
        .await
        .expect("MCP tracing fixture timed out")
}

struct EndpointTask(JoinHandle<()>);
impl Drop for EndpointTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

async fn reply(listener: &TcpListener, instance_id: &str, response_ok: bool) -> wire::WireRequest {
    let (mut stream, _) = bounded(listener.accept()).await.unwrap();
    let request: wire::WireRequest = bounded(wire::read_json(&mut stream)).await.unwrap();
    assert_eq!(request.secret, SECRET_SENTINEL);
    let response = ApplicationResponse {
        api_version: 1,
        request_id: request.request.request_id.clone(),
        instance_id: instance_id.to_owned(),
        document_id: "document".into(),
        revision: 2,
        ok: response_ok,
        result: response_ok.then(|| json!({"revision": 2})),
        error: (!response_ok).then(|| nemo_mcp::contract::ApplicationError {
            code: "fixture_error".into(),
            message: PRIVATE_PATH_SENTINEL.into(),
        }),
    };
    bounded(wire::write_json(&mut stream, &response))
        .await
        .unwrap();
    request
}

fn command(request_id: &str, instance_id: &str) -> CallToolRequestParams {
    CallToolRequestParams::new("nemo_command").with_arguments(
        json!({
            "apiVersion": 1,
            "requestId": request_id,
            "instanceId": instance_id,
            "documentId": "document",
            "expectedRevision": 1,
            "operation": "property.set",
            "payload": {"layerId": "layer", "property": "opacity", "value": 25}
        })
        .as_object()
        .unwrap()
        .clone(),
    )
}

fn assert_trace_pair(logs: &str, application_id: &str, terminal_status: &str) {
    let marker = format!("application_request_id={application_id}}}");
    let events: Vec<_> = logs.lines().filter(|line| line.contains(&marker)).collect();
    assert_eq!(
        events.len(),
        2,
        "expected one start and terminal: {events:?}"
    );
    let mcp_ids: Vec<_> = events
        .iter()
        .map(|line| {
            line.split_once("mcp_request_id=")
                .unwrap()
                .1
                .split_whitespace()
                .next()
                .unwrap()
        })
        .collect();
    assert_eq!(mcp_ids[0], mcp_ids[1], "MCP identity changed: {events:?}");
    assert_eq!(
        events
            .iter()
            .filter(|line| line.contains("event=\"started\""))
            .count(),
        1,
        "missing or duplicate start: {events:?}"
    );
    assert_eq!(
        events
            .iter()
            .filter(|line| {
                line.contains(&format!("event=\"terminal\" status=\"{terminal_status}\""))
            })
            .count(),
        1,
        "missing or misclassified terminal: {events:?}"
    );
}

async fn call(
    client: &rmcp::service::RunningService<rmcp::RoleClient, ()>,
    args: CallToolRequestParams,
) -> Value {
    let result = bounded(client.call_tool(args)).await.unwrap();
    result.structured_content.unwrap()
}

#[tokio::test]
async fn compiled_stdio_tracing_correlates_outcomes_without_payload_or_path_leaks() {
    let root = tempfile::Builder::new()
        .prefix(PRIVATE_PATH_SENTINEL)
        .tempdir()
        .unwrap();
    assert!(root
        .path()
        .to_string_lossy()
        .contains(PRIVATE_PATH_SENTINEL));

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: uuid::Uuid::new_v4().to_string(),
        port: listener.local_addr().unwrap().port(),
        secret: SECRET_SENTINEL.into(),
        build_id: "tracing-fixture".into(),
    };
    let _registration = Registration::create(root.path(), &endpoint).unwrap();
    let instance_id = endpoint.instance_id.clone();
    let server_instance = instance_id.clone();
    let (cancel_received_tx, cancel_received_rx) = oneshot::channel();
    let mut application = EndpointTask(tokio::spawn(async move {
        let success = reply(&listener, &server_instance, true).await;
        assert_eq!(success.request.operation, Operation::PropertySet);
        let structured_error = reply(&listener, &server_instance, false).await;
        assert_eq!(structured_error.request.operation, Operation::PropertySet);

        let (mut stream, _) = bounded(listener.accept()).await.unwrap();
        let transport_error: wire::WireRequest =
            bounded(wire::read_json(&mut stream)).await.unwrap();
        assert_eq!(transport_error.secret, SECRET_SENTINEL);
        drop(stream);

        let (mut stream, _) = bounded(listener.accept()).await.unwrap();
        let cancellation: wire::WireRequest = bounded(wire::read_json(&mut stream)).await.unwrap();
        assert_eq!(cancellation.secret, SECRET_SENTINEL);
        cancel_received_tx.send(()).unwrap();
        assert_eq!(bounded(stream.read(&mut [0u8; 1])).await.unwrap(), 0);

        for _ in 0..20 {
            let filled = reply(&listener, &server_instance, true).await;
            assert_eq!(filled.request.operation, Operation::PropertySet);
        }
    }));

    let mut process = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
    process
        .env("NEMO_MCP_REGISTRY", root.path())
        .env("NEMO_MCP_TRACE", "1")
        .kill_on_drop(true);
    let (transport, mut stderr) = TokioChildProcess::builder(process)
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let client = bounded(().serve(transport)).await.unwrap();

    let success_id = "opacity-command-1";
    let success = call(&client, command(success_id, &instance_id)).await;
    assert_eq!(success["requestId"], success_id);
    assert_eq!(success["ok"], true);

    let error_id = "structured-error-1";
    let structured_error = call(&client, command(error_id, &instance_id)).await;
    assert_eq!(structured_error["error"]["code"], "fixture_error");

    let transport_id = "transport-error-1";
    let transport_error = call(&client, command(transport_id, &instance_id)).await;
    assert_eq!(transport_error["error"]["code"], "unavailable");

    let cancellation_id = "cancelled-command-1";
    let pending = bounded(client.send_request_with_option(
        ClientRequest::CallToolRequest(CallToolRequest::new(command(
            cancellation_id,
            &instance_id,
        ))),
        PeerRequestOptions::no_options(),
    ))
    .await
    .unwrap();
    bounded(cancel_received_rx).await.unwrap();
    bounded(pending.cancel(Some("fixture cancellation".into())))
        .await
        .unwrap();
    for index in 0..20 {
        let request_id = format!("budget-fill-{index}");
        let filled = call(&client, command(&request_id, &instance_id)).await;
        assert_eq!(filled["ok"], true);
    }
    bounded(&mut application.0).await.unwrap();
    bounded(client.cancel()).await.unwrap();

    let mut captured = Vec::new();
    bounded(stderr.as_mut().unwrap().read_to_end(&mut captured))
        .await
        .unwrap();
    let logs = String::from_utf8(captured).unwrap();
    assert_trace_pair(&logs, success_id, "success");
    assert_trace_pair(&logs, error_id, "structured_error");
    assert_trace_pair(&logs, transport_id, "transport_error");
    assert_trace_pair(&logs, cancellation_id, "cancelled");
    assert!(!logs.contains(SECRET_SENTINEL));
    assert!(!logs.contains(PRIVATE_PATH_SENTINEL));
    assert!(
        !logs.contains("fixture cancellation"),
        "cancellation detail leaked to stderr: {logs}"
    );
    assert_eq!(
        logs.len(),
        MAX_TRACE_TOTAL_BYTES,
        "trace budget did not saturate"
    );
    assert!(logs.lines().all(|line| line.len() <= MAX_TRACE_EVENT_BYTES));
}

#[tokio::test]
async fn compiled_stdio_tracing_is_off_by_default() {
    let root = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = Endpoint {
        instance_id: uuid::Uuid::new_v4().to_string(),
        port: listener.local_addr().unwrap().port(),
        secret: SECRET_SENTINEL.into(),
        build_id: "tracing-default-off".into(),
    };
    let _registration = Registration::create(root.path(), &endpoint).unwrap();
    let mut application = EndpointTask(tokio::spawn({
        let instance_id = endpoint.instance_id.clone();
        async move {
            let success = reply(&listener, &instance_id, true).await;
            assert_eq!(success.request.operation, Operation::PropertySet);
        }
    }));
    let mut process = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
    process
        .env("NEMO_MCP_REGISTRY", root.path())
        .env_remove("NEMO_MCP_TRACE")
        .kill_on_drop(true);
    let (transport, mut stderr) = TokioChildProcess::builder(process)
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let client = bounded(().serve(transport)).await.unwrap();
    let request_id = "default-off-command-1";
    let response = call(&client, command(request_id, &endpoint.instance_id)).await;
    assert_eq!(response["requestId"], request_id);
    assert_eq!(response["ok"], true);
    bounded(&mut application.0).await.unwrap();
    bounded(client.cancel()).await.unwrap();

    let mut captured = Vec::new();
    bounded(stderr.as_mut().unwrap().read_to_end(&mut captured))
        .await
        .unwrap();
    assert!(
        captured.is_empty(),
        "default run wrote to stderr: {captured:?}"
    );
}

#[tokio::test]
async fn compiled_stdio_trace_correlates_with_production_application_diagnostics() {
    const APPLICATION_HOST: &str = r#"
'use strict';
const net = require('node:net'), path = require('node:path');
const repo = process.argv[1], instanceId = process.argv[2], secret = process.argv[3];
const core = require(path.join(repo, 'src/js/application/opacity-application.js'));
const domain = require(path.join(repo, 'src/js/domain/animation/opacity.js'));
let identity = 0;
const state = {currentFrame: 0, totalFrames: 24,
  layers: [{layerUid: 'layer-a', name: 'Layer A', motionStatic: {opacity: [100]}}]};
let history = [], context = {};
function copy(value) { return JSON.parse(JSON.stringify(value)); }
const api = core.create({
  newId: () => 'document-' + (++identity), context: () => context, state: () => state,
  canMutate: () => true,
  valueAtFrame: layer => layer.motionStatic.opacity,
  snapshot: () => ({frame: 0, layers: [{id: 'layer-a', name: 'Layer A', opacity: state.layers[0].motionStatic.opacity[0]}]}),
  history: {
    checkpoint() { history.push(copy(state)); },
    undo() { return false; }, redo() { return false; }
  },
  write(operation, layer, payload) {
    if (operation !== 'property.set') throw new Error('unexpected operation');
    domain.setValue(layer, [payload.value], state.currentFrame);
  },
  afterMutation() {}
});
api.setInstanceId(instanceId);
const server = net.createServer(socket => {
  let data = '';
  socket.on('error', () => {});
  socket.on('data', bytes => {
    data += bytes.toString();
    if (!data.includes('\n')) return;
    try {
      const message = JSON.parse(data.slice(0, data.indexOf('\n')));
      if (message.secret !== secret) throw new Error('wrong endpoint secret');
      socket.end(JSON.stringify(api.handle(message.request)) + '\n');
    } catch (_) { socket.destroy(); }
  });
});
server.listen(0, '127.0.0.1', () => console.log(JSON.stringify({port: server.address().port})));
"#;

    let root = tempfile::Builder::new()
        .prefix(PRIVATE_PATH_SENTINEL)
        .tempdir()
        .unwrap();
    let instance_id = uuid::Uuid::new_v4().to_string();
    let secret = SECRET_SENTINEL;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut node = tokio::process::Command::new(
        std::env::var_os("NEMO_TEST_NODE").unwrap_or_else(|| "node".into()),
    )
    .args(["-e", APPLICATION_HOST])
    .arg(repo)
    .args([instance_id.as_str(), secret])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .kill_on_drop(true)
    .spawn()
    .expect("Node is required for production application diagnostics acceptance");
    let mut node_output = BufReader::new(node.stdout.take().unwrap());
    let mut ready_line = String::new();
    bounded(node_output.read_line(&mut ready_line))
        .await
        .unwrap();
    let port = serde_json::from_str::<Value>(&ready_line).unwrap()["port"]
        .as_u64()
        .unwrap() as u16;
    let _registration = Registration::create(
        root.path(),
        &Endpoint {
            instance_id: instance_id.clone(),
            port,
            secret: secret.into(),
            build_id: "production-diagnostics-fixture".into(),
        },
    )
    .unwrap();

    let mut process = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
    process
        .env("NEMO_MCP_REGISTRY", root.path())
        .env("NEMO_MCP_TRACE", "1")
        .kill_on_drop(true);
    let (transport, mut stderr) = TokioChildProcess::builder(process)
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let client = bounded(().serve(transport)).await.unwrap();

    let request_id = "db95a760-edce-4a04-9576-8a4b8054d995";
    let mut request = command(request_id, &instance_id).arguments.unwrap();
    request.insert("documentId".into(), json!("document-2"));
    request.insert("expectedRevision".into(), json!(0));
    request.get_mut("payload").unwrap()["layerId"] = json!("layer-a");
    let response = bounded(
        client.call_tool(CallToolRequestParams::new("nemo_command").with_arguments(request)),
    )
    .await
    .unwrap();
    let response = response.structured_content.unwrap();
    assert_eq!(
        response["ok"], true,
        "application command failed: {response}"
    );

    let diagnostics = bounded(
        client.call_tool(
            CallToolRequestParams::new("nemo_query").with_arguments(
                json!({"instance_id": instance_id, "operation": "diagnostics.trace"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        ),
    )
    .await
    .unwrap()
    .structured_content
    .unwrap();
    let record_id = diagnostics["result"]["entries"][0]["request"]["requestId"]
        .as_str()
        .unwrap();
    assert_eq!(record_id, request_id);

    bounded(client.cancel()).await.unwrap();
    let mut captured = Vec::new();
    bounded(stderr.as_mut().unwrap().read_to_end(&mut captured))
        .await
        .unwrap();
    let logs = String::from_utf8(captured).unwrap();
    assert_trace_pair(&logs, request_id, "success");
    assert!(!logs.contains(SECRET_SENTINEL));
    assert!(!logs.contains(PRIVATE_PATH_SENTINEL));
    assert!(logs.len() <= MAX_TRACE_TOTAL_BYTES);
    assert!(logs.lines().all(|line| line.len() <= MAX_TRACE_EVENT_BYTES));
}
