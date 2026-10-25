//! Shared route acceptance cases; reusable actual-owner fixtures are private.
use crate::application_mcp::revision_sync::object_preview_route::tests::support::{
    fill, output, Fixture, Opacity,
};
use crate::application_mcp::revision_sync::{RevisionBinding, RevisionControl};
use crate::native_dispatch::{NativeDispatch, NativePhase};
use native_engine::{
    commands::NativeOpacityApplication, desktop_viewport::ViewportStatus,
    document::OpacityDocument, object_document::FrameScopeKind, scheduler::OutputSpec,
};
use serde_json::json;
#[path = "native_object_preview_route_test_support.rs"]
mod support;

#[test]
fn gpu_current_route_holds_both_guards_and_refuses_every_stale_pin_before_acquisition() {
    let mut f = Fixture::new();
    f.mode.set(1);
    let first = f.run().unwrap();
    assert!(matches!(first.status(), ViewportStatus::Deferred(_)));
    for field in 0..8 {
        let mut p = f.pins.clone();
        match field {
            0 => p.instance = "foreign".into(),
            1 => p.document = "foreign".into(),
            2 => p.revision += 1,
            3 => p.snapshot = "foreign".into(),
            4 => p.generation += 1,
            _ => {}
        }
        let mut selector = p.selector();
        match field {
            5 => selector.context_id = "foreign",
            6 => selector.scope_kind = FrameScopeKind::Reference,
            7 => selector.frame = 1,
            _ => {}
        }
        assert!(f
            .state
            .present_object_preview(p.generation, &selector, output(), &mut f.gpu, &mut f.view)
            .is_err());
    }
    let wrong = OutputSpec::new("frame", "rgba8", 64, 64, "linear", "straight").unwrap();
    assert!(f
        .state
        .present_object_preview(
            f.pins.generation,
            &f.pins.selector(),
            wrong,
            &mut f.gpu,
            &mut f.view
        )
        .is_err());
    assert_eq!(f.calls.get(), 1);
    f.mode.set(0);
    let receipt = f.run().unwrap();
    assert_eq!(receipt.identity(), first.identity());
    assert_eq!(receipt.status(), &ViewportStatus::Presented);
    f.release();
    assert!(f.run().is_err());
    let old_generation = f.pins.generation;
    f.reinstall();
    assert_ne!(f.pins.generation, old_generation);
    assert!(f
        .state
        .present_object_preview(
            old_generation,
            &f.pins.selector(),
            output(),
            &mut f.gpu,
            &mut f.view
        )
        .unwrap_err()
        .contains("generation"));
    assert_eq!(f.calls.get(), 2);
}

#[test]
fn gpu_common_fill_undo_redo_invalidate_retained_preview_and_refresh_owner_pins() {
    let mut f = Fixture::new();
    f.mode.set(1);
    let mut old = f.run().unwrap();
    for (operation, payload, pixel) in [
        (
            "command.document.object.fill.set",
            fill(1),
            [255, 0, 0, 255],
        ),
        (
            "command.document.object.undo",
            json!({"command":"object.undo"}),
            [0, 0, 255, 255],
        ),
        (
            "command.document.object.redo",
            json!({"command":"object.redo"}),
            [255, 0, 0, 255],
        ),
    ] {
        let response = f.command(operation, payload, false).response;
        assert!(response.ok);
        assert!(f.run().is_err());
        f.refresh();
        f.expected_pixel.set(pixel);
        let next = f.run().unwrap();
        assert_ne!(next.identity().work_id(), old.identity().work_id());
        assert_eq!(
            next.identity().evaluation_key().document_snapshot_id(),
            f.pins.snapshot
        );
        old = next;
    }
    let released = f.release();
    assert_eq!(released.cancelled_preview_work_ids.len(), 1);
    assert_eq!(released.disposed_viewport_work_ids.len(), 4);
}

