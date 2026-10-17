//! L08-K on real documents: the layer records are decoded by the real storage codec, pinned in a real snapshot,
//! selected into a real packet, drawn by the real contour producer and joined by the real L07-C registry before
//! `plan_scene_effects` reads them through the packet. Expected values are hand-derived literals.
use super::support::{
    base_document, contour_claims, packet, paint_plan, pinned, push_layer, render_scene, selector,
    set_layer, snapshot, FRAME,
};
use super::{
    bind_scene_resources, plan_scene_effects, BindResourcesError, EntryOutcome, JoinOptions,
    SceneEffectJoinError, SlotRole,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    PinField, ProducerOutput, ScenePin, UnavailableCause,
};
use crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::{
    ObjectSceneResourceSet, SceneResourceEntry, SceneResourceKind, SceneResourceMismatchKind,
};
use crate::object_render_scene::scene_effect_kernels::scene_effect_order::{
    CompositionPlan, LayerStep, MatteExtra, SkipReason,
};
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::Landing;
use crate::object_render_scene::{prepare_object_composition, ObjectSceneError};
use serde_json::json;
use std::sync::Arc;

fn no_custom_effects(_: &str) -> bool {
    false
}

static NO_CUSTOM: fn(&str) -> bool = no_custom_effects;

fn options() -> JoinOptions<'static> {
    JoinOptions::new(1.0, &NO_CUSTOM)
}

#[test]
fn a_plain_document_joins_to_the_flat_plan_in_paint_order() {
    let snap = snapshot(&base_document(), "join-flat");
    let pk = packet(&snap, FRAME);
    let scene = render_scene(&pk);
    let joined = pinned(&pk, vec![contour_claims(&scene, "contour", &[])]);
    let plan = plan_scene_effects(&pk, &joined, &options()).unwrap();
    let uids: Vec<&str> = plan.layers().iter().map(|l| l.layer_uid.as_str()).collect();
    assert_eq!(uids, ["L0", "L1", "L2"]);
    // The document stores L2, L0, L1: the wire index is the paint position, not the array position.
    let sources: Vec<usize> = plan.layers().iter().map(|l| l.source_layer_index).collect();
    assert_eq!(sources, [1, 2, 0]);
    assert_eq!(plan.wire_index_of_source(0), Some(2));
    assert_eq!(plan.wire_index_of_source(9), None);
    assert_eq!(
        plan.plan(),
        &CompositionPlan::Flat {
            order: vec![0, 1, 2]
        }
    );
    let counts: Vec<usize> = plan.layers().iter().map(|l| l.entries.len()).collect();
    assert_eq!(counts, [3, 1, 1]);
    assert_eq!(plan.producers(), ["contour"]);
    assert!(plan.is_complete());
    assert!(plan.is_for_packet(&pk));
    assert!(plan.declared_resource_versions().is_empty());
}

