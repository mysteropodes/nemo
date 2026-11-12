//! Real NativeState/host transitions; only the viewport/scheduler are injected ports.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{host_error, HostResult};
use crate::native_dispatch::{NativeObjectHost, PreparedRetainedActivation, ReplacementReplay};
use crate::native_object_bootstrap::inactive::activation::{callback, prepare, Admission};
use crate::native_object_bootstrap::inactive::request_with;
use native_engine::object_document::FrameScopeKind;
use native_engine::object_frame_packet::ObjectFrameSelector;
use serde_json::{json, Value};
use std::{
    cell::Cell,
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

struct Fixture {
    state: ApplicationMcp,
    root: PathBuf,
    request: Value,
}
impl Fixture {
    fn new() -> Self {
        let root =
            std::env::temp_dir().join(format!("nemo-retained-route-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let path = root.join("B.json");
        fs::write(
            &path,
            include_bytes!("../../tests/fixtures/p38/native-project/B.json"),
        )
        .unwrap();
        let state = ApplicationMcp::default();
        let (document, _, _) = state
            .install_native_project_file(include_bytes!(
                "../../tests/fixtures/p38/native-project/A.json"
            ))
            .unwrap();
        let info = state.native_project_info(&document).unwrap();
        let prepare = json!({"apiVersion":2,"requestId":"prepare-b","instanceId":state.instance_id(),
            "documentId":document,"expectedRevision":info["contentRevision"],
            "documentSnapshotId":info["documentSnapshotId"],"lifecycleGeneration":info["lifecycleGeneration"],
            "action":{"kind":"prepare","sourcePath":path}});
        let retained = request_with(
            &state,
            &prepare.to_string(),
            "main",
            |p| p.parent() == Some(&root),
            || {},
            || {},
        )
        .unwrap()["project"]
            .clone();
        let request = make_request(&state, &info, &retained, "activate-b");
        Self {
            state,
            root,
            request,
        }
    }
    fn execute(
        &self,
        request: &Value,
    ) -> (u64, String, Vec<u8>, PreparedRetainedActivation, Value) {
        match prepare(&self.state, &request.to_string(), "main").unwrap() {
            Admission::Execute {
                generation,
                request_id,
                fingerprint,
                plan,
                receipt,
            } => (generation, request_id, fingerprint, plan, receipt),
            _ => panic!("expected one new reservation"),
        }
    }
    fn complete(&self, request: &Value) -> Value {
        let (generation, id, _, plan, receipt) = self.execute(request);
        callback::complete(
            &self.state.native_state(),
            generation,
            &id,
            plan,
            receipt,
            || Ok((vec![], "already_absent")),
        )
        .unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn make_request(state: &ApplicationMcp, active: &Value, target: &Value, id: &str) -> Value {
    json!({"apiVersion":2,"requestId":id,"instanceId":state.instance_id(),
        "documentId":active["documentId"],"expectedRevision":active["contentRevision"],
        "documentSnapshotId":active["documentSnapshotId"],"lifecycleGeneration":active["lifecycleGeneration"],
        "action":{"kind":"activate","preparationToken":target["preparationToken"],
            "targetDocumentId":target["documentId"],"targetRevision":target["contentRevision"],
            "targetSnapshotId":target["documentSnapshotId"]}})
}
fn inspect_retained(state: &ApplicationMcp, info: &Value) -> Value {
    let r = json!({"apiVersion":2,"requestId":"inspect-current","instanceId":state.instance_id(),
        "documentId":info["documentId"],"expectedRevision":info["contentRevision"],
        "documentSnapshotId":info["documentSnapshotId"],"lifecycleGeneration":info["lifecycleGeneration"],
        "action":{"kind":"inspect"}});
    request_with(
        state,
        &r.to_string(),
        "main",
        |_| panic!("inspect has no I/O"),
        || {},
        || {},
    )
    .unwrap()["project"]
        .clone()
}
fn with_owner<T>(
    state: &ApplicationMcp,
    info: &Value,
    operation: impl FnOnce(&mut NativeObjectHost) -> Result<T, String>,
) -> T {
    let selector = ObjectFrameSelector {
        instance_id: state.instance_id(),
        document_id: info["documentId"].as_str().unwrap(),
        content_revision: info["contentRevision"].as_u64().unwrap(),
        document_snapshot_id: info["documentSnapshotId"].as_str().unwrap(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    state
        .with_object_preview_owner(
            info["lifecycleGeneration"].as_u64().unwrap(),
            &selector,
            operation,
        )
        .unwrap()
}
fn attach(state: &ApplicationMcp, info: &Value) -> Value {
    let hex = "89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000d49444154789c636060f80f060014f405fbb956037d0000000049454e44ae426082";
    let png: Vec<_> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect();
    with_owner(state, info, |owner| {
        let request = native_engine::commands::OpacityRequest::history_stage(
            "route-reference",
            state.instance_id(),
            info["documentId"].as_str().unwrap(),
            Some(info["contentRevision"].as_u64().unwrap()),
            "command.document.reference.attach",
            json!({"id":"reference","name":"reference.png"}),
        );
        let response = owner.project_reference(
            info["documentSnapshotId"].as_str().unwrap(),
            &request,
            Some(&png),
        )?;
        assert!(response.is_ok());
        Ok(())
    });
    state
        .native_project_info(info["documentId"].as_str().unwrap())
        .unwrap()
}

#[test]
fn cpu_retained_route_past_old_capacity_retires_superseded_receipts() {
    let f = Fixture::new();
    let first = f.complete(&f.request);
    let mut active_id = first["documentId"].as_str().unwrap().to_owned();
    let mut last_request = f.request.clone();
    let mut last_receipt = first.clone();
    // Run well past the old 16-receipt limit: superseded settled receipts retire instead.
    for ordinal in 1..20 {
        let active = f.state.native_project_info(&active_id).unwrap();
        let target = inspect_retained(&f.state, &active);
        let request = make_request(&f.state, &active, &target, &format!("switch-{ordinal}"));
        let receipt = f.complete(&request);
        active_id = receipt["documentId"].as_str().unwrap().to_owned();
        // Both projects survive every switch: the new active one and the parked one.
        let now = f.state.native_project_info(&active_id).unwrap();
        let parked = inspect_retained(&f.state, &now);
        assert_ne!(parked["documentId"], now["documentId"]);
        assert_eq!(parked["documentId"], active["documentId"]);
        last_request = request;
        last_receipt = receipt;
    }
    let before = f.state.native_project_info(&active_id).unwrap();
    let held = inspect_retained(&f.state, &before);
    // A recent switch still replays its receipt.
    match prepare(&f.state, &last_request.to_string(), "main").unwrap() {
        Admission::Replay(value) => assert_eq!(value, last_receipt),
        _ => panic!("a recent request must replay its receipt"),
    }
    // The very first request was retired: typed Expired replay, no re-execution.
    let error = prepare(&f.state, &f.request.to_string(), "main")
        .err()
        .unwrap();
    assert_eq!(error.code, "replacement_indeterminate");
    assert!(error.message.contains("receipt expired"));
    let details = error.details.unwrap();
    assert_eq!(details["disposition"], "indeterminate");
    assert_eq!(details["retryExecution"], false);
    assert_eq!(f.state.native_project_info(&active_id).unwrap(), before);
    assert_eq!(inspect_retained(&f.state, &before), held);
    let changed = attach(&f.state, &before);
    assert_eq!(
        changed["contentRevision"],
        before["contentRevision"].as_u64().unwrap() + 1
    );
}

#[test]
fn cpu_retained_route_preserves_nonzero_revisions_and_exact_replay_across_a_b_a_b() {
    let mut f = Fixture::new();
    let a = f
        .state
        .native_project_info(f.request["documentId"].as_str().unwrap())
        .unwrap();
    let changed_a = attach(&f.state, &a);
    f.request["expectedRevision"] = changed_a["contentRevision"].clone();
    f.request["documentSnapshotId"] = changed_a["documentSnapshotId"].clone();
    let (generation, id, fingerprint, plan, receipt) = f.execute(&f.request);
    assert!(serde_json::to_vec(&receipt).unwrap().len() <= 4096);
    assert!(f
        .state
        .native_project_info(a["documentId"].as_str().unwrap())
        .is_err());
    assert!(matches!(
        prepare(&f.state, &f.request.to_string(), "main")
            .err()
            .unwrap()
            .code
            .as_str(),
        "replacement_pending"
    ));
    let b_receipt = callback::complete(
        &f.state.native_state(),
        generation,
        &id,
        plan,
        receipt,
        || Ok((vec![], "disposed")),
    )
    .unwrap();
    assert_eq!(b_receipt["contentRevision"], 0);
    let b = f
        .state
        .native_project_info(b_receipt["documentId"].as_str().unwrap())
        .unwrap();
    assert_eq!(b["lifecycleGeneration"], generation);
    let changed_b = attach(&f.state, &b);
    let parked_a = inspect_retained(&f.state, &changed_b);
    assert_eq!(parked_a["contentRevision"], changed_a["contentRevision"]);
    assert_eq!(parked_a["documentDigest"], changed_a["documentDigest"]);
    let back = make_request(&f.state, &changed_b, &parked_a, "activate-a");
    let a_receipt = f.complete(&back);
    assert_eq!(a_receipt["contentRevision"], 1);
    let restored_a = f
        .state
        .native_project_info(a["documentId"].as_str().unwrap())
        .unwrap();
    assert_eq!(restored_a["documentDigest"], changed_a["documentDigest"]);
    assert_eq!(
        restored_a["documentSnapshotId"],
        changed_a["documentSnapshotId"]
    );
    let parked_b = inspect_retained(&f.state, &restored_a);
    let again = make_request(&f.state, &restored_a, &parked_b, "activate-b-again");
    let again_receipt = f.complete(&again);
    assert_eq!(again_receipt["contentRevision"], 1);
    assert_eq!(
        again_receipt["documentSnapshotId"],
        changed_b["documentSnapshotId"]
    );
    assert_eq!(
        callback::reconcile(&f.state.native_state(), generation, &id, &fingerprint).unwrap(),
        b_receipt
    );
    match prepare(&f.state, &f.request.to_string(), "main").unwrap() {
        Admission::Replay(value) => assert_eq!(value, b_receipt),
        _ => panic!("old source epoch replay cannot execute again"),
    }
    let mut conflict = f.request.clone();
    conflict["action"]["targetRevision"] = json!(99);
    assert!(prepare(&f.state, &conflict.to_string(), "main").is_err());
    assert_eq!(
        f.state
            .native_project_info(changed_b["documentId"].as_str().unwrap())
            .unwrap()["documentSnapshotId"],
        changed_b["documentSnapshotId"]
    );
}

#[test]
fn cpu_retained_route_strict_raw_refusals_preserve_owner_and_target_before_reservation() {
    let f = Fixture::new();
    let a = f
        .state
        .native_project_info(f.request["documentId"].as_str().unwrap())
        .unwrap();
    let b = inspect_retained(&f.state, &a);
    let mut invalid = vec![];
    for (field, value) in [
        ("instanceId", json!("foreign")),
        ("documentId", json!("foreign")),
        ("expectedRevision", json!(1)),
        ("documentSnapshotId", json!("stale")),
        ("lifecycleGeneration", json!(0)),
        ("cancelledBeforeDispatch", json!(true)),
        ("extra", json!(true)),
        ("lifecycleGeneration", json!(9_007_199_254_740_991u64)),
        ("action", json!(["activate", "x"])),
    ] {
        let mut r = f.request.clone();
        r[field] = value;
        invalid.push(r.to_string());
    }
    for (field, value) in [
        ("preparationToken", json!("stale")),
        ("targetDocumentId", json!("foreign")),
        ("targetRevision", json!(1)),
        ("targetSnapshotId", json!("stale")),
        ("extra", json!(1)),
    ] {
        let mut r = f.request.clone();
        r["action"][field] = value;
        invalid.push(r.to_string());
    }
    let raw = f.request.to_string();
    invalid.push(raw.replacen(
        "\"requestId\":",
        "\"requestId\":\"duplicate\",\"requestId\":",
        1,
    ));
    invalid.push(raw.replacen("\"kind\":", "\"kind\":\"activate\",\"kind\":", 1));
    invalid.push(format!("[{}]", raw));
    invalid.push(format!("{}{}", raw, " ".repeat(4096)));
    for raw in invalid {
        assert!(
            prepare(&f.state, &raw, "main").is_err(),
            "must refuse original bytes: {raw}"
        );
        assert_eq!(
            f.state
                .native_project_info(a["documentId"].as_str().unwrap())
                .unwrap(),
            a
        );
        assert_eq!(inspect_retained(&f.state, &a), b);
    }
    assert!(prepare(&f.state, &raw, "other").is_err());
    let reserved = f.execute(&f.request);
    assert_eq!(reserved.0, a["lifecycleGeneration"].as_u64().unwrap() + 1);
    callback::complete(
        &f.state.native_state(),
        reserved.0,
        &reserved.1,
        reserved.3,
        reserved.4,
        || Ok((vec![], "already_absent")),
    )
    .unwrap();
}

#[test]
fn cpu_retained_callback_drop_cancel_lost_waiter_and_scheduler_error_preserve_exact_outcome() {
    for scenario in 0..4 {
        let f = Fixture::new();
        let (generation, id, fingerprint, plan, receipt) = f.execute(&f.request);
        let effects = Arc::new(AtomicUsize::new(0));
        let observed = effects.clone();
        let mut queued = None;
        let waiter = callback::submit(
            f.state.native_state(),
            generation,
            id.clone(),
            fingerprint.clone(),
            plan,
            receipt,
            |callback| match scenario {
                0 => {
                    drop(callback);
                    Err(())
                }
                3 => {
                    callback();
                    Err(())
                }
                _ => {
                    queued = Some(callback);
                    Ok(())
                }
            },
            move || {
                observed.fetch_add(1, Ordering::SeqCst);
                Ok((vec![], "disposed"))
            },
        );
        assert_eq!(waiter.is_err(), matches!(scenario, 0 | 3));
        drop(waiter);
        if scenario == 2 {
            queued.take().unwrap()();
        }
        drop(queued);
        assert_eq!(effects.load(Ordering::SeqCst), usize::from(scenario >= 2));
        let result = callback::reconcile(&f.state.native_state(), generation, &id, &fingerprint);
        if scenario < 2 {
            assert_eq!(result.unwrap_err().code, "replacement_indeterminate");
            assert!(f.state.native_state().lock().unwrap().active().is_none());
        } else {
            let receipt = result.unwrap();
            match prepare(&f.state, &f.request.to_string(), "main").unwrap() {
                Admission::Replay(replayed) => assert_eq!(replayed, receipt),
                _ => panic!("completed callback is not repeatable"),
            }
            assert_eq!(effects.load(Ordering::SeqCst), 1);
        }
    }
}

#[test]
fn cpu_retained_callback_partial_viewport_failure_and_panic_retain_fence_without_reexecution() {
    for scenario in 0..3 {
        let f = Fixture::new();
        let (generation, id, fingerprint, plan, receipt) = f.execute(&f.request);
        let called = Cell::new(false);
        let result = callback::complete(
            &f.state.native_state(),
            generation,
            &id,
            plan,
            receipt,
            || {
                called.set(true);
                match scenario {
                    0 => Err(host_error("cleanup_failed", "partial viewport release")),
                    1 => Ok((vec![], "uncertain")),
                    _ => panic!("controlled viewport panic"),
                }
            },
        );
        assert!(called.get());
        assert_eq!(result.unwrap_err().code, "replacement_indeterminate");
        let native = f.state.native_state();
        let authority = native.lock().unwrap();
        assert!(authority.active().is_none());
        assert!(matches!(
            authority.lookup_replace_replay(&id, &fingerprint).unwrap(),
            Some(ReplacementReplay::Fenced(_))
        ));
        drop(authority);
        assert!(prepare(&f.state, &f.request.to_string(), "main").is_err());
        assert!(callback::reconcile(&native, generation, &id, &fingerprint).is_err());
    }
}
