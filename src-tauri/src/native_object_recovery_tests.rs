//! Real-owner round trip, retention, listing, DTO and filesystem controls for the
//! private recovery store. Fault injection lives in the failure test file.
use crate::application_mcp::ApplicationMcp;
use crate::native_application::{admit_release_request, complete_release};
use crate::native_application_contract::NativeReleaseRequest;
use crate::native_dispatch::{NativeObjectHost, ReleaseAdmission};
use crate::native_object_bootstrap::{
    bootstrap_raw,
    recovery::{
        discard_request, list_request, publish, quarantine_request, valid_generation_file,
        valid_slot_key, write_request, Entry, Manifest,
    },
    reopen::bootstrap_request,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) struct Scratch(pub(super) PathBuf);
impl Scratch {
    pub(super) fn new() -> Self {
        let path = std::env::temp_dir().join(format!("nemo-recovery-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    pub(super) fn root(&self) -> PathBuf {
        self.0.join("recovery")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub(super) const DAY: u64 = 24 * 60 * 60 * 1000;

/// A native project file with `objects` closed paths over 20 layers.
pub(super) fn project_bytes(objects: usize) -> String {
    let layers: Vec<_> = (0..20)
        .map(|n| json!({"layerUid":format!("drawing-{n}")}))
        .collect();
    let objects: Vec<_> = (0..objects)
        .map(|i| {
            json!({"schemaVersion":1,"family":"closed-solid-fill-cubic-path",
            "target":{"contextId":"scene-root",
                "frameScope":{"kind":"authored","frame":(i / 20) % 21},
                "layerUid":format!("drawing-{}", i % 20),"strokeId":format!("stroke-{i}")},
            "geometry":{"kind":"cubic-path","closed":true,"coordinateSpace":"document-world",
                "handleSpace":"relative","segments":[
                {"point":{"x":10.125,"y":20},"handleIn":{"x":-2,"y":0},"handleOut":{"x":3.25,"y":-1}},
                {"point":{"x":40,"y":20},"handleIn":{"x":-3,"y":2},"handleOut":{"x":0,"y":6}},
                {"point":{"x":25,"y":50},"handleIn":{"x":4,"y":-2},"handleOut":{"x":-5,"y":-3}}]},
            "fill":{"kind":"solid","r":0.2,"g":0.4,"b":0.6,"a":0.75}})
        })
        .collect();
    json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24},
        "drawing":{"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":21,"layers":layers,"objects":objects},
        "viewportReference":null,"assets":[]})
    .to_string()
}

pub(super) fn install(state: &ApplicationMcp, bytes: &str) -> Value {
    serde_json::to_value(
        bootstrap_raw(
            state,
            &json!({"apiVersion":2,"requestId":"recovery-open",
            "instanceId":state.instance_id(),"documentJson":bytes})
            .to_string(),
        )
        .unwrap(),
    )
    .unwrap()
}

fn release(state: &ApplicationMcp, document: &str, revision: u64) {
    let request: NativeReleaseRequest = serde_json::from_value(json!({"apiVersion":2,
        "requestId":"recovery-release","instanceId":state.instance_id(),
        "documentId":document,"expectedRevision":revision}))
    .unwrap();
    let native = state.native_state();
    let generation = match admit_release_request(&native, &request).unwrap() {
        ReleaseAdmission::Execute { generation } => generation,
        _ => panic!("retained release"),
    };
    complete_release(&native, generation, &request, || {
        Ok((vec![], "already_absent"))
    })
    .unwrap();
}

pub(super) fn info(state: &ApplicationMcp, opened: &Value) -> Value {
    state
        .native_project_info(opened["documentId"].as_str().unwrap())
        .unwrap()
}

pub(super) fn write_json(
    state: &ApplicationMcp,
    info: &Value,
    document: &str,
    slot: &str,
) -> Value {
    json!({"apiVersion":2,"requestId":"recovery-write","instanceId":state.instance_id(),
        "documentId":document,"atRevision":info["contentRevision"],
        "documentSnapshotId":info["documentSnapshotId"],"slotKey":slot,
        "displayName":"Untitled","documentDigest":info["documentDigest"]})
}

pub(super) fn entry<'a>(slot: &'a str, revision: u64) -> Entry<'a> {
    Entry {
        slot_key: slot,
        display_name: "Untitled",
        origin_path: None,
        content_revision: revision,
        document_digest: None,
    }
}