#[test]
fn layer_records_reach_the_plan_through_the_actual_snapshot() {
    let mut document = base_document();
    set_layer(&mut document, "L1", json!({"blendMode": "multiply"}));
    set_layer(
        &mut document,
        "L0",
        json!({"matteMode": "alpha", "matteSourceLayerUid": "L1",
               "mattesMore": [{"uid": "L2", "mode": "luma"}]}),
    );
    set_layer(&mut document, "L2", json!({"effectsFrom": "E0"}));
    push_layer(
        &mut document,
        "E0",
        3,
        json!({"isEffectLayer": true,
               "effects": [{"type": "blur", "enabled": true, "p1": 12}]}),
    );
    let snap = snapshot(&document, "join-layered");
    let pk = packet(&snap, FRAME);
    let scene = render_scene(&pk);
    let joined = pinned(&pk, vec![contour_claims(&scene, "contour", &[])]);
    let plan = plan_scene_effects(&pk, &joined, &options()).unwrap();

    let uids: Vec<&str> = plan.layers().iter().map(|l| l.layer_uid.as_str()).collect();
    assert_eq!(uids, ["L0", "L1", "L2", "E0"]);
    let c = plan.composition();
    assert_eq!(c[0].matte_mode.as_deref(), Some("alpha"));
    assert_eq!(c[0].matte_source_index, Some(1));
    assert_eq!(
        c[0].mattes_more,
        Some(vec![MatteExtra {
            mode: Some("luma".to_owned()),
            source_index: Some(2)
        }])
    );
    assert_eq!(c[1].blend_mode.as_deref(), Some("multiply"));
    assert!(c[2].layer_effect_enabled);
    assert_eq!(c[3].is_effect_layer, Some(true));
    assert_eq!(plan.layers()[2].effects.len(), 1);
    assert_eq!(plan.layers()[2].effects[0].effect_type, "blur");
    assert_eq!(plan.layers()[2].effects[0].params[0], Some(12.0));
    assert_eq!(plan.layers()[3].role, SlotRole::Effect);
    assert_eq!(
        plan.layers()[3].stack.as_ref().unwrap().landing,
        Landing::Final
    );
    assert_eq!(
        plan.layers()[2].stack.as_ref().unwrap().landing,
        Landing::StackA
    );
    let CompositionPlan::Layered { background, steps } = plan.plan() else {
        panic!("blend, matte and effects need the layered path");
    };
    assert_eq!(*background, None);
    assert_eq!(steps.len(), 4);
    assert!(matches!(
        steps[1],
        LayerStep::Skipped {
            index: 1,
            reason: SkipReason::MatteSource
        }
    ));
    assert!(matches!(
        steps[2],
        LayerStep::Skipped {
            index: 2,
            reason: SkipReason::MatteSource
        }
    ));
    assert!(matches!(
        steps[3],
        LayerStep::EffectLayer {
            index: 3,
            applies_stack: true
        }
    ));
}

#[test]
fn a_hidden_layer_is_an_empty_slot_that_keeps_its_wire_index() {
    let mut document = base_document();
    set_layer(
        &mut document,
        "L1",
        json!({"visible": false, "blendMode": "multiply"}),
    );
    let snap = snapshot(&document, "join-hidden");
    let pk = packet(&snap, FRAME);
    let scene = render_scene(&pk);
    let joined = pinned(&pk, vec![contour_claims(&scene, "contour", &[])]);
    let plan = plan_scene_effects(&pk, &joined, &options()).unwrap();
    let roles: Vec<SlotRole> = plan.layers().iter().map(|l| l.role).collect();
    assert_eq!(roles, [SlotRole::Plain, SlotRole::Empty, SlotRole::Plain]);
    assert_eq!(plan.composition()[1], Default::default());
    assert_eq!(
        plan.plan(),
        &CompositionPlan::Flat {
            order: vec![0, 1, 2]
        }
    );
    let counts: Vec<usize> = plan.layers().iter().map(|l| l.entries.len()).collect();
    assert_eq!(counts, [3, 0, 1]);
}

