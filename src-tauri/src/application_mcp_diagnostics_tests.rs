//! Real MCP stdio → native wire/revision barrier → one actual Rust authority.
//! Fixture ports replace only rendering; this is not installed UI acceptance.
use crate::application_mcp::tests::*;
use serde_json::Value;
use std::{
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    thread::{self, JoinHandle},
    time::Duration,
};

struct Host {
    state: Arc<ApplicationMcp>,
    root: PathBuf,
    registration: Option<Registration>,
    stop: Arc<AtomicBool>,
    task: Option<JoinHandle<()>>,
    subscription: Arc<Mutex<Option<String>>>,
}
impl Host {
    fn start() -> Self {
        let state = Arc::new(ApplicationMcp::default());
        state
            .install_native(
                NativeApplication::new(
                    state.instance_id.clone(),
                    document(),
                    Port,
                    Compositor,
                    Resolver,
                )
                .unwrap(),
            )
            .unwrap();
        let root =
            std::env::temp_dir().join(format!("nemo-native-diagnostics-{}", uuid::Uuid::new_v4()));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let secret = uuid::Uuid::new_v4().to_string();
        let endpoint = Endpoint {
            instance_id: state.instance_id.clone(),
            port: listener.local_addr().unwrap().port(),
            secret: secret.clone(),
            build_id: "native-diagnostics-test".into(),
        };
        let registration = Registration::create(&root, &endpoint).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let subscription = Arc::new(Mutex::new(None::<String>));
        let (server_state, shutdown, subscriber) =
            (state.clone(), stop.clone(), subscription.clone());
        let task = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let listener = TcpListener::from_std(listener).unwrap();
                while !shutdown.load(Ordering::Acquire) {
                    let Ok(accepted) = tokio::time::timeout(Duration::from_millis(50), listener.accept()).await else { continue; };
                    let (mut stream, _) = accepted.unwrap();
                    let message = tokio::time::timeout(Duration::from_secs(5), wire::read_json::<wire::AuthenticatedWireRequest>(&mut stream)).await.unwrap().unwrap();
                    match message {
                        wire::AuthenticatedWireRequest::Native(message) if message.secret == secret => {
                            server_state.serve_external_for_test(&mut stream, message.native_request, |event| {
                                let id = subscriber.lock().unwrap().clone().ok_or("fixture has no subscriber")?;
                                server_state.control_revision_for_test(json!({"action":"acknowledge", "subscriptionId":id, "event":event})).map(|_| ())
                            }).await.unwrap();
                        }
                        wire::AuthenticatedWireRequest::NativeStatus(message) if message.secret == secret => {
                            let response = server_state.native_status(message.native_status).unwrap();
                            wire::write_json(&mut stream, &response).await.unwrap();
                        }
                        _ => panic!("unexpected authenticated fixture operation"),
                    }
                }
            });
        });
        Self {
            state,
            root,
            registration: Some(registration),
            stop,
            task: Some(task),
            subscription,
        }
    }
    fn request(
        &self,
        id: &str,
        operation: &str,
        payload: Value,
        revision: Option<u64>,
    ) -> NativeApplicationRequest {
        let binding = self
            .state
            .control_revision_for_test(json!({"action":"binding"}))
            .unwrap();
        NativeApplicationRequest {
            api_version: 2,
            request_id: id.into(),
            instance_id: self.state.instance_id.clone(),
            document_id: binding["documentId"].as_str().unwrap().into(),
            expected_revision: revision,
            operation: operation.into(),
            payload,
            cancelled_before_dispatch: false,
        }
    }
    fn query(&self) -> NativeApplicationRequest {
        self.request("inspect", "query.diagnostics.recent", json!({}), None)
    }
    fn set(&self, id: &str, revision: u64, value: u32) -> NativeApplicationRequest {
        self.request(id, "command.document.apply", json!({"command":"layer.opacity.set", "stableTarget":{"layerUid":"r08_curve_layer"}, "value":value}), Some(revision))
    }
    fn subscribe(&self) {
        let binding = self
            .state
            .control_revision_for_test(json!({"action":"binding"}))
            .unwrap();
        self.state
            .control_revision_for_test(json!({"action":"subscribe", "binding":binding}))
            .unwrap();
        *self.subscription.lock().unwrap() =
            Some(binding["subscriptionId"].as_str().unwrap().into());
    }
}
impl Drop for Host {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
        self.registration.take();
        let _ = std::fs::remove_dir(&self.root);
    }
}