#[test]
fn gpu_real_external_revision_admits_only_exact_subscriber_preview_before_ack() {
    let mut f = Fixture::new();
    let binding: RevisionBinding = serde_json::from_value(
        f.state
            .revisions
            .control(&f.state.native, RevisionControl::Binding)
            .unwrap(),
    )
    .unwrap();
    f.state
        .revisions
        .control(
            &f.state.native,
            RevisionControl::Subscribe {
                binding: binding.clone(),
            },
        )
        .unwrap();
    let delivery = f.command("command.document.object.fill.set", fill(1), true);
    assert!(delivery.response.ok);
    let (event, mut receiver) = delivery.notification.unwrap();
    f.refresh();
    assert!(f.run().unwrap_err().contains("pending"));
    let subscription_id = binding.subscription_id.clone();
    let proof_event = serde_json::to_value(&event).unwrap();
    let proof = (subscription_id.as_str(), &proof_event);
    f.expected_pixel.set([255, 0, 0, 255]);
    let pins = f.pins.clone();
    let selector = pins.selector();
    assert!(f
        .state
        .with_object_preview_owner(f.pins.generation, &selector, |_| Ok(()))
        .unwrap_err()
        .contains("pending"));
    let presented = f
        .state
        .with_object_preview_owner_for_preview(f.pins.generation, &selector, Some(proof), |owner| {
            owner.present_object_preview(&selector, output(), &mut f.gpu, &mut f.view)
        })
        .unwrap();
    assert_eq!(presented.status(), &ViewportStatus::Presented);
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
    assert!(f
        .state
        .with_object_preview_owner_for_preview(
            f.pins.generation,
            &selector,
            Some(("foreign-subscriber", &proof_event)),
            |_| Ok(())
        )
        .unwrap_err()
        .contains("pending"));
    for (field, value) in [
        ("instanceId", json!("foreign")),
        ("documentId", json!("foreign")),
        ("lifecycleGeneration", json!(event.lifecycle_generation + 1)),
        ("fromRevision", json!(event.from_revision + 1)),
        ("toRevision", json!(event.to_revision + 1)),
    ] {
        let mut changed_event = serde_json::to_value(&event).unwrap();
        changed_event[field] = value;
        assert!(
            f.state
                .with_object_preview_owner_for_preview(
                    f.pins.generation,
                    &selector,
                    Some((subscription_id.as_str(), &changed_event)),
                    |_| Ok(())
                )
                .unwrap_err()
                .contains("pending"),
            "{field}"
        );
    }
    let mut foreign_event = proof_event.clone();
    foreign_event["requestId"] = json!("foreign-request");
    assert!(f
        .state
        .with_object_preview_owner_for_preview(
            f.pins.generation,
            &selector,
            Some((subscription_id.as_str(), &foreign_event)),
            |_| Ok(())
        )
        .unwrap_err()
        .contains("pending"));
    assert_eq!(f.calls.get(), 1);
    f.state
        .revisions
        .control(
            &f.state.native,
            RevisionControl::Acknowledge {
                subscription_id: binding.subscription_id.clone(),
                event: event.clone(),
            },
        )
        .unwrap();
    assert_eq!(receiver.try_recv(), Ok(()));
    assert!(f.run().unwrap_err().contains("pending"));
    assert!(f
        .state
        .with_object_preview_owner_for_preview(
            f.pins.generation,
            &selector,
            Some(proof),
            |_| Ok(())
        )
        .unwrap_err()
        .contains("pending"));
    assert!(f.state.revisions.settle(&f.state.native, &event, true));
    assert!(f
        .state
        .with_object_preview_owner_for_preview(
            f.pins.generation,
            &selector,
            Some(proof),
            |_| Ok(())
        )
        .unwrap_err()
        .contains("no pending"));
    assert_eq!(f.run().unwrap().status(), &ViewportStatus::Presented);
    let delivery = f.command("command.document.object.fill.set", fill(0), true);
    assert!(delivery.response.ok);
    let (second_event, _) = delivery.notification.unwrap();
    f.refresh();
    let second_event_value = serde_json::to_value(&second_event).unwrap();
    f.state
        .revisions
        .control(
            &f.state.native,
            RevisionControl::Disconnect {
                subscription_id: binding.subscription_id,
            },
        )
        .unwrap();
    assert!(f.run().unwrap_err().contains("indeterminate"));
    let second_selector = f.pins.selector();
    assert!(f
        .state
        .with_object_preview_owner_for_preview(
            f.pins.generation,
            &second_selector,
            Some((subscription_id.as_str(), &second_event_value)),
            |_| Ok(())
        )
        .unwrap_err()
        .contains("indeterminate"));
    assert_eq!(f.calls.get(), 2);
    f.release();
}

