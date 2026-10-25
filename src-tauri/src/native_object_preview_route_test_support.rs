//! Shared authority/barrier route with actual owners and a borrowed Metal target.
use crate::application_mcp::revision_sync::Delivery;
use crate::application_mcp::{
    object_client::{admit_raw, dispatch_raw},
    ApplicationMcp,
};
use crate::native_application::{admit_release_request, complete_release};
use crate::native_application_contract::{NativeReleaseReceipt, NativeReleaseRequest};
use crate::native_dispatch::{NativeDispatch, NativeReleaseProgress, ReleaseAdmission};
use native_engine::{
    commands::{NativeOpacityApplication, OpacityRequest, ResponseEnvelope},
    compositor::Compositor,
    desktop_viewport::{
        AcquiredSurfaceFrame, CssBounds, DesktopViewportHost, PhysicalExtent, PresentationReceipt,
        SettledSurfaceFrame, SurfaceAttempt, SurfacePort, SurfacePresentation,
        SurfaceRecoveryError, ViewportMapping,
    },
    document::OpacityDocument,
    export_job::{JobReceipt, PendingFrame},
    object_codec::decode_storage_project,
    object_document::FrameScopeKind,
    object_frame_packet::ObjectFrameSelector,
    scheduler::OutputSpec,
};
use serde_json::{json, Value};
use std::{any::Any, cell::Cell, rc::Rc};

