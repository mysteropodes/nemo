use super::*;
use serde_json::json;
use std::path::PathBuf;

struct Fixture {
    root: PathBuf,
    native: Arc<Mutex<LoadLog>>,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nemo-host-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let file = std::fs::File::create(root.join("native.json")).unwrap();
        Self {
            root,
            native: Arc::new(Mutex::new(LoadLog::new(
                uuid::Uuid::new_v4().to_string(),
                "a".repeat(64),
                std::process::id(),
                file,
            ))),
        }
    }
    fn slot(&self) -> Slot {
        let mut slot = Slot::default();
        slot.bind(Some(self.native.clone())).unwrap();
        slot
    }
    fn body(&self, sequence: u32, stage: &str) -> InvokeBody {
        InvokeBody::Raw(serde_json::to_vec(&json!({"schema":"nemo.startup-js/1",
            "runId":self.native.lock().unwrap().run, "loadGeneration":1,"sequence":sequence,"stage":stage})).unwrap())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}
fn origin() -> tauri::Url {
    tauri::Url::parse("tauri://localhost/index.html").unwrap()
}
fn accept(slot: &mut Slot, body: &InvokeBody) -> Result<bool, &'static str> {
    slot.admit("main", &origin(), std::process::id(), body)
}
fn records(slot: &Slot) -> Value {
    slot.bridge.as_ref().unwrap().projection()["js"]["records"].clone()
}

#[test]
fn same_native_owner_is_bound_before_early_raw_admission_and_ack_is_literal_true() {
    let f = Fixture::new();
    let mut slot = f.slot();
    assert!(Arc::ptr_eq(
        &slot.bridge.as_ref().unwrap().native,
        &f.native
    ));
    assert_eq!(accept(&mut slot, &f.body(1, "early-script")), Ok(true));
    assert_eq!(
        slot.bridge.as_ref().unwrap().projection()["js"]["status"],
        "provisional"
    );
    {
        let mut native = f.native.lock().unwrap();
        let run = native.run.clone();
        native.confirm_creation("main", std::process::id(), &run);
        native.observe(
            "native-page-started",
            "main",
            &origin(),
            std::process::id(),
            &run,
        );
    }
    assert_eq!(accept(&mut slot, &f.body(2, "dom-ready")), Ok(true));
    assert_eq!(
        slot.bridge.as_ref().unwrap().projection()["js"]["status"],
        "native-start-correlated"
    );
    assert_eq!(
        slot.bridge.as_ref().unwrap().projection()["js"]["sealed"],
        false
    );
    assert_eq!(
        slot.bridge.as_ref().unwrap().projection()["js"]["visiblePaint"],
        "not-observed"
    );
}

#[test]
fn disabled_slot_and_duplicate_disabled_or_owned_binding_cannot_reopen() {
    let f = Fixture::new();
    let mut disabled = Slot::default();
    disabled.bind(None).unwrap();
    assert_eq!(
        accept(&mut disabled, &f.body(1, "early-script")),
        Err("startup disabled")
    );
    assert_eq!(
        disabled.bind(Some(f.native.clone())),
        Err("startup cannot rebind")
    );
    let mut owned = f.slot();
    owned.close();
    assert_eq!(
        owned.bind(Some(f.native.clone())),
        Err("startup cannot rebind")
    );
    assert!(accept(&mut owned, &f.body(1, "early-script")).is_err());
}

#[test]
fn raw_only_no_json_fallback_private_unknown_bytes_and_false_identity_do_not_ack() {
    let f = Fixture::new();
    for body in [
        InvokeBody::Json(json!({"payload":"private"})),
        InvokeBody::Raw(vec![]),
        InvokeBody::Raw(vec![b'x'; 1025]),
        InvokeBody::Raw(b"private-raw-not-json".to_vec()),
    ] {
        let mut slot = f.slot();
        assert!(accept(&mut slot, &body).is_err());
        assert_eq!(records(&slot), json!([]));
        assert_eq!(
            slot.bridge.as_ref().unwrap().projection()["adapterRefused"],
            true
        );
        assert!(!slot
            .bridge
            .as_ref()
            .unwrap()
            .projection()
            .to_string()
            .contains("private"));
    }
    for case in 0..4 {
        let mut slot = f.slot();
        let body = f.body(1, "early-script");
        let label = if case == 0 { "foreign" } else { "main" };
        let url = tauri::Url::parse(match case {
            1 => "about:blank",
            2 => "https://private.example/secret",
            _ => "tauri://localhost/index.html",
        })
        .unwrap();
        let pid = if case == 3 {
            std::process::id() + 1
        } else {
            std::process::id()
        };
        assert!(slot.admit(label, &url, pid, &body).is_err());
        assert_eq!(records(&slot), json!([]));
    }
}

