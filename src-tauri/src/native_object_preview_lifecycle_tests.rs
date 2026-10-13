use crate::native_dispatch::object_host::preview::tests::{
    fill, host, output, present, selector, snapshot, viewport, Mode,
};
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{
    compositor::Compositor, desktop_viewport::ViewportStatus, export_job::ReconciliationStage,
    resource_leases::WorkId,
};
use serde_json::{json, Value};

fn history(host: &mut NativeObjectHost, operation: &str, id: &str) -> Value {
    let request = json!({"apiVersion":2, "requestId":id, "instanceId":host.instance_id(),
        "documentId":host.document_id(), "expectedRevision":host.content_revision(), "operation":operation,
        "payload":{"command":operation.strip_prefix("command.document.object.").map(|kind|format!("object.{kind}")).unwrap()}});
    serde_json::to_value(
        host.dispatch(serde_json::from_value(request).unwrap())
            .unwrap(),
    )
    .unwrap()
}
fn assert_pending(host: &NativeObjectHost, id: WorkId) {
    assert_eq!(host.preview.pending.as_ref().unwrap().work_id(), id);
    assert_eq!(host.preview.scheduler.lease_counters().live(), 1);
}

#[test]
fn cpu_constructor_and_release_are_inert_idempotent_and_preserve_application_receipt() {
    let mut owner = host();
    assert!(owner.preview.pending.is_none());
    assert!(owner.release_progress().is_none());
    assert_eq!(owner.preview.scheduler.lease_counters().acquired(), 0);
    let first = owner.release_project().unwrap();
    assert!(first.reconciliation_complete());
    assert!(first.cancelled_preview.is_empty());
    assert_eq!(first.application.content_revision, 0);
    let second = owner.release_project().unwrap();
    assert_eq!(first.application, second.application);
    assert!(owner.release_progress().unwrap().reconciliation_complete());
    assert!(!owner.object_client_available());
}

