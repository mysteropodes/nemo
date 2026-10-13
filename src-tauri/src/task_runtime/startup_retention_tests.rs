use super::*;
use serde_json::json;

struct Fixture {
    root: PathBuf,
    value: Value,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("nemo-retention-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let value = json!({"adapterRefused":false,"js":{"schema":"nemo.startup-js-native/1","runId":uuid::Uuid::new_v4().to_string(),
            "taskKey":"a".repeat(64),"pid":std::process::id(),"window":"main","loadGeneration":1,"nativePhase":0,"status":"provisional",
            "accepted":0,"rejected":0,"rejectionOverflow":false,"fence":null,"probeRefused":false,"records":[],"sealed":false,"visiblePaint":"not-observed"}});
        Self { root, value }
    }
    fn store(&self) -> Store {
        Store::open(&self.root, &self.root, &self.value).unwrap()
    }
    fn read(&self) -> Value {
        serde_json::from_slice(&std::fs::read(self.root.join(FILE)).unwrap()).unwrap()
    }
    fn record(&self) -> Value {
        json!({"schema":"nemo.startup-js/1","runId":self.value["js"]["runId"],"loadGeneration":1,"sequence":1,"stage":"early-script"})
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.root).unwrap();
    }
}

#[test]
fn reversed_snapshot_order_and_covered_retry_cannot_erase_newer_aggregate() {
    let f = Fixture::new();
    let store = f.store();
    let mut newer = f.value.clone();
    newer["js"]["records"] = json!([f.record()]);
    newer["js"]["accepted"] = json!(1);
    newer["adapterRefused"] = json!(true);
    newer["js"]["rejected"] = json!(1);
    store.publish(2, &newer).unwrap();
    store.publish(1, &f.value).unwrap();
    store.publish(2, &newer).unwrap();
    assert_eq!(f.read(), newer);
    assert!(std::fs::metadata(f.root.join(FILE)).unwrap().len() <= MAX_BYTES as u64);
    let mut older = f.value.clone();
    older["adapterRefused"] = json!(false);
    assert!(store.publish(3, &older).is_err());
    assert_eq!(f.read(), newer);
    assert!(store.publish(4, &newer).is_err());
}

#[test]
fn private_wrong_owner_sealed_or_oversized_projection_refuses_without_overwrite_and_stays_failed() {
    for case in 0..6 {
        let f = Fixture::new();
        let store = f.store();
        store.publish(1, &f.value).unwrap();
        let mut bad = f.value.clone();
        match case {
            0 => bad["private"] = json!("private-secret"),
            1 => bad["js"]["runId"] = json!(uuid::Uuid::new_v4().to_string()),
            2 => bad["js"]["pid"] = json!(1),
            3 => bad["js"]["sealed"] = json!(true),
            4 => bad["js"]["status"] = json!("private-status"),
            _ => bad["js"]["records"] = json!([{"private":"x".repeat(MAX_BYTES+1)}]),
        }
        assert!(store.publish(2, &bad).is_err());
        assert_eq!(f.read(), f.value);
        assert!(store.publish(3, &f.value).is_err());
        assert!(!std::fs::read_to_string(f.root.join(FILE))
            .unwrap()
            .contains("private"));
    }
}

#[test]
fn foreign_root_symlink_existing_file_and_inode_replacement_are_not_owned() {
    let f = Fixture::new();
    let foreign = Fixture::new();
    assert!(Store::open(&foreign.root, &f.root, &f.value).is_err());
    std::os::unix::fs::symlink(&foreign.root, f.root.join("alias")).unwrap();
    assert!(Store::open(&f.root.join("alias"), &f.root.join("alias"), &f.value).is_err());
    std::fs::write(f.root.join(FILE), b"foreign-private").unwrap();
    assert!(Store::open(&f.root, &f.root, &f.value).is_err());
    assert_eq!(
        std::fs::read(f.root.join(FILE)).unwrap(),
        b"foreign-private"
    );
    std::fs::remove_file(f.root.join(FILE)).unwrap();
    let store = f.store();
    store.publish(1, &f.value).unwrap();
    std::fs::remove_file(f.root.join(FILE)).unwrap();
    std::fs::write(f.root.join(FILE), b"foreign-private").unwrap();
    assert!(store.publish(2, &f.value).is_err());
    assert_eq!(
        std::fs::read(f.root.join(FILE)).unwrap(),
        b"foreign-private"
    );
}

