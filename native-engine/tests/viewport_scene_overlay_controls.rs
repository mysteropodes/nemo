//! Negative and mutation controls. Each mutant is a local re-implementation
//! with one deliberate defect; the shared assertion that guards the production
//! behaviour must reject it, so the production tests are not vacuous.
use super::gate_tests::{assert_refuses, assert_refuses_set, shapes};
use super::tests::{offending_lines, oracle_stack_rows_match};
use super::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

const SOURCE: &str = include_str!("../src/viewport_scene_overlay.rs");

/// Attach ignoring the witness class entirely (the contract's mutation).
fn attach_class_blind(
    witness: &ObjectCompositionWitness,
    set: &ViewportOverlaySet,
) -> Result<ViewportOverlayAttachment, OverlayAttachError> {
    Ok(ViewportOverlayAttachment {
        pins: OverlayPins::of(witness),
        set: set.clone(),
    })
}

/// Refuses an export witness only when there is something to draw.
fn attach_only_when_non_empty(
    witness: &ObjectCompositionWitness,
    set: &ViewportOverlaySet,
) -> Result<ViewportOverlayAttachment, OverlayAttachError> {
    if !set.is_empty() && !witness.is_viewport() {
        return Err(OverlayAttachError::ExportClassWitness);
    }
    attach_class_blind(witness, set)
}

/// Refuses layers but forgets the display modes.
fn attach_ignoring_modes(
    witness: &ObjectCompositionWitness,
    set: &ViewportOverlaySet,
) -> Result<ViewportOverlayAttachment, OverlayAttachError> {
    if !set.layers().is_empty() && !witness.is_viewport() {
        return Err(OverlayAttachError::ExportClassWitness);
    }
    attach_class_blind(witness, set)
}

fn panics<F: FnOnce()>(body: F) -> bool {
    catch_unwind(AssertUnwindSafe(body)).is_err()
}

fn shape(name: &str) -> ViewportOverlaySet {
    shapes().into_iter().find(|(n, _)| *n == name).unwrap().1
}

#[test]
fn control_the_real_gate_passes_every_refusal_shape() {
    assert_refuses(&attach_viewport_overlays);
}

#[test]
fn control_a_class_blind_attach_to_an_export_witness_fails_the_refusal_assertion() {
    assert!(panics(|| assert_refuses(&attach_class_blind)));
    for (name, set) in shapes() {
        assert!(
            panics(|| assert_refuses_set(&attach_class_blind, &set)),
            "class-blind attach must be caught on {name}"
        );
    }
}

#[test]
fn control_gates_that_skip_empty_sets_or_display_modes_are_caught_on_that_shape() {
    let empty = shape("empty");
    assert!(panics(|| assert_refuses_set(
        &attach_only_when_non_empty,
        &empty
    )));
    for name in ["pair", "full", "modes_only", "full_with_modes"] {
        let set = shape(name);
        assert!(
            !panics(|| assert_refuses_set(&attach_only_when_non_empty, &set)),
            "non-empty-only still refuses {name}"
        );
    }
    let modes_only = shape("modes_only");
    assert!(panics(|| assert_refuses_set(
        &attach_ignoring_modes,
        &modes_only
    )));
    for name in ["pair", "full", "full_with_modes"] {
        let set = shape(name);
        assert!(
            !panics(|| assert_refuses_set(&attach_ignoring_modes, &set)),
            "ignoring-modes still refuses layers on {name}"
        );
    }
}

fn push_only(set: &ViewportOverlaySet, len: usize, volatile: bool) -> Vec<StackSlot> {
    let mut slots: Vec<StackSlot> = (0..len).map(StackSlot::Artwork).collect();
    for layer in set.layers() {
        if volatile || !layer.category().is_volatile() {
            slots.push(StackSlot::Overlay(layer.category()));
        }
    }
    slots
}

fn below_at_zero(set: &ViewportOverlaySet, len: usize, volatile: bool) -> Vec<StackSlot> {
    let mut slots: Vec<StackSlot> = (0..len).map(StackSlot::Artwork).collect();
    for layer in set.layers() {
        if !volatile && layer.category().is_volatile() {
            continue;
        }
        let slot = StackSlot::Overlay(layer.category());
        match layer.category().placement() {
            OverlayPlacement::BelowArtwork => slots.insert(0, slot),
            OverlayPlacement::AboveArtwork => slots.push(slot),
        }
    }
    slots
}

fn below_reversed(set: &ViewportOverlaySet, len: usize, volatile: bool) -> Vec<StackSlot> {
    let mut slots: Vec<StackSlot> = (0..len).map(StackSlot::Artwork).collect();
    let mut below = vec![];
    for layer in set.layers() {
        match layer.category().placement() {
            OverlayPlacement::BelowArtwork => below.push(StackSlot::Overlay(layer.category())),
            OverlayPlacement::AboveArtwork => {
                if volatile || !layer.category().is_volatile() {
                    slots.push(StackSlot::Overlay(layer.category()));
                }
            }
        }
    }
    for slot in below.into_iter().rev() {
        slots.insert(slots.len().min(1), slot);
    }
    slots
}

fn volatile_always(set: &ViewportOverlaySet, len: usize, _volatile: bool) -> Vec<StackSlot> {
    set.stack(len, true)
}

#[test]
fn control_every_stack_mutant_fails_the_oracle_rows_and_the_real_stack_passes() {
    let mut checked = 0;
    assert!(oracle_stack_rows_match(
        |set, len, volatile| set.stack(len, volatile),
        &mut checked
    ));
    assert_eq!(checked, 14, "the real stack was compared with every row");
    type Mutant = fn(&ViewportOverlaySet, usize, bool) -> Vec<StackSlot>;
    let mutants: [(&str, Mutant); 4] = [
        ("push everything", push_only),
        ("below layers at index 0", below_at_zero),
        ("below layers in reverse order", below_reversed),
        ("volatile layers never dropped", volatile_always),
    ];
    for (name, mutant) in mutants {
        let mut rows = 0;
        assert!(
            !oracle_stack_rows_match(mutant, &mut rows),
            "{name} must differ"
        );
        assert_eq!(rows, 14, "{name} was compared with every row");
    }
}

#[test]
fn control_the_source_guard_flags_a_widened_import_super_paths_and_serialization() {
    assert!(offending_lines(SOURCE).is_empty());
    let witness_import = "use crate::object_render_scene::ObjectCompositionWitness;";
    let widened = SOURCE.replacen(
        witness_import,
        &format!("{witness_import}\nuse crate::scheduler::FrameScheduler;"),
        1,
    );
    assert_ne!(widened, SOURCE);
    assert_eq!(
        offending_lines(&widened),
        ["use crate::scheduler::FrameScheduler;"]
    );
    let via_super = SOURCE.replacen(
        witness_import,
        &format!("{witness_import}\nuse super::scheduler as s;"),
        1,
    );
    assert!(!offending_lines(&via_super).is_empty());
    let serial = SOURCE.replacen(
        "pub(crate) struct OverlayLayer {",
        "#[derive(Serialize)]\npub(crate) struct OverlayLayer {",
        1,
    );
    assert_ne!(serial, SOURCE);
    assert_eq!(offending_lines(&serial), ["#[derive(Serialize)]"]);
}
