//! CPU admission against real owners/work. No GPU, export or installed proof.
use super::{
    prepare_scheduled_object_composition as prepare_selector,
    prepare_scheduled_object_composition_from_witness as prepare, ScheduledObjectSceneError,
};
use crate::compositor::{
    object_test_owner as owner, object_test_plan as plan, object_test_selector as selector,
};
use crate::history::NativeOpacityHistory;
use crate::object_render_scene::{
    prepare_object_composition, ObjectCompositionWitness, ObjectSceneError,
};
use crate::revision::ObjectSnapshot;
use crate::scheduler::{
    EvaluationKey, FrameFailure, FrameFailureKind, FrameScheduler, ObjectSchedulePins,
    ObjectScheduleScope, OutputSpec, ScheduledFrame,
};
use serde_json::json;

fn pins(snapshot: &ObjectSnapshot, frame: u32) -> ObjectSchedulePins {
    ObjectSchedulePins {
        instance_id: snapshot.instance_id().into(),
        document_id: snapshot.document_id().into(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame,
        scope: ObjectScheduleScope::Authored,
    }
}
fn output() -> OutputSpec {
    OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap()
}
fn schedule(
    scheduler: &mut FrameScheduler,
    snapshot: &ObjectSnapshot,
    frame: u32,
) -> ScheduledFrame {
    scheduler
        .schedule_object(
            snapshot.clone(),
            pins(snapshot, frame),
            EvaluationKey::new(
                snapshot.snapshot_id(),
                "scene-root",
                frame,
                "final",
                output(),
                [],
            )
            .unwrap(),
        )
        .unwrap()
}
fn witness(snapshot: &ObjectSnapshot, frame: u32) -> ObjectCompositionWitness {
    prepare_object_composition(&plan(snapshot, frame), &selector(snapshot, frame)).unwrap()
}

#[test]
fn witness_and_selector_share_original_work_and_retained_owner_after_drop() {
    let original = owner("witness-original");
    let snapshot = original.acquire_snapshot(0).unwrap();
    let witness = witness(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, 7);
    let acquired = scheduler.lease_counters().acquired();
    drop(original);
    let live = prepare(&scheduler, &work, &witness).unwrap();
    let old = prepare_selector(&scheduler, &work, witness.plan(), &selector(&snapshot, 7)).unwrap();
    assert_eq!(live.identity(), old.identity());
    assert_eq!(live.dimensions(), (64, 64));
    assert!(live.validate_result(old.prepared()).is_ok());
    assert!(old.validate_result(live.prepared()).is_ok());
    assert_eq!(scheduler.lease_counters().acquired(), acquired);
    assert_eq!(scheduler.lease_counters().live(), 1);
    drop((live, old));
    scheduler.succeed(work.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn equal_numeric_work_on_foreign_scheduler_cannot_supply_original_permit() {
    let snapshot = owner("witness-collision").acquire_snapshot(0).unwrap();
    let witness = witness(&snapshot, 7);
    let mut a = FrameScheduler::new();
    let mut b = FrameScheduler::new();
    let wa = schedule(&mut a, &snapshot, 7);
    let wb = schedule(&mut b, &snapshot, 7);
    assert_eq!(wa.work_id(), wb.work_id());
    assert_eq!(wa.view_generation(), wb.view_generation());
    assert_eq!(wa.key(), wb.key());
    assert!(matches!(
        prepare(&b, &wa, &witness),
        Err(ScheduledObjectSceneError::Inactive)
    ));
    assert!(matches!(
        prepare(&a, &wb, &witness),
        Err(ScheduledObjectSceneError::Inactive)
    ));
    let la = prepare(&a, &wa, &witness).unwrap();
    let lb = prepare(&b, &wb, &witness).unwrap();
    assert_ne!(la.identity(), lb.identity());
    assert_eq!(
        la.validate_result(lb.prepared()),
        Err(ScheduledObjectSceneError::ForeignResult)
    );
    assert_eq!(
        lb.validate_result(la.prepared()),
        Err(ScheduledObjectSceneError::ForeignResult)
    );
    drop((la, lb));
    a.cancel(wa.work_id()).unwrap();
    b.cancel(wb.work_id()).unwrap();
    assert_eq!(
        (a.lease_counters().live(), b.lease_counters().live()),
        (0, 0)
    );
}

#[test]
fn foreign_or_changed_witness_refuses_while_original_revision_remains_pinned() {
    let mut original = owner("witness-pins");
    let snapshot = original.acquire_snapshot(0).unwrap();
    let admitted = witness(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, 7);
    let fill = original.prepare_fill_json(&serde_json::to_vec(&json!({"command":"object.fill.set",
        "stableTarget":{"contextId":"scene-root","frameScope":{"kind":"authored","frame":7},"layerUid":"L2","strokeId":"a"},
        "fill":{"kind":"solid","r":1,"g":1,"b":0,"a":1}})).unwrap()).unwrap();
    assert_eq!(original.commit_fill(fill), Ok(1));
    let next_revision = original.acquire_snapshot(1).unwrap();
    let same_instance_foreign_document = owner("witness-pins").acquire_snapshot(0).unwrap();
    let foreign_instance = owner("witness-other").acquire_snapshot(0).unwrap();
    for changed in [
        witness(&snapshot, 0),
        witness(&next_revision, 7),
        witness(&same_instance_foreign_document, 7),
        witness(&foreign_instance, 7),
    ] {
        let leases = scheduler.lease_counters();
        assert!(matches!(
            prepare(&scheduler, &work, &changed),
            Err(ScheduledObjectSceneError::RetainedSourceMismatch)
        ));
        assert_eq!(scheduler.lease_counters(), leases);
        assert!(scheduler.receipt(work.work_id()).is_none());
    }
    // Export admission remains pinned after an edit; host release is separate.
    assert!(prepare(&scheduler, &work, &admitted).is_ok());
    scheduler.cancel(work.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn selector_identity_errors_and_precedence_remain_compatible() {
    let snapshot = owner("witness-selector").acquire_snapshot(0).unwrap();
    let input = plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, 7);
    for (case, expected) in [
        (0, ObjectSceneError::WrongInstance),
        (1, ObjectSceneError::WrongDocument),
        (2, ObjectSceneError::WrongRevision),
        (3, ObjectSceneError::WrongSnapshot),
        (4, ObjectSceneError::InvalidContext),
        (5, ObjectSceneError::WrongFrame),
        (6, ObjectSceneError::WrongScope),
    ] {
        let mut bad = selector(&snapshot, 7);
        match case {
            0 => bad.instance_id = "foreign",
            1 => bad.document_id = "foreign",
            2 => bad.content_revision = 1,
            3 => bad.document_snapshot_id = "foreign",
            4 => bad.context_id = "foreign",
            5 => bad.frame = 0,
            _ => bad.scope_kind = crate::object_document::FrameScopeKind::Reference,
        }
        assert_eq!(
            prepare_object_composition(&input, &bad).unwrap_err(),
            expected
        );
        assert!(
            matches!(prepare_selector(&scheduler, &work, &input, &bad), Err(ScheduledObjectSceneError::Pins(error)) if error == expected)
        );
    }
    scheduler.cancel(work.work_id()).unwrap();
    let mut wrong = selector(&snapshot, 7);
    wrong.context_id = "foreign";
    assert!(matches!(
        prepare_selector(&scheduler, &work, &input, &wrong),
        Err(ScheduledObjectSceneError::Inactive)
    ));
}

#[test]
fn stale_terminal_and_replaced_work_refuse_without_reacquisition() {
    let snapshot = owner("witness-terminal").acquire_snapshot(0).unwrap();
    let witness = witness(&snapshot, 7);
    for terminal in 0..5 {
        let mut scheduler = FrameScheduler::new();
        let work = schedule(&mut scheduler, &snapshot, 7);
        assert!(prepare(&scheduler, &work, &witness).is_ok());
        let mut newer = None;
        match terminal {
            0 => {
                scheduler.cancel(work.work_id()).unwrap();
            }
            1 => {
                scheduler
                    .fail(
                        work.work_id(),
                        FrameFailure::new(FrameFailureKind::Worker, "failed"),
                    )
                    .unwrap();
            }
            2 => {
                scheduler.succeed(work.work_id()).unwrap();
            }
            3 => {
                newer = Some(schedule(&mut scheduler, &snapshot, 0));
            }
            _ => {
                let replacement = owner("witness-replaced").acquire_snapshot(0).unwrap();
                scheduler
                    .replace_object_document(&replacement, &pins(&replacement, 0))
                    .unwrap();
            }
        }
        let leases = scheduler.lease_counters();
        assert!(matches!(
            prepare(&scheduler, &work, &witness),
            Err(ScheduledObjectSceneError::Inactive)
        ));
        assert_eq!(scheduler.lease_counters(), leases);
        scheduler.cancel(work.work_id()).unwrap();
        if let Some(next) = newer {
            scheduler.cancel(next.work_id()).unwrap();
        }
        assert_eq!(scheduler.lease_counters().live(), 0);
        assert_eq!(
            scheduler.lease_counters().acquired(),
            scheduler.lease_counters().released()
        );
    }
}

#[test]
fn unsupported_output_quality_and_resource_leases_cannot_create_permit() {
    let snapshot = owner("witness-output").acquire_snapshot(0).unwrap();
    let witness = witness(&snapshot, 7);
    for case in 0..6 {
        let mut scheduler = FrameScheduler::new();
        let (kind, format, color, alpha, quality) = match case {
            0 => ("thumbnail", "rgba8", "srgb", "straight", "final"),
            1 => ("frame", "rgba16f", "srgb", "straight", "final"),
            2 => ("frame", "rgba8", "linear", "straight", "final"),
            3 => ("frame", "rgba8", "srgb", "premultiplied", "final"),
            4 => ("frame", "rgba8", "srgb", "straight", "preview"),
            _ => ("frame", "rgba8", "srgb", "straight", "final"),
        };
        let resources = if case == 5 {
            vec![("foreign".into(), "v1".into())]
        } else {
            vec![]
        };
        let key = EvaluationKey::new(
            snapshot.snapshot_id(),
            "scene-root",
            7,
            quality,
            OutputSpec::new(kind, format, 64, 64, color, alpha).unwrap(),
            resources,
        )
        .unwrap();
        let work = scheduler
            .schedule_object(snapshot.clone(), pins(&snapshot, 7), key)
            .unwrap();
        let leases = scheduler.lease_counters();
        assert!(matches!(
            prepare(&scheduler, &work, &witness),
            Err(ScheduledObjectSceneError::UnsupportedOutput)
        ));
        assert!(matches!(
            prepare_selector(&scheduler, &work, witness.plan(), &selector(&snapshot, 7)),
            Err(ScheduledObjectSceneError::UnsupportedOutput)
        ));
        assert_eq!(scheduler.lease_counters(), leases);
        scheduler.cancel(work.work_id()).unwrap();
        assert_eq!(scheduler.lease_counters().live(), 0);
    }
    assert!(OutputSpec::new("frame", "rgba8", 0, 64, "srgb", "straight").is_err());
}

#[test]
fn opacity_work_cannot_substitute_for_retained_object_source() {
    let owner = NativeOpacityHistory::new(
        "witness-opacity",
        crate::codec::decode_project(include_bytes!("fixtures/opacity-v2/project.json")).unwrap(),
    )
    .unwrap();
    let opacity = owner.acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let work = scheduler
        .schedule(
            opacity.clone(),
            EvaluationKey::new(opacity.id(), "scene-root", 7, "final", output(), []).unwrap(),
        )
        .unwrap();
    let object = crate::compositor::object_test_owner("witness-object")
        .acquire_snapshot(0)
        .unwrap();
    let witness = witness(&object, 7);
    assert!(matches!(
        prepare(&scheduler, &work, &witness),
        Err(ScheduledObjectSceneError::RetainedSourceMismatch)
    ));
    let mut bad = selector(&object, 7);
    bad.context_id = "foreign";
    assert!(matches!(
        prepare_selector(&scheduler, &work, witness.plan(), &bad),
        Err(ScheduledObjectSceneError::RetainedSourceMismatch)
    ));
    scheduler.cancel(work.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
}
