//! Export-class witnesses against the real owner, packet and paint plan: an
//! overlay set never attaches to them. No GPU, scheduler pixels or export.
use super::tests::scenario_set;
use super::*;
use crate::compositor::{
    object_test_owner as owner, object_test_plan as plan, object_test_selector as selector,
};
use crate::object_render_scene::prepare_object_composition;

pub(super) fn export_witness(instance: &str, frame: u32) -> ObjectCompositionWitness {
    let snapshot = owner(instance).acquire_snapshot(0).unwrap();
    prepare_object_composition(&plan(&snapshot, frame), &selector(&snapshot, frame)).unwrap()
}

type Attach = dyn Fn(
    &ObjectCompositionWitness,
    &ViewportOverlaySet,
) -> Result<ViewportOverlayAttachment, OverlayAttachError>;

/// The set shapes an export-class witness must refuse, the empty one included.
pub(super) fn shapes() -> Vec<(&'static str, ViewportOverlaySet)> {
    let mut modes_only = ViewportOverlaySet::new();
    modes_only.enable_mode(EditorDisplayMode::CurrentFrameOutline);
    let mut with_modes = scenario_set("full");
    with_modes.enable_mode(EditorDisplayMode::OutlineView);
    vec![
        ("empty", ViewportOverlaySet::new()),
        ("pair", scenario_set("pair")),
        ("full", scenario_set("full")),
        ("modes_only", modes_only),
        ("full_with_modes", with_modes),
    ]
}

/// One shape must be refused with the typed error and left untouched.
pub(super) fn assert_refuses_set(attach: &Attach, set: &ViewportOverlaySet) {
    let witness = export_witness("overlay-gate", 7);
    assert!(!witness.is_viewport(), "fixture must be export class");
    let before = set.clone();
    assert_eq!(
        attach(&witness, set),
        Err(OverlayAttachError::ExportClassWitness)
    );
    assert_eq!(set, &before, "a refused attach must not touch the set");
}

pub(super) fn assert_refuses(attach: &Attach) {
    for (_, set) in shapes() {
        assert_refuses_set(attach, &set);
    }
}

#[test]
fn export_class_witness_refuses_every_overlay_set_shape_with_the_typed_error() {
    assert_refuses(&attach_viewport_overlays);
}

#[test]
fn the_class_check_ignores_which_frame_the_export_witness_is_for() {
    for (instance, frame) in [("overlay-gate-a", 0), ("overlay-gate-b", 7)] {
        let witness = export_witness(instance, frame);
        assert!(!witness.is_viewport());
        assert_eq!(witness.frame(), frame);
        assert_eq!(
            attach_viewport_overlays(&witness, &scenario_set("full")),
            Err(OverlayAttachError::ExportClassWitness)
        );
    }
}

#[test]
fn repeated_refusals_do_not_disturb_the_export_witness() {
    let witness = export_witness("overlay-gate-repeat", 7);
    let twin = witness.clone();
    for _ in 0..3 {
        assert!(attach_viewport_overlays(&witness, &scenario_set("full")).is_err());
    }
    assert!(witness.same_identity_as(&twin));
    assert_eq!(witness.content_revision(), twin.content_revision());
    assert_eq!(witness.frame(), 7);
}
