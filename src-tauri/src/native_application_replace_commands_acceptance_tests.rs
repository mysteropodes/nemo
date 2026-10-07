//! Command-layer acceptance oracles for host replacement and external revision delivery.

use crate::{
    application_mcp::ApplicationMcp,
    native_application::{DesktopNativeApplication, PreparedDesktopReplacement},
    native_application_commands::replace_commands::complete_replacement,
    native_application_contract::{
        admit_project, work_label, GeometryResourceInput, NativePreviewRequest,
    },
    native_application_ports::{DesktopArtifactPort, SharedCompositor},
    native_dispatch::{
        NativeAuthority, NativeDispatch, NativeState, ReplacementAdmission, ReplacementIdentity,
    },
};
use native_engine::compositor::Compositor;
use nemo_mcp::{
    contract::{NativeApplicationRequest, NativeApplicationResponse, NATIVE_API_VERSION},
    wire,
};
use serde_json::{json, Value};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::oneshot,
};

use crate::native_application_commands::replace_replay::replay_result;

#[path = "native_application_replace_viewport_tests.rs"]
mod viewport_tests;

#[test]
fn response_loss_identical_a_to_b_retry_replays_completed_receipt() {
    let (_scratch, native, old_document, prepared) = setup();
    let admission = native
        .lock()
        .unwrap()
        .admit_replace_request(
            "replace-a",
            b"typed-a-to-b",
            "native-fixture",
            &old_document,
            0,
            |_| Ok(()),
        )
        .unwrap();
    let ReplacementAdmission::Execute(fresh) = admission else {
        panic!("first request must execute")
    };
    let pending = native
        .lock()
        .unwrap()
        .lookup_replace_replay("replace-a", b"typed-a-to-b")
        .unwrap()
        .unwrap();
    assert_eq!(
        replay_result(pending).unwrap_err().code,
        "replacement_pending"
    );
    let discarded = complete_replacement(&native, fresh, "native-fixture", prepared).unwrap();
    assert_ne!(discarded.document_id, old_document);
    let retry = native
        .lock()
        .unwrap()
        .admit_replace_request(
            "replace-a",
            b"typed-a-to-b",
            "native-fixture",
            &old_document,
            0,
            |_| panic!("an identical retry must not prepare or execute B again"),
        )
        .unwrap();
    let ReplacementAdmission::Replay(replay) = retry else {
        panic!("identical retry must replay")
    };
    let retrieved = replay_result(replay).unwrap();
    assert_eq!(retrieved.document_id, discarded.document_id);
    assert_eq!(retrieved.request_id, "replace-a");
    assert!(retrieved.retrieved);
    assert!(native
        .lock()
        .unwrap()
        .lookup_replace_replay("replace-a", b"changed-b")
        .unwrap_err()
        .contains("changed replacement body"));
}

const PROJECT: &[u8] = include_bytes!("../../native-engine/tests/fixtures/opacity-v2/project.json");

