use crate::task_runtime::object_window_probe::control::{complete, read_ack};
use crate::task_runtime::object_window_probe::{
    bind_export_scope, failure_stage_allowed, gate, origin, scoped_export_destination,
    scoped_export_root, sink, Packet, Session, PHASES,
};
use serde_json::json;
use std::{cell::RefCell, collections::HashSet, fs, path::PathBuf};

struct Fixture {
    root: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nemo-object-probe-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self {
            root: fs::canonicalize(root).unwrap(),
        }
    }
    fn session(&self) -> Session {
        Session {
            run: "11111111-1111-4111-8111-111111111111".into(),
            source: "a".repeat(40),
            task: "b".repeat(64),
            pid: 42,
            owner: "private-owner".into(),
            root: self.root.clone(),
            export_destination: None,
            sink: sink(&self.root).unwrap(),
            cursor: 0,
            waiting: false,
            failed: false,
            records: vec![],
            export: false,
        }
    }
    fn ack(&self, value: serde_json::Value) {
        fs::write(
            self.root.join("object-window-probe-control.json"),
            value.to_string(),
        )
        .unwrap();
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn packet(
    run: &str,
    phase: &str,
    status: &str,
    revision: Option<u64>,
    work: Option<&str>,
) -> Packet {
    serde_json::from_value(
        json!({"runId":run,"phase":phase,"status":status,"revision":revision,"workId":work}),
    )
    .unwrap()
}
#[test]
fn default_off_and_every_source_owner_or_window_gate_refuse_before_attach() {
    assert_eq!(
        gate(false, false, false, false, false, false, "", ""),
        Ok(false)
    );
    let hash = "a".repeat(40);
    assert_eq!(
        gate(true, true, true, true, true, true, &hash, &hash),
        Ok(true)
    );
    for index in 0..5 {
        let mut flags = [true; 5];
        flags[index] = false;
        assert!(
            gate(true, flags[0], flags[1], flags[2], flags[3], flags[4], &hash, &hash).is_err()
        );
    }
    assert!(gate(true, true, true, true, true, true, &hash, &"b".repeat(40)).is_err());
    assert!(gate(true, true, true, true, true, true, "dirty", "dirty").is_err());
    assert!(origin(
        "main",
        &tauri::Url::parse("tauri://localhost/index.html").unwrap()
    ));
    for (label, url) in [
        ("other", "tauri://localhost/index.html"),
        ("main", "https://localhost/index.html"),
        ("main", "tauri://localhost/index.html?eval=1"),
        ("main", "tauri://localhost/other.html"),
        ("main", "tauri://localhost:12/index.html"),
    ] {
        assert!(!origin(label, &tauri::Url::parse(url).unwrap()));
    }
}
#[test]
fn exact_sequence_requires_presented_revision_and_external_ack_before_continuation() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let run = session.run.clone();
    assert!(session
        .admit(&packet("foreign", "ready", "ready", None, None))
        .is_err());
    assert!(session
        .admit(&packet(&run, "red", "presented", Some(1), Some("work")))
        .is_err());
    assert!(session
        .admit(&packet(&run, "ready", "ready", Some(0), None))
        .is_err());
    for (index, phase) in PHASES.iter().enumerate() {
        let frame = (1..5).contains(&index);
        let status = if index == 0 {
            "ready"
        } else if index == 5 {
            "succeeded"
        } else {
            "presented"
        };
        let row = packet(
            &run,
            phase,
            status,
            frame.then_some(index.saturating_sub(1) as u64),
            frame.then_some("work-current"),
        );
        if frame {
            assert!(session
                .admit(&packet(
                    &run,
                    phase,
                    "deferred-timeout",
                    row.revision,
                    Some("work")
                ))
                .is_err());
            assert!(session
                .admit(&packet(&run, phase, "presented", Some(99), Some("work")))
                .is_err());
        }
        assert_eq!(session.admit(&row), Ok(true));
        assert!(session.waiting);
        assert!(session.admit(&row).is_err());
        assert!(complete(&mut session, "foreign", index, Ok(())).is_err());
        assert!(complete(&mut session, &run, index + 1, Ok(())).is_err());
        assert!(session.waiting);
        complete(&mut session, &run, index, Ok(())).unwrap();
    }
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("object-window-probe.json")).unwrap())
            .unwrap();
    assert_eq!(receipt["complete"], true);
    assert_eq!(receipt["records"].as_array().unwrap().len(), 6);
    assert!(!receipt.to_string().contains("private-owner"));
    assert!(session
        .admit(&packet(&run, "ready", "ready", None, None))
        .is_err());
}
#[test]
fn failure_is_bounded_terminal_and_cleanup_can_be_recorded_after_native_timeout() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    let run = session.run.clone();
    let stage = "preview-invoke:invalid_request";
    let mut extra = packet(&run, "ready", "ready", None, None);
    extra.failure_stage = Some(stage.into());
    assert!(session.admit(&extra).is_err());
    assert!(!failure_stage_allowed("arbitrary"));
    assert!(!failure_stage_allowed("preview-invoke:arbitrary"));
    assert!(!failure_stage_allowed("preview-receipt:invalid_request"));
    session
        .admit(&packet(&run, "ready", "ready", None, None))
        .unwrap();
    assert!(complete(&mut session, &run, 0, Err("probe checkpoint timed out")).is_err());
    assert!(session.failed);
    assert!(!session.waiting);
    let mut fail = packet(&run, "failed", "operation_failed", None, None);
    fail.cleanup_succeeded = Some(true);
    assert!(session.admit(&fail).is_err());
    fail.failure_stage = Some(stage.into());
    assert_eq!(session.admit(&fail), Ok(false));
    assert!(session.failed);
    assert!(session.admit(&fail).is_err());
    assert!(session
        .admit(&packet(&run, "ready", "ready", None, None))
        .is_err());
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("object-window-probe.json")).unwrap())
            .unwrap();
    assert_eq!(receipt["records"][1]["cleanupSucceeded"], true);
    assert_eq!(receipt["records"][1]["failureStage"], stage);
    assert_eq!(receipt["complete"], false);
    assert!(serde_json::from_str::<Packet>(
        "{\"runId\":\"x\",\"runId\":\"y\",\"phase\":\"ready\",\"status\":\"ready\"}"
    )
    .is_err());
    assert!(serde_json::from_value::<Packet>(
        json!({"runId":"x","phase":"ready","status":"ready","script":"arbitrary"})
    )
    .is_err());
}
#[test]
fn control_is_data_only_owned_run_source_checkpoint_and_rejects_unsafe_paths() {
    let fixture = Fixture::new();
    let source = "a".repeat(40);
    let run = "owned-run";
    let read = || read_ack(&fixture.root, run, &source, 2, "private-owner");
    assert_eq!(read(), Ok(false));
    let value = json!({"runId":run,"sourceSha":source,"checkpoint":2,"ownerToken":"private-owner","screenshotSha256":"b".repeat(64)});
    fixture.ack(value.clone());
    assert_eq!(read(), Ok(true));
    for field in ["runId", "sourceSha", "ownerToken", "screenshotSha256"] {
        let mut wrong = value.clone();
        wrong[field] = json!("wrong");
        fixture.ack(wrong);
        assert!(read().is_err(), "{field}");
    }
    let mut stale = value.clone();
    stale["checkpoint"] = json!(1);
    fixture.ack(stale);
    assert_eq!(read(), Ok(false));
    let mut future = value.clone();
    future["checkpoint"] = json!(3);
    fixture.ack(future);
    assert!(read().is_err());
    let mut arbitrary = value.clone();
    arbitrary["eval"] = json!("code");
    fixture.ack(arbitrary);
    assert!(read().is_err());
    fs::write(
        fixture.root.join("object-window-probe-control.json"),
        " ".repeat(2049),
    )
    .unwrap();
    assert!(read().is_err());
    fs::remove_file(fixture.root.join("object-window-probe-control.json")).unwrap();
    fs::create_dir(fixture.root.join("object-window-probe-control.json")).unwrap();
    assert!(read().is_err());
}
#[cfg(unix)]
#[test]
fn fresh_receipt_and_control_refuse_symlink_roots_and_leafs() {
    let fixture = Fixture::new();
    drop(fixture.session());
    assert!(sink(&fixture.root).is_err());
    let alias = fixture.root.with_extension("alias");
    std::os::unix::fs::symlink(&fixture.root, &alias).unwrap();
    assert!(sink(&alias).is_err());
    fs::remove_file(alias).unwrap();
    std::os::unix::fs::symlink(
        fixture.root.join("object-window-probe.json"),
        fixture.root.join("object-window-probe-control.json"),
    )
    .unwrap();
    assert!(read_ack(
        &fixture.root,
        "owned-run",
        &"a".repeat(40),
        0,
        "private-owner"
    )
    .is_err());
}
#[cfg(unix)]
#[test]
fn export_destination_requires_the_task_bound_scoped_temp_root() {
    let fixture = Fixture::new();
    let report = fixture.root.join("reports");
    let temp = fixture.root.join("tmp");
    let foreign = fixture.root.join("foreign");
    fs::create_dir(&report).unwrap();
    fs::create_dir(&temp).unwrap();
    fs::create_dir(&foreign).unwrap();
    assert_eq!(scoped_export_root(&report, &temp), Ok(temp.clone()));
    let run = "11111111-1111-4111-8111-111111111111";
    let destination = temp.join(format!("object-export-{run}"));
    assert_eq!(
        scoped_export_destination(&report, &temp, run),
        Ok(destination.clone())
    );
    assert!(scoped_export_destination(&report, &temp, "../foreign").is_err());
    assert!(
        scoped_export_destination(&report, &temp, "11111111-1111-4111-8111-11111111111A").is_err()
    );
    let grants = RefCell::new(HashSet::<PathBuf>::new());
    let is_allowed = |path: &std::path::Path| grants.borrow().contains(path);
    let allow_file = |path: &std::path::Path| {
        grants.borrow_mut().insert(path.to_path_buf());
        Ok(())
    };
    bind_export_scope(&destination, is_allowed, allow_file).unwrap();
    assert_eq!(grants.borrow().len(), 1);
    assert!(is_allowed(&destination));
    assert!(!is_allowed(&temp));
    assert!(!is_allowed(&temp.join("object-export-other")));
    assert!(!is_allowed(&destination.join("frame_0001.png")));
    assert!(!is_allowed(&foreign));
    std::os::unix::fs::symlink(&foreign, temp.join("escape")).unwrap();
    assert!(!is_allowed(&temp.join("escape/frame_0001.png")));
    grants.borrow_mut().insert(temp.clone());
    assert!(bind_export_scope(&destination, is_allowed, allow_file).is_err());
    assert!(scoped_export_root(&report, &report).is_err());
    assert!(scoped_export_root(&report, &foreign).is_err());
    std::os::unix::fs::symlink(&foreign, &destination).unwrap();
    assert!(scoped_export_destination(&report, &temp, run).is_err());
    fs::remove_file(&destination).unwrap();
    #[cfg(target_os = "macos")]
    {
        let alias = temp.to_string_lossy().replacen("/private/var/", "/var/", 1);
        assert_eq!(
            scoped_export_root(&report, PathBuf::from(alias).as_path()),
            Ok(temp.clone())
        );
    }
    fs::remove_file(temp.join("escape")).unwrap();
    fs::remove_dir(&temp).unwrap();
    std::os::unix::fs::symlink(&foreign, &temp).unwrap();
    assert!(scoped_export_root(&report, &temp).is_err());
}

