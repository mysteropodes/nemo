//! CPU permit admission of a resource-carrying witness against the REAL
//! scheduler, owner and lease counters (no GPU, export or installed proof).
//! The permit is created only when the witness's set equals the key's
//! `declared_resource_versions()` exactly; every refusal is typed and leaves
//! the lease counters unchanged. Expected kinds and counter values below are
//! hand-derived: one snapshot lease plus one lease per declared resource.
use super::tests::{set_of, Payload};
use super::*;
use crate::compositor::{
    object_test_owner as owner, object_test_plan as plan, object_test_selector as selector,
};
use crate::object_render_scene::{
    prepare_object_composition, prepare_scheduled_object_composition as prepare_selector,
    prepare_scheduled_object_composition_from_witness as prepare, ObjectCompositionWitness,
    ScheduledObjectSceneError,
};
use crate::revision::ObjectSnapshot;
use crate::scheduler::{
    EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
    ScheduledFrame,
};
use std::sync::Weak;

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
fn schedule_quality(
    scheduler: &mut FrameScheduler,
    snapshot: &ObjectSnapshot,
    quality: &str,
    declared: BTreeMap<String, String>,
) -> ScheduledFrame {
    let output = OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap();
    let key = EvaluationKey::new(
        snapshot.snapshot_id(),
        "scene-root",
        7,
        quality,
        output,
        declared,
    )
    .unwrap();
    scheduler
        .schedule_object(snapshot.clone(), pins(snapshot, 7), key)
        .unwrap()
}
fn schedule(
    scheduler: &mut FrameScheduler,
    snapshot: &ObjectSnapshot,
    declared: &[(&str, &str)],
) -> ScheduledFrame {
    let declared = declared
        .iter()
        .map(|(id, version)| (id.to_string(), version.to_string()))
        .collect();
    schedule_quality(scheduler, snapshot, "final", declared)
}
fn base(snapshot: &ObjectSnapshot) -> ObjectCompositionWitness {
    prepare_object_composition(&plan(snapshot, 7), &selector(snapshot, 7)).unwrap()
}
fn refused<T>(result: Result<T, ScheduledObjectSceneError>) -> ScheduledObjectSceneError {
    match result {
        Err(error) => error,
        Ok(_) => panic!("permit must be refused"),
    }
}
fn resource(kind: SceneResourceMismatchKind) -> ScheduledObjectSceneError {
    ScheduledObjectSceneError::ResourceDeclaration(kind)
}

#[test]
fn exact_declaration_creates_the_permit_and_the_key_is_built_from_the_set() {
    let snapshot = owner("rs-exact").acquire_snapshot(0).unwrap();
    let set = Arc::new(set_of(&[("raster/1", "v1"), ("media/1", "v4")]));
    let mut scheduler = FrameScheduler::new();
    let work = schedule_quality(&mut scheduler, &snapshot, "final", set.declared_versions());
    assert_eq!(
        work.key().declared_resource_versions(),
        &set.declared_versions()
    );
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.live()),
        (3, 3),
        "snapshot + 2 resources"
    );
    let witness = base(&snapshot).with_scene_resources(Arc::clone(&set));
    let permit = prepare(&scheduler, &work, &witness).unwrap();
    let held = permit.prepared().composition().scene_resources().unwrap();
    assert!(Arc::ptr_eq(held, &set));
    assert_eq!(
        scheduler.lease_counters(),
        counters,
        "admission acquires nothing"
    );
    // The selector entry point builds a resource-free witness: a declared key refuses it.
    let bare = prepare_selector(&scheduler, &work, witness.plan(), &selector(&snapshot, 7));
    assert_eq!(refused(bare), resource(SceneResourceMismatchKind::Missing));
    drop(permit);
    scheduler.succeed(work.work_id()).unwrap();
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (3, 3, 0)
    );
}

