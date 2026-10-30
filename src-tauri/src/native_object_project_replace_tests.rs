//! Actual authority/history, file admission, replay, cleanup fences and Metal reentry.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::{host_error, HostResult};
use crate::native_dispatch::{ReplacementAdmission, ReplacementIdentity};
use crate::native_object_bootstrap::{
    reopen,
    replace::{self, Admission},
};
use serde_json::{json, Value};
use std::{cell::Cell, fs, path::PathBuf};

pub(super) struct Fixture {
    pub(super) state: ApplicationMcp,
    pub(super) root: PathBuf,
    path: PathBuf,
    pub(super) request: Value,
    before: Value,
}
impl Fixture {
    pub(super) fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nemo-replace-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let state = ApplicationMcp::default();
        let cases: Value = serde_json::from_str(include_str!(
            "../../engineering/application/examples/native-object-v1/cases.json"
        ))
        .unwrap();
        let mut record = cases["records"][0].clone();
        record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"a"});
        record["geometry"]["segments"] = json!([[24,24],[40,24],[40,40],[24,40]].map(|[x,y]|
            json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
        record["fill"] = json!({"kind":"solid","r":0,"g":0,"b":1,"a":1});
        let document = json!({"format":"nemo.native-project-document","formatVersion":1,"project":{"canvasW":64,"canvasH":48,"fps":24.0},
            "viewportReference":null,"assets":[],"drawing":{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":1,
            "layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],"objects":[record],"compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[{"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}}]}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}}});
        let (id, _, _) = state
            .install_native_project_file(&serde_json::to_vec(&document).unwrap())
            .unwrap();
        let before = state.native_project_info(&id).unwrap();
        let mut b = document;
        b["project"]["canvasW"] = json!(80);
        let path = root.join("saved-b.json");
        fs::write(&path, serde_json::to_vec(&b).unwrap()).unwrap();
        let request = json!({"apiVersion":2,"requestId":"reopen-b","instanceId":state.instance_id(),"sourcePath":path,"projectOnly":true,
            "replace":{"documentId":id,"expectedRevision":before["contentRevision"],"documentSnapshotId":before["documentSnapshotId"],
                "lifecycleGeneration":before["lifecycleGeneration"]}});
        Self {
            state,
            root,
            path,
            request,
            before,
        }
    }
    pub(super) fn prepare(&self, request: &Value) -> HostResult<Admission> {
        replace::prepare(
            &self.state,
            reopen::file_request(&request.to_string()).unwrap(),
            |p| p.starts_with(&self.root),
        )
    }
    pub(super) fn execute(
        &self,
    ) -> (
        u64,
        String,
        Vec<u8>,
        native_engine::history::PreparedProjectReplacement,
    ) {
        match self.prepare(&self.request).unwrap() {
            Admission::Execute {
                generation,
                request_id,
                fingerprint,
                prepared,
            } => (generation, request_id, fingerprint, prepared),
            _ => panic!("expected one execution"),
        }
    }
    fn assert_a(&self) {
        assert_eq!(
            self.state
                .native_project_info(self.request["replace"]["documentId"].as_str().unwrap())
                .unwrap(),
            self.before
        );
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn cpu_preflight_refusals_preserve_original_owner_and_strict_request() {
    let f = Fixture::new();
    for (field, value) in [
        ("documentId", json!("foreign")),
        ("documentSnapshotId", json!("foreign")),
        ("expectedRevision", json!(1)),
        ("lifecycleGeneration", json!(999)),
    ] {
        let mut request = f.request.clone();
        request["replace"][field] = value;
        let error = f.prepare(&request).err().unwrap();
        assert_eq!(error.code, "replacement_refused");
        assert_eq!(error.details.unwrap()["disposition"], "notDispatched");
        f.assert_a();
    }
    let mut cancelled = f.request.clone();
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert_eq!(
        f.prepare(&cancelled).err().unwrap().details.unwrap()["reasonCode"],
        "cancelled_before_dispatch"
    );
    f.assert_a();
    let raw = f.request.to_string();
    assert!(
        reopen::file_request(&raw.replacen("\"replace\":", "\"replace\":{},\"replace\":", 1))
            .is_none()
    );
    let mut unknown = f.request.clone();
    unknown["replace"]["extra"] = json!(true);
    assert!(reopen::file_request(&unknown.to_string()).is_none());
    assert!(replace::prepare(&f.state, reopen::file_request(&raw).unwrap(), |_| false).is_err());
    f.assert_a();
    fs::write(&f.path, b"{").unwrap();
    assert!(f.prepare(&f.request).is_err());
    f.assert_a();
    fs::remove_file(&f.path).unwrap();
    assert!(f.prepare(&f.request).is_err());
    f.assert_a();
}

#[test]
fn cpu_success_replays_without_rereading_file_or_replacing_again() {
    let f = Fixture::new();
    let (generation, id, fingerprint, prepared) = f.execute();
    assert!(f
        .state
        .native_project_info(f.request["replace"]["documentId"].as_str().unwrap())
        .is_err());
    assert_eq!(
        f.prepare(&f.request).err().unwrap().code,
        "replacement_pending"
    );
    let calls = Cell::new(0);
    let result = replace::complete(&f.state.native_state(), generation, &id, prepared, || {
        calls.set(calls.get() + 1);
        Ok((vec![], "already_absent"))
    })
    .unwrap();
    let result = serde_json::to_value(result).unwrap();
    assert_ne!(result["documentId"], f.request["replace"]["documentId"]);
    let b = f
        .state
        .native_project_info(result["documentId"].as_str().unwrap())
        .unwrap();
    assert_eq!(b["lifecycleGeneration"], generation);
    assert_eq!(b["contentRevision"], 0);
    assert_eq!(b["undoDepth"], 0);
    assert_eq!(b["canvasW"], 80);
    assert!(generation > f.before["lifecycleGeneration"].as_u64().unwrap());
    let bytes = f
        .state
        .native_project_file_snapshot(
            result["documentId"].as_str().unwrap(),
            0,
            b["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap();
    let original: Value = serde_json::from_slice(&fs::read(&f.path).unwrap()).unwrap();
    assert_eq!(serde_json::from_slice::<Value>(&bytes).unwrap(), original);
    fs::remove_file(&f.path).unwrap();
    let Admission::Replay(replayed) = f.prepare(&f.request).unwrap() else {
        panic!("must replay");
    };
    assert_eq!(serde_json::to_value(replayed).unwrap(), result);
    assert_eq!(calls.get(), 1);
    assert_eq!(
        serde_json::to_value(
            replace::reconcile_executor(&f.state.native_state(), generation, &id, &fingerprint)
                .unwrap()
        )
        .unwrap(),
        result
    );
    let mut changed = f.request.clone();
    changed["sourcePath"] = json!(f.root.join("changed.json"));
    assert_eq!(
        f.prepare(&changed).err().unwrap().code,
        "replacement_refused"
    );
    assert_eq!(
        f.state
            .native_project_info(result["documentId"].as_str().unwrap())
            .unwrap(),
        b
    );
}

#[test]
fn cpu_cleanup_and_callback_failures_fence_without_false_success_or_retry() {
    for panic_cleanup in [false, true] {
        let f = Fixture::new();
        let (generation, id, _, prepared) = f.execute();
        let error = replace::complete(&f.state.native_state(), generation, &id, prepared, || {
            if panic_cleanup {
                panic!("injected viewport cleanup panic");
            }
            Err(host_error("cleanup_failed", "injected viewport refusal"))
        })
        .unwrap_err();
        assert_eq!(error.code, "replacement_indeterminate");
        assert_eq!(error.details.unwrap()["disposition"], "indeterminate");
        assert!(f.state.native_state().lock().unwrap().active().is_none());
        assert_eq!(
            f.prepare(&f.request).err().unwrap().code,
            "replacement_indeterminate"
        );
    }
    let f = Fixture::new();
    let (generation, id, fingerprint, prepared) = f.execute();
    drop(prepared);
    assert_eq!(
        replace::reconcile_executor(&f.state.native_state(), generation, &id, &fingerprint)
            .unwrap_err()
            .code,
        "replacement_indeterminate"
    );
    assert_eq!(
        f.prepare(&f.request).err().unwrap().code,
        "replacement_indeterminate"
    );
}

#[test]
fn cpu_reservation_checks_generation_atomically_and_invalidates_old_subscription() {
    let f = Fixture::new();
    let binding = f
        .state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    f.state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap();
    let invoked = Cell::new(false);
    let identity = |revision| ReplacementIdentity {
        request_id: "generation-test",
        fingerprint: b"generation-test",
        instance_id: f.state.instance_id(),
        document_id: f.request["replace"]["documentId"].as_str().unwrap(),
        expected_revision: revision,
    };
    assert!(f
        .state
        .reserve_native_replacement_at_generation(999, identity(0), |_| {
            invoked.set(true);
            Ok(())
        })
        .is_err());
    assert!(!invoked.get());
    f.assert_a();
    let request = json!({"apiVersion":2,"requestId":"external-fill","instanceId":f.state.instance_id(),
        "documentId":f.request["replace"]["documentId"],"operation":"command.document.object.fill.set","expectedRevision":0,
        "payload":{"command":"object.fill.set","stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
        "layerUid":"L0","strokeId":"a"},"fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}}});
    let (response, notification) = f
        .state
        .external_object_preview_for_test(&request.to_string())
        .unwrap();
    assert!(response.ok);
    let (_, mut receiver) = notification.unwrap();
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
    assert!(matches!(
        f.state
            .reserve_native_replacement_at_generation(
                f.before["lifecycleGeneration"].as_u64().unwrap(),
                identity(1),
                |_| Ok(())
            )
            .unwrap(),
        ReplacementAdmission::Execute(_)
    ));
    assert!(
        matches!(
            receiver.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Closed)
        ),
        "reservation must actually disconnect the outstanding old-generation acknowledgement"
    );
}

#[test]
fn gpu_saved_reference_reopens_on_same_authority_and_main_target_slot() {
    use crate::native_application_viewport::TestObjectSlot;
    use native_engine::{
        compositor::Compositor,
        desktop_viewport::{
            AcquiredSurfaceFrame, CssBounds, DesktopViewportHost, PhysicalExtent,
            SettledSurfaceFrame, SurfaceAttempt, SurfacePort, SurfacePresentation,
            SurfaceRecoveryError, ViewportMapping, ViewportStatus,
        },
        object_document::FrameScopeKind,
        object_frame_packet::ObjectFrameSelector,
        scheduler::OutputSpec,
    };
    struct Frame;
    impl AcquiredSurfaceFrame for Frame {
        fn settle(self) -> SettledSurfaceFrame {
            SettledSurfaceFrame::Presented
        }
    }
    struct Port;
    impl SurfacePort for Port {
        type Frame<'p, 's> = Frame;
        fn acquire<'p, 's>(&'p mut self, source: SurfacePresentation<'s>) -> SurfaceAttempt<Frame> {
            let buffer = source.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("reopened-reference-pixel"),
                size: 16,
                usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let mut encoder = source
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            encoder.copy_texture_to_buffer(
                wgpu::TexelCopyTextureInfo {
                    texture: source.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x: 8, y: 24, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                wgpu::TexelCopyBufferInfo {
                    buffer: &buffer,
                    layout: wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: None,
                        rows_per_image: None,
                    },
                },
                wgpu::Extent3d {
                    width: 1,
                    height: 1,
                    depth_or_array_layers: 1,
                },
            );
            source.queue.submit(Some(encoder.finish()));
            let slice = buffer.slice(..);
            let (tx, rx) = std::sync::mpsc::channel();
            slice.map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
            source
                .device
                .poll(wgpu::PollType::wait_indefinitely())
                .unwrap();
            rx.recv().unwrap().unwrap();
            let bytes = slice.get_mapped_range();
            assert!(bytes[..4]
                .iter()
                .zip([128u8, 128, 255, 255])
                .all(|(a, b)| a.abs_diff(b) <= 1));
            drop(bytes);
            buffer.unmap();
            SurfaceAttempt::Acquired(Frame)
        }
        fn recreate(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
            Ok(())
        }
        fn reconfigure(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
            Ok(())
        }
        fn dispose(&mut self) {}
    }
    let mut f = Fixture::new();
    let png = f.root.join("reference.png");
    let hex="89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000d49444154789c636060f80f060014f405fbb956037d0000000049454e44ae426082";
    fs::write(
        &png,
        hex.as_bytes()
            .chunks_exact(2)
            .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let a = f.request["replace"]["documentId"]
        .as_str()
        .unwrap()
        .to_owned();
    crate::native_object_bootstrap::reference::reference_request(&f.state,&json!({"apiVersion":2,"requestId":"attach","instanceId":f.state.instance_id(),
        "documentId":a,"lifecycleGeneration":f.before["lifecycleGeneration"],"expectedRevision":0,"documentSnapshotId":f.before["documentSnapshotId"],
        "action":{"kind":"attach","id":"ref","sourcePath":png}}).to_string(),|p|p.starts_with(&f.root)).unwrap();
    f.before = f.state.native_project_info(&a).unwrap();
    f.request["replace"]["expectedRevision"] = f.before["contentRevision"].clone();
    f.request["replace"]["documentSnapshotId"] = f.before["documentSnapshotId"].clone();
    fs::remove_file(&f.path).unwrap();
    crate::native_object_bootstrap::save::save_request(&f.state,&json!({"apiVersion":2,"requestId":"save-a","instanceId":f.state.instance_id(),
        "documentId":a,"atRevision":f.before["contentRevision"],"documentSnapshotId":f.before["documentSnapshotId"],"destinationPath":f.path}).to_string(),
        |p|p.starts_with(&f.root),|_,_|Ok(())).unwrap();
    let slot = TestObjectSlot::new();
    let creations = Cell::new(0);
    let present = |info: &Value| {
        let selector = ObjectFrameSelector {
            instance_id: f.state.instance_id(),
            document_id: info["documentId"].as_str().unwrap(),
            content_revision: info["contentRevision"].as_u64().unwrap(),
            document_snapshot_id: info["documentSnapshotId"].as_str().unwrap(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame: 0,
        };
        let generation = info["lifecycleGeneration"].as_u64().unwrap();
        f.state
            .with_object_preview_owner(generation, &selector, |owner| {
                slot.with_target(
                    f.state.instance_id(),
                    generation,
                    || {
                        creations.set(creations.get() + 1);
                        let extent = PhysicalExtent {
                            width: 64,
                            height: 48,
                        };
                        let mapping = ViewportMapping::new(
                            CssBounds {
                                x: 0.,
                                y: 0.,
                                width: 64.,
                                height: 48.,
                            },
                            extent,
                            extent,
                            1.,
                        )
                        .unwrap();
                        Ok((
                            DesktopViewportHost::new(Port, mapping),
                            Compositor::new().unwrap(),
                        ))
                    },
                    |viewport, gpu| {
                        owner
                            .present_object_preview(
                                &selector,
                                OutputSpec::new("frame", "rgba8", 64, 48, "srgb", "straight")
                                    .unwrap(),
                                gpu,
                                viewport,
                            )
                            .map_err(|e| host_error("unavailable", e))
                    },
                )
                .map_err(|error| error.message)
            })
            .unwrap()
    };
    assert_eq!(present(&f.before).status(), &ViewportStatus::Presented);
    let (generation, id, _, prepared) = f.execute();
    let result = replace::complete(&f.state.native_state(), generation, &id, prepared, || {
        slot.release(f.state.instance_id(), |viewport| viewport.dispose())
            .map(|ids| (ids, "disposed"))
    })
    .unwrap();
    let b = f.state.native_project_info(&result.document_id).unwrap();
    assert_eq!(b["documentDigest"], f.before["documentDigest"]);
    assert_eq!(present(&b).status(), &ViewportStatus::Presented);
    assert_eq!(creations.get(), 2);
    assert!(slot
        .with_target::<()>(
            f.state.instance_id(),
            f.before["lifecycleGeneration"].as_u64().unwrap(),
            || panic!("old generation must not create"),
            |_, _| panic!("old generation must not consume")
        )
        .is_err());
    slot.release(f.state.instance_id(), |viewport| viewport.dispose())
        .unwrap();
}