#[test]
fn unclaimed_and_reported_entries_stay_typed_on_their_layer_and_other_producers_are_listed() {
    let snap = snapshot(&base_document(), "join-unavailable");
    let pk = packet(&snap, FRAME);
    let scene = render_scene(&pk);
    let records: Vec<(usize, usize)> = pk
        .records()
        .map(|r| (r.source_layer_index(), r.source_object_index()))
        .collect();
    // Record 0 is left to no producer; record 1 is claimed by a text producer that reports it unavailable;
    // record 2 is drawn by a component producer.
    let skip = [records[0].1, records[1].1, records[2].1];
    let pin = ScenePin::of_packet(&pk);
    let text = ProducerOutput::<usize>::new("text", pin.clone())
        .with_unavailable(records[1].1, "glyph outlines unavailable");
    let component = ProducerOutput::<usize>::new("component", pin).with_item(records[2].1, 0);
    let joined = pinned(
        &pk,
        vec![contour_claims(&scene, "contour", &skip), text, component],
    );
    let plan = plan_scene_effects(&pk, &joined, &options()).unwrap();
    assert!(!plan.is_complete());
    let mut unavailable: Vec<(usize, usize, EntryOutcome)> = Vec::new();
    for layer in plan.layers() {
        for entry in &layer.entries {
            if matches!(entry.outcome, EntryOutcome::Unavailable(_)) {
                unavailable.push((
                    layer.source_layer_index,
                    entry.source_object_index,
                    entry.outcome.clone(),
                ));
            }
        }
    }
    assert_eq!(unavailable.len(), 2);
    let find = |object: usize| unavailable.iter().find(|u| u.1 == object).cloned();
    assert_eq!(
        find(records[0].1).unwrap().2,
        EntryOutcome::Unavailable(UnavailableCause::Unclaimed)
    );
    assert_eq!(
        find(records[1].1).unwrap().2,
        EntryOutcome::Unavailable(UnavailableCause::Reported {
            producer: "text".to_owned(),
            reason: "glyph outlines unavailable".to_owned()
        })
    );
    let first = plan.first_unavailable().unwrap();
    assert!(unavailable.iter().any(|u| (u.0, u.1) == first));
    assert_eq!(plan.producers(), ["component", "contour"]);
    let total: usize = plan.layers().iter().map(|l| l.entries.len()).sum();
    assert_eq!(total, 5, "nothing is dropped");
}

#[test]
fn a_scene_joined_onto_another_pin_is_refused_before_anything_is_read() {
    let document = base_document();
    let snap_a = snapshot(&document, "join-pin-a");
    let snap_b = snapshot(&document, "join-pin-b");
    let pk_a = packet(&snap_a, FRAME);
    let joined = pinned(
        &pk_a,
        vec![contour_claims(&render_scene(&pk_a), "contour", &[])],
    );
    let pk_b = packet(&snap_b, FRAME);
    assert_eq!(
        plan_scene_effects(&pk_b, &joined, &options()).unwrap_err(),
        SceneEffectJoinError::WrongPin(PinField::Instance)
    );
    let pk_later = packet(&snap_a, FRAME + 1);
    assert_eq!(
        plan_scene_effects(&pk_later, &joined, &options()).unwrap_err(),
        SceneEffectJoinError::WrongPin(PinField::Frame)
    );
}

#[test]
fn viewport_and_export_doors_read_one_join_for_one_snapshot_and_frame() {
    let mut document = base_document();
    set_layer(&mut document, "L1", json!({"blendMode": "screen"}));
    let snap = snapshot(&document, "join-parity");
    let build = || {
        let pk = packet(&snap, FRAME);
        let scene = render_scene(&pk);
        let joined = pinned(&pk, vec![contour_claims(&scene, "contour", &[])]);
        plan_scene_effects(&pk, &joined, &options()).unwrap()
    };
    let (viewport, export) = (build(), build());
    assert_eq!(viewport, export);
    assert!(matches!(viewport.plan(), CompositionPlan::Layered { .. }));
    assert_eq!(viewport.pin(), export.pin());
}

fn resource_set(entries: &[(&str, &str)]) -> Arc<ObjectSceneResourceSet> {
    let entries = entries
        .iter()
        .map(|(id, version)| {
            SceneResourceEntry::new(*id, *version, SceneResourceKind::Raster, 4096, Arc::new(()))
        })
        .collect();
    Arc::new(ObjectSceneResourceSet::new(entries).unwrap())
}