fn subscribed_cut_fixture() -> (
    Fixture,
    support::Pins,
    serde_json::Value,
    RevisionBinding,
    serde_json::Value,
) {
    let mut f = Fixture::cut_project();
    let original_pins = f.pins.clone();
    let before = f.saved_project();
    let baseline_history = f.history();
    assert_eq!(baseline_history["undoDepth"], 0);
    assert_eq!(baseline_history["redoDepth"], 0);
    assert_eq!(f.run().unwrap().status(), &ViewportStatus::Presented);
    let binding: RevisionBinding = serde_json::from_value(
        f.state
            .revisions
            .control(&f.state.native, RevisionControl::Binding)
            .unwrap(),
    )
    .unwrap();
    f.state
        .revisions
        .control(
            &f.state.native,
            RevisionControl::Subscribe {
                binding: binding.clone(),
            },
        )
        .unwrap();
    let selection = |command| {
        json!({"command":command,"contextId":"scene-root",
        "layerUid":"L0","frame":0,"mode":"motion","strokeIds":["a","b"]})
    };
    let copied = f.command_named(
        "same-owner-copy",
        "command.document.object.clipboard.copy",
        selection("object.clipboard.copy"),
        true,
    );
    assert!(copied.response.ok);
    assert!(copied.notification.is_none());
    let copy_result = copied.response.result.unwrap();
    assert_eq!(copy_result["applied"], false);
    assert_eq!(copy_result["historyEntriesAdded"], 0);
    assert_eq!(
        copy_result["clipboard"]["records"],
        json!([
            before["drawing"]["objects"][0],
            before["drawing"]["objects"][1]
        ])
    );
    assert_eq!(f.saved_project(), before);
    assert_eq!(f.history(), baseline_history);
    (f, original_pins, before, binding, copy_result)
}

#[test]
fn gpu_same_project_owner_copy_cut_keeps_later_holder_and_requires_exact_preview_ack() {
    let (mut f, original_pins, before, binding, copy_result) = subscribed_cut_fixture();
    let cut = f.command_named(
        "same-owner-cut",
        "command.document.object.clipboard.cut",
        json!({"command":"object.clipboard.cut","contextId":"scene-root",
            "layerUid":"L0","frame":0,"mode":"motion","strokeIds":["a","b"]}),
        true,
    );
    assert!(cut.response.ok);
    let result = cut.response.result.unwrap();
    assert_eq!(result["applied"], true);
    assert_eq!(result["historyEntriesAdded"], 1);
    assert_eq!(result["clipboard"], copy_result["clipboard"]);
    let (event, mut receiver) = cut.notification.unwrap();
    f.refresh_for(
        json!({"contextId":"scene-root","layerUid":"L0","strokeId":"survivor",
        "frameScope":{"kind":"authored","frame":0}}),
    );
    assert_eq!(f.pins.instance, original_pins.instance);
    assert_eq!(f.pins.document, original_pins.document);
    assert_eq!(f.pins.generation, original_pins.generation);
    assert_eq!(f.pins.revision, 1);
    assert_ne!(f.pins.snapshot, original_pins.snapshot);
    let expected = Fixture::expected_cut_project(&before);
    assert_eq!(
        f.saved_project(),
        expected,
        "only current selected records and their dependent tables change"
    );
    let after_history = f.history();
    assert_eq!(after_history["undoDepth"], 1);
    assert_eq!(after_history["redoDepth"], 0);
    assert!(f.run().unwrap_err().contains("pending"));
    let pins = f.pins.clone();
    let selector = pins.selector();
    let proof_event = serde_json::to_value(&event).unwrap();
    f.assert_invalid_cut_proofs(&binding.subscription_id, &proof_event);
    assert_eq!(f.calls.get(), 1, "refused proofs acquire no surface");
    f.expected_pixel.set([255, 0, 0, 255]);
    let proof = (binding.subscription_id.as_str(), &proof_event);
    for _ in 0..2 {
        let receipt = f
            .state
            .with_object_preview_owner_for_preview(
                pins.generation,
                &selector,
                Some(proof),
                |owner| owner.present_object_preview(&selector, output(), &mut f.gpu, &mut f.view),
            )
            .unwrap();
        assert_eq!(receipt.status(), &ViewportStatus::Presented);
        assert!(matches!(
            receiver.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ));
        assert_eq!(f.saved_project(), expected);
        assert_eq!(f.history(), after_history);
    }
    f.state
        .revisions
        .control(
            &f.state.native,
            RevisionControl::Acknowledge {
                subscription_id: binding.subscription_id.clone(),
                event: event.clone(),
            },
        )
        .unwrap();
    assert_eq!(receiver.try_recv(), Ok(()));
    assert!(f.run().unwrap_err().contains("pending"));
    assert!(f.state.revisions.settle(&f.state.native, &event, true));
    assert!(f
        .state
        .with_object_preview_owner_for_preview(pins.generation, &selector, Some(proof), |_| Ok(()))
        .unwrap_err()
        .contains("no pending"));
    assert_eq!(f.run().unwrap().status(), &ViewportStatus::Presented);
    let mut later = pins.selector();
    later.frame = 3;
    f.expected_pixel.set([0, 0, 255, 255]);
    assert_eq!(
        f.state
            .present_object_preview(pins.generation, &later, output(), &mut f.gpu, &mut f.view)
            .unwrap()
            .status(),
        &ViewportStatus::Presented
    );
    assert_eq!(f.saved_project(), expected);
    assert_eq!(f.history(), after_history);
    f.release();
}

