//! Actual owner plus public engine facade; instrumented surface, not AppKit.
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{
    compositor::Compositor,
    desktop_viewport::{
        AcquiredSurfaceFrame, CssBounds, DesktopViewportHost, PhysicalExtent, PresentationReceipt,
        SettledSurfaceFrame, SurfaceAttempt, SurfacePort, SurfacePresentation,
        SurfaceRecoveryError, ViewportMapping, ViewportStatus,
    },
    object_codec::decode_storage_project,
    object_document::FrameScopeKind,
    object_frame_packet::ObjectFrameSelector,
    object_snapshot::ObjectSnapshot,
    render_scene::ScheduledFrameIdentity,
    scheduler::OutputSpec,
};
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};

pub(super) fn host() -> NativeObjectHost {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut objects = vec![];
    let mut scopes = vec![];
    for frame in 0..2 {
        let mut record = cases["records"][0].clone();
        record["target"] = json!({"contextId":"scene-root", "frameScope":{"kind":"authored","frame":frame}, "layerUid":"L0", "strokeId":"a"});
        record["geometry"]["segments"] = json!([[4,4],[28,4],[28,28],[4,28]].map(|[x,y]|
            json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
        record["fill"] = json!({"kind":"solid", "r":0, "g":0, "b":1, "a":1});
        objects.push(record);
        scopes.push(json!({"frameScope":{"kind":"authored","frame":frame},
            "elements":[{"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}}]}));
    }
    let document = json!({
        "format":"nemo.native-object-document", "formatVersion":2, "totalFrames":2,
        "layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0},{"frame":1}],"samples":[]}}], "objects":objects,
        "compositionOrder":{
            "format":"nemo.native-object-order-plan", "formatVersion":1, "documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root", "layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}], "scopes":scopes}],
            "migration":"explicit-plan-required", "referencePolicy":"unresolved"
        }
    });
    NativeObjectHost::new(
        "preview-owner".into(),
        decode_storage_project(&serde_json::to_vec(&document).unwrap()).unwrap(),
    )
    .unwrap()
}
pub(super) fn snapshot(host: &NativeObjectHost) -> ObjectSnapshot {
    host.history
        .acquire_snapshot(host.history.content_revision())
        .unwrap()
}
pub(super) fn selector(snapshot: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
pub(super) fn output() -> OutputSpec {
    OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap()
}
#[derive(Clone, Copy, Default)]
pub(super) enum Mode {
    #[default]
    Present,
    Defer,
    Panic,
}
#[derive(Default)]
pub(super) struct Trace {
    pub mode: Mode,
    pub identities: Vec<ScheduledFrameIdentity>,
    pub acquisitions: usize,
    pub settlements: usize,
    pub pixel: Option<[u8; 4]>,
}
pub(super) struct Port {
    context: (wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue),
    trace: Rc<RefCell<Trace>>,
}
pub(super) struct Frame(Rc<RefCell<Trace>>, bool);
impl AcquiredSurfaceFrame for Frame {
    fn settle(self) -> SettledSurfaceFrame {
        if self.1 {
            self.0.borrow_mut().settlements += 1;
            SettledSurfaceFrame::Presented
        } else {
            SettledSurfaceFrame::ValidationFailure
        }
    }
}
fn pixel(source: &SurfacePresentation<'_>, expected: [u8; 4]) {
    let buffer = source.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("owner-preview-oracle"),
        size: 64 * 256,
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
    slice.map_async(wgpu::MapMode::Read, move |value| {
        tx.send(value).unwrap();
    });
    source
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let bytes = slice.get_mapped_range();
    assert_eq!(&bytes[(8 * 64 + 8) * 4..(8 * 64 + 8) * 4 + 4], &expected);
    assert_eq!(
        &bytes[(40 * 64 + 40) * 4..(40 * 64 + 40) * 4 + 4],
        &[255; 4]
    );
    drop(bytes);
    buffer.unmap();
}
impl SurfacePort for Port {
    type Frame<'p, 's> = Frame;
    fn acquire<'p, 's>(&'p mut self, source: SurfacePresentation<'s>) -> SurfaceAttempt<Frame> {
        let mut trace = self.trace.borrow_mut();
        trace.identities.push(source.identity.clone());
        if source.instance != &self.context.0
            || source.adapter != &self.context.1
            || source.device != &self.context.2
            || source.queue != &self.context.3
        {
            return SurfaceAttempt::Acquired(Frame(self.trace.clone(), false));
        }
        match trace.mode {
            Mode::Defer => return SurfaceAttempt::Timeout,
            Mode::Panic => panic!("injected synchronous presentation unwind"),
            Mode::Present => {}
        }
        if let Some(expected) = trace.pixel {
            pixel(&source, expected);
        }
        trace.acquisitions += 1;
        SurfaceAttempt::Acquired(Frame(self.trace.clone(), true))
    }
    fn recreate(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn reconfigure(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn dispose(&mut self) {}
}
pub(super) fn viewport(
    gpu: &Compositor,
    size: u32,
) -> (DesktopViewportHost<Port>, Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let port = Port {
        context: (
            gpu.instance().clone(),
            gpu.adapter().clone(),
            gpu.device().clone(),
            gpu.queue().clone(),
        ),
        trace: trace.clone(),
    };
    let extent = PhysicalExtent {
        width: size,
        height: size,
    };
    let mapping = ViewportMapping::new(
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
    .unwrap();
    (DesktopViewportHost::new(port, mapping), trace)
}
pub(super) fn present(
    host: &mut NativeObjectHost,
    gpu: &mut Compositor,
    view: &mut DesktopViewportHost<Port>,
) -> Result<PresentationReceipt, String> {
    host.present_object_preview(&selector(&snapshot(host), 0), output(), gpu, view)
}
pub(super) fn fill(host: &mut NativeObjectHost, id: &str, color: Value) -> Value {
    let request = json!({"apiVersion":2, "requestId":id, "instanceId":host.instance_id(), "documentId":host.document_id(),
        "expectedRevision":host.content_revision(), "operation":"command.document.object.fill.set",
        "payload":{"command":"object.fill.set", "stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"a"}, "fill":color}});
    serde_json::to_value(
        host.dispatch(serde_json::from_value(request).unwrap())
            .unwrap(),
    )
    .unwrap()
}

#[test]
fn gpu_deferred_retry_uses_same_work_and_literal_pixels_then_releases_once() {
    let mut gpu = Compositor::new().unwrap();
    let mut owner = host();
    let (mut view, trace) = viewport(&gpu, 64);
    trace.borrow_mut().mode = Mode::Defer;
    let deferred = present(&mut owner, &mut gpu, &mut view).unwrap();
    assert!(matches!(deferred.status(), ViewportStatus::Deferred(_)));
    assert_eq!(owner.preview.scheduler.lease_counters().live(), 1);
    trace.borrow_mut().mode = Mode::Present;
    trace.borrow_mut().pixel = Some([0, 0, 255, 255]);
    let shown = present(&mut owner, &mut gpu, &mut view).unwrap();
    assert_eq!(shown.identity(), deferred.identity());
    assert_eq!(shown.status(), &ViewportStatus::Presented);
    assert_eq!(
        (trace.borrow().acquisitions, trace.borrow().settlements),
        (1, 1)
    );
    assert!(owner.preview.pending.is_none());
    assert_eq!(owner.preview.scheduler.lease_counters().live(), 0);
    assert_eq!(owner.preview.scheduler.lease_counters().acquired(), 1);
    assert!(owner.release_project().unwrap().reconciliation_complete());
}

#[test]
fn gpu_invalid_selector_and_output_preserve_previous_deferred_work() {
    let mut gpu = Compositor::new().unwrap();
    let mut owner = host();
    let (mut view, trace) = viewport(&gpu, 64);
    trace.borrow_mut().mode = Mode::Defer;
    let initial = present(&mut owner, &mut gpu, &mut view).unwrap();
    let saved = snapshot(&owner);
    for field in 0..7 {
        let mut wrong = selector(&saved, 0);
        match field {
            0 => wrong.instance_id = "foreign",
            1 => wrong.document_id = "foreign",
            2 => wrong.content_revision += 1,
            3 => wrong.document_snapshot_id = "foreign",
            4 => wrong.context_id = "foreign",
            5 => wrong.scope_kind = FrameScopeKind::Reference,
            _ => wrong.frame = 2,
        }
        assert!(owner
            .present_object_preview(&wrong, output(), &mut gpu, &mut view)
            .is_err());
    }
    for field in 0..4 {
        let wrong = OutputSpec::new(
            if field == 0 { "tile" } else { "frame" },
            if field == 1 { "rgba16f" } else { "rgba8" },
            64,
            64,
            if field == 2 { "linear" } else { "srgb" },
            if field == 3 {
                "premultiplied"
            } else {
                "straight"
            },
        )
        .unwrap();
        assert!(owner
            .present_object_preview(&selector(&saved, 0), wrong, &mut gpu, &mut view)
            .is_err());
    }
    assert_eq!(trace.borrow().identities.len(), 1);
    assert_eq!(
        owner.preview.pending.as_ref().unwrap().work_id(),
        initial.identity().work_id()
    );
    assert!(owner.preview.blocked.is_none());
    assert_eq!(owner.preview.scheduler.lease_counters().live(), 1);
    assert!(owner.release_project().unwrap().reconciliation_complete());
}