fn document() -> Value {
    let source: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = source["records"][0].clone();
    record["target"] = target();
    record["geometry"]["segments"] =
        json!([[4,4],[28,4],[28,28],[4,28]].map(|[x,y]|
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    record["fill"] = json!({"kind":"solid", "r":0, "g":0, "b":1, "a":1});
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":1,
        "layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],"objects":[record],"compositionOrder":{
        "format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
        "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[{"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}}]}]}],
        "migration":"explicit-plan-required","referencePolicy":"unresolved"}})
}
fn target() -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"a"})
}
pub(super) fn output() -> OutputSpec {
    OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap()
}
#[derive(Clone)]
pub(super) struct Pins {
    pub(super) instance: String,
    pub(super) document: String,
    pub(super) snapshot: String,
    pub(super) revision: u64,
    pub(super) generation: u64,
}
impl Pins {
    pub(super) fn selector(&self) -> ObjectFrameSelector<'_> {
        ObjectFrameSelector {
            instance_id: &self.instance,
            document_id: &self.document,
            content_revision: self.revision,
            document_snapshot_id: &self.snapshot,
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame: 0,
        }
    }
}
pub(super) struct Port {
    device: wgpu::Device,
    pub(super) mode: Rc<Cell<u8>>,
    pub(super) calls: Rc<Cell<usize>>,
    guards: Box<dyn Fn()>,
    expected_pixel: Rc<Cell<[u8; 4]>>,
}
pub(super) struct Frame;
impl AcquiredSurfaceFrame for Frame {
    fn settle(self) -> SettledSurfaceFrame {
        SettledSurfaceFrame::Presented
    }
}
fn pixel(source: &SurfacePresentation<'_>, expected: [u8; 4]) {
    let buffer = source.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("shared-preview-route-oracle"),
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
        (self.guards)();
        self.calls.set(self.calls.get() + 1);
        assert_eq!(source.device, &self.device);
        assert_eq!(source.texture.width(), 64);
        pixel(&source, self.expected_pixel.get());
        match self.mode.get() {
            1 => SurfaceAttempt::Timeout,
            2 => panic!("route consumer unwind"),
            _ => SurfaceAttempt::Acquired(Frame),
        }
    }
    fn recreate(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn reconfigure(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn dispose(&mut self) {}
}
pub(super) struct Fixture {
    pub(super) state: ApplicationMcp,
    pub(super) pins: Pins,
    pub(super) gpu: Compositor,
    pub(super) view: DesktopViewportHost<Port>,
    pub(super) mode: Rc<Cell<u8>>,
    pub(super) calls: Rc<Cell<usize>>,
    pub(super) expected_pixel: Rc<Cell<[u8; 4]>>,
}
impl Fixture {
    pub(super) fn new() -> Self {
        let state = ApplicationMcp::default();
        state
            .install_native_object(
                decode_storage_project(&serde_json::to_vec(&document()).unwrap()).unwrap(),
            )
            .unwrap();
        Self::from_state(state)
    }
    pub(super) fn cut_project() -> Self {
        let mut drawing = document();
        drawing["totalFrames"] = json!(4);
        drawing["layers"][0]["timing"]["keys"] = json!([{"frame":0},{"frame":3}]);
        drawing["layers"][0]["groups"] = json!({"contextId":"scene-root",
            "entries":[{"gid":"pair","name":"Pair","combineMode":"none","order":["a","b"]}],
            "members":[{"strokeId":"a","groupId":"pair"},{"strokeId":"b","groupId":"pair"}]});
        let mut a = drawing["objects"][0].clone();
        a["family"] = json!("editable-vector-path");
        a["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":3,
            "strokeCap":"round","strokeJoin":"round","miterLimit":10,"dashOffset":0,"opacity":1});
        let mut b = a.clone();
        b["target"]["strokeId"] = json!("b");
        let mut survivor = drawing["objects"][0].clone();
        survivor["target"]["strokeId"] = json!("survivor");
        survivor["fill"] = json!({"kind":"solid","r":1,"g":0,"b":0,"a":1});
        let mut later = a.clone();
        later["target"]["frameScope"]["frame"] = json!(3);
        drawing["objects"] = json!([a, b, survivor, later]);
        drawing["propertyHolders"] = json!(["a", "b"].map(|id| json!({
            "target":{"contextId":"scene-root","layerUid":"L0","strokeId":id},
            "properties":[{"property":"position","static":[0,0]}]})));
        drawing["compositionOrder"]["contexts"][0]["scopes"] = json!([
            {"frameScope":{"kind":"authored","frame":0},"elements":[
                {"layerUid":"L0","strokeId":"survivor","naturalPosition":0,"order":{}},
                {"layerUid":"L0","strokeId":"a","naturalPosition":1,"order":{}},
                {"layerUid":"L0","strokeId":"b","naturalPosition":2,"order":{}}]},
            {"frameScope":{"kind":"authored","frame":3},"elements":[
                {"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}}]}]);
        let project = json!({"format":"nemo.native-project-document","formatVersion":1,
            "project":{"canvasW":64,"canvasH":64,"fps":24},"drawing":drawing,"viewportReference":null});
        let state = ApplicationMcp::default();
        state
            .install_native_project_file(&serde_json::to_vec(&project).unwrap())
            .unwrap();
        Self::from_state(state)
    }
    fn from_state(state: ApplicationMcp) -> Self {
        let gpu = Compositor::new().unwrap();
        let mode = Rc::new(Cell::new(0));
        let calls = Rc::new(Cell::new(0));
        let expected_pixel = Rc::new(Cell::new([0, 0, 255, 255]));
        let native = state.native.clone();
        let revisions = state.revisions.0.clone();
        let port = Port {
            device: gpu.device().clone(),
            mode: mode.clone(),
            calls: calls.clone(),
            expected_pixel: expected_pixel.clone(),
            guards: Box::new(move || {
                assert!(matches!(
                    native.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ));
                assert!(matches!(
                    revisions.try_lock(),
                    Err(std::sync::TryLockError::WouldBlock)
                ));
            }),
        };
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
        let mut value = Self {
            state,
            gpu,
            view: DesktopViewportHost::new(port, mapping),
            mode,
            calls,
            expected_pixel,
            pins: Pins {
                instance: String::new(),
                document: String::new(),
                snapshot: String::new(),
                revision: 0,
                generation: 0,
            },
        };
        value.refresh();
        value
    }
    pub(super) fn reinstall(&mut self) {
        self.state
            .install_native_object(
                decode_storage_project(&serde_json::to_vec(&document()).unwrap()).unwrap(),
            )
            .unwrap();
        self.refresh();
    }
    pub(super) fn refresh(&mut self) {
        self.refresh_for(target());
    }
    pub(super) fn refresh_for(&mut self, stable_target: Value) {
        let guard = self.state.native.lock().unwrap();
        let (generation, owner) = guard.active().unwrap();
        let mut p = Pins {
            instance: owner.instance_id().into(),
            document: owner.document_id().into(),
            revision: owner.content_revision(),
            generation,
            snapshot: String::new(),
        };
        drop(guard);
        let read = json!({"apiVersion":2,"requestId":"preview-pins","instanceId":p.instance,"documentId":p.document,
            "operation":"query.document.object","payload":{"atRevision":p.revision,"stableTarget":stable_target}});
        let response = dispatch_raw(&self.state, &read.to_string()).unwrap();
        assert!(response.ok);
        p.snapshot = response.result.unwrap()["documentSnapshotId"]
            .as_str()
            .unwrap()
            .into();
        self.pins = p;
    }
    pub(super) fn run(&mut self) -> Result<PresentationReceipt, String> {
        self.state.present_object_preview(
            self.pins.generation,
            &self.pins.selector(),
            output(),
            &mut self.gpu,
            &mut self.view,
        )
    }
    pub(super) fn command(&self, operation: &str, payload: Value, external: bool) -> Delivery {
        self.command_named(
            &format!("preview-edit-{}", self.pins.revision),
            operation,
            payload,
            external,
        )
    }
    pub(super) fn command_named(
        &self,
        request_id: &str,
        operation: &str,
        payload: Value,
        external: bool,
    ) -> Delivery {
        let request = json!({"apiVersion":2,"requestId":request_id,"instanceId":self.pins.instance,
            "documentId":self.pins.document,"expectedRevision":self.pins.revision,"operation":operation,"payload":payload});
        self.state
            .revisions
            .dispatch_object(
                self.state.instance_id(),
                &self.state.native,
                admit_raw(&request.to_string()).unwrap(),
                external,
            )
            .unwrap()
    }
    pub(super) fn history(&self) -> Value {
        let request = json!({"apiVersion":2,"requestId":"preview-history","instanceId":self.pins.instance,
            "documentId":self.pins.document,"operation":"query.document.object.history",
            "payload":{"atRevision":self.pins.revision,"documentSnapshotId":self.pins.snapshot}});
        let response = dispatch_raw(&self.state, &request.to_string()).unwrap();
        assert!(response.ok);
        response.result.unwrap()
    }
    pub(super) fn saved_project(&self) -> Value {
        let mut authority = self.state.native.lock().unwrap();
        let owner = authority
            .active_mut(self.pins.generation)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<crate::native_dispatch::NativeObjectHost>()
            .unwrap();
        let bytes = owner
            .project_file_snapshot(self.pins.revision, &self.pins.snapshot)
            .unwrap()
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
    pub(super) fn expected_cut_project(before: &Value) -> Value {
        let mut expected = before.clone();
        expected["drawing"]["objects"]
            .as_array_mut()
            .unwrap()
            .drain(..2);
        expected["drawing"]["propertyHolders"]
            .as_array_mut()
            .unwrap()
            .remove(1);
        expected["drawing"]["layers"][0]["groups"]["entries"] = json!([]);
        expected["drawing"]["layers"][0]["groups"]["members"] = json!([]);
        expected["drawing"]["compositionOrder"]["contexts"][0]["scopes"][0]["elements"] = json!([
            {"layerUid":"L0","strokeId":"survivor","naturalPosition":0,"order":{}}]);
        expected
    }
    pub(super) fn assert_invalid_cut_proofs(&self, subscription_id: &str, event: &Value) {
        let selector = self.pins.selector();
        let mut stale = event.clone();
        stale["toRevision"] = json!(0);
        for (subscriber, proof) in [("foreign", event.clone()), (subscription_id, stale)] {
            assert!(self
                .state
                .with_object_preview_owner_for_preview(
                    self.pins.generation,
                    &selector,
                    Some((subscriber, &proof)),
                    |_| -> Result<(), String> {
                        panic!("invalid proof must not borrow preview owner")
                    }
                )
                .unwrap_err()
                .contains("pending"));
        }
    }
    pub(super) fn release(&mut self) -> NativeReleaseReceipt {
        let request = NativeReleaseRequest {
            api_version: 2,
            request_id: "preview-release".into(),
            instance_id: self.pins.instance.clone(),
            document_id: self.pins.document.clone(),
            expected_revision: self.pins.revision,
            cancelled_before_dispatch: false,
        };
        let ReleaseAdmission::Execute { generation } =
            admit_release_request(&self.state.native, &request).unwrap()
        else {
            panic!("fresh release expected")
        };
        let receipt = complete_release(&self.state.native, generation, &request, || {
            Ok((self.view.dispose(), "disposed"))
        })
        .unwrap();
        assert!(receipt.reentry_available);
        receipt
    }
}
pub(super) fn fill(red: u8) -> Value {
    json!({"command":"object.fill.set","stableTarget":target(),"fill":{"kind":"solid","r":red,"g":0,"b":0,"a":1}})
}

pub(super) struct Opacity(pub(super) NativeOpacityApplication);
impl NativeDispatch for Opacity {
    fn instance_id(&self) -> &str {
        self.0.instance_id()
    }
    fn document_id(&self) -> &str {
        self.0.document_id()
    }
    fn content_revision(&self) -> u64 {
        self.0.content_revision()
    }
    fn dispatch(&mut self, request: OpacityRequest) -> Result<ResponseEnvelope, String> {
        Ok(self.0.handle(request))
    }
    fn replace_document(&mut self, _: OpacityDocument) -> Result<Vec<JobReceipt>, String> {
        Err("unused".into())
    }
    fn start_next_export_frame(&mut self, _: &str) -> Result<Option<PendingFrame>, String> {
        Err("unused".into())
    }
    fn finish_export_frame(&mut self, _: PendingFrame) -> Result<JobReceipt, String> {
        Err("unused".into())
    }
    fn release_project(&mut self) -> Result<NativeReleaseProgress, String> {
        Err("unused".into())
    }
    fn release_progress(&self) -> Option<NativeReleaseProgress> {
        None
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
