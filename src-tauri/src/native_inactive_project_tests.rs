//! Actual desktop owner and ordinary legacy-opacity preparation, independent oracles.
use super::*;
use crate::native_application_ports::{DesktopArtifactPort, SharedCompositor};
use crate::native_application_viewport as viewport;
use crate::native_dispatch::NativePhase;

use native_engine::{commands::OpacityRequest, compositor::Compositor};
use serde_json::json;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, OnceLock,
    },
};

fn ordinary_b() -> Value {
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

struct Fixture {
    state: ApplicationMcp,
    root: PathBuf,
    generation: u64,
    document: String,
}
impl Fixture {
    fn new() -> Self {
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
    fn native(&self) -> NativeState {
        self.state.native_state()
    }
    fn value(&self, id: &str) -> Value {
        let b = ordinary_b();
        json!({"apiVersion":HOST_API_VERSION,"requestId":id,"instanceId":self.state.instance_id(),
            "documentId":self.document,"expectedRevision":self.revision(),"lifecycleGeneration":self.generation,
            "projection":b["projection"],"resources":b["resources"]})
    }
    fn revision(&self) -> u64 {
        self.native()
            .lock()
            .unwrap()
            .active()
            .unwrap()
            .1
            .content_revision()
    }
    fn prepare(&self, v: Value) -> HostResult<PreparationReceipt> {
        prepare_with(
            &self.native(),
            self.state.instance_id(),
            "main",
            serde_json::from_value(v).unwrap(),
            || {},
        )
    }
    fn cancellation(&self, token: &str, id: &str) -> CancelRequest {
        serde_json::from_value(json!({"apiVersion":HOST_API_VERSION,"requestId":id,"instanceId":self.state.instance_id(),
            "documentId":self.document,"expectedRevision":self.revision(),"lifecycleGeneration":self.generation,
            "preparationToken":token})).unwrap()
    }
    fn cancel(&self, token: &str, id: &str) -> HostResult<CancellationReceipt> {
        cancel(
            &self.native(),
            self.state.instance_id(),
            "main",
            self.cancellation(token, id),
        )
    }
    fn dispatch(&self, request: OpacityRequest) -> Value {
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
    fn edit(&self, id: &str, value: u8) {
        let result = self.dispatch(OpacityRequest::command(id, self.state.instance_id(), &self.document, self.revision(),
            json!({"command":"layer.opacity.set","stableTarget":{"layerUid":"r08_curve_layer"},"value":value})));
        assert_eq!(result["ok"], true);
    }
    fn snapshot(&self, id: &str) -> Value {
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
fn code<T>(result: HostResult<T>) -> String {
    result.err().expect("must refuse").code
}

struct ViewportSpy(Arc<AtomicUsize>);
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

#[test]
fn ordinary_b_retained_with_exact_geometry_a_owner_history_sync_viewport_export_unchanged() {
    let f = Fixture::new();
    f.edit("existing-history", 40);
    let initial = f.snapshot("before");
    let binding = f
        .state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    f.state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    viewport::install_test_viewport(
        f.state.instance_id().into(),
        Box::new(ViewportSpy(calls.clone())),
    );
    let receipt = f.prepare(f.value("prepare-b")).unwrap();
    assert_eq!(
        (
            receipt.state,
            receipt.layer_count,
            receipt.total_frames,
            receipt.geometry_count
        ),
        ("inactive-prepared", 1, 21, 21)
    );
    assert!(uuid::Uuid::parse_str(&receipt.preparation_token).is_ok());
    assert!(serde_json::to_string(&receipt).unwrap().len() < 4096);
    assert!(serde_json::to_value(&receipt)
        .unwrap()
        .get("documentId")
        .is_none());
    {
        let native = f.native();
        let owner = native.lock().unwrap();
        let b = owner
            .inactive_project
            .as_ref()
            .unwrap()
            .project
            .as_ref()
            .unwrap();
        assert_eq!(
            serde_json::to_value(&b.document).unwrap(),
            json!({"format":"nemo.native-opacity-document",
            "formatVersion":1,"totalFrames":21,"layers":[{"layerUid":"r08_curve_layer","motionStatic":{"opacity":[60]}}]})
        );
        assert_eq!(b.resources.len(), 21);
        for (index, input) in b.resources.iter().enumerate() {
            let literal = json!({"resourceId":format!("geometry/r08/frame-{index}"),"resourceVersion":"v1",
                "layers":[{"layerUid":"r08_curve_layer","bounds":[20,60,40,80],"transform":[1,0,0,1,index as f64*6.4,0],
                    "paint":{"red":255,"green":0,"blue":0}}]});
            let expected = admit_project(
                &ordinary_b()["projection"],
                &serde_json::from_value::<Vec<GeometryResourceInput>>(json!([literal])).unwrap(),
            )
            .unwrap();
            assert_eq!(input, &expected.resources[0]);
        }
    }
    assert_eq!(initial, f.snapshot("prepared"));
    assert_eq!(f.prepare(f.value("prepare-b")).unwrap(), receipt);
    assert_eq!(
        f.cancel(&receipt.preparation_token, "cancel-b")
            .unwrap()
            .state,
        "inactive-cancelled"
    );
    assert_eq!(initial, f.snapshot("cancelled"));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    // A still has its original subscribed transport, not a B binding.
    f.state
        .control_revision_for_test(
            json!({"action":"disconnect","subscriptionId":binding["subscriptionId"]}),
        )
        .unwrap();
    assert!(viewport::remove_test_viewport().is_empty());
    let mut undo = OpacityRequest::query(
        "undo-existing",
        f.state.instance_id(),
        &f.document,
        "history.undo",
        json!({}),
    );
    let mut wire = serde_json::to_value(&undo).unwrap();
    wire["expectedRevision"] = json!(1);
    undo = serde_json::from_value(wire).unwrap();
    assert_eq!(f.dispatch(undo)["ok"], true);
    assert_eq!(f.revision(), 2);
    let bytes = f.snapshot("undone");
    assert_eq!(
        bytes["bytes"]["document"]["layers"][0]["motionStatic"]["opacity"],
        json!([25])
    );
    let progress = f
        .native()
        .lock()
        .unwrap()
        .active_mut(f.generation)
        .unwrap()
        .release_project()
        .unwrap();
    assert!(progress.application.exports.receipts.is_empty());
    assert!(progress.cancelled_preview.is_empty());
    assert!(progress.unresolved_preview.is_empty());
    assert_eq!(progress.application.redo_depth, 1);
}

#[test]
fn exact_prepare_replay_busy_and_cancel_tombstone_are_bounded() {
    let f = Fixture::new();
    let initial = f.snapshot("before");
    let v = f.value("p");
    let receipt = f.prepare(v.clone()).unwrap();
    assert_eq!(f.prepare(v.clone()).unwrap(), receipt);
    let mut changed = v.clone();
    changed["projection"]["layers"][0]["motionStatic"]["opacity"] = json!([61]);
    assert_eq!(code(f.prepare(changed)), "request_conflict");
    assert_eq!(code(f.prepare(f.value("other"))), "preparation_busy");
    assert_eq!(code(f.cancel("foreign-token", "c")), "preparation_expired");
    let mut wrong = f.cancellation(&receipt.preparation_token, "c");
    wrong.lifecycle_generation += 1;
    assert_eq!(
        code(cancel(&f.native(), f.state.instance_id(), "main", wrong)),
        "stale_owner"
    );
    assert_eq!(
        code(cancel(
            &f.native(),
            f.state.instance_id(),
            "foreign",
            f.cancellation(&receipt.preparation_token, "c")
        )),
        "wrong_instance"
    );
    let cancelled = f.cancel(&receipt.preparation_token, "c").unwrap();
    assert_eq!(
        f.cancel(&receipt.preparation_token, "c").unwrap(),
        cancelled
    );
    assert_eq!(
        code(f.cancel(&receipt.preparation_token, "different")),
        "request_conflict"
    );
    assert_eq!(code(f.prepare(v)), "preparation_cancelled");
    assert!(f
        .native()
        .lock()
        .unwrap()
        .inactive_project
        .as_ref()
        .unwrap()
        .project
        .is_none());
    let fresh = f.prepare(f.value("fresh")).unwrap();
    assert_ne!(fresh.preparation_token, receipt.preparation_token);
    assert_eq!(
        code(f.cancel(&receipt.preparation_token, "old")),
        "preparation_expired"
    );
    assert_eq!(initial, f.snapshot("after"));
}

#[test]
fn request_dtos_and_active_lease_negatives_preserve_a_and_empty_slot() {
    let f = Fixture::new();
    let initial = f.snapshot("before");
    let valid = f.value("p");
    for (field, value) in [
        ("apiVersion", json!(99)),
        ("requestId", json!("")),
        ("instanceId", json!("other")),
        ("documentId", json!("other")),
        ("expectedRevision", json!(1)),
        ("lifecycleGeneration", json!(2)),
        ("lifecycleGeneration", json!(0)),
        ("expectedRevision", json!(MAX_SAFE_INTEGER + 1)),
    ] {
        let mut v = valid.clone();
        v[field] = value;
        assert!(f.prepare(v).is_err(), "{field}");
    }
    assert_eq!(
        code(prepare_with(
            &f.native(),
            f.state.instance_id(),
            "foreign",
            serde_json::from_value(valid.clone()).unwrap(),
            || {}
        )),
        "wrong_instance"
    );
    for field in [
        "apiVersion",
        "requestId",
        "instanceId",
        "documentId",
        "expectedRevision",
        "lifecycleGeneration",
        "projection",
        "resources",
    ] {
        let mut v = valid.clone();
        v.as_object_mut().unwrap().remove(field);
        assert!(serde_json::from_value::<PrepareRequest>(v).is_err());
    }
    let mut v = valid.clone();
    v["unknown"] = json!(true);
    assert!(serde_json::from_value::<PrepareRequest>(v).is_err());
    let raw =
        serde_json::to_string(&valid)
            .unwrap()
            .replacen("{", "{\"requestId\":\"duplicate\",", 1);
    assert!(serde_json::from_str::<PrepareRequest>(&raw).is_err());
    assert!(f.native().lock().unwrap().inactive_project.is_none());
    assert_eq!(initial, f.snapshot("after"));
}

#[test]
fn malformed_projection_and_geometry_refuse_without_slot_or_owner_effect() {
    let f = Fixture::new();
    let initial = f.snapshot("before");
    let valid = f.value("p");
    for path in [
        "empty",
        "mismatch",
        "duplicate",
        "oversize",
        "unknown-projection",
        "unknown-resource",
        "legacy",
        "bad-bounds",
    ] {
        let mut v = valid.clone();
        match path {
            "empty" => v["resources"] = json!([]),
            "mismatch" => v["resources"][0]["layers"][0]["layerUid"] = json!("other-layer"),
            "duplicate" => {
                let a = v["resources"][0].clone();
                v["resources"].as_array_mut().unwrap().push(a);
            }
            "oversize" => {
                v["projection"]["layers"][0]["layerUid"] = json!("x".repeat(MAX_REQUEST_BYTES))
            }
            "unknown-projection" => v["projection"]["symbols"] = json!([]),
            "unknown-resource" => v["resources"][0]["image"] = json!(true),
            "legacy" => v["projection"] = json!({"version":13,"layers":[]}),
            "bad-bounds" => v["resources"][0]["layers"][0]["bounds"] = json!([0, 0, 0, 10]),
            _ => unreachable!(),
        }
        match serde_json::from_value::<PrepareRequest>(v) {
            Ok(r) => assert!(
                prepare_with(&f.native(), f.state.instance_id(), "main", r, || {}).is_err(),
                "{path}"
            ),
            Err(_) => assert_eq!(path, "unknown-resource"),
        }
    }
    assert!(f.native().lock().unwrap().inactive_project.is_none());
    assert_eq!(initial, f.snapshot("after"));
}

#[test]
fn off_lock_edit_race_refuses_and_preserves_intervening_history() {
    let f = Fixture::new();
    let request = serde_json::from_value(f.value("held")).unwrap();
    assert_eq!(
        code(prepare_with(
            &f.native(),
            f.state.instance_id(),
            "main",
            request,
            || f.edit("intervening", 45)
        )),
        "stale_owner"
    );
    assert_eq!(f.revision(), 1);
    assert!(f.native().lock().unwrap().inactive_project.is_none());
    let receipt = f.prepare(f.value("fresh")).unwrap();
    f.edit("later", 50); // revision drift alone keeps the immutable B, but fences old A request pins.
    let mut stale = f.cancellation(&receipt.preparation_token, "cancel");
    stale.expected_revision = 1;
    assert_eq!(
        code(cancel(&f.native(), f.state.instance_id(), "main", stale)),
        "stale_owner"
    );
    assert!(f
        .native()
        .lock()
        .unwrap()
        .inactive_project
        .as_ref()
        .unwrap()
        .project
        .is_some());
    assert!(f.cancel(&receipt.preparation_token, "cancel").is_ok());
    f.edit("later-still", 55);
    assert_eq!(
        code(f.cancel(&receipt.preparation_token, "cancel")),
        "request_conflict"
    );
}

#[test]
fn held_admission_rechecks_busy_slot_and_identical_concurrent_request() {
    let f = Fixture::new();
    let request = serde_json::from_value(f.value("p")).unwrap();
    let mut first = None;
    let replay = prepare_with(&f.native(), f.state.instance_id(), "main", request, || {
        first = Some(f.prepare(f.value("p")).unwrap())
    })
    .unwrap();
    assert_eq!(Some(replay), first);
    let f = Fixture::new();
    let request = serde_json::from_value(f.value("p")).unwrap();
    assert_eq!(
        code(prepare_with(
            &f.native(),
            f.state.instance_id(),
            "main",
            request,
            || {
                f.prepare(f.value("other")).unwrap();
            }
        )),
        "preparation_busy"
    );
    assert_eq!(
        f.native()
            .lock()
            .unwrap()
            .inactive_project
            .as_ref()
            .unwrap()
            .receipt
            .request_id,
        "other"
    );
}

#[test]
fn lifecycle_release_replacement_and_object_owner_expire_old_token() {
    for mode in ["replacement", "release"] {
        let f = Fixture::new();
        let value = f.value("p");
        let receipt = f.prepare(value.clone()).unwrap();
        let cancel_request = f.cancellation(&receipt.preparation_token, "cancel");
        let native = f.native();
        if mode == "replacement" {
            native
                .lock()
                .unwrap()
                .admit_replace_request(
                    "replace",
                    b"different",
                    f.state.instance_id(),
                    &f.document,
                    0,
                    |_| Ok(()),
                )
                .unwrap();
        } else {
            native
                .lock()
                .unwrap()
                .admit_release(
                    "release",
                    f.state.instance_id(),
                    &f.document,
                    0,
                    b"release",
                    false,
                )
                .unwrap();
        }
        assert!(cancel(&native, f.state.instance_id(), "main", cancel_request).is_err());
        assert!(native.lock().unwrap().inactive_project.is_none());
        assert!(f.prepare(value).is_err());
    }
    let state = ApplicationMcp::default();
    let object = json!({"format":"nemo.native-object-document","formatVersion":1,
        "totalFrames":21,"layers":[{"layerUid":"layer"}],"objects":[]});
    let document =
        native_engine::object_codec::decode_storage_project(&serde_json::to_vec(&object).unwrap())
            .expect("valid object fixture must admit");
    state.install_native_object(document).unwrap();
    let native = state.native_state();
    let guard = native.lock().unwrap();
    let (generation, owner) = guard.active().unwrap();
    let b = ordinary_b();
    let v = json!({"apiVersion":HOST_API_VERSION,"requestId":"p","instanceId":state.instance_id(),
            "documentId":owner.document_id(),"expectedRevision":0,"lifecycleGeneration":generation,"projection":b["projection"],"resources":b["resources"]});
    drop(guard);
    assert_eq!(
        code(prepare_with(
            &native,
            state.instance_id(),
            "main",
            serde_json::from_value(v).unwrap(),
            || {}
        )),
        "unavailable"
    );
}

#[test]
fn off_lock_vacancy_and_new_install_generation_refuse_without_b_publication() {
    for installing in [false, true] {
        let f = Fixture::new();
        let request = serde_json::from_value(f.value("held-owner")).unwrap();
        let result = prepare_with(&f.native(), f.state.instance_id(), "main", request, || {
            let native = f.native();
            let mut authority = native.lock().unwrap();
            authority.phase = NativePhase::Vacant;
            if installing {
                authority.reserve_install().unwrap();
            }
        });
        assert_eq!(code(result), "stale_owner");
        assert!(f.native().lock().unwrap().inactive_project.is_none());
    }
}
