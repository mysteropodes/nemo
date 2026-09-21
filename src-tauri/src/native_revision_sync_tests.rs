use crate::application_mcp::revision_sync::*;
use crate::application_mcp::*;
use crate::native_application::{admit_release_request, ReleaseAdmission};
use crate::native_application_contract::{NativeReleaseRequest, HOST_API_VERSION};
use native_engine::{
    application::{ResourceResolutionError, ResourceResolutionErrorKind},
    codec::decode_project,
    export_job::{ExportArtifact, ExportReadback},
    protocol::OpaqueResourceHandle,
    render_scene::{GeometryPaintInput, RenderScene},
};
use serde_json::json;

struct Port;
impl StagedArtifactPort for Port {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn write_frame(&mut self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn publish(&mut self, _: &str, _: &str, _: &[String]) -> Result<ExportArtifact, String> {
        Err("unused artifact port".into())
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
}
struct Compositor;
impl ExportCompositor for Compositor {
    type Composition = ExportReadback;
    fn compose(&mut self, _: &RenderScene) -> Result<Self::Composition, String> {
        Err("unused".into())
    }
    fn readback_rgba8(&self, result: &Self::Composition) -> Result<ExportReadback, String> {
        Ok(result.clone())
    }
}
struct Resolver;
impl ExportResourceResolver for Resolver {
    fn resolve_geometry(
        &mut self,
        _: &OpaqueResourceHandle,
    ) -> Result<GeometryPaintInput, ResourceResolutionError> {
        Err(ResourceResolutionError::new(
            ResourceResolutionErrorKind::Unavailable,
            "unused",
        ))
    }
}
fn install(state: &ApplicationMcp) {
    let doc = decode_project(include_bytes!(
        "../../native-engine/tests/fixtures/opacity-v2/project.json"
    ))
    .unwrap();
    state
        .install_native(
            NativeApplication::new(state.instance_id.clone(), doc, Port, Compositor, Resolver)
                .unwrap(),
        )
        .unwrap();
}
fn fixture() -> (ApplicationMcp, RevisionBinding) {
    let mut state = ApplicationMcp::default();
    state.instance_id = "revision-fixture".into();
    install(&state);
    let binding = serde_json::from_value(
        state
            .revisions
            .control(&state.native, RevisionControl::Binding)
            .unwrap(),
    )
    .unwrap();
    (state, binding)
}
fn register(state: &ApplicationMcp, binding: &RevisionBinding) {
    state
        .revisions
        .control(
            &state.native,
            RevisionControl::Subscribe {
                binding: binding.clone(),
            },
        )
        .unwrap();
}
fn set(binding: &RevisionBinding, id: &str, revision: u64, value: i32) -> NativeApplicationRequest {
    NativeApplicationRequest {
        api_version: 2,
        request_id: id.into(),
        instance_id: binding.instance_id.clone(),
        document_id: binding.document_id.clone(),
        expected_revision: Some(revision),
        operation: "command.document.apply".into(),
        payload: json!({"command":"layer.opacity.set", "stableTarget":{"layerUid":"r08_curve_layer"}, "value":value}),
        cancelled_before_dispatch: false,
    }
}
fn stage(
    binding: &RevisionBinding,
    id: &str,
    operation: &str,
    payload: serde_json::Value,
) -> NativeApplicationRequest {
    NativeApplicationRequest {
        operation: operation.into(),
        payload,
        expected_revision: None,
        ..set(binding, id, 0, 40)
    }
}
fn ack(
    state: &ApplicationMcp,
    binding: &RevisionBinding,
    event: &RevisionEvent,
) -> Result<serde_json::Value, String> {
    state.revisions.control(
        &state.native,
        RevisionControl::Acknowledge {
            subscription_id: binding.subscription_id.clone(),
            event: event.clone(),
        },
    )
}
fn dispatch(state: &ApplicationMcp, request: NativeApplicationRequest) -> Delivery {
    state
        .revisions
        .dispatch(state.instance_id(), &state.native, request, true)
        .unwrap()
}
fn settle(state: &ApplicationMcp, binding: &RevisionBinding, delivery: Delivery) -> RevisionEvent {
    let (event, mut receiver) = delivery.notification.unwrap();
    ack(state, binding, &event).unwrap();
    assert_eq!(receiver.try_recv(), Ok(()));
    assert!(state.revisions.settle(&state.native, &event, true));
    event
}

#[test]
fn subscriber_is_exact_main_lifecycle_binding_and_absence_rejects_before_commit() {
    let (state, binding) = fixture();
    let denied = dispatch(&state, set(&binding, "without-subscriber", 0, 40));
    assert!(!denied.response.ok);
    assert_eq!(denied.response.content_revision, 0);
    assert!(denied.notification.is_none());
    for field in [
        "instanceId",
        "documentId",
        "lifecycleGeneration",
        "contentRevision",
        "extra",
    ] {
        let mut altered = serde_json::to_value(&binding).unwrap();
        altered[field] = if field.ends_with("Id") {
            json!("other")
        } else {
            json!(99)
        };
        let request = serde_json::from_value::<RevisionControl>(
            json!({"action":"subscribe", "binding":altered}),
        );
        assert!(
            request.is_err()
                || state
                    .revisions
                    .control(&state.native, request.unwrap())
                    .is_err()
        );
    }
    register(&state, &binding);
    assert!(state
        .revisions
        .control(
            &state.native,
            RevisionControl::Subscribe {
                binding: binding.clone()
            }
        )
        .is_err());
    let committed = dispatch(&state, set(&binding, "advance", 0, 40));
    assert!(committed.response.ok);
    let event = &committed.notification.as_ref().unwrap().0;
    assert_eq!(
        serde_json::to_value(event).unwrap(),
        json!({"instanceId":"revision-fixture", "documentId":binding.document_id,
        "lifecycleGeneration":binding.lifecycle_generation, "fromRevision":0, "toRevision":1, "requestId":"advance"})
    );
    let later = state
        .dispatch_native(set(&binding, "ui-advance", 1, 60))
        .unwrap();
    assert!(!later.ok);
    assert_eq!(later.content_revision, 1);
    assert!(
        !dispatch(&state, set(&binding, "wire-advance", 1, 60))
            .response
            .ok
    );
    let query = stage(
        &binding,
        "pinned",
        "query.document.serialize",
        json!({"atRevision":1}),
    );
    assert!(dispatch(&state, query).response.ok);
    for (value, code) in [(40, "unavailable"), (60, "invalid_request")] {
        let replay = dispatch(&state, set(&binding, "advance", 0, value));
        assert_eq!(replay.response.error.unwrap().code, code);
        assert!(replay.notification.is_none());
    }
    let mut cancelled = set(&binding, "advance", 0, 40);
    cancelled.cancelled_before_dispatch = true;
    assert!(!dispatch(&state, cancelled).response.ok);
    for field in [
        "instanceId",
        "documentId",
        "lifecycleGeneration",
        "fromRevision",
        "toRevision",
        "requestId",
        "extra",
    ] {
        let mut altered = serde_json::to_value(event).unwrap();
        altered[field] = if field.ends_with("Id") {
            json!("other")
        } else {
            json!(99)
        };
        let request = serde_json::from_value::<RevisionControl>(
            json!({"action":"acknowledge", "subscriptionId":binding.subscription_id, "event":altered}),
        );
        assert!(
            request.is_err()
                || state
                    .revisions
                    .control(&state.native, request.unwrap())
                    .is_err()
        );
    }
    let event = settle(&state, &binding, committed);
    assert!(ack(&state, &binding, &event).is_err());
    assert!(dispatch(&state, set(&binding, "advance", 0, 40))
        .notification
        .is_none());
    let changed = dispatch(&state, set(&binding, "advance", 0, 60));
    assert!(!changed.response.ok);
    assert_eq!(changed.response.error.unwrap().code, "invalid_request");
    assert!(changed.notification.is_none());
}

#[test]
fn only_committed_advances_notify_including_transaction_commit_and_history() {
    let (state, binding) = fixture();
    register(&state, &binding);
    let mut cancelled = set(&binding, "cancelled", 0, 40);
    cancelled.cancelled_before_dispatch = true;
    let mut wrong = set(&binding, "wrong-instance", 0, 40);
    wrong.instance_id = "other".into();
    let mut wrong_document = set(&binding, "wrong-document", 0, 40);
    wrong_document.document_id = "other".into();
    for request in [
        cancelled,
        wrong,
        wrong_document,
        set(&binding, "stale", 1, 40),
        set(&binding, "invalid", 0, 101),
        stage(&binding, "read", "query.document.revision", json!({})),
        stage(
            &binding,
            "poll",
            "job.export.png.status",
            json!({"jobId":"missing"}),
        ),
        set(&binding, "noop", 0, 25),
    ] {
        let delivery = dispatch(&state, request);
        assert!(delivery.notification.is_none());
        assert_eq!(delivery.response.content_revision, 0);
    }
    let mut begin = stage(
        &binding,
        "begin",
        "transaction.begin",
        json!({"stableTarget":{"layerUid":"r08_curve_layer"}}),
    );
    begin.expected_revision = Some(0);
    let begun = dispatch(&state, begin);
    assert!(begun.notification.is_none());
    let id = begun.response.result.unwrap()["transactionId"].clone();
    assert!(dispatch(
        &state,
        stage(
            &binding,
            "update",
            "transaction.update",
            json!({"transactionId":id,"value":40})
        )
    )
    .notification
    .is_none());
    let committed = dispatch(
        &state,
        stage(
            &binding,
            "commit",
            "transaction.commit",
            json!({"transactionId":id}),
        ),
    );
    assert_eq!(settle(&state, &binding, committed).to_revision, 1);
    for (id, operation, revision) in [("undo", "history.undo", 1), ("redo", "history.redo", 2)] {
        let mut request = stage(&binding, id, operation, json!({}));
        request.expected_revision = Some(revision);
        assert_eq!(
            settle(&state, &binding, dispatch(&state, request)).to_revision,
            revision + 1
        );
    }
}

#[test]
fn wire_success_is_withheld_until_exact_acknowledgment() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let (state, binding) = fixture();
            register(&state, &binding);
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let mut client = TcpStream::connect(listener.local_addr().unwrap())
                .await
                .unwrap();
            let (mut server, _) = listener.accept().await.unwrap();
            let (sent, received) = oneshot::channel();
            let serve = serve_native(
                &mut server,
                state.instance_id(),
                &state.native,
                &state.revisions,
                set(&binding, "wire", 0, 40),
                |event| sent.send(event).map_err(|_| "listener failed".into()),
                Duration::from_secs(1),
            );
            let observe = async {
                let event = received.await.unwrap();
                assert!(
                    tokio::time::timeout(
                        Duration::from_millis(20),
                        wire::read_json::<NativeApplicationResponse>(&mut client)
                    )
                    .await
                    .is_err(),
                    "no success bytes before acknowledgment"
                );
                ack(&state, &binding, &event).unwrap();
                let response: NativeApplicationResponse =
                    wire::read_json(&mut client).await.unwrap();
                assert!(response.ok);
                assert_eq!(response.content_revision, 1);
            };
            let (served, _) = tokio::join!(serve, observe);
            served.unwrap();
            drop(client);
            server.readable().await.unwrap();
            assert!(serve_native(
                &mut server,
                state.instance_id(),
                &state.native,
                &state.revisions,
                set(&binding, "closed-wire", 1, 60),
                |_| panic!("closed connection must not notify"),
                Duration::from_secs(1)
            )
            .await
            .is_err());
            let current = state
                .revisions
                .control(&state.native, RevisionControl::Binding)
                .unwrap();
            assert_eq!(current["contentRevision"], 1);
        });
}