pub(super) fn publish_ok(root: &Path, slot: &str, revision: u64, bytes: &[u8], now: u64) {
    publish(root, &entry(slot, revision), bytes, now, &mut |_, _| Ok(())).unwrap();
}

pub(super) fn project_with(marker: &str) -> Vec<u8> {
    format!(r#"{{"format":"nemo.native-project-document","marker":"{marker}"}}"#).into_bytes()
}

pub(super) fn manifest(root: &Path, slot: &str) -> Manifest {
    serde_json::from_slice(&fs::read(root.join(slot).join("manifest.json")).unwrap()).unwrap()
}

pub(super) fn slot_files(root: &Path, slot: &str) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(root.join(slot))
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

#[test]
fn written_bytes_are_the_save_as_bytes_and_reopen_through_the_real_route() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let opened = install(&state, &project_bytes(40));
    let document = opened["documentId"].as_str().unwrap().to_owned();
    let info = info(&state, &opened);
    let raw = write_json(&state, &info, &document, "untitled-0a1b2c3d").to_string();
    let receipt = serde_json::to_value(
        write_request(&state, &scratch.root(), &raw, 1_000, |_, _| Ok(())).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["published"], true);
    assert_eq!(receipt["cleanupPending"], false);
    assert_eq!(receipt["savedAtMs"], 1_000);
    let expected = state
        .native_project_file_snapshot(&document, 0, info["documentSnapshotId"].as_str().unwrap())
        .unwrap()
        .unwrap();
    assert_eq!(receipt["byteCount"], expected.len());
    let listing = serde_json::to_value(
        list_request(
            &state,
            &scratch.root(),
            &json!({"apiVersion":2,"requestId":"recovery-list","instanceId":state.instance_id()})
                .to_string(),
            2_000,
        )
        .unwrap(),
    )
    .unwrap();
    let generation = &listing["entries"][0]["generations"][0];
    assert_eq!(listing["entries"][0]["slotKey"], "untitled-0a1b2c3d");
    assert_eq!(generation["contentRevision"], 0);
    assert_eq!(generation["documentDigest"], info["documentDigest"]);
    let path = PathBuf::from(generation["path"].as_str().unwrap());
    assert_eq!(fs::read(&path).unwrap(), expected);
    // The strict codec admits the recovery bytes and they carry the same digest.
    let restored = NativeObjectHost::from_project_file("recovery-check".into(), &expected).unwrap();
    assert_eq!(
        restored.project_file_info().unwrap()["documentDigest"],
        info["documentDigest"]
    );
    release(&state, &document, 0);
    // The existing open route accepts a path under the recovery directory.
    let reopen = json!({"apiVersion":2,"requestId":"recovery-reopen",
        "instanceId":state.instance_id(),"sourcePath":path,"projectOnly":true})
    .to_string();
    let reopened = serde_json::to_value(
        bootstrap_request(&state, &reopen, |p| p.starts_with(scratch.root())).unwrap(),
    )
    .unwrap();
    assert_ne!(reopened["documentId"], opened["documentId"]);
    assert_eq!(reopened["contentRevision"], 0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = |p: &Path| fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&path.parent().unwrap().join("manifest.json")), 0o600);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        assert_eq!(mode(&scratch.root()), 0o700);
    }
}

#[test]
fn stale_unsupported_and_foreign_pins_are_data_or_refusals_with_zero_disk_effect() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let opened = install(&state, &project_bytes(3));
    let document = opened["documentId"].as_str().unwrap().to_owned();
    let info = info(&state, &opened);
    let run = |request: Value| {
        write_request(&state, &scratch.root(), &request.to_string(), 5, |_, _| {
            Ok(())
        })
    };
    let mut stale = write_json(&state, &info, &document, "slot-a");
    stale["atRevision"] = json!(7);
    let receipt = serde_json::to_value(run(stale).unwrap()).unwrap();
    assert_eq!(
        (receipt["published"].clone(), receipt["reason"].clone()),
        (json!(false), json!("stale"))
    );
    let mut other_document = write_json(&state, &info, &document, "slot-a");
    other_document["documentId"] = json!("another-document");
    let receipt = serde_json::to_value(run(other_document).unwrap()).unwrap();
    assert_eq!(receipt["reason"], "stale");
    assert!(
        !scratch.root().exists(),
        "no directory is created for a refused pin"
    );
}