#[test]
fn fixed_export_mode_requires_refusal_published_manifest_and_release_ack() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    session.export = true;
    session.persist().unwrap();
    let run = session.run.clone();
    let export = json!({"apiVersion":2,"requestId":format!("object-export-{run}"),
        "instanceId":"owned-instance","documentId":"owned-doc","contentRevision":0,
        "documentSnapshotId":"owned-snapshot","status":"succeeded","cleanupStatus":"complete",
        "externalEffectDisposition":"committed","errorCode":null,
        "artifactFiles":["frame_0001.png"],"published":true,"cleanupComplete":true});
    let row = |phase: &str,
               status: &str,
               revision: Option<u64>,
               work: Option<&str>,
               evidence: Option<serde_json::Value>,
               cleanup: Option<bool>| {
        serde_json::from_value::<Packet>(json!({"runId":run,"phase":phase,"status":status,
            "revision":revision,"workId":work,"export":evidence,"cleanupSucceeded":cleanup}))
        .unwrap()
    };
    assert!(session
        .admit(&row(
            "exported",
            "succeeded",
            Some(0),
            None,
            Some(export.clone()),
            None
        ))
        .is_err());
    for (index, packet) in [
        row("ready", "ready", None, None, None, None),
        row(
            "blue",
            "presented",
            Some(0),
            Some("native-work-1"),
            None,
            None,
        ),
        row("refused", "refused", Some(0), None, None, None),
        row(
            "exported",
            "succeeded",
            Some(0),
            None,
            Some(export.clone()),
            None,
        ),
        row("released", "succeeded", None, None, None, Some(true)),
    ]
    .iter()
    .enumerate()
    {
        if index == 3 {
            let mut false_claim = export.clone();
            false_claim["published"] = json!(false);
            assert!(session
                .admit(&row(
                    "exported",
                    "succeeded",
                    Some(0),
                    None,
                    Some(false_claim),
                    None
                ))
                .is_err());
            let mut foreign = export.clone();
            foreign["requestId"] = json!("foreign");
            assert!(session
                .admit(&row(
                    "exported",
                    "succeeded",
                    Some(0),
                    None,
                    Some(foreign),
                    None
                ))
                .is_err());
        }
        assert_eq!(session.admit(packet), Ok(true));
        assert!(session.admit(packet).is_err());
        complete(&mut session, &run, index, Ok(())).unwrap();
    }
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("object-window-probe.json")).unwrap())
            .unwrap();
    assert_eq!(receipt["schema"], "nemo.object-window-export-probe/1");
    assert_eq!(receipt["complete"], true);
    assert_eq!(
        receipt["records"][3]["export"]["artifactFiles"],
        json!(["frame_0001.png"])
    );
    assert!(!receipt.to_string().contains("private-owner"));
}