#[test]
fn a_resource_set_binds_to_the_witness_only_when_it_equals_the_scene_declaration() {
    let snap = snapshot(&base_document(), "join-resources");
    let pk = packet(&snap, FRAME);
    let scene = render_scene(&pk);
    let declared = contour_claims(&scene, "contour", &[]).with_resource("img-1", "v1");
    let joined = pinned(&pk, vec![declared]);
    let plan = plan_scene_effects(&pk, &joined, &options()).unwrap();
    assert_eq!(plan.declared_resource_versions().len(), 1);
    let witness = prepare_object_composition(&paint_plan(&scene), &selector(&snap, FRAME)).unwrap();

    let set = resource_set(&[("img-1", "v1")]);
    let bound = bind_scene_resources(witness.clone(), &plan, set.clone()).unwrap();
    assert!(Arc::ptr_eq(bound.scene_resources().unwrap(), &set));
    assert!(
        witness.scene_resources().is_none(),
        "the original witness is untouched"
    );

    let refusal = |entries: &[(&str, &str)]| match bind_scene_resources(
        witness.clone(),
        &plan,
        resource_set(entries),
    ) {
        Err(BindResourcesError::Resources(mismatch)) => (mismatch.kind, mismatch.resource_id),
        other => panic!("expected a resource mismatch, got {other:?}"),
    };
    assert_eq!(
        refusal(&[]),
        (SceneResourceMismatchKind::Missing, "img-1".to_owned())
    );
    assert_eq!(
        refusal(&[("img-1", "v2")]),
        (SceneResourceMismatchKind::VersionSkew, "img-1".to_owned())
    );
    assert_eq!(
        refusal(&[("img-1", "v1"), ("img-2", "v1")]),
        (SceneResourceMismatchKind::Extra, "img-2".to_owned())
    );
}

#[test]
fn a_witness_from_another_snapshot_cannot_take_the_resource_set() {
    let document = base_document();
    let snap_a = snapshot(&document, "join-witness-a");
    let snap_b = snapshot(&document, "join-witness-b");
    let pk_a = packet(&snap_a, FRAME);
    let scene_a = render_scene(&pk_a);
    let joined = pinned(&pk_a, vec![contour_claims(&scene_a, "contour", &[])]);
    let plan = plan_scene_effects(&pk_a, &joined, &options()).unwrap();
    let scene_b = render_scene(&packet(&snap_b, FRAME));
    let foreign =
        prepare_object_composition(&paint_plan(&scene_b), &selector(&snap_b, FRAME)).unwrap();
    assert_eq!(
        bind_scene_resources(foreign, &plan, resource_set(&[])).unwrap_err(),
        BindResourcesError::WrongWitness
    );
}

#[test]
fn a_witness_carries_only_a_plan_joined_from_its_own_packet_and_the_plan_is_part_of_its_identity() {
    let mut document = base_document();
    set_layer(&mut document, "L1", json!({"blendMode": "multiply"}));
    let build = |instance: &str| {
        let snap = snapshot(&document, instance);
        let pk = packet(&snap, FRAME);
        let scene = render_scene(&pk);
        let joined = pinned(&pk, vec![contour_claims(&scene, "contour", &[])]);
        let plan = Arc::new(plan_scene_effects(&pk, &joined, &options()).unwrap());
        let witness =
            prepare_object_composition(&paint_plan(&scene), &selector(&snap, FRAME)).unwrap();
        (witness, plan)
    };
    let (witness, plan) = build("join-identity");
    assert!(witness.scene_effects().is_none());
    let attached = witness.clone().with_scene_effects(plan.clone()).unwrap();
    assert!(Arc::ptr_eq(attached.scene_effects().unwrap(), &plan));
    assert!(
        !attached.same_identity_as(&witness),
        "the plan is part of the identity"
    );
    // Identity is compared by value, so cloning keeps it and a second independent join attaches the same way.
    let (other_witness, other_plan) = build("join-identity");
    let other = other_witness.with_scene_effects(other_plan).unwrap();
    assert!(attached.same_identity_as(&attached.clone()));
    assert_eq!(other.scene_effects().unwrap().pin().frame, FRAME);
    // A plan from another instance cannot be attached.
    let (_, foreign) = build("join-identity-foreign");
    assert_eq!(
        witness.with_scene_effects(foreign).unwrap_err(),
        ObjectSceneError::WrongSnapshot
    );
}