pub(super) struct Scratch(pub(super) PathBuf);
impl Scratch {
    pub(super) fn new() -> Self {
        let path = std::env::temp_dir().join(format!("nemo-n20r2-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn geometry(id: &str, version: &str, red: u8, width: f64) -> Vec<GeometryResourceInput> {
    serde_json::from_value(json!([{
        "resourceId": id, "resourceVersion": version,
        "layers": [{
            "layerUid": "r08_curve_layer", "bounds": [0.0, 0.0, width, 180.0],
            "transform": [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            "paint": {"red": red, "green": 34, "blue": 56}
        }]
    }]))
    .unwrap()
}
pub(super) fn geometry_a() -> Vec<GeometryResourceInput> {
    geometry("geometry-a", "v1", 12, 320.0)
}
pub(super) fn geometry_b() -> Vec<GeometryResourceInput> {
    geometry("geometry-b", "v2", 220, 160.0)
}
pub(super) fn project(opacity: u64) -> Value {
    let mut value: Value = serde_json::from_slice(PROJECT).unwrap();
    value["layers"][0]["motionStatic"]["opacity"] = json!([opacity]);
    value
}

fn desktop(instance: &str, scratch: &Scratch) -> DesktopNativeApplication {
    let admitted = admit_project(&project(25), &geometry_a()).unwrap();
    let artifacts = DesktopArtifactPort::new(
        scratch.0.join("staging"),
        std::iter::empty::<(String, PathBuf)>(),
    )
    .unwrap();
    DesktopNativeApplication::new(
        instance.into(),
        admitted,
        artifacts,
        SharedCompositor::new(Compositor::new().unwrap()),
    )
    .unwrap()
}

pub(super) fn setup() -> (Scratch, NativeState, String, PreparedDesktopReplacement) {
    let scratch = Scratch::new();
    let application = desktop("native-fixture", &scratch);
    let old_document = application.document_id().to_owned();
    let admitted = admit_project(&project(35), &geometry_b()).unwrap();
    let prepared = DesktopNativeApplication::prepare_replacement(admitted).unwrap();
    let mut authority = NativeAuthority::default();
    let generation = authority.reserve_install().unwrap();
    authority
        .install(generation, Box::new(application))
        .unwrap();
    (
        scratch,
        Arc::new(Mutex::new(authority)),
        old_document,
        prepared,
    )
}

fn installed() -> (Scratch, ApplicationMcp, String, u64) {
    let scratch = Scratch::new();
    let state = ApplicationMcp::default();
    let application = desktop(state.instance_id(), &scratch);
    let document = application.document_id().to_owned();
    let installation = state.reserve_native_install().unwrap();
    let generation = installation.generation();
    state
        .install_dispatch(generation, Box::new(application))
        .unwrap();
    (scratch, state, document, generation)
}

async fn pending_external(state: &ApplicationMcp, document: &str, generation: u64, cancel: bool) {
    let binding = state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    assert_eq!(binding["lifecycleGeneration"], generation);
    state
        .control_revision_for_test(json!({"action":"subscribe", "binding":binding}))
        .unwrap();
    let request = NativeApplicationRequest {
        api_version: NATIVE_API_VERSION,
        request_id: format!("external-{cancel}"),
        instance_id: state.instance_id().into(),
        document_id: document.into(),
        expected_revision: Some(0),
        operation: "command.document.apply".into(),
        payload: json!({"command":"layer.opacity.set", "stableTarget":{"layerUid":"r08_curve_layer"}, "value":40}),
        cancelled_before_dispatch: false,
    };
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .await
        .unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap())
        .await
        .unwrap();
    let (mut server, _) = listener.accept().await.unwrap();
    let (tx, rx) = oneshot::channel::<Value>();
    let serve = state.serve_external_for_test(&mut server, request, move |event| {
        tx.send(serde_json::to_value(event).map_err(|error| error.to_string())?)
            .map_err(|_| "revision event receiver dropped".into())
    });
    let drive = async {
        let event = tokio::time::timeout(Duration::from_secs(2), rx)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event["fromRevision"], 0);
        assert_eq!(event["toRevision"], 1);
        assert_eq!(event["lifecycleGeneration"], generation);
        assert!(state
            .reserve_native_replacement(
                ReplacementIdentity {
                    request_id: "rejected",
                    fingerprint: b"body-rejected",
                    instance_id: state.instance_id(),
                    document_id: document,
                    expected_revision: 0
                },
                |_| Ok(())
            )
            .is_err());
        let ack = json!({"action":"acknowledge", "subscriptionId":binding["subscriptionId"], "event":event});
        let fresh = if cancel {
            let next = state
                .reserve_native_replacement(
                    ReplacementIdentity {
                        request_id: "admitted",
                        fingerprint: b"body-admitted",
                        instance_id: state.instance_id(),
                        document_id: document,
                        expected_revision: 1,
                    },
                    |application| {
                        if application.as_any_mut().is::<DesktopNativeApplication>() {
                            Ok(())
                        } else {
                            Err("unavailable:native desktop host is unavailable".into())
                        }
                    },
                )
                .unwrap();
            let ReplacementAdmission::Execute(next) = next else {
                panic!("fresh request must execute")
            };
            assert!(next > generation);
            assert!(matches!(
                state.reserve_native_replacement(
                    ReplacementIdentity {
                        request_id: "admitted",
                        fingerprint: b"body-admitted",
                        instance_id: state.instance_id(),
                        document_id: document,
                        expected_revision: 1
                    },
                    |_| panic!("pending replacement replay cannot rerun preflight")
                ),
                Ok(ReplacementAdmission::Replay(
                    crate::native_dispatch::ReplacementReplay::Pending(_)
                ))
            ));
            assert!(state
                .control_revision_for_test(ack)
                .unwrap_err()
                .contains("unavailable"));
            Some(next)
        } else {
            state.control_revision_for_test(ack).unwrap();
            None
        };
        let response: NativeApplicationResponse =
            tokio::time::timeout(Duration::from_secs(2), wire::read_json(&mut client))
                .await
                .unwrap()
                .unwrap();
        assert_eq!(response.ok, !cancel);
        assert_eq!(response.content_revision, 1);
        if let Some(fresh) = fresh {
            assert_eq!(
                response.error.unwrap().details.unwrap()["disposition"],
                "indeterminate"
            );
            let admitted = admit_project(&project(35), &geometry_b()).unwrap();
            let prepared = DesktopNativeApplication::prepare_replacement(admitted).unwrap();
            let receipt =
                complete_replacement(&state.native_state(), fresh, state.instance_id(), prepared)
                    .unwrap();
            assert_ne!(receipt.document_id, document);
            assert!(state
                .control_revision_for_test(json!({"action":"subscribe", "binding":binding}))
                .is_err());
            let new_binding = state
                .control_revision_for_test(json!({"action":"binding"}))
                .unwrap();
            assert_eq!(new_binding["lifecycleGeneration"], fresh);
            assert_eq!(new_binding["documentId"], receipt.document_id);
            state
                .control_revision_for_test(json!({"action":"subscribe", "binding":new_binding}))
                .unwrap();
        }
    };
    let (served, _) = tokio::join!(serve, drive);
    served.unwrap();
}

#[test]
fn external_waiter_survives_rejected_admission_then_cancels_on_reservation() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let (_scratch, state, document, generation) = installed();
            pending_external(&state, &document, generation, false).await;
            let (_scratch, state, document, generation) = installed();
            pending_external(&state, &document, generation, true).await;
        });
}

