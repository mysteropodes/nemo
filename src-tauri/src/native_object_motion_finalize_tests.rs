//! Actual owner receipts, original RevisionSync ACK/settle, CPU fallback. UNRUN.
use crate::application_mcp::revision_sync::RevisionEvent;
use crate::application_mcp::ApplicationMcp;
use crate::native_dispatch::{finalizer_fixture, finalizer_svg, NativeDispatch, NativeObjectHost};
use native_engine::{
    object_codec::encode_storage_project, object_document::FrameScopeKind,
    object_frame_packet::ObjectFrameSelector, object_snapshot::ObjectSnapshot,
    scheduler::OutputSpec,
};
use nemo_mcp::contract::NativeApplicationRequest;
use serde_json::{json, Value};
use std::{cell::Cell, time::Duration};

fn fixture(code: &str) -> (ApplicationMcp, ObjectSnapshot, OutputSpec, u64) {
    let (owner, origin, output) = finalizer_fixture(code);
    let mut state = ApplicationMcp::default();
    state.instance_id = origin.instance_id().into();
    state.install_native(owner).unwrap();
    let generation = state.native.lock().unwrap().active_generation().unwrap();
    (state, origin, output, generation)
}
fn pins(s: &ObjectSnapshot, frame: u32) -> ObjectFrameSelector<'_> {
    ObjectFrameSelector {
        instance_id: s.instance_id(),
        document_id: s.document_id(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame,
    }
}
fn owner_state(s: &ApplicationMcp) -> (ObjectSnapshot, (usize, usize), String) {
    let mut authority = s.native.lock().unwrap();
    let generation = authority.active_generation().unwrap();
    authority
        .active_mut(generation)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<NativeObjectHost>()
        .unwrap()
        .motion_state_for_test()
}
fn subscribe(s: &ApplicationMcp) -> Value {
    let binding = s
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    s.control_revision_for_test(json!({"action":"subscribe","binding":binding.clone()}))
        .unwrap();
    binding
}
fn batch(s: &ApplicationMcp) -> String {
    s.revisions
        .0
        .lock()
        .unwrap()
        .motion_reservation
        .as_ref()
        .unwrap()
        .batch_id
        .clone()
}
fn acknowledge(s: &ApplicationMcp, b: &Value, event: &RevisionEvent) {
    assert!(s.native.try_lock().is_ok());
    assert!(s.revisions.0.try_lock().is_ok());
    s.control_revision_for_test(
        json!({"action":"acknowledge","subscriptionId":b["subscriptionId"],"event":event}),
    )
    .unwrap();
}
fn read(
    s: &ApplicationMcp,
    generation: u64,
    origin: &ObjectSnapshot,
    output: OutputSpec,
) -> Result<(), String> {
    s.with_object_preview_owner(generation, &pins(origin, 1), |owner| {
        owner.with_prepared_object_preview(&pins(origin, 1), output, |_| Ok(()))
    })
}
fn fill(s: &ApplicationMcp, origin: &ObjectSnapshot, id: &str) -> NativeApplicationRequest {
    serde_json::from_value(json!({"apiVersion":2,"requestId":id,
        "instanceId":s.instance_id(),"documentId":origin.document_id(),
        "expectedRevision":origin.content_revision(),"operation":"command.document.object.fill.set",
        "payload":{"command":"object.fill.set","stableTarget":{"contextId":"scene-root",
            "frameScope":{"kind":"authored","frame":1},"layerUid":"L0","strokeId":"a"},
            "fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}}}))
    .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn diagnostics_only_callback_keeps_revision_and_history_and_actual_svg_uses_fallback() {
    let (s, origin, output, g) = fixture("'wrong dimension'");
    let before = encode_storage_project(origin.document()).unwrap();
    assert!(read(&s, g, &origin, output).is_err());
    let id = batch(&s);
    assert!(s
        .complete_object_motion_effects(
            &id,
            |_| panic!("diagnostics do not advance revision"),
            Duration::from_secs(1)
        )
        .await
        .unwrap());
    let (current, depths, diagnostics) = owner_state(&s);
    assert_eq!((current.content_revision(), depths), (0, (0, 0)));
    assert!(diagnostics.contains("dupOffsetPos"));
    assert_eq!(encode_storage_project(current.document()).unwrap(), before);
    let mut authority = s.native.lock().unwrap();
    let owner = authority
        .active_mut(g)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<NativeObjectHost>()
        .unwrap();
    let svg = finalizer_svg(owner, &current);
    for x in [4, 14, 24] {
        assert!(svg.contains(&format!("<path d=\"M{x},4")));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn timeout_requires_subscriber_and_exact_frames_and_commits_one_original_event_once() {
    let (s, origin, output, g) = fixture("while(true){}");
    assert!(read(&s, g, &origin, output).is_err());
    let id = batch(&s);
    assert!(s
        .complete_object_motion_effects(&id, |_| panic!("no subscriber"), Duration::from_millis(1))
        .await
        .is_err());
    assert_eq!(
        (owner_state(&s).0.content_revision(), owner_state(&s).1),
        (0, (0, 0))
    );
    let binding = subscribe(&s);
    let emitted = Cell::new(0);
    assert!(!s
        .complete_object_motion_effects(
            &id,
            |event| {
                emitted.set(emitted.get() + 1);
                assert_eq!((event.from_revision, event.to_revision), (0, 1));
                acknowledge(&s, &binding, &event);
                Ok(())
            },
            Duration::from_secs(1)
        )
        .await
        .unwrap());
    assert_eq!(emitted.get(), 1);
    let (current, depths, _) = owner_state(&s);
    assert_eq!((current.content_revision(), depths), (1, (1, 0)));
    let stored: Value =
        serde_json::from_slice(&encode_storage_project(current.document()).unwrap()).unwrap();
    assert_eq!(
        stored["propertyHolders"][0]["properties"][0]["expression"]["enabled"],
        false
    );
    assert!(!s
        .complete_object_motion_effects(
            &id,
            |_| panic!("duplicate completion"),
            Duration::from_secs(1)
        )
        .await
        .unwrap());
    assert_eq!(owner_state(&s).1, (1, 0));
}

#[tokio::test(flavor = "current_thread")]
async fn exact_r1_proof_renders_cached_timeout_fallback_then_ack_settle_allows_one_r2_disable() {
    let (s, origin, output, g) = fixture("while(true){}");
    let binding = subscribe(&s);
    let delivery = s
        .revisions
        .dispatch(
            s.instance_id(),
            &s.native,
            fill(&s, &origin, "external-r1"),
            true,
        )
        .unwrap();
    assert!(delivery.response.ok);
    let (r1, mut receiver) = delivery.notification.unwrap();
    let (at_r1, depths, _) = owner_state(&s);
    assert_eq!((at_r1.content_revision(), depths), (1, (1, 0)));
    let event = serde_json::to_value(&r1).unwrap();
    let ran = Cell::new(0);
    s.with_object_preview_owner_for_preview(
        g,
        &pins(&at_r1, 1),
        Some((binding["subscriptionId"].as_str().unwrap(), &event)),
        |owner| {
            owner.with_prepared_object_preview_for_revision_sync(&pins(&at_r1, 1), output, |_| {
                ran.set(ran.get() + 1);
                Ok(())
            })
        },
    )
    .unwrap();
    assert_eq!(ran.get(), 1);
    assert_eq!(owner_state(&s).1, (1, 0));
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
    let id = batch(&s);
    let emitted = Cell::new(0);
    let complete = s.complete_object_motion_effects(
        &id,
        |r2| {
            emitted.set(emitted.get() + 1);
            assert_eq!((r2.from_revision, r2.to_revision), (1, 2));
            acknowledge(&s, &binding, &r2);
            Ok(())
        },
        Duration::from_secs(1),
    );
    let settle_r1 = async {
        tokio::task::yield_now().await;
        assert_eq!(
            owner_state(&s).0.content_revision(),
            1,
            "no mutation under pending read proof"
        );
        acknowledge(&s, &binding, &r1);
        assert_eq!(receiver.try_recv(), Ok(()));
        assert!(s.revisions.settle(&s.native, &r1, true));
        // The queued exact-source batch, not an unrelated next command, owns R2.
        let denied = s
            .revisions
            .dispatch(
                s.instance_id(),
                &s.native,
                fill(&s, &at_r1, "no-piggyback"),
                false,
            )
            .unwrap();
        assert!(!denied.response.ok);
        assert_eq!(owner_state(&s).1, (1, 0));
    };
    let (done, ()) = tokio::join!(complete, settle_r1);
    assert!(!done.unwrap());
    assert_eq!(emitted.get(), 1);
    let (at_r2, depths, _) = owner_state(&s);
    assert_eq!((at_r2.content_revision(), depths), (2, (2, 0)));
    assert!(read(
        &s,
        g,
        &at_r1,
        OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap()
    )
    .is_err());
    assert!(read(
        &s,
        g,
        &at_r2,
        OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap()
    )
    .is_ok());
}

#[tokio::test(flavor = "current_thread")]
async fn disconnect_before_prior_ack_preserves_effects_and_never_advances_or_drains() {
    let (s, origin, output, g) = fixture("while(true){}");
    let binding = subscribe(&s);
    let delivery = s
        .revisions
        .dispatch(
            s.instance_id(),
            &s.native,
            fill(&s, &origin, "disconnect-r1"),
            true,
        )
        .unwrap();
    let (r1, _receiver) = delivery.notification.unwrap();
    let current = owner_state(&s).0;
    let event = serde_json::to_value(&r1).unwrap();
    s.with_object_preview_owner_for_preview(
        g,
        &pins(&current, 1),
        Some((binding["subscriptionId"].as_str().unwrap(), &event)),
        |owner| {
            owner.with_prepared_object_preview_for_revision_sync(&pins(&current, 1), output, |_| {
                Ok(())
            })
        },
    )
    .unwrap();
    let id = batch(&s);
    s.control_revision_for_test(
        json!({"action":"disconnect","subscriptionId":binding["subscriptionId"]}),
    )
    .unwrap();
    assert!(s
        .complete_object_motion_effects(&id, |_| panic!("disconnected R1"), Duration::from_secs(1))
        .await
        .is_err());
    assert_eq!(
        (owner_state(&s).0.content_revision(), owner_state(&s).1),
        (1, (1, 0))
    );
    assert!(s.revisions.0.lock().unwrap().motion_reservation.is_some());
    assert!(s
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .is_err());
    s.revisions.invalidate();
    assert!(s.revisions.0.lock().unwrap().motion_reservation.is_none());
    assert_eq!(
        owner_state(&s).1,
        (1, 0),
        "teardown of barrier does not discard the owner's effect receipt"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn emit_failure_retains_actual_committed_pending_event_and_cannot_commit_twice() {
    let (s, origin, output, g) = fixture("while(true){}");
    subscribe(&s);
    assert!(read(&s, g, &origin, output).is_err());
    let id = batch(&s);
    let error = s
        .complete_object_motion_effects(
            &id,
            |_| Err("injected emit failure".into()),
            Duration::from_secs(1),
        )
        .await
        .unwrap_err();
    assert!(error.contains("committed"));
    assert_eq!(owner_state(&s).1, (1, 0));
    let revisions = s.revisions.0.lock().unwrap();
    assert!(revisions.pending.as_ref().unwrap().failed);
    drop(revisions);
    assert!(!s
        .complete_object_motion_effects(&id, |_| panic!("second commit"), Duration::from_secs(1))
        .await
        .unwrap());
    assert_eq!(owner_state(&s).0.content_revision(), 1);
}

#[tokio::test(flavor = "current_thread")]
async fn cancellation_and_foreign_completion_preserve_exact_owner_batch_without_command_authority()
{
    let (s, origin, output, g) = fixture("while(true){}");
    assert!(read(&s, g, &origin, output).is_err());
    let id = batch(&s);
    assert!(tokio::time::timeout(
        Duration::from_millis(1),
        s.complete_object_motion_effects(&id, |_| panic!("no subscriber"), Duration::from_secs(5))
    )
    .await
    .is_err());
    assert_eq!(owner_state(&s).1, (0, 0));
    assert!(s
        .complete_object_motion_effects(
            "foreign-batch",
            |_| panic!("foreign"),
            Duration::from_secs(1)
        )
        .await
        .is_err());
    assert_eq!(batch(&s), id);
    assert_eq!(owner_state(&s).1, (0, 0));
    let denied = s
        .revisions
        .dispatch(
            s.instance_id(),
            &s.native,
            fill(&s, &origin, "unrelated-command"),
            false,
        )
        .unwrap();
    assert!(!denied.response.ok);
    assert_eq!(owner_state(&s).1, (0, 0));
    let binding = subscribe(&s);
    s.complete_object_motion_effects(
        &id,
        |event| {
            acknowledge(&s, &binding, &event);
            Ok(())
        },
        Duration::from_secs(1),
    )
    .await
    .unwrap();
    assert_eq!(owner_state(&s).1, (1, 0));
}

#[tokio::test(flavor = "current_thread")]
async fn lifecycle_reconcile_retires_only_its_reservation_and_keeps_original_owner_receipts() {
    let (s, origin, output, g) = fixture("while(true){}");
    assert!(read(&s, g, &origin, output).is_err());
    let id = batch(&s);
    {
        let mut revisions = s.revisions.0.lock().unwrap();
        let reserved = revisions.motion_reservation.as_ref().unwrap();
        assert!(!reserved.matches_owner(g, "other-instance", "other-document", 0));
        revisions.reconcile(g + 1, "other-document");
        assert!(revisions.motion_reservation.is_none());
    }
    assert!(!s
        .complete_object_motion_effects(&id, |_| panic!("retired callback"), Duration::from_secs(1))
        .await
        .unwrap());
    let (current, depths, _) = owner_state(&s);
    assert_eq!((current.content_revision(), depths), (0, (0, 0)));
    let raw: Value =
        serde_json::from_slice(&encode_storage_project(current.document()).unwrap()).unwrap();
    assert_eq!(
        raw["propertyHolders"][0]["properties"][0]["expression"]["enabled"],
        true
    );
}

#[tokio::test(flavor = "current_thread")]
async fn missing_actual_ack_keeps_the_committed_event_indeterminate_without_a_second_disable() {
    let (s, origin, output, g) = fixture("while(true){}");
    subscribe(&s);
    assert!(read(&s, g, &origin, output).is_err());
    let id = batch(&s);
    let emitted = Cell::new(0);
    let refused = s
        .complete_object_motion_effects(
            &id,
            |event| {
                assert_eq!((event.from_revision, event.to_revision), (0, 1));
                assert!(s.native.try_lock().is_ok());
                assert!(s.revisions.0.try_lock().is_ok());
                emitted.set(emitted.get() + 1);
                Ok(()) // The actual consumer never ACKs this original pending event.
            },
            Duration::from_millis(1),
        )
        .await
        .unwrap_err();
    assert!(refused.contains("committed"));
    assert_eq!(emitted.get(), 1);
    assert_eq!(
        (owner_state(&s).0.content_revision(), owner_state(&s).1),
        (1, (1, 0))
    );
    assert!(
        s.revisions
            .0
            .lock()
            .unwrap()
            .pending
            .as_ref()
            .unwrap()
            .failed
    );
    assert!(!s
        .complete_object_motion_effects(&id, |_| panic!("duplicate commit"), Duration::from_secs(1))
        .await
        .unwrap());
    assert_eq!(owner_state(&s).1, (1, 0));
}

#[path = "native_object_motion_finalize_diagnostics_tests.rs"]
mod diagnostic_tests;
