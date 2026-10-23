//! Ordinary native PNG history after real desktop preview and complete sequence export.
//! The application and one compositor stay alive across every operation.
use crate::application_mcp::ApplicationMcp;
use crate::native_application::{admit_release_request, complete_release};
use crate::native_application_contract::{host_error, NativeReleaseRequest};
use crate::native_dispatch::ReleaseAdmission;
use crate::native_object_desktop_export::{admit_request, sequence::SequenceExport};
use crate::native_object_desktop_preview::{admit_request as admit_preview, preview_with_target};
use native_engine::{
    compositor::Compositor,
    desktop_viewport::{
        AcquiredSurfaceFrame, CssBounds, DesktopViewportHost, PhysicalExtent, SettledSurfaceFrame,
        SurfaceAttempt, SurfacePort, SurfacePresentation, SurfaceRecoveryError, ViewportMapping,
    },
};
use nemo_mcp::contract::NativeApplicationRequest;
use serde_json::{json, Value};
use std::{cell::Cell, fs, path::PathBuf, rc::Rc, sync::Arc};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("nemo-png-undo-sequence-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn png(&self) -> PathBuf {
        let path = self.0.join("native-png-16x16.png");
        // Exact original installed 16x16 quadrant fixture, SHA-256
        // e40c0dd589c398a64919ed516a884ab1d782d392a5ad6194a2d175880c285234.
        let hex = "89504e470d0a1a0a0000000d49484452000000100000001008060000001ff3ff61000000224944415478da636060f8ff1f1ffe4f4001c3a801c3c380fff82121306ac0b0300000fe8c7d9f3613f1dd0000000049454e44ae426082";
        let bytes: Vec<_> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        fs::write(&path, bytes).unwrap();
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
fn info(state: &ApplicationMcp) -> Value {
    let native = state.native_state();
    let guard = native.lock().unwrap();
    let document = guard.active().unwrap().1.document_id().to_owned();
    drop(guard);
    state.native_project_info(&document).unwrap()
}
fn save(state: &ApplicationMcp) -> Vec<u8> {
    let pin = info(state);
    state
        .native_project_file_snapshot(
            pin["documentId"].as_str().unwrap(),
            pin["contentRevision"].as_u64().unwrap(),
            pin["documentSnapshotId"].as_str().unwrap(),
        )
        .unwrap()
        .unwrap()
}
async fn attach(state: &ApplicationMcp, path: &PathBuf, id: &str) {
    let pin = info(state);
    let request: NativeApplicationRequest = serde_json::from_value(json!({
        "apiVersion":2,"requestId":id,"instanceId":state.instance_id(),
        "documentId":pin["documentId"],"expectedRevision":pin["contentRevision"],
        "operation":"command.document.object.asset.attach",
        "payload":{"documentSnapshotId":pin["documentSnapshotId"],"lifecycleGeneration":pin["lifecycleGeneration"],
            "name":"native-png-16x16.png","mediaType":"image/png","source":{"kind":"file","path":path},
            "placement":{"kind":"newLayer"}}
    })).unwrap();
    let expected = path.clone();
    let response = state
        .dispatch_native_scoped(request, Arc::new(move |p| p == expected))
        .await
        .unwrap();
    assert!(response.ok, "actual scoped image attach: {response:?}");
}
fn request(state: &ApplicationMcp, frame: u32) -> Value {
    let pin = info(state);
    assert_eq!(pin["totalFrames"], 120);
    json!({"apiVersion":2,"requestId":format!("frame-{}-{frame}",pin["contentRevision"]),
        "instanceId":state.instance_id(),"documentId":pin["documentId"],
        "contentRevision":pin["contentRevision"],"documentSnapshotId":pin["documentSnapshotId"],
        "lifecycleGeneration":pin["lifecycleGeneration"],"contextId":"scene-root","scopeKind":"authored","frame":frame,
        "outputSpec":{"kind":"frame","format":"rgba8","width":64,"height":64,"colorInterpretation":"srgb","alphaMode":"straight"},
        "viewport":{"cssBounds":{"x":0,"y":0,"width":64,"height":64},"physicalExtent":{"width":64,"height":64},
            "compositionExtent":{"width":64,"height":64},"reportedDpr":1}})
}

struct Frame;
impl AcquiredSurfaceFrame for Frame {
    fn settle(self) -> SettledSurfaceFrame {
        SettledSurfaceFrame::Presented
    }
}
struct Port {
    state: Arc<ApplicationMcp>,
    image: Rc<Cell<bool>>,
    acquisitions: Rc<Cell<u32>>,
    disposed: Rc<Cell<bool>>,
    context: (wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue),
}
impl SurfacePort for Port {
    type Frame<'p, 's> = Frame;
    fn acquire<'p, 's>(&'p mut self, source: SurfacePresentation<'s>) -> SurfaceAttempt<Frame> {
        assert!(self.state.preview_locks_held_for_test());
        assert_eq!(source.instance, &self.context.0);
        assert_eq!(source.adapter, &self.context.1);
        assert_eq!(source.device, &self.context.2);
        assert_eq!(source.queue, &self.context.3);
        assert_eq!((source.texture.width(), source.texture.height()), (64, 64));
        assert_pixels(&source, self.image.get());
        self.acquisitions.set(self.acquisitions.get() + 1);
        SurfaceAttempt::Acquired(Frame)
    }
    fn recreate(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn reconfigure(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn dispose(&mut self) {
        self.disposed.set(true);
    }
}
fn assert_pixels(source: &SurfacePresentation<'_>, image: bool) {
    let buffer = source.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("ordinary-png-undo-pixels"),
        size: 256 * 64,
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
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(64),
            },
        },
        wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
    );
    source.queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
    source
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let bytes = slice.get_mapped_range();
    for y in 0..64 {
        for x in 0..64 {
            let expected = if image && (24..40).contains(&x) && (24..40).contains(&y) {
                match (x < 32, y < 32) {
                    (true, true) => [0, 0, 255, 255],
                    (false, true) => [255, 0, 0, 255],
                    (true, false) => [0, 255, 0, 255],
                    _ => [255; 4],
                }
            } else {
                [255; 4]
            };
            assert_eq!(
                &bytes[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4],
                &expected,
                "pixel {x},{y}"
            );
        }
    }
    drop(bytes);
    buffer.unmap();
}
fn preview(
    state: &ApplicationMcp,
    gpu: &mut Compositor,
    viewport: &mut DesktopViewportHost<Port>,
    frame: u32,
) {
    let before = info(state);
    let raw = request(state, frame);
    let admitted = admit_preview(&raw.to_string(), state.instance_id()).unwrap();
    let result = preview_with_target(state, &admitted, |consumer| {
        consumer
            .present(gpu, viewport)
            .map_err(|error| host_error("unavailable", error))
    })
    .unwrap_or_else(|error| {
        panic!(
            "preview after revision {} at frame {frame}: {error:?}",
            before["contentRevision"]
        )
    });
    assert_eq!(result.status, "presented");
    assert_eq!(info(state), before);
}
fn export(state: &ApplicationMcp, gpu: &mut Compositor, scratch: &Scratch, name: &str) {
    let before = info(state);
    let mut raw = request(state, 0);
    let destination = scratch.0.join(name);
    let staging = scratch.0.join(format!("staging-{name}"));
    raw["requestId"] = json!(format!("sequence-{name}"));
    raw["frameRange"] = json!({"start":0,"end":119});
    raw["destinationDirectory"] = json!(destination);
    raw["maximumEncodedBytes"] = json!(4 * 1024 * 1024);
    let allowed = destination.clone();
    let admitted = Arc::new(
        admit_request(&raw.to_string(), state.instance_id(), move |path| {
            path == allowed
        })
        .unwrap(),
    );
    let mut sequence = SequenceExport::new(admitted, staging.clone());
    for batch in 0..4 {
        let done = sequence
            .step_with_target(state, |consumer, port| {
                assert!(state.preview_locks_held_for_test());
                assert!(!destination.exists());
                assert_eq!(
                    consumer.frames,
                    (batch * 32..((batch + 1) * 32).min(120)).collect::<Vec<_>>()
                );
                consumer
                    .run_with_port(gpu, port)
                    .map_err(|error| host_error("unavailable", error))
            })
            .unwrap();
        assert_eq!(done, batch == 3);
        assert!(!destination.exists());
    }
    let receipt = sequence.finish_with_owner(state).unwrap();
    assert_eq!(receipt.status, "succeeded");
    assert_eq!(receipt.external_effect_disposition, "committed");
    let expected: Vec<_> = (1..=120).map(|i| format!("frame_{i:04}.png")).collect();
    assert_eq!(receipt.artifact_files, expected);
    let mut actual: Vec<_> = fs::read_dir(&destination)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    actual.sort();
    assert_eq!(actual, expected);
    assert!(!staging.exists() || fs::read_dir(staging).unwrap().next().is_none());
    assert_eq!(info(state), before);
    let selector = native_engine::object_frame_packet::ObjectFrameSelector {
        instance_id: state.instance_id(),
        document_id: before["documentId"].as_str().unwrap(),
        content_revision: before["contentRevision"].as_u64().unwrap(),
        document_snapshot_id: before["documentSnapshotId"].as_str().unwrap(),
        context_id: "scene-root",
        scope_kind: native_engine::object_document::FrameScopeKind::Authored,
        frame: 0,
    };
    state
        .with_object_preview_owner(
            before["lifecycleGeneration"].as_u64().unwrap(),
            &selector,
            |owner| {
                assert!(owner.matches_project_snapshot(selector.document_snapshot_id));
                assert!(owner.pending_object_motion_frames().is_empty());
                assert_eq!(
                    owner.export_test_state()?.0,
                    0,
                    "all native export leases released"
                );
                Ok(())
            },
        )
        .unwrap();
}
fn history(state: &ApplicationMcp, action: &str) {
    let pin = info(state);
    let response=state.dispatch_object_request_json(&json!({"apiVersion":2,
        "requestId":format!("{action}-{}",pin["contentRevision"]),"instanceId":state.instance_id(),
        "documentId":pin["documentId"],"expectedRevision":pin["contentRevision"],
        "operation":format!("command.document.object.{action}"),"payload":{"command":format!("object.{action}")}
    }).to_string()).unwrap();
    assert!(response.ok, "ordinary history: {response:?}");
    assert_eq!(
        info(state)["contentRevision"].as_u64().unwrap(),
        pin["contentRevision"].as_u64().unwrap() + 1
    );
}
#[tokio::test]
async fn gpu_original_png_sequence_then_ordinary_undo_keeps_remaining_image_presented() {
    let scratch = Scratch::new();
    let path = scratch.png();
    let state = Arc::new(ApplicationMcp::default());
    crate::native_object_bootstrap::bootstrap_raw(
        &state,
        &json!({"apiVersion":2,"requestId":"new-original-png",
        "instanceId":state.instance_id(),"newProject":{"canvasW":64,"canvasH":64,"fps":24.0}})
        .to_string(),
    )
    .unwrap();
    let mut gpu = Compositor::new().unwrap();
    let image = Rc::new(Cell::new(false));
    let acquisitions = Rc::new(Cell::new(0));
    let disposed = Rc::new(Cell::new(false));
    let port = Port {
        state: state.clone(),
        image: image.clone(),
        acquisitions: acquisitions.clone(),
        disposed: disposed.clone(),
        context: (
            gpu.instance().clone(),
            gpu.adapter().clone(),
            gpu.device().clone(),
            gpu.queue().clone(),
        ),
    };
    let extent = PhysicalExtent {
        width: 64,
        height: 64,
    };
    let mut viewport = DesktopViewportHost::new(
        port,
        ViewportMapping::new(
            CssBounds {
                x: 0.,
                y: 0.,
                width: 64.,
                height: 64.,
            },
            extent,
            extent,
            1.,
        )
        .unwrap(),
    );
    let blank = save(&state);
    preview(&state, &mut gpu, &mut viewport, 0);
    export(&state, &mut gpu, &scratch, "baseline");
    attach(&state, &path, "first").await;
    let first = save(&state);
    image.set(true);
    for frame in [0, 119] {
        preview(&state, &mut gpu, &mut viewport, frame);
    }
    export(&state, &mut gpu, &scratch, "first");
    attach(&state, &path, "duplicate").await;
    let duplicate = save(&state);
    for frame in [0, 119] {
        preview(&state, &mut gpu, &mut viewport, frame);
    }
    export(&state, &mut gpu, &scratch, "duplicate");
    for (action, expected, present) in [
        ("undo", &first, true),
        ("undo", &blank, false),
        ("redo", &first, true),
        ("redo", &duplicate, true),
    ] {
        history(&state, action);
        assert_eq!(&save(&state), expected);
        image.set(present);
        for frame in [119, 0] {
            preview(&state, &mut gpu, &mut viewport, frame);
        }
    }
    assert_eq!(acquisitions.get(), 13);
    let pin = info(&state);
    let request: NativeReleaseRequest = serde_json::from_value(json!({
        "apiVersion":2,"requestId":"release-original-png","instanceId":state.instance_id(),
        "documentId":pin["documentId"],"expectedRevision":pin["contentRevision"]}))
    .unwrap();
    let native = state.native_state();
    let ReleaseAdmission::Execute { generation } =
        admit_release_request(&native, &request).unwrap()
    else {
        panic!("fresh release")
    };
    let released = complete_release(&native, generation, &request, || {
        Ok((viewport.dispose(), "disposed"))
    })
    .unwrap();
    assert_eq!(released.status, "succeeded");
    assert!(released.authority_removal_completed);
    assert!(released.error.is_none());
    assert_eq!(released.unresolved_preview_work_ids, Some(Vec::new()));
    assert!(disposed.get());
    assert!(native.lock().unwrap().active().is_none());
}