fn snapshot_id(application: &mut DesktopNativeApplication, request_id: &str) -> String {
    let response = application
        .dispatch(
            serde_json::from_value(json!({
                "apiVersion":NATIVE_API_VERSION, "requestId":request_id,
                "instanceId":application.instance_id(), "documentId":application.document_id(),
                "operation":"query.document.snapshot.acquire", "payload":{}
            }))
            .unwrap(),
        )
        .unwrap();
    assert!(response.is_ok());
    response.result().unwrap()["documentSnapshotId"]
        .as_str()
        .unwrap()
        .into()
}

fn preview_request(
    document: &str,
    snapshot: &str,
    resource: &str,
    version: &str,
) -> NativePreviewRequest {
    serde_json::from_value(json!({
        "apiVersion":NATIVE_API_VERSION, "instanceId":"native-fixture", "documentId":document,
        "contentRevision":0, "documentSnapshotId":snapshot, "contextId":"scene-root",
        "frame":0, "quality":"final",
        "outputSpec":{"kind":"frame","format":"rgba8","width":320,"height":180,
            "colorInterpretation":"srgb","alphaMode":"straight"},
        "geometryHandle":{"resourceId":resource,"resourceVersion":version}
    }))
    .unwrap()
}

#[test]
fn distinct_a_preview_reconciles_and_b_preview_export_use_only_b_resources() {
    let (scratch, native, old_document, prepared_b) = setup();
    let (old, a_work, a_pixels) = {
        let mut authority = native.lock().unwrap();
        let old = authority.active_generation().unwrap();
        let desktop = authority
            .active_mut(old)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<DesktopNativeApplication>()
            .unwrap();
        let snapshot = snapshot_id(desktop, "snapshot-a");
        let preview = desktop
            .prepare_preview(&preview_request(
                &old_document,
                &snapshot,
                "geometry-a",
                "v1",
            ))
            .unwrap();
        let pixels = desktop
            .preview_compositor()
            .inner()
            .lock()
            .unwrap()
            .readback_rgba8(&preview.result)
            .unwrap()
            .bytes()
            .to_vec();
        (old, preview.identity.work_id(), pixels)
    };
    let admission = native
        .lock()
        .unwrap()
        .admit_replace_request(
            "replace-resources",
            b"typed-b-resources",
            "native-fixture",
            &old_document,
            0,
            |application| {
                if application.as_any_mut().is::<DesktopNativeApplication>() {
                    Ok(())
                } else {
                    Err("unavailable:native desktop host is unavailable".into())
                }
            },
        )
        .unwrap();
    let ReplacementAdmission::Execute(fresh) = admission else {
        panic!("fresh B must execute")
    };
    assert!(fresh > old);
    let receipt = complete_replacement(&native, fresh, "native-fixture", prepared_b).unwrap();
    let replay = native
        .lock()
        .unwrap()
        .lookup_replace_replay("replace-resources", b"typed-b-resources")
        .unwrap()
        .unwrap();
    let retrieved = replay_result(replay).unwrap();
    assert_eq!(retrieved.document_id, receipt.document_id);
    assert!(retrieved.retrieved);
    assert!(receipt
        .cancelled_preview_work_ids
        .contains(&work_label(a_work)));
    let mut authority = native.lock().unwrap();
    let desktop = authority
        .active_mut(fresh)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<DesktopNativeApplication>()
        .unwrap();
    let snapshot = snapshot_id(desktop, "snapshot-b");
    let a_request = preview_request(&receipt.document_id, &snapshot, "geometry-a", "v1");
    assert_eq!(
        desktop.prepare_preview(&a_request).err().unwrap().code,
        "not_found"
    );
    let b_preview = desktop
        .prepare_preview(&preview_request(
            &receipt.document_id,
            &snapshot,
            "geometry-b",
            "v2",
        ))
        .unwrap();
    let b_pixels = desktop
        .preview_compositor()
        .inner()
        .lock()
        .unwrap()
        .readback_rgba8(&b_preview.result)
        .unwrap()
        .bytes()
        .to_vec();
    assert_ne!(a_pixels, b_pixels);
    assert_eq!(b_preview.result.document_id(), receipt.document_id);
    desktop
        .cancel_preview(&[b_preview.identity.work_id()])
        .unwrap();

    desktop
        .bind_output("output-b".into(), scratch.0.join("render-b"))
        .unwrap();
    let response = desktop
        .dispatch(
            serde_json::from_value(json!({
                "apiVersion":NATIVE_API_VERSION, "requestId":"export-b",
                "instanceId":"native-fixture", "documentId":receipt.document_id,
                "expectedRevision":0, "operation":"job.export.png.begin",
                "payload":{"contextId":"scene-root", "quality":"final", "outputHandle":"output-b",
                    "frames":[{"sourceFrame":0,"geometryHandle":{
                        "resourceId":"geometry-b","resourceVersion":"v2"}}]}
            }))
            .unwrap(),
        )
        .unwrap();
    assert!(response.is_ok());
    let job_id = response.result().unwrap()["jobId"].as_str().unwrap();
    let pending = desktop.start_next_export_frame(job_id).unwrap().unwrap();
    let exported = desktop.finish_export_frame(pending).unwrap();
    assert_eq!(format!("{:?}", exported.status), "Succeeded");
    assert_eq!(exported.document_snapshot_id, snapshot);

    desktop
        .bind_output("output-old".into(), scratch.0.join("render-old"))
        .unwrap();
    let response = desktop
        .dispatch(
            serde_json::from_value(json!({
                "apiVersion":NATIVE_API_VERSION, "requestId":"export-old-resource",
                "instanceId":"native-fixture", "documentId":receipt.document_id,
                "expectedRevision":0, "operation":"job.export.png.begin",
                "payload":{"contextId":"scene-root", "quality":"final", "outputHandle":"output-old",
                    "frames":[{"sourceFrame":0,"geometryHandle":{
                        "resourceId":"geometry-a","resourceVersion":"v1"}}]}
            }))
            .unwrap(),
        )
        .unwrap();
    assert!(!response.is_ok());
    assert_eq!(
        format!("{:?}", response.error().unwrap().code()),
        "NotFound"
    );
}
