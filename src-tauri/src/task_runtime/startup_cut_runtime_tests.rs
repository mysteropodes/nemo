use super::super::{cut::Phase, retention, Slot};
use super::*;
use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;

struct Fixture {
    task: std::path::PathBuf,
    runtime: TaskRuntime,
    state: StartupTransport,
    store: Arc<Store>,
}
impl Fixture {
    fn new() -> Self {
        Self::with_creation(true)
    }
    fn with_creation(created: bool) -> Self {
        let task = std::env::temp_dir().join(format!("nemo-slot-owner-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&task).unwrap();
        let task = std::fs::canonicalize(task).unwrap();
        std::fs::create_dir(task.join("reports")).unwrap();
        std::fs::create_dir(task.join("data")).unwrap();
        let runtime = TaskRuntime {
            task_id: "slot-owner-fixture".into(),
            task_key: "a".repeat(64),
            data_dir: task.join("data"),
            owner_token: Some("fixture-private-owner-token".into()),
        };
        let native = Arc::new(Mutex::new(LoadLog::new(
            uuid::Uuid::new_v4().to_string(),
            runtime.task_key.clone(),
            std::process::id(),
            std::fs::File::create(task.join("native.json")).unwrap(),
        )));
        if created {
            let mut log = native.lock().unwrap();
            let run = log.run.clone();
            log.confirm_creation("main", std::process::id(), &run);
            log.persist().unwrap();
        }
        let mut slot = Slot::default();
        slot.bind(Some(native)).unwrap();
        let (version, projection) = slot.snapshot().unwrap().unwrap();
        let reports = task.join("reports");
        let store = Arc::new(retention::Store::open(&reports, &reports, &projection).unwrap());
        store.publish(version, &projection).unwrap();
        let state = StartupTransport(
            Arc::new(Mutex::new(slot)),
            Arc::new(Mutex::new(Some(store.clone()))),
        );
        install(&state, &runtime, store.clone()).unwrap();
        Self {
            task,
            runtime,
            state,
            store,
        }
    }
    fn request(&self) -> Value {
        let slot = self.state.0.lock().unwrap();
        let p = slot.bridge.as_ref().unwrap().stream.projection();
        json!({"schema":"nemo.startup-owner-cut/1", "ownerToken":self.runtime.owner_token,
            "runId":p["runId"],"taskKey":p["taskKey"],"pid":p["pid"],"window":"main","loadGeneration":1})
    }
    fn publish(&self, bytes: &[u8]) {
        let temp = self.task.join("reports/private-request.tmp");
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temp)
            .unwrap();
        file.write_all(bytes).unwrap();
        file.sync_all().unwrap();
        std::fs::rename(temp, self.task.join("reports/startup-cut-request.json")).unwrap();
    }
    fn input(&self) -> request::Input {
        let slot = self.state.0.lock().unwrap();
        let root = slot.owner.as_ref().unwrap().root.clone();
        drop(slot);
        root.read().unwrap().unwrap()
    }
    fn advance(&self) {
        let (version, projection) = {
            let mut slot = self.state.0.lock().unwrap();
            let p = slot.bridge.as_ref().unwrap().stream.projection();
            let event = json!({"schema":"nemo.startup-js/1","runId":p["runId"],"loadGeneration":1,"sequence":1,"stage":"early-script"});
            slot.admit(
                "main",
                &tauri::Url::parse("tauri://localhost/index.html").unwrap(),
                std::process::id(),
                &tauri::ipc::InvokeBody::Raw(serde_json::to_vec(&event).unwrap()),
            )
            .unwrap();
            slot.snapshot().unwrap().unwrap()
        };
        self.store.publish(version, &projection).unwrap();
    }
    fn state(&self) -> owner::State {
        let slot = self.state.0.lock().unwrap();
        slot.owner.as_ref().unwrap().session.state()
    }
    fn native(&self) -> Arc<Mutex<LoadLog>> {
        self.state
            .0
            .lock()
            .unwrap()
            .bridge
            .as_ref()
            .unwrap()
            .native
            .clone()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        retire(&self.state.0);
        let _ = std::fs::remove_dir_all(&self.task);
    }
}
#[test]
fn real_original_slot_store_native_and_physical_input_create_one_requested_cut() {
    let f = Fixture::new();
    f.advance();
    f.publish(&serde_json::to_vec(&f.request()).unwrap());
    let before = {
        let slot = f.state.0.lock().unwrap();
        (
            slot.version,
            slot.bridge.as_ref().unwrap().projection(),
            std::fs::read(f.task.join("reports/startup-js.json")).unwrap(),
        )
    };
    let work = prepare(&f.state.0, "main", &f.native()).unwrap();
    let exited = work.exited.clone();
    run(work, Duration::from_secs(1));
    assert!(exited.load(Ordering::Acquire));
    let mut slot = f.state.0.lock().unwrap();
    assert_eq!(slot.version, before.0);
    assert_eq!(slot.bridge.as_ref().unwrap().projection(), before.1);
    assert_eq!(
        std::fs::read(f.task.join("reports/startup-js.json")).unwrap(),
        before.2
    );
    let binding = slot.owner.as_mut().unwrap();
    let cut = binding.session.pending().unwrap();
    assert!(uuid::Uuid::parse_str(cut.id()).is_ok());
    assert_eq!(cut.phase(), Phase::Requested);
    drop(slot);
    assert!(prepare(&f.state.0, "main", &f.native()).is_err());
    assert_eq!(f.state(), owner::State::Refused);
}
#[test]
fn actual_bind_before_created_then_real_js_reconciliation_can_request() {
    let f = Fixture::with_creation(false);
    let native = f.native();
    {
        let mut log = native.lock().unwrap();
        let run = log.run.clone();
        log.confirm_creation("main", std::process::id(), &run);
        log.persist().unwrap();
    }
    let work = prepare(&f.state.0, "main", &native).unwrap();
    f.advance(); // Actual producer reconciliation/publication, never synthetic health.
    f.publish(&serde_json::to_vec(&f.request()).unwrap());
    run(work, Duration::from_secs(1));
    let mut slot = f.state.0.lock().unwrap();
    let binding = slot.owner.as_mut().unwrap();
    assert_eq!(binding.session.pending().unwrap().phase(), Phase::Requested);
}
#[test]
fn raw_request_schema_and_missing_or_invalid_configuration_fail_closed() {
    for field in [
        "ownerToken",
        "runId",
        "taskKey",
        "pid",
        "window",
        "loadGeneration",
        "schema",
        "cutId",
    ] {
        let f = Fixture::new();
        let mut body = f.request();
        body[field] = if field == "pid" || field == "loadGeneration" {
            json!(99)
        } else {
            json!("foreign")
        };
        f.publish(&serde_json::to_vec(&body).unwrap());
        run(
            prepare(&f.state.0, "main", &f.native()).unwrap(),
            Duration::from_secs(1),
        );
        assert_eq!(f.state(), owner::State::Refused);
    }
    for body in [b"null".to_vec(), b"[]".to_vec(), b"{}".to_vec()] {
        let f = Fixture::new();
        f.publish(&body);
        run(
            prepare(&f.state.0, "main", &f.native()).unwrap(),
            Duration::from_secs(1),
        );
        assert_eq!(f.state(), owner::State::Refused);
    }
    let f = Fixture::new();
    assert!(install(&f.state, &f.runtime, f.store.clone()).is_err());
    let mut runtime = f.runtime.clone();
    runtime.owner_token = None;
    assert!(install(&f.state, &runtime, f.store.clone()).is_err());
    assert!(prepare(&f.state.0, "foreign", &f.native()).is_err());
    assert_eq!(f.state(), owner::State::Refused);
}
#[test]
fn metadata_input_changes_refuse_and_point_health_does_not_claim_finality() {
    for fault in [
        "version",
        "adapter",
        "native",
        "holder",
        "input",
        "retention",
        "deadline",
        "close",
    ] {
        let f = Fixture::new();
        f.publish(&serde_json::to_vec(&f.request()).unwrap());
        let mut input = f.input();
        let deadline = Instant::now() + Duration::from_millis(40);
        let result = attempt_after(&f.state.0, &mut input, deadline, || {
            assert!(f.state.0.try_lock().is_ok());
            assert!(f.native().try_lock().is_ok());
            match fault {
                "version" => f.advance(),
                "adapter" => {
                    let mut slot = f.state.0.lock().unwrap();
                    slot.bridge.as_mut().unwrap().adapter_refused = true;
                }
                "native" => f.native().lock().unwrap().refused = true,
                "holder" => *f.state.1.lock().unwrap() = None,
                "input" => {
                    std::fs::write(f.task.join("reports/startup-cut-request.json"), b"foreign")
                        .unwrap()
                }
                "retention" => {
                    std::fs::remove_file(f.task.join("reports/startup-js.json")).unwrap()
                }
                "deadline" => std::thread::sleep(Duration::from_millis(50)),
                _ => f.state.0.lock().unwrap().close(),
            }
        });
        // Retention health was a point-in-time proof: post-proof file mutation
        // alone requires later terminal H1, not an invented perpetual proof.
        if fault == "retention" {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err(), "{fault}");
        }
        retire(&f.state.0);
        assert_eq!(f.state(), owner::State::Refused);
    }
}
#[test]
fn final_original_native_lock_crossing_deadline_cannot_create_requested_cut() {
    let f = Fixture::new();
    f.publish(&serde_json::to_vec(&f.request()).unwrap());
    let mut input = f.input();
    let native = f.native();
    let deadline = Instant::now() + Duration::from_secs(1);
    let (health_send, health_recv) = std::sync::mpsc::channel();
    let (held_send, held_recv) = std::sync::mpsc::channel();
    let (wait_send, wait_recv) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let slot = &f.state.0;
        let worker = scope.spawn(move || {
            attempt_with_wait(
                slot,
                &mut input,
                deadline,
                || {
                    health_send.send(()).unwrap();
                    held_recv.recv().unwrap();
                },
                || wait_send.send(()).unwrap(),
            )
        });
        health_recv.recv_timeout(Duration::from_secs(2)).unwrap();
        let held = native.lock().unwrap(); // REAL original Arc; receipt stays unchanged.
        held_send.send(()).unwrap();
        wait_recv.recv_timeout(Duration::from_secs(2)).unwrap();
        assert!(Instant::now() < deadline); // Passed the pre-lock deadline check.
        std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
        assert!(Instant::now() >= deadline);
        drop(held);
        assert!(worker.join().unwrap().is_err());
    });
    assert_eq!(f.state(), owner::State::Checking); // No confirm/pending Cut.
    retire(&f.state.0);
    assert_eq!(f.state(), owner::State::Refused);
}
#[test]
fn waiting_missing_input_cancellation_deadline_and_actual_worker_exit_are_bounded() {
    for cancel in [false, true] {
        let f = Fixture::new();
        let work = prepare(&f.state.0, "main", &f.native()).unwrap();
        let exited = work.exited.clone();
        let worker = std::thread::spawn(move || run(work, Duration::from_millis(30)));
        if cancel {
            f.state.0.lock().unwrap().close();
        }
        worker.join().unwrap();
        assert!(exited.load(Ordering::Acquire));
        assert_eq!(f.state(), owner::State::Refused);
        assert!(prepare(&f.state.0, "main", &f.native()).is_err());
    }
    let f = Fixture::new();
    let work = prepare(&f.state.0, "main", &f.native()).unwrap();
    let _guard = f.state.0.lock().unwrap();
    let (send, recv) = std::sync::mpsc::channel();
    let root = work.root.clone();
    let worker = std::thread::spawn(move || send.send(root.read().unwrap().is_none()).unwrap());
    assert!(recv.recv_timeout(Duration::from_secs(1)).unwrap());
    worker.join().unwrap(); // IO needs no Slot/native guard.
}

#[test]
fn no_created_or_foreign_native_missing_owner_and_poison_never_arm() {
    let f = Fixture::new();
    let foreign = Fixture::new();
    assert!(prepare(&f.state.0, "main", &foreign.native()).is_err());
    let f = Fixture::new();
    f.native().lock().unwrap().phase = 0;
    assert!(prepare(&f.state.0, "main", &f.native()).is_err());
    let f = Fixture::new();
    f.state.0.lock().unwrap().owner = None;
    let mut runtime = f.runtime.clone();
    runtime.owner_token = None;
    install(&f.state, &runtime, f.store.clone()).unwrap();
    assert!(f.state.0.lock().unwrap().owner.is_none());
    assert!(prepare(&f.state.0, "main", &f.native()).is_err());
    runtime.owner_token = Some("short".into());
    assert!(install(&f.state, &runtime, f.store.clone()).is_err());
    let native = f.native();
    let slot = f.state.0.clone();
    assert!(std::thread::spawn(move || {
        let _guard = slot.lock().unwrap();
        panic!("slot poison");
    })
    .join()
    .is_err());
    assert!(prepare(&f.state.0, "main", &native).is_err());
}