#[test]
fn same_inode_foreign_write_and_poison_fail_closed() {
    let f = Fixture::new();
    let store = f.store();
    store.publish(1, &f.value).unwrap();
    std::fs::write(f.root.join(FILE), b"private-write").unwrap();
    assert!(store.publish(2, &f.value).is_err());
    assert_eq!(std::fs::read(f.root.join(FILE)).unwrap(), b"private-write");
    let p = Fixture::new();
    let poisoned = std::sync::Arc::new(p.store());
    let other = poisoned.clone();
    assert!(std::thread::spawn(move || {
        let _guard = other.disk.lock().unwrap();
        panic!("fixture poison");
    })
    .join()
    .is_err());
    assert!(poisoned.publish(1, &p.value).is_err());
    assert_eq!(std::fs::metadata(p.root.join(FILE)).unwrap().len(), 0);
}

#[test]
fn owned_file_unlinks_normally_while_c1_can_hold_final_read_descriptor() {
    use std::io::Read;
    let f = Fixture::new();
    let store = f.store();
    store.publish(1, &f.value).unwrap();
    let mut reader = std::fs::File::open(f.root.join(FILE)).unwrap();
    let mut newer = f.value.clone();
    newer["adapterRefused"] = json!(true);
    store.publish(2, &newer).unwrap();
    std::fs::remove_file(f.root.join(FILE)).unwrap();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), newer);
    assert!(store.publish(3, &newer).is_err());
}

#[test]
#[cfg(target_os = "macos")]
fn standard_var_alias_is_allowed_without_accepting_real_symlink_roots() {
    let f = Fixture::new();
    let canonical = f.root.to_str().unwrap();
    assert!(canonical.starts_with("/private/var/"));
    let alias = PathBuf::from(canonical.replacen("/private/var/", "/var/", 1));
    let store = Store::open(&alias, &f.root, &f.value).unwrap();
    store.publish(1, &f.value).unwrap();
    assert_eq!(f.read(), f.value);
}

