//! Actual host/history and native desktop artifact controls; no UI acceptance.
use crate::native_application_ports::DesktopArtifactPort;
use crate::native_dispatch::object_host::retained_tabs::{
    settled_export, PreparedRetainedActivation,
};
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{
    commands::OpacityRequest,
    export_job::{ExportArtifact, JobStatus, StagedArtifactPort},
    object_document::FrameScopeKind,
    object_frame_packet::ObjectFrameSelector,
    object_snapshot::ObjectSnapshot,
    scheduler::OutputSpec,
};
use serde_json::{json, Value};
use std::{cell::Cell, fs, path::PathBuf};

fn host() -> NativeObjectHost {
    NativeObjectHost::new_blank_project("retained-host".into(), 64, 64, 24.0).unwrap()
}
fn snapshot(host: &NativeObjectHost) -> ObjectSnapshot {
    host.history
        .acquire_snapshot(host.content_revision())
        .unwrap()
}
fn saved(host: &NativeObjectHost) -> Vec<u8> {
    let snapshot = snapshot(host);
    host.history
        .serialize_project_file(snapshot.content_revision(), snapshot.snapshot_id())
        .unwrap()
}
fn prepare_b(host: &mut NativeObjectHost) -> Value {
    let input = NativeObjectHost::new_blank_project("fixture-input".into(), 96, 64, 24.0).unwrap();
    let origin = snapshot(host);
    host.prepare_inactive_project(
        origin.instance_id(),
        origin.document_id(),
        origin.content_revision(),
        origin.snapshot_id(),
        &saved(&input),
    )
    .unwrap()
}
fn plan(host: &NativeObjectHost, request: &str) -> PreparedRetainedActivation {
    let origin = snapshot(host);
    host.prepare_retained_activation(
        origin.instance_id(),
        origin.document_id(),
        origin.content_revision(),
        origin.snapshot_id(),
        host.retained_project_token().unwrap(),
        request,
    )
    .unwrap()
}
fn attach(host: &mut NativeObjectHost) -> (Value, Value, Vec<u8>) {
    let hex = "89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000d49444154789c636060f80f060014f405fbb956037d0000000049454e44ae426082";
    let png: Vec<_> = hex
        .as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect();
    let request = OpacityRequest::history_stage(
        "retained-reference",
        host.instance_id(),
        host.document_id(),
        Some(host.content_revision()),
        "command.document.reference.attach",
        json!({"id":"reference","name":"reference.png"}),
    );
    let raw = serde_json::to_value(&request).unwrap();
    let origin = snapshot(host);
    let reply = host
        .project_reference(origin.snapshot_id(), &request, Some(&png))
        .unwrap();
    assert!(reply.is_ok());
    (raw, serde_json::to_value(reply).unwrap(), png)
}

#[test]
fn cpu_retained_host_repeat_preserves_identity_redo_old_reads_and_command_receipts() {
    let mut owner = host();
    let (request, reply, png) = attach(&mut owner);
    let old = snapshot(&owner);
    let old_bytes = saved(&owner);
    owner.history.undo(owner.content_revision()).unwrap();
    let a = owner.project_file_info().unwrap();
    let a_bytes = saved(&owner);
    let b = prepare_b(&mut owner);
    let activation = plan(&owner, "switch-first");
    let result = owner
        .activate_retained_project(activation, || Ok(()))
        .unwrap();
    assert_eq!(result.active, b);
    assert_eq!(
        snapshot(&owner).snapshot_id(),
        b["documentSnapshotId"].as_str().unwrap()
    );
    assert_eq!(owner.history.history_depths(), (0, 0));
    assert_eq!(
        owner
            .parked_project
            .as_ref()
            .unwrap()
            .history
            .history_depths(),
        (0, 1)
    );
    assert!(owner.inactive_project.is_none());
    assert_eq!(
        owner.retained_project_metadata().unwrap(),
        Some(result.inactive)
    );
    assert!(prepare_b_refused(&mut owner));
    assert_ne!(
        owner.retained_project_token().unwrap(),
        b["preparationToken"].as_str().unwrap()
    );
    let back = plan(&owner, "switch-back");
    owner.activate_retained_project(back, || Ok(())).unwrap();
    assert_eq!(owner.project_file_info().unwrap(), a);
    assert_eq!(saved(&owner), a_bytes);
    assert_eq!(old.serialize_project_file().unwrap(), old_bytes);
    let replay = owner
        .project_reference(
            snapshot(&owner).snapshot_id(),
            &serde_json::from_value(request).unwrap(),
            Some(&png),
        )
        .unwrap();
    assert_eq!(serde_json::to_value(replay).unwrap(), reply);
    assert_eq!(saved(&owner), a_bytes);
    owner.history.redo(owner.content_revision()).unwrap();
    let changed = owner.project_file_info().unwrap();
    let forward = plan(&owner, "switch-again");
    owner.activate_retained_project(forward, || Ok(())).unwrap();
    assert_eq!(
        owner.project_file_info().unwrap()["documentSnapshotId"],
        b["documentSnapshotId"]
    );
    let back = plan(&owner, "switch-again-back");
    owner.activate_retained_project(back, || Ok(())).unwrap();
    assert_eq!(owner.project_file_info().unwrap(), changed);
    assert!(owner.object_client_available());
}
fn prepare_b_refused(owner: &mut NativeObjectHost) -> bool {
    let origin = snapshot(owner);
    owner
        .prepare_inactive_project(
            origin.instance_id(),
            origin.document_id(),
            origin.content_revision(),
            origin.snapshot_id(),
            &saved(owner),
        )
        .is_err()
}