#[test]
fn bare_object_documents_are_not_recoverable_projects() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let bare = r#"{"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,"layers":[{"layerUid":"0"}],"objects":[]}"#;
    let opened = install(&state, bare);
    let request = json!({"apiVersion":2,"requestId":"recovery-bare","instanceId":state.instance_id(),
        "documentId":opened["documentId"],"atRevision":0,
        "documentSnapshotId":format!("native-object:{}:0",opened["documentId"].as_str().unwrap()),
        "slotKey":"slot-bare","displayName":"Bare"});
    let receipt = serde_json::to_value(
        write_request(&state, &scratch.root(), &request.to_string(), 5, |_, _| {
            Ok(())
        })
        .unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["published"], false);
    assert_eq!(receipt["reason"], "unsupported");
    assert!(!scratch.root().exists());
}

#[test]
fn strict_request_envelope_rejects_before_any_filesystem_effect() {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let opened = install(&state, &project_bytes(2));
    let document = opened["documentId"].as_str().unwrap().to_owned();
    let info = info(&state, &opened);
    let valid = write_json(&state, &info, &document, "slot-a");
    let mut bad: Vec<String> = vec![
        "[]".into(),
        "null".into(),
        format!("{valid} trailing"),
        format!("{valid}{}", " ".repeat(4097 - valid.to_string().len())),
    ];
    let mut mutate = |key: &str, value: Value| {
        let mut request = valid.clone();
        request[key] = value;
        bad.push(request.to_string());
    };
    mutate("unknown", json!(1));
    mutate("apiVersion", json!(1));
    mutate("instanceId", json!("other-instance"));
    mutate("requestId", json!("bad id"));
    mutate("slotKey", json!("../escape"));
    mutate("slotKey", json!("UPPER"));
    mutate("slotKey", json!("a/b"));
    mutate("slotKey", json!(""));
    mutate("slotKey", json!("x".repeat(65)));
    mutate("displayName", json!(""));
    mutate("displayName", json!("tab\there"));
    mutate("displayName", json!("n".repeat(129)));
    mutate("originPath", json!("relative/path.json"));
    mutate("supersedes", json!("slot-a"));
    mutate("supersedes", json!("../up"));
    mutate("documentDigest", json!("not-a-digest"));
    mutate("documentSnapshotId", json!("bad snapshot"));
    for raw in bad {
        assert!(
            write_request(&state, &scratch.root(), &raw, 5, |_, _| Ok(())).is_err(),
            "{}",
            &raw[..raw.len().min(80)]
        );
    }
    for raw in [
        json!({"apiVersion":2,"requestId":"d","instanceId":state.instance_id(),"slotKey":"../x"}),
        json!({"apiVersion":2,"requestId":"d","instanceId":state.instance_id(),"slotKey":"a","extra":1}),
    ] {
        assert!(discard_request(&state, &scratch.root(), &raw.to_string()).is_err());
    }
    assert!(quarantine_request(
        &state,
        &scratch.root(),
        &json!({"apiVersion":2,"requestId":"q","instanceId":state.instance_id(),
            "slotKey":"slot-a","file":"../../etc/passwd"})
        .to_string(),
        1
    )
    .is_err());
    assert!(!scratch.root().exists());
}

#[test]
fn slot_and_generation_names_are_closed_vocabularies() {
    for ok in ["a", "untitled-0a1b2c3d", "9-x"] {
        assert!(valid_slot_key(ok), "{ok}");
    }
    for bad in ["", "-a", "A", "a.b", "a/b", "..", "a b", "é"] {
        assert!(!valid_slot_key(bad), "{bad}");
    }
    assert!(valid_generation_file("rec-1000-3-0a1b2c3d.json"));
    for bad in [
        "rec-1000-3.json",
        "rec-1000-3-0A1B2C3D.json",
        "rec-1000-3-0a1b2c3.json",
        "../rec-1000-3-0a1b2c3d.json",
        "rec-1000-3-0a1b2c3d.json.corrupt-5",
        "manifest.json",
    ] {
        assert!(!valid_generation_file(bad), "{bad}");
    }
}