#[test]
fn timeout_and_listener_failure_are_indeterminate_without_double_execution() {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            for failed_listener in [false, true] {
                let (state, binding) = fixture();
                register(&state, &binding);
                let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
                let mut client = TcpStream::connect(listener.local_addr().unwrap())
                    .await
                    .unwrap();
                let (mut server, _) = listener.accept().await.unwrap();
                serve_native(
                    &mut server,
                    state.instance_id(),
                    &state.native,
                    &state.revisions,
                    set(&binding, "fault", 0, 40),
                    |_| {
                        if failed_listener {
                            Err("listener failed".into())
                        } else {
                            Ok(())
                        }
                    },
                    Duration::from_millis(1),
                )
                .await
                .unwrap();
                let response: NativeApplicationResponse =
                    wire::read_json(&mut client).await.unwrap();
                assert!(!response.ok);
                assert_eq!(response.content_revision, 1);
                assert_eq!(
                    response.error.unwrap().details.unwrap()["disposition"],
                    "indeterminate"
                );
                let replay = dispatch(&state, set(&binding, "fault", 0, 40));
                assert!(!replay.response.ok);
                assert_eq!(replay.response.content_revision, 1);
                assert!(replay.notification.is_none());
                let changed = dispatch(&state, set(&binding, "fault", 0, 60));
                assert_eq!(changed.response.error.unwrap().code, "invalid_request");
                assert!(
                    !state
                        .dispatch_native(set(&binding, "later", 1, 60))
                        .unwrap()
                        .ok
                );
                assert!(state
                    .revisions
                    .control(&state.native, RevisionControl::Subscribe { binding })
                    .is_err());
            }
        });
}