#[test]
fn cpu_retained_preflight_refuses_wrong_stale_and_changed_target_before_release() {
    let mut owner = host();
    let b = prepare_b(&mut owner);
    let a = saved(&owner);
    let origin = snapshot(&owner);
    for (instance, document, revision, snap, token) in [
        (
            "foreign",
            origin.document_id(),
            origin.content_revision(),
            origin.snapshot_id(),
            b["preparationToken"].as_str().unwrap(),
        ),
        (
            origin.instance_id(),
            "foreign",
            origin.content_revision(),
            origin.snapshot_id(),
            b["preparationToken"].as_str().unwrap(),
        ),
        (
            origin.instance_id(),
            origin.document_id(),
            999,
            origin.snapshot_id(),
            b["preparationToken"].as_str().unwrap(),
        ),
        (
            origin.instance_id(),
            origin.document_id(),
            origin.content_revision(),
            "stale",
            b["preparationToken"].as_str().unwrap(),
        ),
        (
            origin.instance_id(),
            origin.document_id(),
            origin.content_revision(),
            origin.snapshot_id(),
            "stale",
        ),
    ] {
        assert!(owner
            .prepare_retained_activation(instance, document, revision, snap, token, "refused")
            .is_err());
        assert_eq!(saved(&owner), a);
        assert_eq!(owner.retained_project_metadata().unwrap(), Some(b.clone()));
        assert!(owner.released.is_none());
    }
    let prepared = plan(&owner, "retired-plan");
    owner
        .close_inactive_project(
            origin.instance_id(),
            origin.document_id(),
            origin.content_revision(),
            origin.snapshot_id(),
            b["preparationToken"].as_str().unwrap(),
        )
        .unwrap();
    prepare_b(&mut owner);
    let called = Cell::new(false);
    assert!(owner
        .activate_retained_project(prepared, || {
            called.set(true);
            Ok(())
        })
        .is_err());
    assert!(!called.get());
    assert_eq!(saved(&owner), a);
    assert!(owner.released.is_none());
}

#[test]
fn cpu_retained_viewport_partial_failure_keeps_both_histories_and_dispatch_fence() {
    let mut owner = host();
    let b = prepare_b(&mut owner);
    let a = saved(&owner);
    let prepared = plan(&owner, "viewport-fails");
    assert!(owner
        .activate_retained_project(prepared, || Err("viewport disposal uncertain".into()))
        .is_err());
    assert_eq!(saved(&owner), a);
    assert_eq!(owner.retained_project_metadata().unwrap(), Some(b));
    assert!(owner.release_progress().unwrap().reconciliation_complete());
    assert!(!owner.object_client_available());
    assert!(owner
        .prepare_retained_activation(
            "retained-host",
            owner.document_id(),
            owner.content_revision(),
            snapshot(&owner).snapshot_id(),
            owner.retained_project_token().unwrap(),
            "retry"
        )
        .is_err());
    let request = OpacityRequest::history_stage(
        "fenced-read",
        owner.instance_id(),
        owner.document_id(),
        Some(owner.content_revision()),
        "query.document.object.list",
        json!({}),
    );
    assert!(owner.dispatch(request).is_err());
}