#[test]
fn supersedes_removes_the_old_slot_only_after_the_new_one_is_durable() {
    let scratch = Scratch::new();
    let root = scratch.root();
    let state = ApplicationMcp::default();
    let opened = install(&state, &project_bytes(2));
    let document = opened["documentId"].as_str().unwrap().to_owned();
    let info = info(&state, &opened);
    publish_ok(&root, "untitled-aaaa0000", 1, &project_with("old"), 1_000);
    let mut request = write_json(&state, &info, &document, "slot-new");
    request["supersedes"] = json!("untitled-aaaa0000");
    let failed = write_request(&state, &root, &request.to_string(), 2_000, |stage, _| {
        if stage == "manifest-rename" {
            Err(std::io::Error::from_raw_os_error(28))
        } else {
            Ok(())
        }
    });
    assert!(failed.is_err());
    assert!(
        root.join("untitled-aaaa0000").is_dir(),
        "old slot survives a failed write"
    );
    let receipt = serde_json::to_value(
        write_request(&state, &root, &request.to_string(), 3_000, |_, _| Ok(())).unwrap(),
    )
    .unwrap();
    assert_eq!(receipt["supersededRemoved"], true);
    assert!(!root.join("untitled-aaaa0000").exists());
    assert!(root.join("slot-new").join("manifest.json").exists());
}

/// Lock-hold measurement for `native_project_recovery_snapshot` (authority lock
/// held while the project is serialized). Run explicitly:
/// `cargo test --lib recovery_lock_hold -- --ignored --nocapture`.
#[test]
#[ignore]
fn recovery_lock_hold_at_workload_size() {
    let objects: usize = std::env::var("NEMO_RECOVERY_OBJECTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2600);
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let source = scratch.0.join("workload.nemo");
    fs::write(&source, project_bytes(objects)).unwrap();
    let raw = json!({"apiVersion":2,"requestId":"recovery-workload",
        "instanceId":state.instance_id(),"sourcePath":source,"projectOnly":true})
    .to_string();
    let opened = serde_json::to_value(
        bootstrap_request(&state, &raw, |p| p.starts_with(&scratch.0)).unwrap(),
    )
    .unwrap();
    let document = opened["documentId"].as_str().unwrap().to_owned();
    let info = info(&state, &opened);
    let snapshot = info["documentSnapshotId"].as_str().unwrap().to_owned();
    // A contender thread acquires the authority lock in a tight loop and records
    // how long each acquisition blocked: that is the stall a UI command sees.
    let measure = |inline: bool| {
        let stop = std::sync::atomic::AtomicBool::new(false);
        let native = state.native_state();
        let (worst, size, total) = std::thread::scope(|scope| {
            let contender = scope.spawn(|| {
                let mut stalls = Vec::new();
                while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                    let started = std::time::Instant::now();
                    drop(native.lock().unwrap());
                    stalls.push(started.elapsed().as_secs_f64() * 1000.0);
                    std::thread::sleep(std::time::Duration::from_micros(200));
                }
                stalls
            });
            let mut size = 0;
            let started = std::time::Instant::now();
            for _ in 0..30 {
                // Idle gap: a periodic writer is not back-to-back, and an unfair
                // mutex would otherwise starve the contender and hide the stall.
                std::thread::sleep(std::time::Duration::from_millis(5));
                size = if inline {
                    state
                        .native_project_file_snapshot(&document, 0, &snapshot)
                        .unwrap()
                        .unwrap()
                        .len()
                } else {
                    match state
                        .native_project_recovery_snapshot(&document, 0, &snapshot)
                        .unwrap()
                    {
                        crate::native_object_bootstrap::reference::RecoverySnapshot::Bytes(b) => {
                            b.len()
                        }
                        _ => panic!("project snapshot expected"),
                    }
                };
            }
            let total = (started.elapsed().as_secs_f64() * 1000.0 - 30.0 * 5.0) / 30.0;
            stop.store(true, std::sync::atomic::Ordering::Relaxed);
            (contender.join().unwrap(), size, total)
        });
        let mut stalls = worst;
        stalls.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p95 = stalls[stalls.len() * 95 / 100];
        let max = stalls[stalls.len() - 1];
        (size, total, p95, max)
    };
    let (size, total, p95, max) = measure(true);
    println!(
        "RECOVERY_LOCK_HOLD variant=inline objects={objects} bytes={size} serialize={total:.2}ms contender_p95={p95:.2}ms contender_max={max:.2}ms"
    );
    let (size, total, p95, max) = measure(false);
    println!(
        "RECOVERY_LOCK_HOLD variant=detached objects={objects} bytes={size} serialize={total:.2}ms contender_p95={p95:.2}ms contender_max={max:.2}ms"
    );
}

#[path = "native_object_recovery_failure_tests.rs"]
mod failure_tests;
#[path = "native_object_recovery_list_tests.rs"]
mod list_tests;