#[test]
fn export_mode_failure_never_completes_after_bad_artifact_or_ack_timeout() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    session.export = true;
    let run = session.run.clone();
    session
        .admit(&packet(&run, "ready", "ready", None, None))
        .unwrap();
    assert!(complete(&mut session, &run, 0, Err("probe checkpoint timed out")).is_err());
    let mut failed = packet(&run, "failed", "operation_failed", None, None);
    failed.cleanup_succeeded = Some(false);
    failed.failure_stage = Some("blue-checkpoint".into());
    assert_eq!(session.admit(&failed), Ok(false));
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("object-window-probe.json")).unwrap())
            .unwrap();
    assert_eq!(receipt["complete"], false);
    assert_eq!(receipt["failed"], true);
    assert_eq!(receipt["records"][1]["cleanupSucceeded"], false);
    assert_eq!(receipt["records"][1]["failureStage"], "blue-checkpoint");
}

#[test]
fn export_failure_stage_is_fixed_and_never_accepted_on_success_packets() {
    let fixture = Fixture::new();
    let mut session = fixture.session();
    session.export = true;
    let run = session.run.clone();
    let mut ready = packet(&run, "ready", "ready", None, None);
    ready.failure_stage = Some("ready".into());
    assert!(session.admit(&ready).is_err());
    session
        .admit(&packet(&run, "ready", "ready", None, None))
        .unwrap();
    complete(&mut session, &run, 0, Ok(())).unwrap();
    for stage in ["arbitrary", "private-owner", "", "failed"] {
        let mut failed = packet(&run, "failed", "operation_failed", None, None);
        failed.cleanup_succeeded = Some(true);
        failed.failure_stage = Some(stage.into());
        assert!(session.admit(&failed).is_err(), "{stage}");
    }
    let mut no_stage = packet(&run, "failed", "operation_failed", None, None);
    no_stage.cleanup_succeeded = Some(true);
    assert!(session.admit(&no_stage).is_err());
    let mut failed = packet(&run, "failed", "operation_failed", None, None);
    failed.cleanup_succeeded = Some(true);
    failed.failure_stage = Some("stale-refusal".into());
    assert_eq!(session.admit(&failed), Ok(false));
    let receipt: serde_json::Value =
        serde_json::from_slice(&fs::read(fixture.root.join("object-window-probe.json")).unwrap())
            .unwrap();
    assert_eq!(
        receipt["records"].as_array().unwrap().last().unwrap()["failureStage"],
        "stale-refusal"
    );
    assert!(!receipt.to_string().contains("private-owner"));
}

#[test]
fn export_failure_diagnostic_classes_are_fixed_and_terminal() {
    for stage in [
        "stale-refusal-invalid-request",
        "stale-refusal-non-object",
        "stale-refusal-no-code",
        "stale-refusal-other-code",
        "stale-refusal-no-error",
    ] {
        let fixture = Fixture::new();
        let mut session = fixture.session();
        session.export = true;
        let run = session.run.clone();
        session
            .admit(&packet(&run, "ready", "ready", None, None))
            .unwrap();
        complete(&mut session, &run, 0, Ok(())).unwrap();
        let mut failed = packet(&run, "failed", "operation_failed", None, None);
        failed.cleanup_succeeded = Some(true);
        failed.failure_stage = Some(stage.into());
        assert_eq!(session.admit(&failed), Ok(false), "{stage}");
        let receipt: serde_json::Value = serde_json::from_slice(
            &fs::read(fixture.root.join("object-window-probe.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(receipt["records"][1]["failureStage"], stage);
        assert_eq!(receipt["complete"], false);
    }
}
