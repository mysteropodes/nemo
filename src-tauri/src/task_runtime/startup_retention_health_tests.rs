use super::super::super::{cut::Cut, Bridge, LoadLog};
use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::sync::Arc;

struct Fixture {
    top: PathBuf,
    root: PathBuf,
    bridge: Bridge,
    store: Arc<Store>,
}
impl Fixture {
    fn new() -> Self {
        let top = std::env::temp_dir().join(format!("nemo-health-{}", uuid::Uuid::new_v4()));
        let root = top.join("owner/task/reports");
        std::fs::create_dir_all(&root).unwrap();
        let root = std::fs::canonicalize(root).unwrap();
        let top = std::fs::canonicalize(top).unwrap();
        let file = File::create(root.join("native.json")).unwrap();
        let native = Arc::new(Mutex::new(LoadLog::new(
            uuid::Uuid::new_v4().to_string(),
            "a".repeat(64),
            std::process::id(),
            file,
        )));
        let bridge = Bridge::new(native).unwrap();
        let store = Arc::new(Store::open(&root, &root, &bridge.projection()).unwrap());
        Self {
            top,
            root,
            bridge,
            store,
        }
    }
    fn path(&self) -> PathBuf {
        self.root.join(FILE)
    }
    fn publish(&self, version: u64) -> Value {
        let snapshot = self.bridge.projection();
        self.store.publish(version, &snapshot).unwrap();
        snapshot
    }
    fn advance(&mut self) {
        let projection = self.bridge.stream.projection();
        let record = serde_json::json!({"schema":"nemo.startup-js/1",
            "runId":projection["runId"],"loadGeneration":1,"sequence":1,"stage":"early-script"});
        assert_eq!(
            self.bridge.admit(
                "main",
                &tauri::Url::parse("tauri://localhost/index.html").unwrap(),
                std::process::id(),
                &tauri::ipc::InvokeBody::Raw(serde_json::to_vec(&record).unwrap())
            ),
            Ok(true)
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.top);
    }
}

#[test]
fn actual_store_health_creates_native_bound_kn_input_without_writing_or_advancing() {
    let f = Fixture::new();
    assert!(f.store.current_health(1, &f.bridge.projection()).is_err());
    let value = f.publish(1);
    let path = f.path();
    let bytes = std::fs::read(&path).unwrap();
    let before = std::fs::metadata(&path).unwrap();
    for _ in 0..2 {
        let proof = f.store.current_health(1, &value).unwrap();
        assert_eq!(proof.run, value["js"]["runId"]);
        assert_eq!(proof.task, "a".repeat(64));
        assert_eq!(proof.pid, std::process::id());
        assert_eq!(proof.version, 1);
        let native = f.bridge.native.lock().unwrap();
        assert!(Cut::request(&native, &f.bridge.stream, Some(&proof)).is_ok());
    }
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert_eq!(stamp(&std::fs::metadata(f.path()).unwrap()), stamp(&before));
    assert_eq!(f.store.disk.lock().unwrap().version, 1);
    assert_eq!(value["js"]["sealed"], false); // no collector/schema seal
}

#[test]
fn actual_b1_prefix_publish_invalidates_old_expectation_without_poisoning_current_store() {
    let mut f = Fixture::new();
    let old = f.publish(1);
    f.advance();
    let new = f.publish(2);
    assert!(f.store.current_health(1, &old).is_err());
    assert!(f.store.current_health(2, &old).is_err());
    assert!(f.store.current_health(1, &new).is_err());
    let proof = f.store.current_health(2, &new).unwrap();
    assert_eq!(proof.version, 2);
    assert_eq!(new["js"]["accepted"], 1);
    let native = f.bridge.native.lock().unwrap();
    assert!(Cut::request(&native, &f.bridge.stream, Some(&proof)).is_ok());
    assert!(!f.store.disk.lock().unwrap().failed);
}

#[test]
fn wrong_expected_owner_shape_zero_version_and_excess_bytes_cannot_create_health() {
    let f = Fixture::new();
    let value = f.publish(1);
    assert!(f.store.current_health(0, &value).is_err());
    for key in [
        "runId",
        "taskKey",
        "pid",
        "window",
        "loadGeneration",
        "sealed",
    ] {
        let mut bad = value.clone();
        bad["js"][key] = serde_json::json!("private-owner");
        assert!(f.store.current_health(1, &bad).is_err());
    }
    let mut huge = value.clone();
    huge["private"] = serde_json::json!("x".repeat(MAX_BYTES));
    assert!(f.store.current_health(1, &huge).is_err());
    assert!(f.store.current_health(1, &value).is_ok());
}

#[test]
fn actual_same_inode_bytes_size_or_mode_tamper_stays_failed_after_restoration() {
    for case in ["bytes", "truncate", "grow", "mode"] {
        let f = Fixture::new();
        let value = f.publish(1);
        let path = f.path();
        let original = std::fs::read(&path).unwrap();
        let inode = std::fs::metadata(&path).unwrap().ino();
        match case {
            "bytes" => {
                let mut bytes = original.clone();
                bytes[0] = b'[';
                std::fs::write(&path, bytes).unwrap();
            }
            "truncate" => std::fs::write(&path, b"").unwrap(),
            "grow" => {
                let mut bytes = original.clone();
                bytes.push(b' ');
                std::fs::write(&path, bytes).unwrap();
            }
            _ => std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap(),
        }
        assert_eq!(std::fs::metadata(&path).unwrap().ino(), inode);
        let foreign = std::fs::read(&path).unwrap();
        assert!(f.store.current_health(1, &value).is_err(), "{case}");
        assert_eq!(std::fs::read(&path).unwrap(), foreign); // never correct foreign content
        std::fs::write(&path, original).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        assert!(f.store.current_health(1, &value).is_err());
        assert!(f.store.publish(2, &value).is_err());
    }
}