struct Client {
    child: Child,
    input: ChildStdin,
    output: mpsc::Receiver<Value>,
    reader: Option<JoinHandle<()>>,
    next_id: u64,
}
impl Client {
    fn start(host: &Host) -> Self {
        Self::in_registry(&host.root)
    }
    fn in_registry(root: &std::path::Path) -> Self {
        let binary = std::env::var_os("NEMO_TEST_MCP_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nemo-mcp/target/debug/nemo-mcp")
            });
        assert!(
            binary.is_file(),
            "build the candidate nemo-mcp binary or set NEMO_TEST_MCP_BINARY"
        );
        let mut child = Command::new(binary)
            .env("NEMO_MCP_REGISTRY", root)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn candidate MCP");
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, output) = mpsc::channel();
        let reader = thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else {
                    break;
                };
                let Ok(value) = serde_json::from_str(&line) else {
                    break;
                };
                if tx.send(value).is_err() {
                    break;
                }
            }
        });
        let mut client = Self {
            child,
            input,
            output,
            reader: Some(reader),
            next_id: 0,
        };
        let init = client.rpc("initialize", json!({"protocolVersion":"2024-11-05", "capabilities":{}, "clientInfo":{"name":"native-diagnostics-oracle", "version":"1"}}));
        assert!(init.get("result").is_some(), "MCP initialization rejected");
        client.send(json!({"jsonrpc":"2.0", "method":"notifications/initialized"}));
        client
    }
    fn send(&mut self, value: Value) {
        serde_json::to_writer(&mut self.input, &value).unwrap();
        self.input.write_all(b"\n").unwrap();
        self.input.flush().unwrap();
    }
    fn rpc(&mut self, method: &str, params: Value) -> Value {
        self.next_id += 1;
        self.send(json!({"jsonrpc":"2.0", "id":self.next_id, "method":method, "params":params}));
        loop {
            let value = self
                .output
                .recv_timeout(Duration::from_secs(15))
                .expect("bounded MCP response");
            if value["id"] == self.next_id {
                return value;
            }
        }
    }
    fn call(&mut self, request: &NativeApplicationRequest) -> Value {
        let result = self.rpc(
            "tools/call",
            json!({"name":"nemo_command", "arguments":request}),
        );
        assert!(
            result.get("error").is_none(),
            "MCP protocol rejected a declared request"
        );
        result["result"]["structuredContent"].clone()
    }
}

/// A fault-injection relay holds bytes produced by the genuine native endpoint.
/// No result is fabricated or reapplied when the old response is released.
struct HeldResponse {
    root: PathBuf,
    registration: Option<Registration>,
    captured: mpsc::Receiver<()>,
    release: Option<mpsc::Sender<()>>,
    task: Option<JoinHandle<()>>,
}
impl HeldResponse {
    fn start(host: &Host) -> Self {
        let upstream = registry::read_endpoints(&host.root).unwrap().remove(0);
        let root =
            std::env::temp_dir().join(format!("nemo-held-response-{}", uuid::Uuid::new_v4()));
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let endpoint = Endpoint {
            port: listener.local_addr().unwrap().port(),
            ..upstream.clone()
        };
        let registration = Registration::create(&root, &endpoint).unwrap();
        let (captured_tx, captured) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let task = thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                tokio::time::timeout(Duration::from_secs(15), async move {
                    let listener = TcpListener::from_std(listener).unwrap();
                    let (mut client, _) = listener.accept().await.unwrap();
                    let request: wire::NativeWireRequest =
                        wire::read_json(&mut client).await.unwrap();
                    let mut native = TcpStream::connect(("127.0.0.1", upstream.port))
                        .await
                        .unwrap();
                    wire::write_json(&mut native, &request).await.unwrap();
                    let response: NativeApplicationResponse =
                        wire::read_json(&mut native).await.unwrap();
                    captured_tx.send(()).unwrap();
                    released.recv_timeout(Duration::from_secs(10)).unwrap();
                    wire::write_json(&mut client, &response).await.unwrap();
                })
                .await
                .unwrap();
            });
        });
        Self {
            root,
            registration: Some(registration),
            captured,
            release: Some(release),
            task: Some(task),
        }
    }
    fn release(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}
