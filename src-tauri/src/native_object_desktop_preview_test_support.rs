use crate::application_mcp::ApplicationMcp;
use crate::native_application::{admit_release_request, complete_release};
use crate::native_application_contract::{host_error, NativeReleaseReceipt, NativeReleaseRequest};
use crate::native_application_viewport::TestObjectSlot;
use crate::native_dispatch::ReleaseAdmission;
use crate::native_object_desktop_preview::{admit_request, preview_with_target};
use native_engine::{
    compositor::Compositor,
    desktop_viewport::{
        AcquiredSurfaceFrame, DesktopViewportHost, SettledSurfaceFrame, SurfaceAttempt,
        SurfacePort, SurfacePresentation, SurfaceRecoveryError, ViewportMapping,
    },
    object_codec::decode_storage_project,
};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

pub(super) fn target() -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"a"})
}
pub(super) fn document() -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = target();
    record["geometry"]["segments"] =
        json!([[4,4],[28,4],[28,28],[4,28]].map(|[x,y]|
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    record["fill"] = json!({"kind":"solid","r":0,"g":0,"b":1,"a":1});
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":1,
        "layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],"objects":[record],"compositionOrder":{
        "format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
        "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[{"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}}]}]}],
        "migration":"explicit-plan-required","referencePolicy":"unresolved"}})
}
#[derive(Default)]
pub(super) struct Trace {
    pub(super) acquisitions: usize,
    pub(super) dispositions: usize,
    pub(super) mode: u8,
    pub(super) color: [u8; 4],
}
struct Port {
    context: (wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue),
    state: Arc<ApplicationMcp>,
    trace: Rc<RefCell<Trace>>,
}
struct Frame;
impl AcquiredSurfaceFrame for Frame {
    fn settle(self) -> SettledSurfaceFrame {
        SettledSurfaceFrame::Presented
    }
}
impl SurfacePort for Port {
    type Frame<'p, 's> = Frame;
    fn acquire<'p, 's>(&'p mut self, source: SurfacePresentation<'s>) -> SurfaceAttempt<Frame> {
        assert!(self.state.preview_locks_held_for_test());
        assert_eq!(source.instance, &self.context.0);
        assert_eq!(source.adapter, &self.context.1);
        assert_eq!(source.device, &self.context.2);
        assert_eq!(source.queue, &self.context.3);
        assert_eq!(source.texture.width(), 320);
        let mut trace = self.trace.borrow_mut();
        trace.acquisitions += 1;
        crate::native_object_desktop_preview::tests::pixel(&source, trace.color);
        match trace.mode {
            1 => SurfaceAttempt::Timeout,
            2 => panic!("desktop preview callback unwind"),
            _ => SurfaceAttempt::Acquired(Frame),
        }
    }
    fn recreate(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn reconfigure(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn dispose(&mut self) {
        self.trace.borrow_mut().dispositions += 1;
    }
}
pub(super) struct View {
    host: DesktopViewportHost<Port>,
}
pub(super) struct Fixture {
    pub(super) state: Arc<ApplicationMcp>,
    pub(super) raw: Value,
    pub(super) trace: Rc<RefCell<Trace>>,
    pub(super) creations: Rc<Cell<usize>>,
    slot: TestObjectSlot<View>,
}
impl Fixture {
    pub(super) fn new() -> Self { Self::with_document(document()) }
    pub(super) fn with_document(source: Value) -> Self {
        let state = Arc::new(ApplicationMcp::default());
        state
            .install_native_object(
                decode_storage_project(&serde_json::to_vec(&source).unwrap()).unwrap(),
            )
            .unwrap();
        let mut fixture = Self {
            state,
            raw: json!({}),
            trace: Rc::new(RefCell::new(Trace {
                color: [0, 0, 255, 255],
                ..Default::default()
            })),
            creations: Rc::new(Cell::new(0)),
            slot: TestObjectSlot::new(),
        };
        fixture.refresh();
        fixture
    }
    pub(super) fn refresh(&mut self) {
        let native = self.state.native_state();
        let guard = native.lock().unwrap();
        let (generation, owner) = guard.active().unwrap();
        let (document, revision) = (owner.document_id().to_owned(), owner.content_revision());
        drop(guard);
        let query = json!({"apiVersion":2,"requestId":"desktop-pins","instanceId":self.state.instance_id(),"documentId":document,
            "operation":"query.document.object","payload":{"atRevision":revision,"stableTarget":target()}});
        let response = self
            .state
            .serialize_object_file_snapshot(&query.to_string())
            .unwrap();
        assert!(response.ok);
        self.raw = json!({"apiVersion":2,"requestId":"desktop-preview","instanceId":self.state.instance_id(),"documentId":document,
            "contentRevision":revision,"documentSnapshotId":response.result.unwrap()["documentSnapshotId"],"lifecycleGeneration":generation,
            "contextId":"scene-root","scopeKind":"authored","frame":0,
            "outputSpec":{"kind":"frame","format":"rgba8","width":320,"height":180,"colorInterpretation":"srgb","alphaMode":"straight"},
            "viewport":{"cssBounds":{"x":0,"y":0,"width":320,"height":180},"physicalExtent":{"width":320,"height":180},
                "compositionExtent":{"width":320,"height":180},"reportedDpr":1}});
    }
    pub(super) fn request(&self, operation: &str, payload: Value) -> String {
        json!({"apiVersion":2,"requestId":format!("desktop-command-{}",self.raw["contentRevision"]),"instanceId":self.raw["instanceId"],
            "documentId":self.raw["documentId"],"expectedRevision":self.raw["contentRevision"],"operation":operation,"payload":payload}).to_string()
    }
    pub(super) fn run(
        &self,
        raw: &str,
    ) -> crate::native_application_contract::HostResult<
        crate::native_application_contract::ObjectPreviewReceipt,
    > {
        let request = admit_request(raw, self.state.instance_id())?;
        preview_with_target(&self.state, &request, |consumer| {
            self.slot.with_target(
                &request.instance_id,
                request.lifecycle_generation,
                || {
                    assert!(self.state.preview_locks_held_for_test());
                    self.creations.set(self.creations.get() + 1);
                    let gpu =
                        Compositor::new().map_err(|e| host_error("unavailable", e.to_string()))?;
                    let port = Port {
                        context: (
                            gpu.instance().clone(),
                            gpu.adapter().clone(),
                            gpu.device().clone(),
                            gpu.queue().clone(),
                        ),
                        state: self.state.clone(),
                        trace: self.trace.clone(),
                    };
                    Ok((
                        View {
                            host: DesktopViewportHost::new(port, request.viewport.admit()?),
                        },
                        gpu,
                    ))
                },
                |view, gpu| {
                    consumer
                        .present(gpu, &mut view.host)
                        .map_err(|e| host_error("unavailable", e))
                },
            )
        })
    }
    pub(super) fn release(&self) -> NativeReleaseReceipt {
        let request: NativeReleaseRequest=serde_json::from_value(json!({"apiVersion":2,"requestId":format!("desktop-release-{}",self.raw["lifecycleGeneration"]),
            "instanceId":self.raw["instanceId"],"documentId":self.raw["documentId"],"expectedRevision":self.raw["contentRevision"]})).unwrap();
        let native = self.state.native_state();
        let ReleaseAdmission::Execute { generation } =
            admit_release_request(&native, &request).unwrap()
        else {
            panic!("fresh release")
        };
        complete_release(&native, generation, &request, || {
            self.slot
                .release(self.state.instance_id(), |view| view.host.dispose())
                .map(|work| (work, "disposed"))
        })
        .unwrap()
    }
}
pub(super) fn fill(red: u8) -> Value {
    json!({"command":"object.fill.set","stableTarget":target(),"fill":{"kind":"solid","r":red,"g":0,"b":0,"a":1}})
}