#[test]
fn held_descriptor_cannot_attest_replaced_missing_symlink_or_renamed_owned_path() {
    for case in ["replace", "symlink", "missing", "reports", "ancestor"] {
        let f = Fixture::new();
        let value = f.publish(1);
        let path = f.path();
        let original = std::fs::read(&path).unwrap();
        match case {
            "replace" | "symlink" | "missing" => {
                let saved = f.root.join("saved.json");
                std::fs::rename(&path, &saved).unwrap();
                if case == "replace" {
                    std::fs::write(&path, &original).unwrap();
                    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))
                        .unwrap();
                } else if case == "symlink" {
                    symlink(saved, &path).unwrap();
                }
            }
            "reports" => {
                let saved = f.root.with_file_name("held-reports");
                std::fs::rename(&f.root, &saved).unwrap();
                symlink(saved, &f.root).unwrap();
            }
            _ => {
                let owner = f.top.join("owner");
                let saved = f.top.join("held-owner");
                std::fs::rename(&owner, &saved).unwrap();
                symlink(saved, owner).unwrap();
                // Final named file and its immediate parents retain exact inode,
                // so only canonical ancestor validation catches this alias.
                assert_eq!(std::fs::metadata(&path).unwrap().ino(), f.store.ino);
            }
        }
        assert!(f.store.current_health(1, &value).is_err(), "{case}");
        assert!(f.store.disk.lock().unwrap().failed);
        assert!(f.store.publish(2, &value).is_err());
    }
}

#[test]
fn actual_store_mutex_serializes_health_and_file_tamper_remains_visible() {
    let f = Fixture::new();
    let value = f.publish(1);
    let guard = f.store.disk.lock().unwrap();
    let store = f.store.clone();
    let copy = value.clone();
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let other = barrier.clone();
    let worker = std::thread::spawn(move || {
        other.wait();
        store.current_health(1, &copy).is_err()
    });
    barrier.wait();
    let original = std::fs::read(f.path()).unwrap();
    let mut foreign = original.clone();
    foreign[0] = b'[';
    std::fs::write(f.path(), &foreign).unwrap();
    drop(guard);
    assert!(worker.join().unwrap());
    assert_eq!(std::fs::read(f.path()).unwrap(), foreign);
    assert!(f.store.current_health(1, &value).is_err());
}

#[test]
fn replacement_reports_directory_with_original_file_hardlink_must_refuse() {
    let f = Fixture::new();
    let value = f.publish(1);
    let saved = f.root.with_file_name("original-reports");
    std::fs::rename(&f.root, &saved).unwrap();
    std::fs::create_dir(&f.root).unwrap();
    std::fs::hard_link(saved.join(FILE), f.path()).unwrap();
    assert_eq!(std::fs::metadata(f.path()).unwrap().ino(), f.store.ino);
    assert!(f.store.current_health(1, &value).is_err());
}

#[test]
fn replacement_task_parent_cannot_attest_the_original_reports_inode() {
    let f = Fixture::new();
    let value = f.publish(1);
    let task = f.root.parent().unwrap().to_owned();
    let saved = task.with_file_name("original-task");
    std::fs::rename(&task, &saved).unwrap();
    std::fs::create_dir(&task).unwrap();
    std::fs::rename(saved.join("reports"), &f.root).unwrap();
    assert_eq!(std::fs::metadata(f.path()).unwrap().ino(), f.store.ino);
    assert!(f.store.current_health(1, &value).is_err());
    assert!(f.store.disk.lock().unwrap().failed);
}

#[test]
fn root_capture_verification_refuses_replacement_in_the_open_interval() {
    let f = Fixture::new();
    let captured = RootIdentity::capture(&f.root).unwrap();
    let saved = f.root.with_file_name("original-reports");
    std::fs::rename(&f.root, saved).unwrap();
    std::fs::create_dir(&f.root).unwrap();
    assert!(captured.verify(&f.root).is_err());
}

#[test]
fn poisoned_or_already_failed_store_never_issues_proof_and_unrepresentable_pid_refuses() {
    let f = Fixture::new();
    let value = f.publish(1);
    let store = f.store.clone();
    assert!(std::thread::spawn(move || {
        let _guard = store.disk.lock().unwrap();
        panic!("fixture poison");
    })
    .join()
    .is_err());
    assert!(f.store.current_health(1, &value).is_err());
    let mut f = Fixture::new();
    let store = Arc::get_mut(&mut f.store).unwrap();
    store.pid = u64::from(u32::MAX) + 1;
    let mut value = f.bridge.projection();
    value["js"]["pid"] = serde_json::json!(store.pid);
    store.publish(1, &value).unwrap();
    assert!(store.current_health(1, &value).is_err());
    assert!(store.disk.lock().unwrap().failed);
}
