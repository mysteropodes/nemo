//! Viewport-class witnesses (need the `application` + `project_media_codec`
//! features that own `prepare_object_viewport_composition`): the only witness
//! class an overlay set attaches to. Real owner, packet, paint plan and
//! scheduler; no GPU, no reference image bytes, no installed display.
use super::gate_tests::export_witness;
use super::tests::scenario_set;
use super::*;
use crate::compositor::{
    object_test_owner as owner, object_test_plan as plan, object_test_selector as selector,
    schedule_test_object as schedule,
};
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_viewport_composition,
    prepare_scheduled_object_composition_from_witness as prepare_live,
};
use crate::scheduler::FrameScheduler;

pub(super) fn viewport_witness(instance: &str, frame: u32) -> ObjectCompositionWitness {
    let snapshot = owner(instance).acquire_snapshot(0).unwrap();
    prepare_object_viewport_composition(&plan(&snapshot, frame), &selector(&snapshot, frame), None)
        .unwrap()
}

/// Export and viewport witnesses from ONE snapshot: every pin is equal, so the
/// witness class is the only difference between them.
fn pair(instance: &str, frame: u32) -> (ObjectCompositionWitness, ObjectCompositionWitness) {
    let snapshot = owner(instance).acquire_snapshot(0).unwrap();
    let export =
        prepare_object_composition(&plan(&snapshot, frame), &selector(&snapshot, frame)).unwrap();
    let viewport = prepare_object_viewport_composition(
        &plan(&snapshot, frame),
        &selector(&snapshot, frame),
        None,
    )
    .unwrap();
    (export, viewport)
}

#[test]
fn viewport_witness_admits_every_set_shape_and_the_attachment_keeps_the_set() {
    let witness = viewport_witness("overlay-viewport", 7);
    assert!(witness.is_viewport());
    let mut full = scenario_set("full");
    full.enable_mode(EditorDisplayMode::OutlineView);
    full.enable_mode(EditorDisplayMode::CurrentFrameOutline);
    for set in [
        ViewportOverlaySet::new(),
        scenario_set("pair"),
        scenario_set("guides"),
        full,
    ] {
        let attached = attach_viewport_overlays(&witness, &set).unwrap();
        assert_eq!(attached.set(), &set);
        assert_eq!(attached.frame(), 7);
        assert!(attached.belongs_to(&witness));
    }
    let attached = attach_viewport_overlays(&witness, &scenario_set("full")).unwrap();
    assert_eq!(attached.set().layers().len(), 30);
    assert_eq!(attached.set().stack(3, true).len(), 33);
    assert_eq!(attached.set().stack(3, false).len(), 28);
}

#[test]
fn only_the_witness_class_separates_an_admitted_pair_of_equal_pins() {
    let (export, viewport) = pair("overlay-pair", 7);
    assert_eq!(export.instance_id(), viewport.instance_id());
    assert_eq!(export.document_id(), viewport.document_id());
    assert_eq!(export.content_revision(), viewport.content_revision());
    assert_eq!(
        export.document_snapshot_id(),
        viewport.document_snapshot_id()
    );
    assert_eq!(export.context_id(), viewport.context_id());
    assert_eq!(export.frame(), viewport.frame());
    assert!(!export.is_viewport() && viewport.is_viewport());
    assert!(!export.same_identity_as(&viewport));
    let set = scenario_set("full");
    assert_eq!(
        attach_viewport_overlays(&export, &set),
        Err(OverlayAttachError::ExportClassWitness)
    );
    let attached = attach_viewport_overlays(&viewport, &set).unwrap();
    assert!(attached.belongs_to(&viewport));
    assert!(
        !attached.belongs_to(&export),
        "equal pins on an export-class witness do not belong"
    );
}

#[test]
fn attachment_is_bound_to_the_pins_of_the_witness_it_was_admitted_against() {
    let witness = viewport_witness("overlay-bind", 7);
    let attached = attach_viewport_overlays(&witness, &scenario_set("pair")).unwrap();
    assert!(attached.belongs_to(&witness));
    let other_frame = viewport_witness("overlay-bind", 0);
    assert_eq!(other_frame.frame(), 0);
    assert!(!attached.belongs_to(&other_frame));
    let other_owner = viewport_witness("overlay-bind-other", 7);
    assert!(!witness.same_identity_as(&other_owner));
    assert!(!attached.belongs_to(&other_owner));
    assert!(!attached.belongs_to(&export_witness("overlay-bind", 7)));
}

#[test]
fn attaching_overlays_cannot_change_the_scheduled_identity_or_the_lease_balance() {
    let snapshot = owner("overlay-identity").acquire_snapshot(0).unwrap();
    let witness =
        prepare_object_viewport_composition(&plan(&snapshot, 7), &selector(&snapshot, 7), None)
            .unwrap();
    let mut scheduler = FrameScheduler::new();
    let work = schedule(&mut scheduler, &snapshot, 7);
    let live = prepare_live(&scheduler, &work, &witness).unwrap();
    let identity = live.identity().clone();
    let counters = scheduler.lease_counters();
    let attached = attach_viewport_overlays(&witness, &scenario_set("full")).unwrap();
    assert_eq!(live.identity(), &identity);
    assert_eq!(scheduler.lease_counters(), counters);
    drop(live);
    scheduler.succeed(work.work_id()).unwrap();
    assert_eq!(scheduler.lease_counters().live(), 0);
    drop(scheduler);
    // The attachment never borrowed the scheduler, the snapshot or a lease.
    assert!(attached.belongs_to(&witness));
    assert_eq!(attached.set().layers().len(), 30);
}

#[test]
fn control_pins_alone_cannot_tell_the_classes_apart_so_the_class_check_is_load_bearing() {
    let (export, viewport) = pair("overlay-pins-only", 7);
    let attached = attach_viewport_overlays(&viewport, &scenario_set("pair")).unwrap();
    // A pins-only belongs_to mutant accepts the export-class witness...
    assert!(attached.pins == OverlayPins::of(&export));
    // ...which is exactly what the real check rejects.
    assert!(!attached.belongs_to(&export));
    assert!(attached.belongs_to(&viewport));
}
