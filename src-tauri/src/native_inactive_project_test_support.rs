//! Shared actual-owner fixtures for inactive preparation controls; no product code.
use crate::native_application_ports::{DesktopArtifactPort, SharedCompositor};
use crate::native_application_viewport as viewport;
use crate::{
    application_mcp::ApplicationMcp,
    native_application::DesktopNativeApplication,
    native_application_contract::{
        admit_project, GeometryResourceInput, HostResult, HOST_API_VERSION,
    },
    native_dispatch::{
        inactive_project::{
            cancel, prepare_with, CancelRequest, CancellationReceipt, PreparationReceipt,
        },
        NativeState,
    },
};

use native_engine::{commands::OpacityRequest, compositor::Compositor};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, OnceLock,
    },
};

pub(super) fn ordinary_b() -> Value {
    static B: OnceLock<Value> = OnceLock::new();
    B.get_or_init(|| {
        // Exercise the real already-supported frontend projection, not a second decoder.
        let output = Command::new("node").arg("-e").arg(
            "const f=require('fs'),d=require('./src/js/project-document.js');let x=JSON.parse(f.readFileSync('tests/animation/fixtures/curve-workflow.json'));x.layers[0].motionStatic={opacity:[60]};let b=d.prepareNativeOpacity(JSON.stringify(x));process.stdout.write(JSON.stringify({projection:b.projection,resources:b.resources}));"
        ).current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent().unwrap()).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        serde_json::from_slice(&output.stdout).unwrap()
    }).clone()
}

pub(super) struct Fixture {
    pub(super) state: ApplicationMcp,
    root: PathBuf,
    pub(super) generation: u64,
    pub(super) document: String,
}
impl Fixture {
    pub(super) fn new() -> Self {
        let state = ApplicationMcp::default();
        let root = std::env::temp_dir().join(format!("nemo-p39a-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let mut a = ordinary_b();
        a["projection"]["layers"][0]["motionStatic"]["opacity"] = json!([25]);
        let resources: Vec<GeometryResourceInput> =
            serde_json::from_value(a["resources"].clone()).unwrap();
        let admitted = admit_project(&a["projection"], &resources).unwrap();
        let app = DesktopNativeApplication::new(
            state.instance_id().into(),
            admitted,
            DesktopArtifactPort::new(
                root.join("staging"),
                std::iter::empty::<(String, PathBuf)>(),
            )
            .unwrap(),
            SharedCompositor::new(Compositor::new().unwrap()),
        )
        .unwrap();
        let document = app.document_id().into();
        let reservation = state.reserve_native_install().unwrap();
        let generation = reservation.generation();
        state.install_dispatch(generation, Box::new(app)).unwrap();
        Self {
            state,
            root,
            generation,
            document,
        }
    }
    pub(super) fn native(&self) -> NativeState {
        self.state.native_state()
    }
    pub(super) fn value(&self, id: &str) -> Value {
        let b = ordinary_b();
        json!({"apiVersion":HOST_API_VERSION,"requestId":id,"instanceId":self.state.instance_id(),
            "documentId":self.document,"expectedRevision":self.revision(),"lifecycleGeneration":self.generation,
            "projection":b["projection"],"resources":b["resources"]})
    }
    pub(super) fn revision(&self) -> u64 {
        self.native()
            .lock()
            .unwrap()
            .active()
            .unwrap()
            .1
            .content_revision()
    }
    pub(super) fn prepare(&self, v: Value) -> HostResult<PreparationReceipt> {
        prepare_with(
            &self.native(),
            self.state.instance_id(),
            "main",
            serde_json::from_value(v).unwrap(),
            || {},
        )
    }
    pub(super) fn cancellation(&self, token: &str, id: &str) -> CancelRequest {
        serde_json::from_value(json!({"apiVersion":HOST_API_VERSION,"requestId":id,"instanceId":self.state.instance_id(),
            "documentId":self.document,"expectedRevision":self.revision(),"lifecycleGeneration":self.generation,
            "preparationToken":token})).unwrap()
    }
    pub(super) fn cancel(&self, token: &str, id: &str) -> HostResult<CancellationReceipt> {
        cancel(
            &self.native(),
            self.state.instance_id(),
            "main",
            self.cancellation(token, id),
        )
    }
    pub(super) fn dispatch(&self, request: OpacityRequest) -> Value {
        serde_json::to_value(
            self.native()
                .lock()
                .unwrap()
                .active_mut(self.generation)
                .unwrap()
                .dispatch(request)
                .unwrap(),
        )
        .unwrap()
    }
    pub(super) fn edit(&self, id: &str, value: u8) {
        let result = self.dispatch(OpacityRequest::command(id, self.state.instance_id(), &self.document, self.revision(),
            json!({"command":"layer.opacity.set","stableTarget":{"layerUid":"r08_curve_layer"},"value":value})));
        assert_eq!(result["ok"], true);
    }
    pub(super) fn snapshot(&self, id: &str) -> Value {
        let revision = self.revision();
        let serialize = self.dispatch(OpacityRequest::query(
            format!("bytes-{id}"),
            self.state.instance_id(),
            &self.document,
            "query.document.serialize",
            json!({"atRevision":revision}),
        ));
        let snapshot = self.dispatch(OpacityRequest::query(
            format!("snapshot-{id}"),
            self.state.instance_id(),
            &self.document,
            "query.document.snapshot.acquire",
            json!({"atRevision":revision}),
        ));
        assert_eq!(serialize["ok"], true);
        assert_eq!(snapshot["ok"], true);
        json!({"instance":self.state.instance_id(),"document":self.document,"generation":self.native().lock().unwrap().active_generation().unwrap(),
            "authorityCounter":self.native().lock().unwrap().generation,
            "revision":revision,"bytes":serialize["result"],"snapshot":snapshot["result"]})
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
pub(super) fn code<T>(result: HostResult<T>) -> String {
    result.err().expect("must refuse").code
}

pub(super) struct ViewportSpy(pub(super) Arc<AtomicUsize>);
impl viewport::TestViewportDriver for ViewportSpy {
    fn present(
        &mut self,
        _: &Compositor,
        _: &viewport::TestCompositionResult,
        _: viewport::TestFrameIdentity,
    ) -> HostResult<viewport::Presentation> {
        self.0.fetch_add(1, Ordering::SeqCst);
        panic!("preparation must not present")
    }
    fn retire_replacement(&mut self) -> Vec<viewport::TestWorkId> {
        self.0.fetch_add(1, Ordering::SeqCst);
        vec![]
    }
    fn rebind_existing(&mut self, _: &Compositor) -> HostResult<()> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}
