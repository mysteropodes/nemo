//! Actual original-file/owner oracles; no app, GPU, or substituted authority.
use crate::application_mcp::ApplicationMcp;
use crate::native_application::{admit_release_request, complete_release};
use crate::native_application_contract::{HostResult, NativeReleaseRequest};
use crate::native_dispatch::ReleaseAdmission;
use crate::native_object_bootstrap::inactive::request_with;
use serde_json::{json, Value};
use std::{cell::Cell, fs, path::PathBuf};
const A: &[u8] = include_bytes!("../../tests/fixtures/p38/native-project/A.json");
const B: &[u8] = include_bytes!("../../tests/fixtures/p38/native-project/B.json");
struct Fixture {
    state: ApplicationMcp,
    document: String,
    root: PathBuf,
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nemo-inactive-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let path = root.join("B.json");
        fs::write(&path, B).unwrap();
        let state = ApplicationMcp::default();
        let (document, _, _) = state.install_native_project_file(A).unwrap();
        Self {
            state,
            document,
            root,
            path,
        }
    }
    fn info(&self) -> Value {
        self.state.native_project_info(&self.document).unwrap()
    }
    fn request(&self, id: &str, action: Value) -> Value {
        let p = self.info();
        json!({"apiVersion":2,"requestId":id,"instanceId":self.state.instance_id(),
            "documentId":self.document,"expectedRevision":p["contentRevision"],
            "documentSnapshotId":p["documentSnapshotId"],"lifecycleGeneration":p["lifecycleGeneration"],"action":action})
    }
    fn prepare(&self, id: &str) -> Value {
        self.request(id, json!({"kind":"prepare","sourcePath":self.path}))
    }
    fn run(&self, request: &Value) -> HostResult<Value> {
        request_with(
            &self.state,
            &request.to_string(),
            "main",
            |p| p.parent() == Some(&self.root),
            || {},
            || {},
        )
    }
    fn without_io(&self, request: &Value) -> HostResult<Value> {
        request_with(
            &self.state,
            &request.to_string(),
            "main",
            |_| panic!("request must not reach file I/O"),
            || {},
            || {},
        )
    }
    fn inspect(&self, id: &str) -> Value {
        self.run(&self.request("inspect", json!({"kind":"inspect","recoverRequestId":id})))
            .unwrap()
    }
    fn saved(&self) -> Vec<u8> {
        let p = self.info();
        self.state
            .native_project_file_snapshot(
                &self.document,
                p["contentRevision"].as_u64().unwrap(),
                p["documentSnapshotId"].as_str().unwrap(),
            )
            .unwrap()
            .unwrap()
    }
    fn edit_request(&self) -> Value {
        let source: Value = serde_json::from_slice(A).unwrap();
        json!({"apiVersion":2,"requestId":uuid::Uuid::new_v4().to_string(),
            "instanceId":self.state.instance_id(),"documentId":self.document,
            "expectedRevision":self.info()["contentRevision"],"operation":"command.document.object.fill.set",
            "payload":{"command":"object.fill.set","stableTarget":source["drawing"]["objects"][0]["target"],
                "fill":{"kind":"solid","r":if self.info()["contentRevision"]==0 {0.95}else{0.3},"g":0.05,"b":0.2,"a":1}}})
    }
    fn edit(&self) {
        assert!(
            self.state
                .serialize_object_file_snapshot(&self.edit_request().to_string())
                .unwrap()
                .ok
        );
    }
    fn release(&self) {
        let request: NativeReleaseRequest =
            serde_json::from_value(json!({"apiVersion":2,"requestId":"release",
            "instanceId":self.state.instance_id(),"documentId":self.document,
            "expectedRevision":self.info()["contentRevision"],"cancelledBeforeDispatch":false}))
            .unwrap();
        let ReleaseAdmission::Execute { generation } =
            admit_release_request(&self.state.native_state(), &request).unwrap()
        else {
            panic!("fresh release must execute")
        };
        let receipt = complete_release(&self.state.native_state(), generation, &request, || {
            Ok((vec![], "already_absent"))
        })
        .unwrap();
        assert_eq!(receipt.status, "succeeded");
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn original_file_retention_replay_and_late_close_preserve_dirty_a() {
    let f = Fixture::new();
    f.edit();
    let before = (f.info(), f.saved());
    assert_eq!(before.0["undoDepth"], 1);
    let prepare = f.prepare("prepare");
    let reads = Cell::new(0);
    let first = request_with(
        &f.state,
        &prepare.to_string(),
        "main",
        |_| {
            reads.set(reads.get() + 1);
            true
        },
        || {
            assert!(f.state.native_state().try_lock().is_ok());
        },
        || assert!(f.state.preview_locks_held_for_test()),
    )
    .unwrap();
    assert_eq!(first["state"], "inactive-prepared");
    assert_eq!(first["project"]["preparationRequestId"], "prepare");
    assert_ne!(first["project"]["documentId"], f.document);
    fs::remove_file(&f.path).unwrap();
    assert_eq!(
        request_with(
            &f.state,
            &prepare.to_string(),
            "main",
            |_| panic!("replay must not reread"),
            || {},
            || {}
        )
        .unwrap(),
        first
    );
    assert_eq!(reads.get(), 1);
    assert_eq!(f.inspect("prepare")["project"], first["project"]);
    let mut changed = prepare.clone();
    changed["action"]["sourcePath"] = json!(f.root.join("other"));
    assert_eq!(f.run(&changed).unwrap_err().code, "request_conflict");
    let close = f.request(
        "close",
        json!({"kind":"close","preparationToken":first["preparationToken"]}),
    );
    let closed = f.run(&close).unwrap();
    assert_eq!(closed["state"], "inactive-closed");
    assert_eq!(f.run(&close).unwrap(), closed);
    assert_eq!(f.run(&prepare).unwrap_err().code, "preparation_closed");
    fs::write(&f.path, B).unwrap();
    let newer = f.run(&f.prepare("newer")).unwrap();
    assert_ne!(newer["preparationToken"], first["preparationToken"]);
    let late = f.request("late-close", close["action"].clone());
    assert!(f.run(&late).is_err());
    assert_eq!(f.inspect("newer")["project"], newer["project"]);
    assert_eq!((f.info(), f.saved()), before);
}

#[test]
fn pending_duplicate_is_not_reexecuted_and_a_edit_during_read_cannot_orphan_b() {
    let f = Fixture::new();
    let prepare = f.prepare("pending");
    let failure = request_with(
        &f.state,
        &prepare.to_string(),
        "main",
        |_| true,
        || {
            assert_eq!(f.run(&prepare).unwrap_err().code, "inactive_pending");
            assert_eq!(f.inspect("pending")["pendingRequestId"], "pending");
            assert_eq!(
                f.run(&f.prepare("other")).unwrap_err().code,
                "inactive_pending"
            );
            f.edit();
        },
        || panic!("stale request must not execute"),
    )
    .unwrap_err();
    assert_eq!(failure.details.unwrap()["disposition"], "notDispatched");
    let recovered = f.inspect("pending");
    assert_eq!(recovered["state"], "inactive-absent");
    assert_eq!(
        recovered["recoveredRequest"]["outcome"]["error"]["code"],
        "stale_revision"
    );
    let current = f.run(&f.prepare("current")).unwrap();
    assert_eq!(current["state"], "inactive-prepared");
    f.edit();
    let recovered = f.inspect("current");
    assert_eq!(recovered["project"], current["project"]);
    assert_eq!(recovered["recoveredRequest"]["outcome"]["ok"], true);
    assert!(f.run(&prepare).is_err());
}

#[test]
fn panics_after_prepare_and_close_are_reconciled_without_reexecution_or_lock_poison() {
    let f = Fixture::new();
    let prepare = f.prepare("uncertain");
    let failure = request_with(
        &f.state,
        &prepare.to_string(),
        "main",
        |_| true,
        || {},
        || {
            assert!(f.state.preview_locks_held_for_test());
            panic!("lost prepare reply");
        },
    )
    .unwrap_err();
    assert_eq!(failure.details.unwrap()["disposition"], "indeterminate");
    assert_eq!(f.run(&prepare).unwrap_err().code, "inactive_indeterminate");
    let retained = f.inspect("uncertain");
    assert_eq!(retained["state"], "inactive-retained");
    assert_eq!(retained["project"]["preparationRequestId"], "uncertain");
    assert_eq!(
        retained["recoveredRequest"]["outcome"]["error"]["code"],
        "inactive_indeterminate"
    );
    let close = f.request(
        "uncertain-close",
        json!({"kind":"close","preparationToken":retained["preparationToken"]}),
    );
    assert!(request_with(
        &f.state,
        &close.to_string(),
        "main",
        |_| panic!("close has no I/O"),
        || {},
        || panic!("lost close reply")
    )
    .is_err());
    assert_eq!(f.run(&close).unwrap_err().code, "inactive_indeterminate");
    let absent = f.inspect("uncertain-close");
    assert_eq!(absent["state"], "inactive-absent");
    assert_eq!(
        absent["recoveredRequest"]["outcome"]["error"]["code"],
        "inactive_indeterminate"
    );
    assert_eq!(
        f.inspect("uncertain")["recoveredRequest"]["outcome"]["error"]["code"],
        "preparation_closed"
    );
    assert!(f.run(&f.prepare("fresh")).is_ok());
}

#[test]
fn strict_envelope_owner_and_path_refusals_precede_file_io() {
    let f = Fixture::new();
    let original = f.prepare("strict");
    let before = f.saved();
    for (field, value) in [
        ("apiVersion", json!(1)),
        ("requestId", json!("")),
        ("instanceId", json!("foreign")),
        ("documentId", json!("foreign")),
        ("documentSnapshotId", json!("stale")),
        ("expectedRevision", json!(1)),
        ("lifecycleGeneration", json!(0)),
        ("lifecycleGeneration", json!(99)),
        ("expectedRevision", json!(9007199254740992u64)),
        ("extra", json!(true)),
    ] {
        let mut request = original.clone();
        request[field] = value;
        assert!(request_with(
            &f.state,
            &request.to_string(),
            "main",
            |_| panic!("invalid pins reached I/O"),
            || {},
            || {}
        )
        .is_err());
    }
    let raw = original.to_string();
    for invalid in [
        "[]".to_owned(),
        format!("{{\"requestId\":\"duplicate\",{}", &raw[1..]),
        " ".repeat(4097),
        raw.replace(
            "\"kind\":\"prepare\"",
            "\"kind\":\"prepare\",\"kind\":\"prepare\"",
        ),
    ] {
        assert!(request_with(
            &f.state,
            &invalid,
            "main",
            |_| panic!("invalid JSON reached I/O"),
            || {},
            || {}
        )
        .is_err());
    }
    assert!(request_with(
        &f.state,
        &raw,
        "secondary",
        |_| panic!("wrong window reached I/O"),
        || {},
        || {}
    )
    .is_err());
    for path in ["relative", "bad\0path"] {
        let mut request = original.clone();
        request["action"]["sourcePath"] = json!(path);
        assert!(f.run(&request).is_err());
    }
    assert_eq!(f.saved(), before);
}

#[test]
fn cancelled_capacity_and_file_refusals_are_retained_without_eviction() {
    let f = Fixture::new();
    let mut cancelled = f.prepare("cancelled");
    cancelled["cancelledBeforeDispatch"] = true.into();
    let first = f.without_io(&cancelled).unwrap_err();
    assert_eq!(first.code, "cancelled_before_dispatch");
    assert_eq!(f.run(&cancelled).unwrap_err(), first);
    cancelled["cancelledBeforeDispatch"] = false.into();
    assert_eq!(f.run(&cancelled).unwrap_err().code, "request_conflict");
    for i in 0..13 {
        let request = f.prepare(&format!("denied-{i}"));
        let failure = request_with(
            &f.state,
            &request.to_string(),
            "main",
            |_| false,
            || {},
            || {},
        )
        .unwrap_err();
        assert_eq!(f.run(&request).unwrap_err(), failure);
    }
    let prepared = f.run(&f.prepare("last-prepare")).unwrap();
    let close = f.request(
        "last-close",
        json!({"kind":"close","preparationToken":prepared["preparationToken"]}),
    );
    let mut cancelled_close = close.clone();
    cancelled_close["cancelledBeforeDispatch"] = true.into();
    let mut wrong_close = close.clone();
    wrong_close["action"]["preparationToken"] = "foreign".into();
    for blocked in [cancelled_close, wrong_close, f.prepare("busy")] {
        assert!(f
            .without_io(&blocked)
            .unwrap_err()
            .message
            .contains("capacity"));
    }
    assert_eq!(f.without_io(&close).unwrap()["state"], "inactive-closed");
    let exhausted = f.without_io(&f.prepare("exhausted")).unwrap_err();
    assert!(exhausted.message.contains("capacity"));
    assert_eq!(
        f.inspect("cancelled")["recoveredRequest"]["outcome"]["error"]["code"],
        "cancelled_before_dispatch"
    );
    assert_eq!(
        f.inspect("unrecorded")["recoveredRequest"]["outcome"],
        Value::Null
    );
}

#[test]
fn original_regular_native_project_size_and_strict_codec_are_required() {
    let f = Fixture::new();
    let before = f.saved();
    for (index, bytes) in [
        b"{}".as_slice(),
        A,
        b"{\"format\":\"nemo.native-object-document\"}".as_slice(),
    ]
    .iter()
    .enumerate()
    {
        let mut bytes = bytes.to_vec();
        if index == 1 {
            bytes.splice(1..1, b"\"formatVersion\":1,".iter().copied());
        }
        fs::write(&f.path, bytes).unwrap();
        assert!(f.run(&f.prepare(&format!("bad-{index}"))).is_err());
        assert_eq!(f.inspect("unknown")["state"], "inactive-absent");
    }
    fs::File::create(&f.path)
        .unwrap()
        .set_len(32 * 1024 * 1024 + 1)
        .unwrap();
    assert_eq!(
        f.run(&f.prepare("oversized")).unwrap_err().code,
        "invalid_request"
    );
    fs::remove_file(&f.path).unwrap();
    fs::create_dir(&f.path).unwrap();
    assert_eq!(
        f.run(&f.prepare("directory")).unwrap_err().code,
        "invalid_request"
    );
    assert_eq!(f.saved(), before);
}

#[test]
fn release_during_read_and_replacement_cannot_recover_stale_owner_records() {
    let mut f = Fixture::new();
    let request = f.prepare("release-race");
    assert!(request_with(
        &f.state,
        &request.to_string(),
        "main",
        |_| true,
        || f.release(),
        || panic!("released effect")
    )
    .is_err());
    let (document, _, _) = f.state.install_native_project_file(A).unwrap();
    f.document = document;
    let before = f.saved();
    assert!(f.run(&request).is_err());
    assert_eq!(
        f.inspect("release-race")["recoveredRequest"]["outcome"],
        Value::Null
    );
    let prepared = f.run(&f.prepare("new-owner")).unwrap();
    assert_eq!(prepared["project"]["preparationRequestId"], "new-owner");
    assert_eq!(f.saved(), before);
}

#[test]
fn revision_notification_barrier_blocks_all_inactive_actions_without_subscriber_mutation() {
    let f = Fixture::new();
    let binding = f
        .state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    f.state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap();
    let prepare = f.prepare("before-external");
    let b = f.run(&prepare).unwrap();
    let (response, notification) = f
        .state
        .external_object_preview_for_test(&f.edit_request().to_string())
        .unwrap();
    assert!(response.ok);
    let (event, mut receiver) = notification.unwrap();
    for action in [
        json!({"kind":"inspect"}),
        json!({"kind":"close","preparationToken":b["preparationToken"]}),
        json!({"kind":"prepare","sourcePath":f.path}),
    ] {
        let request = f.request("blocked", action);
        assert!(request_with(
            &f.state,
            &request.to_string(),
            "main",
            |_| panic!("barrier I/O"),
            || {},
            || {}
        )
        .unwrap_err()
        .message
        .contains("pending"));
    }
    f.state.control_revision_for_test(json!({"action":"acknowledge","subscriptionId":binding["subscriptionId"],"event":event})).unwrap();
    assert_eq!(receiver.try_recv(), Ok(()));
    assert!(f
        .run(&f.request("unsettled", json!({"kind":"inspect"})))
        .is_err());
    assert!(f.state.settle_object_preview_for_test(event, true));
    assert_eq!(f.inspect("before-external")["project"], b["project"]);
    let (_, notification) = f
        .state
        .external_object_preview_for_test(&f.edit_request().to_string())
        .unwrap();
    assert!(notification.is_some());
    f.state
        .control_revision_for_test(
            json!({"action":"disconnect","subscriptionId":binding["subscriptionId"]}),
        )
        .unwrap();
    assert!(f
        .run(&f.request("disconnected", json!({"kind":"inspect"})))
        .unwrap_err()
        .message
        .contains("indeterminate"));
}