impl Drop for HeldResponse {
    fn drop(&mut self) {
        self.release();
        if let Some(task) = self.task.take() {
            let _ = task.join();
        }
        self.registration.take();
        let _ = std::fs::remove_dir(&self.root);
    }
}
impl Drop for Client {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

#[test]
fn native_diagnostics_stdio_observes_ui_and_mcp_edits_on_one_real_authority() {
    let host = Host::start();
    let mut client = Client::start(&host);
    assert!(
        host.state
            .dispatch_native(host.set("ui-40", 0, 40))
            .unwrap()
            .ok
    );
    let blocked = client.call(&host.set("no-subscriber", 1, 60));
    assert_eq!(blocked["error"]["code"], "unavailable");
    host.subscribe();
    let edit = host.set("mcp-60", 1, 60);
    let committed = client.call(&edit);
    assert_eq!(committed["contentRevision"], 2);
    assert_eq!(committed["ok"], true);
    assert_eq!(client.call(&edit), committed);
    for (operation, revision) in [("history.undo", 2), ("history.redo", 3)] {
        let response = client.call(&host.request(operation, operation, json!({}), Some(revision)));
        assert_eq!(response["ok"], true);
    }
    let query = host.query();
    let trace = client.call(&query);
    assert_eq!(
        trace,
        serde_json::to_value(host.state.dispatch_native(query.clone()).unwrap()).unwrap()
    );
    assert_eq!(
        trace["result"]["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|record| record["requestId"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["ui-40", "mcp-60", "mcp-60", "history.undo", "history.redo"]
    );
    let before = host
        .state
        .dispatch_native(host.request(
            "serialize-before",
            "query.document.serialize",
            json!({"atRevision":4}),
            None,
        ))
        .unwrap();
    for _ in 0..10 {
        assert_eq!(client.call(&query), trace);
    }
    let after = host
        .state
        .dispatch_native(host.request(
            "serialize-after",
            "query.document.serialize",
            json!({"atRevision":4}),
            None,
        ))
        .unwrap();
    assert_eq!(after.result, before.result);
    assert_eq!(after.content_revision, before.content_revision);
    let other = Host::start();
    assert_eq!(
        other
            .state
            .dispatch_native(other.query())
            .unwrap()
            .result
            .unwrap()["records"],
        json!([]),
        "a second native authority must not observe this trace"
    );
    let mut wrong = query.clone();
    wrong.instance_id = other.state.instance_id.clone();
    assert_eq!(client.call(&wrong)["error"]["code"], "unavailable");
    let mut held = HeldResponse::start(&host);
    let mut delayed_client = Client::in_registry(&held.root);
    let old_query = query.clone();
    let delayed = thread::spawn(move || delayed_client.call(&old_query));
    held.captured.recv_timeout(Duration::from_secs(10)).unwrap();
    let mut previous_identity = trace["documentId"].clone();
    for turn in 0..2 {
        host.state.replace_native_document(document()).unwrap();
        assert_eq!(client.call(&query)["error"]["code"], "wrong_document");
        let current = client.call(&host.query());
        assert_ne!(current["documentId"], previous_identity);
        previous_identity = current["documentId"].clone();
        assert_eq!(current["result"], json!({"records":[], "truncated":false}));
        if turn == 0 {
            held.release();
        }
        assert_eq!(client.call(&host.query()), current);
    }
    let late = delayed.join().unwrap();
    assert_eq!(
        late, trace,
        "the held response really came from A's authority"
    );
    assert_eq!(
        client.call(&host.query())["result"],
        json!({"records":[], "truncated":false}),
        "late A bytes cannot contaminate the second B identity"
    );
}