#[test]
fn blocked_file_io_does_not_hold_bridge_or_native_guard() {
    use super::super::{transact, StartupTransport};
    use std::sync::{Arc, Mutex};
    let f = Fixture::new();
    let file = File::create(f.root.join("native.json")).unwrap();
    let native = Arc::new(Mutex::new(super::super::super::LoadLog::new(
        f.value["js"]["runId"].as_str().unwrap().into(),
        "a".repeat(64),
        std::process::id(),
        file,
    )));
    let state = Arc::new(StartupTransport::default());
    let (version, initial) = {
        let mut slot = state.0.lock().unwrap();
        slot.bind(Some(native.clone())).unwrap();
        slot.snapshot().unwrap().unwrap()
    };
    let store = Arc::new(Store::open(&f.root, &f.root, &initial).unwrap());
    store.publish(version, &initial).unwrap();
    *state.1.lock().unwrap() = Some(store.clone());
    let body=tauri::ipc::InvokeBody::Raw(serde_json::to_vec(&json!({"schema":"nemo.startup-js/1","runId":initial["js"]["runId"],"loadGeneration":1,"sequence":1,"stage":"early-script"})).unwrap());
    let io = store.disk.lock().unwrap();
    let worker_state = state.clone();
    let worker = std::thread::spawn(move || {
        transact(
            &worker_state,
            "main",
            Ok(&tauri::Url::parse("tauri://localhost/index.html").unwrap()),
            std::process::id(),
            &body,
        )
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut released = false;
    while std::time::Instant::now() < deadline {
        if let Ok(slot) = state.0.try_lock() {
            if slot.version == 2 {
                released = true;
                break;
            }
        }
        std::thread::yield_now();
    }
    assert!(released);
    assert!(native.try_lock().is_ok());
    drop(io);
    assert_eq!(worker.join().unwrap(), Ok(true));
    std::fs::write(f.root.join(FILE), b"foreign-private-write").unwrap();
    let second=tauri::ipc::InvokeBody::Raw(serde_json::to_vec(&json!({"schema":"nemo.startup-js/1","runId":initial["js"]["runId"],"loadGeneration":1,"sequence":2,"stage":"dom-ready"})).unwrap());
    assert!(transact(
        &state,
        "main",
        Ok(&tauri::Url::parse("tauri://localhost/index.html").unwrap()),
        std::process::id(),
        &second
    )
    .is_err());
    assert_eq!(
        state
            .0
            .lock()
            .unwrap()
            .bridge
            .as_ref()
            .unwrap()
            .projection()["js"]["accepted"],
        2
    );
    assert_eq!(
        std::fs::read(f.root.join(FILE)).unwrap(),
        b"foreign-private-write"
    );
}

#[test]
fn actual_retained_file_is_consumed_by_accepted_c1_as_unsealed_partial() {
    let f = Fixture::new();
    let task = f.root.join("task");
    let reports = task.join("reports");
    let dest = f.root.join("out");
    std::fs::create_dir_all(&reports).unwrap();
    std::fs::create_dir(&dest).unwrap();
    let store = Store::open(&reports, &reports, &f.value).unwrap();
    store.publish(1, &f.value).unwrap();
    let native = json!({"schema":"nemo.startup-observer/1","runId":f.value["js"]["runId"],"taskKey":"a".repeat(64),"pid":std::process::id(),"window":"main",
        "status":"partial","stages":[],"accepted":0,"rejected":0,"dropped":0,"sealed":false,"jsReadiness":"not-observed","visiblePaint":"not-observed"});
    std::fs::write(
        reports.join("startup-observer.json"),
        serde_json::to_vec(&native).unwrap(),
    )
    .unwrap();
    let script = r#"const fs=require('fs'),{prepareCollection}=require(process.argv[1]);
const root=process.argv[2],out=process.argv[3],run=process.argv[4],pid=Number(process.argv[5]);
const pins={runId:run,taskKey:'a'.repeat(64),pid,window:'main',loadGeneration:1,sourceSha:'b'.repeat(40),executableSha256:'c'.repeat(64),buildProofSha256:'d'.repeat(64)};
const handle=prepareCollection(root,pins,out);fs.rmSync(require('path').dirname(root),{recursive:true});
handle.finalize(()=>({schema:'nemo.startup-stop-proof/1',runId:run,taskKey:pins.taskKey,pid,appExited:true,launcherExited:true,processGroupExited:true,rootsRemoved:true,slotsReleased:true}))
.then(receipt=>{if(!handle.closed())process.exitCode=1;process.stdout.write(JSON.stringify(receipt))});"#;
    let helper = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("tests/desktop/startup-collection.cjs");
    let output = std::process::Command::new("node")
        .args(["-e", script])
        .arg(helper)
        .arg(&reports)
        .arg(&dest)
        .arg(f.value["js"]["runId"].as_str().unwrap())
        .arg(std::process::id().to_string())
        .output()
        .unwrap();
    assert!(output.status.success());
    let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(receipt["status"], "partial");
    assert_eq!(receipt["js"], f.value);
    assert_eq!(receipt["sealed"], false);
    assert!(!task.exists());
}

#[test]
fn covered_or_equal_snapshot_cannot_ack_tampered_newest_inode_or_bytes() {
    for replaced in [false, true] {
        for equal in [false, true] {
            let f = Fixture::new();
            let store = f.store();
            let mut newest = f.value.clone();
            newest["adapterRefused"] = json!(true);
            store.publish(2, &newest).unwrap();
            if replaced {
                std::fs::remove_file(f.root.join(FILE)).unwrap();
            }
            std::fs::write(f.root.join(FILE), b"foreign-private").unwrap();
            let (version, snapshot) = if equal { (2, &newest) } else { (1, &f.value) };
            assert!(store.publish(version, snapshot).is_err());
            assert!(store.publish(3, &newest).is_err());
            assert_eq!(
                std::fs::read(f.root.join(FILE)).unwrap(),
                b"foreign-private"
            );
        }
    }
}
