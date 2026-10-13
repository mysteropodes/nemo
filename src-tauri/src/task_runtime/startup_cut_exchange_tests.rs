use super::super::{cut::Phase, retention};
use super::*;
use footer::{Ack, Stage};
use serde_json::{json, Value};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use tauri::ipc::InvokeBody;
pub(super) struct Fixture {
    pub(super) root: std::path::PathBuf,
    pub(super) state: StartupTransport,
    pub(super) native: Arc<Mutex<LoadLog>>,
}
pub(super) fn origin() -> tauri::Url {
    tauri::Url::parse("tauri://localhost/index.html").unwrap()
}
impl Fixture {
    pub(super) fn new() -> Self {
        Self::boot(true)
    }
    pub(super) fn boot(requested: bool) -> Self {
        let root = std::env::temp_dir().join(format!("nemo-exchange-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        std::fs::create_dir(root.join("reports")).unwrap();
        std::fs::create_dir(root.join("data")).unwrap();
        let runtime = TaskRuntime {
            task_id: "exchange".into(),
            task_key: "a".repeat(64),
            data_dir: root.join("data"),
            owner_token: Some("private-exchange-fixture-token".into()),
        };
        let mut log = LoadLog::new(
            uuid::Uuid::new_v4().to_string(),
            runtime.task_key.clone(),
            std::process::id(),
            std::fs::File::create(root.join("native.json")).unwrap(),
        );
        let run = log.run.clone();
        log.confirm_creation("main", log.pid, &run);
        for stage in ["native-page-started", "native-page-finished"] {
            log.observe(stage, "main", &origin(), log.pid, &run);
        }
        let native = Arc::new(Mutex::new(log));
        let mut slot = Slot::default();
        slot.bind(Some(native.clone())).unwrap();
        let (version, projection) = slot.snapshot().unwrap().unwrap();
        let reports = root.join("reports");
        let store = Arc::new(retention::Store::open(&reports, &reports, &projection).unwrap());
        store.publish(version, &projection).unwrap();
        let state = StartupTransport(
            Arc::new(Mutex::new(slot)),
            Arc::new(Mutex::new(Some(store.clone()))),
        );
        install(&state, &runtime, store).unwrap();
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(reports.join("startup-cut-request.json"))
            .unwrap();
        file.write_all(&serde_json::to_vec(&json!({"schema":"nemo.startup-owner-cut/1","ownerToken":runtime.owner_token,"runId":run,"taskKey":runtime.task_key,"pid":std::process::id(),"window":"main","loadGeneration":1})).unwrap()).unwrap();
        let work = prepare(&state.0, "main", &native).unwrap();
        if requested {
            super::run(work, Duration::from_secs(1));
        }
        Self {
            root,
            state,
            native,
        }
    }
    pub(super) fn body(&self) -> InvokeBody {
        let mut slot = self.state.0.lock().unwrap();
        let id = slot
            .owner
            .as_mut()
            .unwrap()
            .session
            .pending()
            .unwrap()
            .id()
            .to_owned();
        InvokeBody::Raw(serde_json::to_vec(&json!({"runId":self.native.lock().unwrap().run,"loadGeneration":1,"cutId":id,"status":"drained","generated":0,"sent":0,"failed":0,"dropped":0,"late":0,"pending":0,"visiblePaint":"not-observed"})).unwrap())
    }
    pub(super) fn ack(&self, kind: Ack, body: &InvokeBody) -> Result<bool, &'static str> {
        footer::acknowledge(
            &self.state,
            "main",
            Ok(&origin()),
            std::process::id(),
            body,
            kind,
        )
    }
    pub(super) fn drain(&self) -> dispatch::Payload {
        dispatch::next(&self.state).unwrap().unwrap()
    }
    pub(super) fn free(&self) {
        assert!(self.state.0.try_lock().is_ok());
        assert!(self.native.try_lock().is_ok());
    }
    fn failed(&self) {
        let slot = self.state.0.lock().unwrap();
        assert!(slot
            .owner
            .as_ref()
            .is_none_or(|b| b.session.state() == owner::State::Refused));
        assert!(slot.bridge.as_ref().unwrap().adapter_refused);
        let bytes = std::fs::read(self.root.join("reports/startup-js.json")).unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&bytes).unwrap()["adapterRefused"],
            true
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        retire(&self.state.0);
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
#[test]
fn both_real_footer_cores_only_ack_registered_pending_draining_stages() {
    let f = Fixture::new();
    let before = std::fs::read(f.root.join("reports/startup-js.json")).unwrap();
    f.drain();
    let body = f.body();
    assert_eq!(f.ack(Ack::Drain, &body), Ok(true));
    assert_eq!(f.ack(Ack::Drain, &body), Ok(true));
    let payload = dispatch::next(&f.state).unwrap().unwrap();
    assert_eq!(serde_json::to_value(payload).unwrap()["phase"], "snapshot");
    assert_eq!(f.ack(Ack::Snapshot, &body), Ok(true));
    assert_eq!(f.ack(Ack::Snapshot, &body), Ok(true));
    assert_eq!(
        before,
        std::fs::read(f.root.join("reports/startup-js.json")).unwrap()
    );
    let mut slot = f.state.0.lock().unwrap();
    let binding = slot.owner.as_mut().unwrap();
    assert_eq!(binding.session.pending().unwrap().phase(), Phase::Draining);
}
#[test]
fn raw_shape_sender_stage_duplicate_and_current_owner_negatives_are_sticky() {
    for fault in [
        "json",
        "null",
        "array",
        "empty",
        "oversize",
        "extra",
        "duplicate",
        "pid",
        "window",
        "origin",
        "snapshot-first",
        "requested",
        "foreign-native",
        "holder",
        "terminal",
        "runId",
        "loadGeneration",
        "cutId",
        "generated",
        "late",
        "status",
        "wrongtype",
        "no-owner",
        "poison-native",
    ] {
        let f = Fixture::new();
        if fault != "requested" {
            f.drain();
        }
        let mut body = f.body();
        match fault {
            "json" => body = InvokeBody::Json(json!({})),
            "null" | "array" | "empty" => {
                body = InvokeBody::Raw(match fault {
                    "null" => b"null".to_vec(),
                    "array" => b"[]".to_vec(),
                    _ => vec![],
                })
            }
            "oversize" => body = InvokeBody::Raw(vec![0; 1025]),
            "extra" | "duplicate" => {
                if let InvokeBody::Raw(bytes) = &mut body {
                    let field = if fault == "duplicate" {
                        &b"\"runId\":\"foreign\","[..]
                    } else {
                        &b"\"extra\":1,"[..]
                    };
                    bytes.splice(1..1, field.iter().copied());
                }
            }
            "runId" | "loadGeneration" | "cutId" | "generated" | "late" | "status"
            | "wrongtype" => {
                if let InvokeBody::Raw(bytes) = &mut body {
                    let mut v: Value = serde_json::from_slice(bytes).unwrap();
                    let key = if fault == "wrongtype" {
                        "loadGeneration"
                    } else {
                        fault
                    };
                    v[key] = match fault {
                        "loadGeneration" => json!(2),
                        "generated" => json!(-1),
                        "late" => json!(u64::MAX),
                        _ => json!("foreign"),
                    };
                    *bytes = serde_json::to_vec(&v).unwrap();
                }
            }
            "no-owner" => f.state.0.lock().unwrap().owner = None,
            "poison-native" => {
                let n = f.native.clone();
                let _ = std::thread::spawn(move || {
                    let _guard = n.lock().unwrap();
                    panic!("fixture native poison");
                })
                .join();
            }
            "foreign-native" => {
                let other = Fixture::new();
                f.state.0.lock().unwrap().bridge.as_mut().unwrap().native = other.native.clone();
            }
            "holder" => *f.state.1.lock().unwrap() = None,
            "terminal" => f.state.0.lock().unwrap().close(),
            _ => {}
        }
        let kind = if fault == "snapshot-first" {
            Ack::Snapshot
        } else {
            Ack::Drain
        };
        let label = if fault == "window" { "other" } else { "main" };
        let url = if fault == "origin" {
            tauri::Url::parse("https://example.invalid/").unwrap()
        } else {
            origin()
        };
        let pid = if fault == "pid" {
            1
        } else {
            std::process::id()
        };
        assert!(
            footer::acknowledge(&f.state, label, Ok(&url), pid, &body, kind).is_err(),
            "{fault}"
        );
        f.failed();
        assert!(dispatch::next(&f.state).is_err());
    }
}
#[test]
fn h1_reconfirmation_detects_slot_native_stage_and_input_health_changes() {
    for fault in ["version", "native", "stage", "close", "deadline", "disk"] {
        let f = Fixture::new();
        f.drain();
        let result = footer::acknowledge_after(
            &f.state,
            "main",
            Ok(&origin()),
            std::process::id(),
            &f.body(),
            Ack::Drain,
            || {
                f.free();
                match fault {
                    "version" => {
                        f.state.0.lock().unwrap().snapshot().unwrap();
                    }
                    "native" => f.native.lock().unwrap().refused = true,
                    "stage" => f.state.0.lock().unwrap().exchange.stage = Stage::WaitSnapshot,
                    "close" => f.state.0.lock().unwrap().close(),
                    "deadline" => {
                        f.state.0.lock().unwrap().owner.as_mut().unwrap().deadline =
                            Some(Instant::now())
                    }
                    _ => {
                        std::fs::remove_file(f.root.join("reports/startup-js.json")).unwrap();
                    } // Point H1 only.
                }
            },
        );
        if fault == "disk" {
            assert_eq!(result, Ok(true));
        } else {
            assert!(result.is_err());
            f.failed();
        }
    }
    let f = Fixture::new();
    f.drain();
    std::fs::remove_file(f.root.join("reports/startup-js.json")).unwrap();
    assert!(f.ack(Ack::Drain, &f.body()).is_err());
    assert!(f.state.0.lock().unwrap().retention_failed);
}
#[test]
fn actual_dispatch_workers_emit_outside_guards_and_exit_without_rearming() {
    for fault in ["none", "emit", "deadline", "cancel"] {
        let f = Fixture::new();
        if fault == "deadline" {
            f.state.0.lock().unwrap().owner.as_mut().unwrap().deadline = Some(Instant::now());
        }
        let work = dispatch::prepare(&f.state, "main", &origin(), std::process::id()).unwrap();
        let exited = f.state.0.lock().unwrap().exchange.exited.clone();
        let mut phases = vec![];
        std::thread::scope(|scope| {
            scope
                .spawn(|| {
                    dispatch::run(work, |payload| {
                        f.free();
                        let phase = serde_json::to_value(payload).unwrap()["phase"]
                            .as_str()
                            .unwrap()
                            .to_owned();
                        phases.push(phase.clone());
                        if fault == "emit" {
                            return Err(());
                        }
                        if fault == "cancel" {
                            f.state.0.lock().unwrap().close();
                            return Ok(());
                        }
                        f.ack(
                            if phase == "drain" {
                                Ack::Drain
                            } else {
                                Ack::Snapshot
                            },
                            &f.body(),
                        )
                        .unwrap();
                        Ok(())
                    })
                })
                .join()
                .unwrap()
        });
        assert!(exited.load(Ordering::Acquire));
        assert!(dispatch::prepare(&f.state, "main", &origin(), std::process::id()).is_err());
        if fault == "none" {
            assert_eq!(phases, vec!["drain", "snapshot"]);
        } else {
            f.failed();
        }
    }
}