#[test]
fn empty_declaration_keeps_the_existing_behaviour_with_or_without_an_empty_set() {
    let snapshot = owner("rs-empty").acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, &[]);
    let plain = base(&snapshot);
    let with_empty =
        base(&snapshot).with_scene_resources(Arc::new(ObjectSceneResourceSet::empty()));
    assert!(plain.scene_resources().is_none() && with_empty.scene_resources().is_some());
    let a = prepare(&scheduler, &work, &plain).unwrap();
    let b = prepare(&scheduler, &work, &with_empty).unwrap();
    let c = prepare_selector(&scheduler, &work, plain.plan(), &selector(&snapshot, 7)).unwrap();
    assert!(
        a.validate_result(b.prepared()).is_ok(),
        "absent and empty sets are one identity"
    );
    assert!(a.validate_result(c.prepared()).is_ok());
    assert_eq!(scheduler.lease_counters().acquired(), 1);
    drop((a, b, c));
    scheduler.cancel(work.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn every_declaration_mismatch_refuses_typed_and_leaves_lease_counters_unchanged() {
    use super::SceneResourceMismatchKind::{Extra, Missing, VersionSkew};
    type Pairs = &'static [(&'static str, &'static str)];
    // (name, key declaration, held set or no set, expected kind)
    let cases: &[(&str, Pairs, Option<Pairs>, SceneResourceMismatchKind)] = &[
        ("declared-no-set", &[("a", "1")], None, Missing),
        (
            "declared-two-held-one",
            &[("a", "1"), ("b", "1")],
            Some(&[("a", "1")]),
            Missing,
        ),
        ("held-undeclared", &[], Some(&[("a", "1")]), Extra),
        (
            "extra-beyond-declared",
            &[("a", "1")],
            Some(&[("a", "1"), ("b", "1")]),
            Extra,
        ),
        (
            "version-skew",
            &[("a", "2")],
            Some(&[("a", "1")]),
            VersionSkew,
        ),
        (
            "skew-with-equal-counts",
            &[("a", "1"), ("b", "2")],
            Some(&[("a", "1"), ("b", "3")]),
            VersionSkew,
        ),
    ];
    for (name, declared, held, kind) in cases {
        let snapshot = owner("rs-matrix").acquire_snapshot(0).unwrap();
        let mut scheduler = FrameScheduler::new();
        let work = schedule(&mut scheduler, &snapshot, declared);
        let witness = match held {
            Some(pairs) => base(&snapshot).with_scene_resources(Arc::new(set_of(pairs))),
            None => base(&snapshot),
        };
        let counters = scheduler.lease_counters();
        assert_eq!(counters.acquired(), 1 + declared.len() as u64, "{name}");
        assert_eq!(
            refused(prepare(&scheduler, &work, &witness)),
            resource(*kind),
            "{name}"
        );
        assert_eq!(scheduler.lease_counters(), counters, "{name}");
        scheduler.cancel(work.work_id()).unwrap();
        let counters = scheduler.lease_counters();
        assert_eq!(counters.live(), 0, "{name}");
        assert_eq!(counters.acquired(), counters.released(), "{name}");
    }
}

#[test]
fn output_refusals_still_take_precedence_over_a_resource_mismatch() {
    let snapshot = owner("rs-precedence").acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let declared = BTreeMap::from([("a".to_string(), "1".to_string())]);
    let preview = schedule_quality(&mut scheduler, &snapshot, "preview", declared);
    let witness = base(&snapshot);
    let error = refused(prepare(&scheduler, &preview, &witness));
    assert_eq!(error, ScheduledObjectSceneError::UnsupportedOutput);
    scheduler.cancel(preview.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn a_new_version_key_stales_the_old_work_through_the_existing_current_check() {
    let snapshot = owner("rs-version").acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let (old_set, new_set) = (
        Arc::new(set_of(&[("a", "v1")])),
        Arc::new(set_of(&[("a", "v2")])),
    );
    let old_work = schedule(&mut scheduler, &snapshot, &[("a", "v1")]);
    let old_witness = base(&snapshot).with_scene_resources(old_set);
    drop(prepare(&scheduler, &old_work, &old_witness).unwrap());
    let new_work = schedule(&mut scheduler, &snapshot, &[("a", "v2")]);
    assert_ne!(old_work.view_generation(), new_work.view_generation());
    assert!(!scheduler.is_current(&old_work) && scheduler.is_current(&new_work));
    let counters = scheduler.lease_counters();
    let stale = refused(prepare(&scheduler, &old_work, &old_witness));
    assert_eq!(stale, ScheduledObjectSceneError::Inactive);
    let skew = refused(prepare(&scheduler, &new_work, &old_witness));
    assert_eq!(skew, resource(SceneResourceMismatchKind::VersionSkew));
    assert_eq!(scheduler.lease_counters(), counters);
    let new_witness = base(&snapshot).with_scene_resources(new_set);
    drop(prepare(&scheduler, &new_work, &new_witness).unwrap());
    scheduler.cancel(old_work.work_id()).unwrap();
    scheduler.cancel(new_work.work_id()).unwrap();
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (4, 4, 0)
    );
}

#[test]
fn retention_tokens_live_exactly_as_long_as_witness_permit_and_result() {
    let snapshot = owner("rs-lifetime").acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, &[("r", "v1")]);
    let payload = Arc::new(Payload(1));
    let weak: Weak<Payload> = Arc::downgrade(&payload);
    let retention: SceneResourceRetention = payload.clone();
    let entry = SceneResourceEntry::new("r", "v1", SceneResourceKind::Media, 64, retention);
    let set = Arc::new(ObjectSceneResourceSet::new(vec![entry]).unwrap());
    drop(payload);
    let witness = base(&snapshot).with_scene_resources(set);
    assert_eq!(weak.strong_count(), 1, "only the set holds the token");
    let permit = prepare(&scheduler, &work, &witness).unwrap();
    let result = permit.prepared().clone();
    drop(witness);
    assert_eq!(weak.strong_count(), 1, "the permit still holds it");
    drop(permit);
    assert_eq!(weak.strong_count(), 1, "the retained result still holds it");
    let entry = result
        .composition()
        .scene_resources()
        .unwrap()
        .get("r")
        .unwrap();
    assert_eq!(
        entry.retention().downcast_ref::<Payload>().map(|p| p.0),
        Some(1)
    );
    drop(result);
    assert_eq!(weak.strong_count(), 0, "released with the last holder");
    assert!(weak.upgrade().is_none());
    scheduler.succeed(work.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
}

#[test]
fn equal_descriptors_over_different_tokens_are_foreign_results() {
    let snapshot = owner("rs-foreign").acquire_snapshot(0).unwrap();
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, &[("r", "v1")]);
    let shared = Arc::new(set_of(&[("r", "v1")]));
    let other = Arc::new(set_of(&[("r", "v1")]));
    assert!(
        !shared.same_as(&other),
        "same id/version, different allocation"
    );
    let w_shared = base(&snapshot).with_scene_resources(Arc::clone(&shared));
    let w_same = base(&snapshot).with_scene_resources(shared);
    let w_other = base(&snapshot).with_scene_resources(other);
    let a = prepare(&scheduler, &work, &w_shared).unwrap();
    let same = prepare(&scheduler, &work, &w_same).unwrap();
    let foreign = prepare(&scheduler, &work, &w_other).unwrap();
    assert!(a.validate_result(same.prepared()).is_ok());
    assert_eq!(
        a.validate_result(foreign.prepared()),
        Err(ScheduledObjectSceneError::ForeignResult)
    );
    drop((a, same, foreign));
    scheduler.cancel(work.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
}
