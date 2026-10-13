use super::*;
use serde_json::json;

struct Fixture {
    root: PathBuf,
    runtime: TaskRuntime,
    native: Arc<Mutex<LoadLog>>,
    bridge: Arc<Mutex<Bridge>>,
    store: Arc<Store>,
    version: u64,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nemo-owner-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let runtime = TaskRuntime {
            task_id: "owner-core-fixture".into(),
            task_key: "a".repeat(64),
            data_dir: root.join("data"),
            owner_token: Some("fixture-private-owner-token".into()),
        };
        let file = std::fs::File::create(root.join("native.json")).unwrap();
        let native = Arc::new(Mutex::new(LoadLog::new(
            uuid::Uuid::new_v4().to_string(),
            runtime.task_key.clone(),
            std::process::id(),
            file,
        )));
        let bridge = Bridge::new(native.clone()).unwrap();
        let projection = bridge.projection();
        let store = Arc::new(Store::open(&root, &root, &projection).unwrap());
        store.publish(1, &projection).unwrap();
        Self {
            root,
            runtime,
            native,
            bridge: Arc::new(Mutex::new(bridge)),
            store,
            version: 1,
        }
    }
    fn session(&self) -> Session {
        let b = self.bridge.lock().unwrap();
        let n = self.native.lock().unwrap();
        Session::new(&self.runtime, &n, &b, self.store.clone()).unwrap()
    }
    fn capture(&self, s: &mut Session) -> Capture {
        let b = self.bridge.lock().unwrap();
        let n = self.native.lock().unwrap();
        s.capture(&self.runtime, &n, &b, self.version).unwrap()
    }
    fn confirm(&self, s: &mut Session, a: Authorization) -> Result<String, &'static str> {
        let b = self.bridge.lock().unwrap();
        let n = self.native.lock().unwrap();
        s.confirm(a, &self.runtime, &n, &b, self.version)
            .map(str::to_owned)
    }
    fn request(&self) -> Value {
        let n = self.native.lock().unwrap();
        json!({"schema":"nemo.startup-owner-cut/1",
            "ownerToken":self.runtime.owner_token,"runId":n.run,"taskKey":n.task_key,
            "pid":n.pid,"window":"main","loadGeneration":1})
    }
    fn advance(&mut self) {
        let mut b = self.bridge.lock().unwrap();
        let p = b.stream.projection();
        let e = json!({"schema":"nemo.startup-js/1","runId":p["runId"],"loadGeneration":1,"sequence":1,"stage":"early-script"});
        b.admit(
            "main",
            &tauri::Url::parse("tauri://localhost/index.html").unwrap(),
            std::process::id(),
            &tauri::ipc::InvokeBody::Raw(serde_json::to_vec(&e).unwrap()),
        )
        .unwrap();
        self.version += 1;
        self.store.publish(self.version, &b.projection()).unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn bytes(v: &Value) -> Vec<u8> {
    serde_json::to_vec(v).unwrap()
}

#[test]
fn native_auth_current_h1_and_single_pending_identity_do_not_advance_data() {
    let mut f = Fixture::new();
    f.advance();
    let mut s = f.session();
    let body = bytes(&f.request());
    let before = std::fs::read(f.root.join("startup-js.json")).unwrap();
    let work = f.capture(&mut s);
    let id = f.confirm(&mut s, work.authorize(&body));
    // Separate mutable operations: capture leaves no guard/borrow in work.
    assert!(id.is_ok());
    let id = id.unwrap();
    assert!(uuid::Uuid::parse_str(&id).is_ok());
    assert_eq!(s.state(), State::Pending);
    assert_eq!(s.pending().unwrap().phase(), Phase::Requested);
    let work = f.capture(&mut s);
    assert_eq!(f.confirm(&mut s, work.authorize(&body)), Ok(id.clone()));
    assert_eq!(
        std::fs::read(f.root.join("startup-js.json")).unwrap(),
        before
    );
    let projection = f.bridge.lock().unwrap().projection();
    let proof = f.store.current_health(f.version, &projection).unwrap();
    assert_eq!(proof.version, f.version);
    let footer = json!({"runId":projection["js"]["runId"],"loadGeneration":1,"cutId":id,
        "status":"drained","generated":1,"sent":1,"failed":0,"dropped":0,"late":0,"pending":0,
        "visiblePaint":"not-observed"});
    let bridge = f.bridge.lock().unwrap();
    let native = f.native.lock().unwrap();
    let cut = s.pending().unwrap();
    cut.begin_drain().unwrap();
    assert_eq!(
        cut.validate_provisional(&bytes(&footer), &native, &bridge.stream, Some(&proof)),
        Ok(())
    );
    assert_eq!(cut.phase(), Phase::Draining); // validator is not a handler/seal
}

#[test]
fn native_configuration_and_typed_request_cannot_be_forged_or_reopened() {
    let mut f = Fixture::new();
    for token in [None, Some("short".into())] {
        f.runtime.owner_token = token;
        let b = f.bridge.lock().unwrap();
        let n = f.native.lock().unwrap();
        assert!(Session::new(&f.runtime, &n, &b, f.store.clone()).is_err());
    }
    f.runtime.owner_token = Some("fixture-private-owner-token".into());
    for (key, value) in [
        ("ownerToken", json!("foreign")),
        ("runId", json!("foreign")),
        ("taskKey", json!("b".repeat(64))),
        ("pid", json!(1)),
        ("window", json!("other")),
        ("loadGeneration", json!(2)),
        ("cutId", json!(uuid::Uuid::new_v4().to_string())),
        ("pid", json!(1.5)),
        ("ownerToken", Value::Null),
        ("schema", json!("other")),
    ] {
        let mut s = f.session();
        let mut body = f.request();
        body[key] = value;
        let work = f.capture(&mut s);
        assert_eq!(
            f.confirm(&mut s, work.authorize(&bytes(&body))),
            Err("startup owner refused")
        );
        assert_eq!(s.state(), State::Refused);
        assert!(s.pending().is_err());
        let b = f.bridge.lock().unwrap();
        let n = f.native.lock().unwrap();
        assert!(s.capture(&f.runtime, &n, &b, f.version).is_err());
    }
    let good = serde_json::to_string(&f.request()).unwrap();
    for body in [
        Vec::new(),
        vec![b' '; 1025],
        vec![0xff],
        good.replacen("{", "{\"pid\":1,", 1).into_bytes(),
    ] {
        let mut s = f.session();
        let work = f.capture(&mut s);
        assert!(f.confirm(&mut s, work.authorize(&body)).is_err());
    }
}

#[test]
fn stale_changed_native_js_configuration_or_adapter_projection_refuses_confirmation() {
    for case in [
        "before-health",
        "after-health",
        "native",
        "token",
        "adapter",
        "fence",
    ] {
        let mut f = Fixture::new();
        let mut s = f.session();
        let work = f.capture(&mut s);
        let body = bytes(&f.request());
        if case == "before-health" {
            f.advance();
        }
        let auth = work.authorize(&body);
        match case {
            "after-health" => f.advance(),
            "native" => f.native.lock().unwrap().refused = true,
            "token" => f.runtime.owner_token = Some("changed-private-owner-token".into()),
            "adapter" => f.bridge.lock().unwrap().adapter_refused = true,
            "fence" => f
                .bridge
                .lock()
                .unwrap()
                .stream
                .fence(super::super::super::js::Fence::Closed),
            _ => {}
        }
        assert!(f.confirm(&mut s, auth).is_err(), "{case}");
        assert_eq!(s.state(), State::Refused);
    }
}

#[test]
fn original_store_and_session_identity_are_required_and_retention_tamper_blocks_pending() {
    let f = Fixture::new();
    let mut a = f.session();
    let mut b = f.session();
    let body = bytes(&f.request());
    let aa = f.capture(&mut a).authorize(&body);
    let _bb = f.capture(&mut b);
    assert!(f.confirm(&mut b, aa).is_err());
    let mut s = f.session();
    let mut work = f.capture(&mut s);
    let foreign = f.root.join("foreign");
    std::fs::create_dir(&foreign).unwrap();
    let p = f.bridge.lock().unwrap().projection();
    let store = Arc::new(Store::open(&foreign, &foreign, &p).unwrap());
    store.publish(1, &p).unwrap();
    work.store = store;
    assert!(f.confirm(&mut s, work.authorize(&body)).is_err());
    for fault in ["tamper", "missing", "failed"] {
        let f = Fixture::new();
        let mut s = f.session();
        let work = f.capture(&mut s);
        let body = bytes(&f.request());
        match fault {
            "tamper" => std::fs::write(f.root.join("startup-js.json"), b"foreign bytes").unwrap(),
            "missing" => std::fs::remove_file(f.root.join("startup-js.json")).unwrap(),
            _ => {
                let mut bad = f.bridge.lock().unwrap().projection();
                bad["private"] = json!(true);
                assert!(f.store.publish(2, &bad).is_err());
            }
        }
        assert!(f.confirm(&mut s, work.authorize(&body)).is_err());
        assert!(s.pending().is_err());
    }
}

#[test]
fn owned_authorization_can_run_while_both_original_metadata_guards_are_held_elsewhere() {
    let f = Fixture::new();
    let mut s = f.session();
    let work = f.capture(&mut s);
    let body = bytes(&f.request());
    let bridge = f.bridge.lock().unwrap();
    let native = f.native.lock().unwrap();
    let (send, recv) = std::sync::mpsc::channel();
    let worker = std::thread::spawn(move || {
        send.send(work.authorize(&body)).unwrap();
    });
    let auth = recv
        .recv_timeout(std::time::Duration::from_secs(2))
        .expect("H1 must not reacquire bridge/native locks");
    drop(native);
    drop(bridge);
    worker.join().unwrap();
    assert!(f.confirm(&mut s, auth).is_ok());
}

#[test]
fn failed_overlap_terminal_or_retired_session_never_creates_a_second_cut() {
    let f = Fixture::new();
    let body = bytes(&f.request());
    let mut s = f.session();
    let first = f.capture(&mut s);
    let bridge = f.bridge.lock().unwrap();
    let native = f.native.lock().unwrap();
    assert!(s.capture(&f.runtime, &native, &bridge, f.version).is_err());
    drop(native);
    drop(bridge);
    assert!(f.confirm(&mut s, first.authorize(&body)).is_err());
    let mut s = f.session();
    let work = f.capture(&mut s);
    let id = f.confirm(&mut s, work.authorize(&body)).unwrap();
    s.pending().unwrap().begin_drain().unwrap();
    let work = f.capture(&mut s);
    assert_eq!(f.confirm(&mut s, work.authorize(&body)), Ok(id));
    let b = f.bridge.lock().unwrap();
    let n = f.native.lock().unwrap();
    assert!(s
        .pending()
        .unwrap()
        .finish(b"{}", &n, &b.stream, None)
        .is_err());
    assert!(s.capture(&f.runtime, &n, &b, f.version).is_err());
    assert!(s.pending().is_err());
    drop(n);
    drop(b);
    let mut s = f.session();
    let work = f.capture(&mut s);
    s.retire();
    assert!(f.confirm(&mut s, work.authorize(&body)).is_err());
}