#[test]
fn disconnect_release_and_reinstall_drain_waiters_and_reject_old_acknowledgments() {
    let (state, binding) = fixture();
    register(&state, &binding);
    let delivery = dispatch(&state, set(&binding, "pending", 0, 40));
    let (event, mut receiver) = delivery.notification.unwrap();
    let release = NativeReleaseRequest {
        api_version: HOST_API_VERSION,
        request_id: "release".into(),
        instance_id: binding.instance_id.clone(),
        document_id: binding.document_id.clone(),
        expected_revision: 0,
        cancelled_before_dispatch: false,
    };
    assert!(admit_release_request(&state.native, &release).is_err());
    assert!(matches!(
        receiver.try_recv(),
        Err(oneshot::error::TryRecvError::Empty)
    ));
    let release = NativeReleaseRequest {
        expected_revision: 1,
        ..release
    };
    assert!(matches!(
        admit_release_request(&state.native, &release),
        Ok(ReleaseAdmission::Execute { .. })
    ));
    state.revisions.invalidate();
    assert!(matches!(
        receiver.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
    assert!(ack(&state, &binding, &event).is_err());
    state.retire_native_for_test(json!({"status":"succeeded"}));
    install(&state);
    let fresh: RevisionBinding = serde_json::from_value(
        state
            .revisions
            .control(&state.native, RevisionControl::Binding)
            .unwrap(),
    )
    .unwrap();
    assert_ne!(fresh.lifecycle_generation, binding.lifecycle_generation);
    register(&state, &fresh);
    assert!(ack(&state, &binding, &event).is_err());
    let delivery = dispatch(&state, set(&fresh, "new", 0, 40));
    let (event, mut receiver) = delivery.notification.unwrap();
    state
        .revisions
        .control(
            &state.native,
            RevisionControl::Disconnect {
                subscription_id: fresh.subscription_id.clone(),
            },
        )
        .unwrap();
    assert!(matches!(
        receiver.try_recv(),
        Err(oneshot::error::TryRecvError::Closed)
    ));
    assert!(ack(&state, &fresh, &event).is_err());
    assert!(!dispatch(&state, set(&fresh, "later", 1, 60)).response.ok);
}