#[test]
fn cpu_retained_parked_close_and_destructive_new_obey_existing_disposal_intent() {
    for destructive_new in [false, true] {
        let mut owner = host();
        let old = snapshot(&owner);
        let old_bytes = saved(&owner);
        prepare_b(&mut owner);
        let prepared = plan(&owner, "before-disposal");
        owner
            .activate_retained_project(prepared, || Ok(()))
            .unwrap();
        let current = snapshot(&owner);
        if destructive_new {
            let replacement = owner
                .prepare_blank_project_replacement(current.snapshot_id(), 80, 80, 30.0)
                .unwrap();
            assert!(owner.release_project().unwrap().reconciliation_complete());
            owner.commit_project_replacement(replacement).unwrap();
            assert_ne!(owner.document_id(), current.document_id());
            assert_eq!(owner.project_file_info().unwrap()["canvasW"], 80);
            assert_eq!(owner.history.history_depths(), (0, 0));
        } else {
            let token = owner.retained_project_token().unwrap().to_owned();
            assert!(owner
                .close_inactive_project(
                    current.instance_id(),
                    current.document_id(),
                    current.content_revision(),
                    current.snapshot_id(),
                    "wrong-token"
                )
                .is_err());
            assert!(owner.parked_project.is_some());
            owner
                .close_inactive_project(
                    current.instance_id(),
                    current.document_id(),
                    current.content_revision(),
                    current.snapshot_id(),
                    &token,
                )
                .unwrap();
            assert_eq!(snapshot(&owner).snapshot_id(), current.snapshot_id());
        }
        assert!(!owner.has_inactive_project());
        assert_eq!(old.serialize_project_file().unwrap(), old_bytes);
        assert!(owner.object_client_available());
        prepare_b(&mut owner);
        assert!(owner.has_inactive_project());
    }
}

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("nemo-retained-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn port(&self) -> DesktopArtifactPort {
        DesktopArtifactPort::new_svg(
            self.0.join("stage"),
            [("target".into(), self.0.join("output"))],
        )
        .unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn export(owner: &mut NativeObjectHost, request: &str, port: &mut impl StagedArtifactPort) -> bool {
    let origin = snapshot(owner);
    let selector = ObjectFrameSelector {
        instance_id: origin.instance_id(),
        document_id: origin.document_id(),
        content_revision: origin.content_revision(),
        document_snapshot_id: origin.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let (w, h) = owner.history.project_canvas_dimensions().unwrap().unwrap();
    owner
        .export_pinned_object_svg_frames(
            &selector,
            &[0],
            request.into(),
            "target".into(),
            OutputSpec::new("frame", "rgba8", w, h, "srgb", "straight").unwrap(),
            65536,
            port,
        )
        .map(|outcome| {
            outcome
                .receipt
                .is_ok_and(|r| r.status == JobStatus::Succeeded)
        })
        .unwrap_or(false)
}
struct DebtPort;
impl StagedArtifactPort for DebtPort {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn write_frame(&mut self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
        Err("write failed".into())
    }
    fn publish(&mut self, _: &str, _: &str, _: &[String]) -> Result<ExportArtifact, String> {
        panic!("no failed write may publish")
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        Err("cleanup uncertain".into())
    }
}

#[test]
fn cpu_retained_export_barrier_follows_project_and_real_cleanup_debt_refuses_switch() {
    let mut owner = host();
    let output = Scratch::new();
    assert!(export(&mut owner, "same-export", &mut output.port()));
    assert!(!export(
        &mut owner,
        "same-export",
        &mut Scratch::new().port()
    ));
    prepare_b(&mut owner);
    let forward = plan(&owner, "export-forward");
    owner.activate_retained_project(forward, || Ok(())).unwrap();
    assert!(owner.object_export.is_none());
    assert!(export(
        &mut owner,
        "same-export",
        &mut Scratch::new().port()
    ));
    let back = plan(&owner, "export-back");
    owner.activate_retained_project(back, || Ok(())).unwrap();
    assert!(!export(
        &mut owner,
        "same-export",
        &mut Scratch::new().port()
    ));
    let before_debt = plan(&owner, "prepared-before-export-debt");
    let a = saved(&owner);
    let b = owner.retained_project_metadata().unwrap();
    // A real failed native write with failed cleanup creates retained debt.
    let _ = export(&mut owner, "different-export", &mut DebtPort);
    assert!(!settled_export(&owner.object_export));
    let called = Cell::new(false);
    assert!(owner
        .activate_retained_project(before_debt, || {
            called.set(true);
            Ok(())
        })
        .is_err());
    assert!(!called.get());
    assert_eq!(saved(&owner), a);
    assert_eq!(owner.retained_project_metadata().unwrap(), b);
    let origin = snapshot(&owner);
    assert!(owner
        .prepare_retained_activation(
            origin.instance_id(),
            origin.document_id(),
            origin.content_revision(),
            origin.snapshot_id(),
            owner.retained_project_token().unwrap(),
            "debt-switch"
        )
        .is_err());
    assert!(owner.released.is_none());
    assert!(owner.parked_project.is_some());
}