#[test]
fn gpu_only_real_fill_undo_redo_advances_cancel_deferred_work_and_update_pixels() {
    let mut gpu = Compositor::new().unwrap();
    let mut owner = host();
    let (mut view, trace) = viewport(&gpu, 64);
    trace.borrow_mut().mode = Mode::Defer;
    let first = present(&mut owner, &mut gpu, &mut view).unwrap();
    let old = first.identity().work_id();
    let noop = fill(
        &mut owner,
        "noop",
        json!({"kind":"solid","r":0,"g":0,"b":1,"a":1}),
    );
    assert_eq!(noop["ok"], true);
    assert_eq!(noop["contentRevision"], 0);
    assert_pending(&owner, old);
    let rejected = fill(
        &mut owner,
        "rejected",
        json!({"kind":"solid","r":2,"g":0,"b":0,"a":1}),
    );
    assert_eq!(rejected["ok"], false);
    assert_pending(&owner, old);
    let saved = snapshot(&owner);
    let read = json!({"apiVersion":2,"requestId":"read","instanceId":owner.instance_id(),"documentId":owner.document_id(),
        "operation":"query.document.object","payload":{"atRevision":0,"stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"a"}}});
    let response = serde_json::to_value(
        owner
            .dispatch(serde_json::from_value(read).unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(response["ok"], true);
    assert_pending(&owner, old);
    let edited = fill(
        &mut owner,
        "edit",
        json!({"kind":"solid","r":1,"g":0,"b":0,"a":1}),
    );
    assert_eq!(edited["ok"], true);
    assert_eq!(edited["contentRevision"], 1);
    assert!(owner.preview.pending.is_none());
    assert_eq!(owner.preview.scheduler.lease_counters().live(), 0);
    assert!(owner
        .present_object_preview(&selector(&saved, 0), output(), &mut gpu, &mut view)
        .is_err());
    for (operation, revision, color) in [
        ("command.document.object.undo", 2, [0, 0, 255, 255]),
        ("command.document.object.redo", 3, [255, 0, 0, 255]),
    ] {
        trace.borrow_mut().mode = Mode::Defer;
        present(&mut owner, &mut gpu, &mut view).unwrap();
        let response = history(&mut owner, operation, &format!("history-{revision}"));
        assert_eq!(response["ok"], true, "{response}");
        assert_eq!(response["contentRevision"], revision);
        assert!(owner.preview.pending.is_none());
        trace.borrow_mut().mode = Mode::Present;
        trace.borrow_mut().pixel = Some(color);
        assert_eq!(
            present(&mut owner, &mut gpu, &mut view).unwrap().status(),
            &ViewportStatus::Presented
        );
    }
    assert!(owner.release_project().unwrap().reconciliation_complete());
}

#[test]
fn gpu_committed_mutation_keeps_truthful_response_when_cancellation_fails() {
    let mut gpu = Compositor::new().unwrap();
    let mut owner = host();
    let (mut view, trace) = viewport(&gpu, 64);
    trace.borrow_mut().mode = Mode::Defer;
    let shown = present(&mut owner, &mut gpu, &mut view).unwrap();
    let id = shown.identity().work_id();
    owner.preview.fail_cancel = true;
    let edited = fill(
        &mut owner,
        "commit-with-debt",
        json!({"kind":"solid","r":1,"g":0,"b":0,"a":1}),
    );
    assert_eq!(edited["ok"], true);
    assert_eq!(edited["contentRevision"], 1);
    assert_eq!(owner.history.history_depths(), (1, 0));
    assert_pending(&owner, id);
    assert!(present(&mut owner, &mut gpu, &mut view).is_err());
    assert_eq!(trace.borrow().identities.len(), 1);
    let failed = owner.release_project().unwrap();
    assert!(!failed.reconciliation_complete());
    assert_eq!(failed.preview_stage, ReconciliationStage::Unknown);
    assert_eq!(failed.unresolved_preview, vec![id]);
    assert!(failed.cancelled_preview.is_empty());
    assert!(failed
        .preview_error
        .unwrap()
        .contains("cancellation failure"));
    assert_eq!(failed.application.content_revision, 1);
    assert_eq!(
        owner.release_progress().unwrap().unresolved_preview,
        vec![id]
    );
    assert!(!owner.release_project().unwrap().reconciliation_complete());
    owner.preview.fail_cancel = false;
    let done = owner.release_project().unwrap();
    assert!(done.reconciliation_complete());
    assert_eq!(done.cancelled_preview, vec![id]);
    assert_eq!(owner.preview.scheduler.lease_counters().live(), 0);
    assert_eq!(owner.release_project().unwrap().cancelled_preview, vec![id]);
    assert_eq!(owner.preview.scheduler.lease_counters().released(), 1);
    assert!(present(&mut owner, &mut gpu, &mut view).is_err());
}

#[test]
fn gpu_presentation_error_unwind_and_wrong_gpu_keep_identifiable_cleanup_debt() {
    let mut gpu = Compositor::new().unwrap();
    let other_gpu = Compositor::new().unwrap();
    for mode in 0..3 {
        let mut owner = host();
        let (mut view, trace) = viewport(
            if mode == 2 { &other_gpu } else { &gpu },
            if mode == 0 { 32 } else { 64 },
        );
        if mode == 1 {
            trace.borrow_mut().mode = Mode::Panic;
        }
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            present(&mut owner, &mut gpu, &mut view)
        }));
        match mode {
            0 => assert!(result.unwrap().is_err()),
            1 => assert!(result.is_err()),
            _ => assert!(matches!(
                result.unwrap().unwrap().status(),
                ViewportStatus::Failed(_)
            )),
        }
        let pending = owner.preview.pending.as_ref().unwrap().work_id();
        assert_pending(&owner, pending);
        assert!(owner.preview.blocked.is_some());
        if mode != 0 {
            assert_eq!(trace.borrow().identities[0].work_id(), pending);
        }
        assert_eq!(
            (trace.borrow().acquisitions, trace.borrow().settlements),
            (0, 0)
        );
        assert!(present(&mut owner, &mut gpu, &mut view).is_err());
        let released = owner.release_project().unwrap();
        assert!(released.reconciliation_complete());
        assert_eq!(released.cancelled_preview, vec![pending]);
        assert_eq!(owner.preview.scheduler.lease_counters().live(), 0);
    }
}

#[test]
fn gpu_equal_number_foreign_owner_work_and_frame_replacement_do_not_cross_present() {
    let mut gpu = Compositor::new().unwrap();
    let mut a = host();
    let mut b = host();
    let (mut view, trace) = viewport(&gpu, 64);
    trace.borrow_mut().mode = Mode::Defer;
    let old = present(&mut a, &mut gpu, &mut view).unwrap();
    assert!(present(&mut b, &mut gpu, &mut view).is_err());
    let foreign = b.preview.pending.as_ref().unwrap();
    assert_eq!(old.identity().work_id(), foreign.work_id());
    assert_eq!(old.identity().view_generation(), foreign.view_generation());
    assert_eq!(trace.borrow().identities.len(), 1);
    assert_pending(&a, old.identity().work_id());
    assert!(b.release_project().unwrap().reconciliation_complete());
    let current = snapshot(&a);
    let next = a
        .present_object_preview(&selector(&current, 1), output(), &mut gpu, &mut view)
        .unwrap();
    assert_ne!(next.identity().work_id(), old.identity().work_id());
    assert!(a
        .preview
        .scheduler
        .receipt(old.identity().work_id())
        .is_some());
    assert_pending(&a, next.identity().work_id());
    assert!(a.release_project().unwrap().reconciliation_complete());
    assert_eq!(a.preview.scheduler.lease_counters().released(), 2);
}
