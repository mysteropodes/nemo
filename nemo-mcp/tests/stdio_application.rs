//! Compiled MCP and direct calls share the production opacity service.
//! Node is a development-test prerequisite. The deterministic document/history
//! ports below do not establish native UI history or installed-client acceptance.
use nemo_mcp::{
    registry::{Endpoint, Registration},
    wire,
};
use rmcp::{
    model::CallToolRequestParams, service::RunningService, transport::TokioChildProcess,
    RoleClient, ServiceExt,
};
use serde_json::{json, Value};
use std::{path::Path, process::Stdio, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    process::{Child, ChildStdin, ChildStdout},
};

// This host supplies only document/history ports and test transport. Request
// validation, revisions, retry retention and mutation dispatch are production JS.
const APPLICATION_HOST: &str = r#"
'use strict';
const path = require('node:path');
const readline = require('node:readline');
const net = require('node:net');
const repo = process.argv[1], instanceId = process.argv[2], secret = process.argv[3];
const core = require(path.join(repo, 'src/js/application/opacity-application.js'));
const domain = require(path.join(repo, 'src/js/domain/animation/opacity.js'));
let identity = 0, context = {}, checkpoints = 0, mutations = 0;
let state = fresh(), undo = [], redo = [];
function fresh() {
  return {currentFrame: 0, totalFrames: 24,
    layers: [{layerUid: 'layer-a', name: 'Layer A', motionStatic: {opacity: [100]}}]};
}
function copy(value) { return JSON.parse(JSON.stringify(value)); }
const api = core.create({
  newId: () => 'document-' + (++identity), context: () => context,
  state: () => state, canMutate: () => true,
  valueAtFrame: layer => layer.motionStatic.opacity,
  snapshot: () => ({frame: state.currentFrame, layers: state.layers.map(layer =>
    ({id: layer.layerUid, name: layer.name, opacity: layer.motionStatic.opacity[0]}))}),
  history: {
    checkpoint() { undo.push(copy(state)); redo = []; checkpoints++; },
    undo() { if (!undo.length) return false; redo.push(copy(state)); state = undo.pop(); return true; },
    redo() { if (!redo.length) return false; undo.push(copy(state)); state = redo.pop(); return true; }
  },
  write(operation, layer, payload) {
    if (operation !== 'property.set') throw new Error('Unexpected fixture operation: ' + operation);
    domain.setValue(layer, [payload.value], state.currentFrame);
  },
  afterMutation() { mutations++; }
});
if (!api.setInstanceId(instanceId).ok) throw new Error('Instance binding failed');
function local(message) {
  if (message.action === 'call') return api.handle(message.request);
  if (message.action === 'replace') {
    state = fresh(); undo = []; redo = []; context = {}; api.documentChanged();
  } else if (message.action !== 'inspect') throw new Error('Unexpected fixture action');
  return {meta: api.meta(), state: copy(state), checkpoints, mutations,
    undoDepth: undo.length, redoDepth: redo.length};
}
const server = net.createServer(socket => {
  socket.setTimeout(5000, () => socket.destroy());
  let data = '', handled = false;
  socket.on('error', () => {});
  socket.on('data', bytes => {
    if (handled) return;
    data += bytes.toString();
    if (Buffer.byteLength(data) > 1048576) return socket.destroy();
    if (!data.includes('\n')) return;
    handled = true;
    try {
      const message = JSON.parse(data.slice(0, data.indexOf('\n')));
      if (message.secret !== secret) throw new Error('Wrong test endpoint secret');
      socket.end(JSON.stringify(api.handle(message.request)) + '\n');
    } catch (error) { console.error(error.message); socket.destroy(); }
  });
});
server.listen(0, '127.0.0.1', () => {
  console.log(JSON.stringify({port: server.address().port}));
  readline.createInterface({input: process.stdin}).on('line', line => {
    console.log(JSON.stringify(local(JSON.parse(line))));
  }).on('close', () => process.exit(0));
});
"#;

