//! Real host/history/compositor pixels through a borrowed surface; not AppKit.
use crate::native_dispatch::object_host::preview::tests::{output, selector, snapshot};
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{
    compositor::Compositor,
    desktop_viewport::{
        AcquiredSurfaceFrame, CssBounds, DesktopViewportHost, PhysicalExtent, SettledSurfaceFrame,
        SurfaceAttempt, SurfacePort, SurfacePresentation, SurfaceRecoveryError, ViewportMapping,
        ViewportStatus,
    },
};
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc};

const BLUE:&str="89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000d49444154789c636060f80f060014f405fbb956037d0000000049454e44ae426082";
fn host() -> NativeObjectHost {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"a"});
    record["geometry"]["segments"] =
        json!([[24,24],[40,24],[40,40],[24,40]].map(|[x,y]|
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    record["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
    NativeObjectHost::from_project_file("reference-preview-host".into(),&serde_json::to_vec(&json!({
        "format":"nemo.native-project-document","formatVersion":1,"project":{"canvasW":64,"canvasH":64,"fps":24},"viewportReference":null,"assets":[],
        "drawing":{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":1,"layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],"objects":[record],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[{"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}}]}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}}})).unwrap()).unwrap()
}
fn reference(owner: &mut NativeObjectHost, kind: &str, payload: Value, png: Option<&[u8]>) {
    let request=serde_json::from_value(json!({"apiVersion":2,"requestId":format!("{kind}-{}",owner.content_revision()),
        "instanceId":owner.instance_id(),"documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
        "operation":format!("command.document.reference.{kind}"),"payload":payload})).unwrap();
    let pin = snapshot(owner);
    let result = owner
        .project_reference(pin.snapshot_id(), &request, png)
        .unwrap();
    assert!(result.is_ok());
}
fn attach(owner: &mut NativeObjectHost) {
    let bytes: Vec<_> = BLUE
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect();
    reference(
        owner,
        "attach",
        json!({"id":"ref","name":"reference.png"}),
        Some(&bytes),
    );
}
#[derive(Default)]
struct Trace {
    defer: bool,
    acquired: usize,
    settled: usize,
    pixels: Vec<Vec<u8>>,
}
struct Port(Rc<RefCell<Trace>>);
struct Frame(Rc<RefCell<Trace>>);
impl AcquiredSurfaceFrame for Frame {
    fn settle(self) -> SettledSurfaceFrame {
        self.0.borrow_mut().settled += 1;
        SettledSurfaceFrame::Presented
    }
}
impl SurfacePort for Port {
    type Frame<'p, 's> = Frame;
    fn acquire<'p, 's>(&'p mut self, source: SurfacePresentation<'s>) -> SurfaceAttempt<Frame> {
        if self.0.borrow().defer {
            return SurfaceAttempt::Timeout;
        }
        let buffer = source.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("reference-host-pixel-oracle"),
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
        slice.map_async(wgpu::MapMode::Read, move |result| tx.send(result).unwrap());
        source
            .device
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .unwrap()
            .unwrap();
        let pixels = slice.get_mapped_range().to_vec();
        buffer.unmap();
        let mut trace = self.0.borrow_mut();
        trace.acquired += 1;
        trace.pixels.push(pixels);
        SurfaceAttempt::Acquired(Frame(self.0.clone()))
    }
    fn recreate(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn reconfigure(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn dispose(&mut self) {}
}
fn viewport() -> (DesktopViewportHost<Port>, Rc<RefCell<Trace>>) {
    let trace = Rc::new(RefCell::new(Trace::default()));
    let extent = PhysicalExtent {
        width: 64,
        height: 64,
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
    (
        DesktopViewportHost::new(Port(trace.clone()), mapping),
        trace,
    )
}
fn present(
    owner: &mut NativeObjectHost,
    gpu: &mut Compositor,
    viewport: &mut DesktopViewportHost<Port>,
) {
    assert_eq!(
        owner
            .present_object_preview(&selector(&snapshot(owner), 0), output(), gpu, viewport)
            .unwrap()
            .status(),
        &ViewportStatus::Presented
    );
}
fn pixel(trace: &Rc<RefCell<Trace>>, x: usize, y: usize, expected: [u8; 4]) {
    let trace = trace.borrow();
    let bytes = trace.pixels.last().unwrap();
    let actual = &bytes[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
    assert!(
        actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
        "{actual:?} != {expected:?}"
    );
}

#[test]
fn gpu_original_host_presents_reference_reuses_deferred_work_and_drops_cache_on_release() {
    let mut owner = host();
    attach(&mut owner);
    let mut gpu = Compositor::new().unwrap();
    let (mut viewport, trace) = viewport();
    trace.borrow_mut().defer = true;
    let deferred = owner
        .present_object_preview(
            &selector(&snapshot(&owner), 0),
            output(),
            &mut gpu,
            &mut viewport,
        )
        .unwrap();
    assert!(matches!(deferred.status(), ViewportStatus::Deferred(_)));
    assert_eq!(owner.preview.scheduler.lease_counters().live(), 1);
    assert!(owner
        .preview
        .reference_cache
        .as_ref()
        .unwrap()
        .has_viewport_reference());
    trace.borrow_mut().defer = false;
    let shown = owner
        .present_object_preview(
            &selector(&snapshot(&owner), 0),
            output(),
            &mut gpu,
            &mut viewport,
        )
        .unwrap();
    assert_eq!(shown.identity(), deferred.identity());
    assert_eq!(shown.status(), &ViewportStatus::Presented);
    pixel(&trace, 8, 32, [128, 128, 255, 255]);
    pixel(&trace, 8, 8, [255; 4]);
    pixel(&trace, 32, 32, [255, 0, 0, 255]);
    assert_eq!(owner.preview.scheduler.lease_counters().live(), 0);
    let old_cache = owner.preview.reference_cache.as_ref().unwrap().clone();
    let current = snapshot(&owner);
    let mut bad = selector(&current, 0);
    bad.document_snapshot_id = "foreign";
    assert!(owner
        .present_object_preview(&bad, output(), &mut gpu, &mut viewport)
        .is_err());
    let bad_output =
        native_engine::scheduler::OutputSpec::new("frame", "rgba8", 4097, 64, "srgb", "straight")
            .unwrap();
    assert!(owner
        .present_object_preview(&selector(&current, 0), bad_output, &mut gpu, &mut viewport)
        .is_err());
    assert!(owner
        .preview
        .reference_cache
        .as_ref()
        .unwrap()
        .same_identity_as(&old_cache));
    assert_eq!(owner.preview.scheduler.lease_counters().live(), 0);
    drop(old_cache);
    reference(
        &mut owner,
        "settings",
        json!({"name":"reference.png","visible":false,"opacity":0.5,"offsetFrames":0}),
        None,
    );
    present(&mut owner, &mut gpu, &mut viewport);
    pixel(&trace, 8, 32, [255; 4]);
    assert!(owner
        .preview
        .reference_cache
        .as_ref()
        .unwrap()
        .has_viewport_reference());
    owner.history.undo(owner.content_revision()).unwrap();
    owner.preview.invalidate();
    present(&mut owner, &mut gpu, &mut viewport);
    pixel(&trace, 8, 32, [128, 128, 255, 255]);
    let saved = owner
        .project_file_snapshot(owner.content_revision(), snapshot(&owner).snapshot_id())
        .unwrap()
        .unwrap();
    assert!(owner.release_project().unwrap().reconciliation_complete());
    assert!(owner.preview.reference_cache.is_none());
    let mut reopened =
        NativeObjectHost::from_project_file("reference-reopened-host".into(), &saved).unwrap();
    let (mut new_view, new_trace) = self::viewport();
    present(&mut reopened, &mut gpu, &mut new_view);
    pixel(&new_trace, 8, 32, [128, 128, 255, 255]);
    assert!(reopened
        .release_project()
        .unwrap()
        .reconciliation_complete());
}

#[test]
fn cpu_reference_preview_refuses_wrong_pins_before_target_and_preserves_deferred_cache() {
    let mut owner = host();
    attach(&mut owner);
    let original = snapshot(&owner);
    let mut bad = selector(&original, 0);
    bad.document_snapshot_id = "foreign";
    assert!(owner
        .with_prepared_object_preview::<()>(&bad, output(), |_| panic!(
            "target factory must not run"
        ))
        .is_err());
    assert!(owner.preview.reference_cache.is_none());
    assert!(owner.preview.prepared.is_none());
    owner
        .with_prepared_object_preview(&selector(&original, 0), output(), |_| Ok(()))
        .unwrap();
    // CPU pins alone cannot retain/decode pixels before the original compositor
    // has retired a potentially old Vello image cache.
    assert!(owner.preview.reference_cache.is_none());
    assert!(owner.release_project().unwrap().reconciliation_complete());
    assert!(owner.preview.reference_cache.is_none());
}

#[test]
fn gpu_reference_replacement_decode_failure_drops_stale_caches_and_retries() {
    let mut owner = host();
    attach(&mut owner);
    let mut gpu = Compositor::new().unwrap();
    let device = gpu.device().clone();
    let (mut viewport, trace) = viewport();
    present(&mut owner, &mut gpu, &mut viewport);
    pixel(&trace, 8, 32, [128, 128, 255, 255]);
    let red = "89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000e49444154789c63f8cfc0f01f040110f803fd4e95c16f0000000049454e44ae426082";
    let bytes: Vec<_> = red
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect();
    reference(
        &mut owner,
        "attach",
        json!({"id":"ref","name":"replacement.png"}),
        Some(&bytes),
    );
    let current = snapshot(&owner);
    let prepared = owner
        .prepare_object_preview(&selector(&current, 0), output(), false)
        .unwrap();
    assert!(owner.preview.reference_cache.is_some());
    let count = trace.borrow().acquired;
    let failed = owner.prepare_reference_preview(&prepared.plan, (64, 64), &mut gpu, |previous| {
        assert!(
            previous.is_none(),
            "old host raster must be gone before replacement decode"
        );
        Err("injected decoder capacity refusal".into())
    });
    assert!(failed.unwrap_err().contains("capacity refusal"));
    assert!(owner.preview.reference_cache.is_none());
    assert!(owner.preview.pending.is_none());
    assert!(owner.preview.blocked.is_none());
    assert_eq!(
        trace.borrow().acquired,
        count,
        "failed decode must not present"
    );
    assert_eq!(gpu.device(), &device);
    present(&mut owner, &mut gpu, &mut viewport);
    pixel(&trace, 8, 32, [255, 128, 128, 255]);
    owner.history.undo(owner.content_revision()).unwrap();
    owner.preview.invalidate();
    present(&mut owner, &mut gpu, &mut viewport);
    pixel(&trace, 8, 32, [128, 128, 255, 255]);
    assert!(owner.release_project().unwrap().reconciliation_complete());
    assert!(owner.preview.reference_cache.is_none());
}

#[path = "native_object_exposure_tests.rs"]
mod exposure_tests;