#[test]
fn malformed_duplicate_gap_and_payload_origin_claims_never_mutate_prefix() {
    let f = Fixture::new();
    let mut slot = f.slot();
    let first = f.body(1, "early-script");
    accept(&mut slot, &first).unwrap();
    let prefix = records(&slot);
    assert!(accept(&mut slot, &first).is_err());
    assert_eq!(records(&slot), prefix);
    assert!(accept(&mut slot, &f.body(3, "dom-ready")).is_err());
    assert_eq!(records(&slot), prefix);
    if let InvokeBody::Raw(bytes) = f.body(2, "dom-ready") {
        let mut value: Value = serde_json::from_slice(&bytes).unwrap();
        value["origin"] = json!("tauri://localhost");
        assert!(accept(
            &mut slot,
            &InvokeBody::Raw(serde_json::to_vec(&value).unwrap())
        )
        .is_err());
    }
    assert_eq!(records(&slot), prefix);
}

#[test]
fn native_refusal_navigation_and_destroy_fences_preserve_prefix_irreversibly() {
    for mode in ["native", "destroy"] {
        let f = Fixture::new();
        let mut slot = f.slot();
        accept(&mut slot, &f.body(1, "early-script")).unwrap();
        let prefix = records(&slot);
        if mode == "native" {
            f.native.lock().unwrap().refused = true;
        } else {
            slot.close();
        }
        assert!(accept(&mut slot, &f.body(2, "dom-ready")).is_err());
        assert_eq!(records(&slot), prefix);
        f.native.lock().unwrap().refused = false;
        assert!(accept(&mut slot, &f.body(2, "dom-ready")).is_err());
        assert_eq!(records(&slot), prefix);
        assert_eq!(
            slot.bridge.as_ref().unwrap().projection()["js"]["status"],
            "fenced"
        );
    }
}

#[test]
fn poison_and_failed_native_binding_never_become_enabled_or_acknowledged() {
    let f = Fixture::new();
    let mut slot = f.slot();
    let native = f.native.clone();
    assert!(std::thread::spawn(move || {
        let _guard = native.lock().unwrap();
        panic!("fixture poison");
    })
    .join()
    .is_err());
    let body = InvokeBody::Raw(b"{}".to_vec());
    assert_eq!(accept(&mut slot, &body), Err("startup native unavailable"));
    assert_eq!(records(&slot), json!([]));
    let mut fresh = Slot::default();
    assert!(fresh.bind(Some(f.native.clone())).is_err());
    assert!(!fresh.bound);
}

#[test]
fn concurrent_native_transition_reconciles_without_gui_or_reverse_locking() {
    let f = Fixture::new();
    let state = StartupTransport::default();
    state
        .0
        .lock()
        .unwrap()
        .bind(Some(f.native.clone()))
        .unwrap();
    let thread_slot = state.0.clone();
    let native = f.native.clone();
    let first = f.body(1, "early-script");
    let worker = std::thread::spawn(move || {
        thread_slot
            .lock()
            .unwrap()
            .admit("main", &origin(), std::process::id(), &first)
    });
    {
        let mut log = native.lock().unwrap();
        let run = log.run.clone();
        log.confirm_creation("main", std::process::id(), &run);
        log.observe(
            "native-page-started",
            "main",
            &origin(),
            std::process::id(),
            &run,
        );
    }
    assert_eq!(worker.join().unwrap(), Ok(true));
    assert_eq!(
        state.0.lock().unwrap().admit(
            "main",
            &origin(),
            std::process::id(),
            &f.body(2, "dom-ready")
        ),
        Ok(true)
    );
}