struct ApplicationHost {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
impl ApplicationHost {
    async fn start(instance: &str, secret: &str) -> (Self, u16) {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        let mut child = tokio::process::Command::new(
            std::env::var_os("NEMO_TEST_NODE").unwrap_or_else(|| "node".into()),
        )
        .args(["-e", APPLICATION_HOST])
        .arg(repo)
        .args([instance, secret])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .expect("Node is required for production application/MCP parity tests");
        let input = child.stdin.take().unwrap();
        let output = BufReader::new(child.stdout.take().unwrap());
        let mut host = Self {
            child,
            input,
            output,
        };
        let ready = host.read().await;
        let port = ready["port"].as_u64().unwrap().try_into().unwrap();
        (host, port)
    }
    async fn read(&mut self) -> Value {
        let mut line = String::new();
        assert_ne!(
            self.output.read_line(&mut line).await.unwrap(),
            0,
            "application host exited"
        );
        serde_json::from_str(&line).unwrap()
    }
    async fn send(&mut self, value: Value) -> Value {
        wire::write_json(&mut self.input, &value).await.unwrap();
        self.read().await
    }
    async fn direct(&mut self, request: &Value) -> Value {
        self.send(json!({"action":"call", "request":request})).await
    }
    async fn inspect(&mut self) -> Value {
        self.send(json!({"action":"inspect"})).await
    }
}

fn request(meta: &Value, id: &str, operation: &str, payload: Value) -> Value {
    json!({"apiVersion":1, "requestId":id, "instanceId":meta["instanceId"],
        "documentId":meta["documentId"], "expectedRevision":meta["revision"],
        "operation":operation, "payload":payload})
}
fn opacity(value: u32) -> Value {
    json!({"layerId":"layer-a", "property":"opacity", "value":value})
}
async fn command(client: &RunningService<RoleClient, ()>, request: &Value) -> Value {
    let response = client
        .call_tool(
            CallToolRequestParams::new("nemo_command")
                .with_arguments(request.as_object().unwrap().clone()),
        )
        .await
        .unwrap();
    let body = response.structured_content.unwrap();
    assert_eq!(
        response.is_error.unwrap_or(false),
        !body["ok"].as_bool().unwrap()
    );
    body
}
fn state_is(state: &Value, value: u32, revision: u32, checkpoints: u32, depth: u32) {
    assert_eq!(
        state["state"]["layers"][0]["motionStatic"]["opacity"],
        json!([value])
    );
    assert_eq!(state["meta"]["revision"], revision);
    assert_eq!(state["checkpoints"], checkpoints);
    assert_eq!(state["mutations"], checkpoints);
    assert_eq!(state["undoDepth"], depth);
}

#[tokio::test]
async fn compiled_mcp_shares_application_revision_retries_and_history() {
    tokio::time::timeout(Duration::from_secs(20), async {
        let registry = tempfile::tempdir().unwrap();
        let instance = uuid::Uuid::new_v4().to_string();
        let secret = uuid::Uuid::new_v4().to_string();
        let (mut app, port) = ApplicationHost::start(&instance, &secret).await;
        let _registration = Registration::create(
            registry.path(),
            &Endpoint {
                instance_id: instance.clone(),
                port,
                secret,
                build_id: "application-parity-fixture".into(),
            },
        )
        .unwrap();
        let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_nemo-mcp"));
        child
            .env("NEMO_MCP_REGISTRY", registry.path())
            .kill_on_drop(true);
        let client = ().serve(TokioChildProcess::new(child).unwrap()).await.unwrap();

        let initial = app.inspect().await;
        state_is(&initial, 100, 0, 0, 0);
        let capabilities = request(
            &initial["meta"],
            "direct-capabilities",
            "capabilities",
            json!({}),
        );
        let direct_capabilities = app.direct(&capabilities).await;
        let discovery = client
            .call_tool(CallToolRequestParams::new("nemo_discover"))
            .await
            .unwrap()
            .structured_content
            .unwrap();
        assert_eq!(discovery["instances"].as_array().unwrap().len(), 1);
        assert_eq!(discovery["instances"][0]["instanceId"], instance);
        assert_eq!(
            discovery["instances"][0]["documentId"],
            initial["meta"]["documentId"]
        );
        assert_eq!(
            discovery["instances"][0]["capabilities"],
            direct_capabilities["result"]
        );
        let direct_set = request(&initial["meta"], "direct-set", "property.set", opacity(75));
        let direct_result = app.direct(&direct_set).await;
        assert_eq!(direct_result["ok"], true);
        assert_eq!(direct_result["result"]["value"], 75);
        assert_eq!(direct_result["revision"], 1);
        // MCP must see the direct call's retained result, not a second document/cache.
        assert_eq!(command(&client, &direct_set).await, direct_result);
        state_is(&app.inspect().await, 75, 1, 1, 1);

        let mcp_set = request(&direct_result, "mcp-set", "property.set", opacity(25));
        let committed = command(&client, &mcp_set).await;
        assert_eq!(committed["ok"], true);
        assert_eq!(committed["result"]["value"], 25);
        assert_eq!(committed["revision"], 2);
        assert_eq!(app.direct(&mcp_set).await, committed);
        assert_eq!(command(&client, &mcp_set).await, committed);
        state_is(&app.inspect().await, 25, 2, 2, 2);

        let unchanged = app.inspect().await;
        let mut changed = mcp_set.clone();
        changed["payload"] = opacity(10);
        assert_eq!(
            command(&client, &changed).await["error"]["code"],
            "invalid_request"
        );
        assert_eq!(app.inspect().await, unchanged);
        let stale = request(&initial["meta"], "stale-set", "property.set", opacity(10));
        assert_eq!(
            command(&client, &stale).await["error"]["code"],
            "stale_revision"
        );
        assert_eq!(app.inspect().await, unchanged);

        // One undo returns to the direct edit, proving retries added no checkpoint.
        let undo = request(&committed, "undo", "history.undo", json!({}));
        let undone = command(&client, &undo).await;
        assert_eq!(undone["ok"], true);
        assert_eq!(command(&client, &undo).await, undone);
        state_is(&app.inspect().await, 75, 3, 2, 1);
        let redo = request(&undone, "redo", "history.redo", json!({}));
        assert_eq!(app.direct(&redo).await["ok"], true);
        state_is(&app.inspect().await, 25, 4, 2, 2);
        // Retrying an old success returns its original revision without rewinding state.
        assert_eq!(command(&client, &mcp_set).await, committed);
        state_is(&app.inspect().await, 25, 4, 2, 2);

        let replaced = app.send(json!({"action":"replace"})).await;
        assert_ne!(
            replaced["meta"]["documentId"],
            initial["meta"]["documentId"]
        );
        state_is(&replaced, 100, 0, 2, 0);
        let old_retry = command(&client, &mcp_set).await;
        assert_eq!(old_retry["error"]["code"], "wrong_document");
        assert_eq!(app.inspect().await, replaced);
        // A current-document command still succeeds after rejecting the old retry.
        let next = request(
            &replaced["meta"],
            "new-document-set",
            "property.set",
            opacity(10),
        );
        assert_eq!(command(&client, &next).await["ok"], true);
        state_is(&app.inspect().await, 10, 1, 3, 1);

        client.cancel().await.unwrap();
        app.child.kill().await.unwrap();
        app.child.wait().await.unwrap();
    })
    .await
    .expect("compiled MCP/application parity exceeded its bounded deadline");
}