#[test]
fn gpu_consumer_unwind_preserves_owner_debt_without_poisoning_shared_locks() {
    let mut f = Fixture::new();
    f.mode.set(2);
    assert!(f.run().unwrap_err().contains("panicked"));
    assert!(f.state.native.lock().is_ok());
    assert!(f.state.revisions.0.lock().is_ok());
    assert!(f.run().unwrap_err().contains("cleanup"));
    assert_eq!(f.calls.get(), 1);
    let released = f.release();
    assert_eq!(released.cancelled_preview_work_ids.len(), 1);
    assert_eq!(released.disposed_viewport_work_ids.len(), 1);
    assert!(f.run().is_err());
}

#[test]
fn gpu_non_object_and_inactive_lifecycle_phases_never_invoke_surface() {
    for phase in 0..5 {
        let mut f = Fixture::new();
        if phase == 4 {
            f.release();
            let document: OpacityDocument = serde_json::from_slice(include_bytes!(
                "../../native-engine/tests/fixtures/opacity-v2/project.json"
            ))
            .unwrap();
            let owner =
                Opacity(NativeOpacityApplication::new(f.state.instance_id(), document).unwrap());
            f.pins.document = owner.document_id().into();
            f.pins.revision = owner.content_revision();
            let reservation = f.state.reserve_native_install().unwrap();
            f.pins.generation = reservation.generation();
            f.state
                .install_dispatch(reservation.generation(), Box::new(owner))
                .unwrap();
        } else {
            let mut guard = f.state.native.lock().unwrap();
            let NativePhase::Active {
                generation,
                application,
                ..
            } = std::mem::replace(&mut guard.phase, NativePhase::Vacant)
            else {
                unreachable!()
            };
            guard.phase = match phase {
                0 => NativePhase::Vacant,
                1 => NativePhase::Replacing {
                    generation,
                    application,
                    progress: Default::default(),
                },
                2 => NativePhase::Releasing {
                    generation,
                    application: Some(application),
                    request_id: "pending".into(),
                    fingerprint: vec![],
                },
                _ => {
                    drop(application);
                    NativePhase::Vacant
                }
            };
            if phase == 3 {
                guard.reserve_install().unwrap();
            }
        }
        assert!(f.run().is_err());
        assert_eq!(f.calls.get(), 0);
    }
}
