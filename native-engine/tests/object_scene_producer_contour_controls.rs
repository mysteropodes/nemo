//! Negative and mutation controls for the contour producer join (L07-F).
//!
//! Each control makes one thing wrong and requires a named failure: a mutated pin field,
//! a foreign or stale snapshot, a dropped or unavailable command, wrong scene orders
//! (the oracle's natural / compact / storage mutants), an out-of-range paint number and
//! two text drift guards for the consumer wiring that the CPU-only harness cannot execute.

use super::rows::{
    cases, divergences, document, packet, paint_scene, reference_scene, selector, snapshot, Case,
};
use super::{command_sequence, sequence_of, PRODUCER};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    ProducerOutput, ScenePin,
};
use crate::evaluation::object_scene_style_kernels::object_scene_style_paint::{
    ScenePaintError, SceneSolid,
};
use crate::object_codec::decode_storage_project;
use crate::object_document::{FrameScopeKind, SolidFill};
use crate::object_frame_packet::ObjectFramePacket;
use crate::object_render_scene::{
    paint_test_document, prepare_object_paint_plan, ObjectPaintPlanError, ObjectSceneError,
};

const COMPOSITOR: &str = include_str!("../src/object_compositor.rs");
const PRODUCER_SOURCE: &str = include_str!("../src/object_scene_producer_contour.rs");

/// One item per selected record of the packet, payload = source object index.
fn full_output(packet: &ObjectFramePacket, pin: &ScenePin) -> ProducerOutput<usize> {
    let mut output = ProducerOutput::new(PRODUCER, pin.clone());
    for record in packet.records() {
        output = output.with_item(record.source_object_index(), record.source_object_index());
    }
    output
}

/// The pin with exactly one field changed, with the scene error that field maps to.
fn mutants(good: &ScenePin) -> Vec<(ObjectSceneError, ScenePin)> {
    vec![
        (
            ObjectSceneError::WrongInstance,
            ScenePin {
                instance_id: "foreign".to_owned(),
                ..good.clone()
            },
        ),
        (
            ObjectSceneError::WrongDocument,
            ScenePin {
                document_id: "foreign".to_owned(),
                ..good.clone()
            },
        ),
        (
            ObjectSceneError::WrongRevision,
            ScenePin {
                content_revision: good.content_revision + 1,
                ..good.clone()
            },
        ),
        (
            ObjectSceneError::WrongSnapshot,
            ScenePin {
                document_snapshot_id: "stale".to_owned(),
                ..good.clone()
            },
        ),
        (
            ObjectSceneError::InvalidContext,
            ScenePin {
                context_id: "component-1".to_owned(),
                ..good.clone()
            },
        ),
        (
            ObjectSceneError::WrongScope,
            ScenePin {
                scope_kind: FrameScopeKind::Reference,
                ..good.clone()
            },
        ),
        (
            ObjectSceneError::WrongFrame,
            ScenePin {
                frame: good.frame + 1,
                ..good.clone()
            },
        ),
    ]
}

#[test]
fn every_field_of_the_expected_pin_is_checked_and_maps_to_its_scene_error() {
    let snap = snapshot(&paint_test_document(&[7]), "contour-pins");
    let pk = packet(&snap);
    let good = ScenePin::of_packet(&pk);
    assert!(sequence_of(&pk, &good, full_output(&pk, &good)).is_ok());
    for (error, expected) in mutants(&good) {
        assert_eq!(
            sequence_of(&pk, &expected, full_output(&pk, &good)).unwrap_err(),
            ObjectPaintPlanError::Pins(error),
            "{error:?}"
        );
    }
}

#[test]
fn a_producer_output_pinned_to_another_snapshot_refuses_the_whole_join() {
    let snap = snapshot(&paint_test_document(&[7]), "contour-pins");
    let pk = packet(&snap);
    let good = ScenePin::of_packet(&pk);
    for (error, foreign) in mutants(&good) {
        assert_eq!(
            sequence_of(&pk, &good, full_output(&pk, &foreign)).unwrap_err(),
            ObjectPaintPlanError::Pins(error),
            "{error:?}"
        );
    }
}

#[test]
fn a_foreign_or_stale_snapshot_pin_is_refused_by_the_real_paint_plan() {
    let value = paint_test_document(&[7]);
    let scene = paint_scene(&value, 7);
    let other = snapshot(&value, "another-instance");
    let other_packet = packet(&other);
    assert_eq!(
        prepare_object_paint_plan(&scene, &selector(&other_packet)).unwrap_err(),
        ObjectPaintPlanError::Pins(ObjectSceneError::WrongInstance)
    );
    let mut stale = selector(scene.packet());
    stale.document_snapshot_id = "stale";
    assert_eq!(
        prepare_object_paint_plan(&scene, &stale).unwrap_err(),
        ObjectPaintPlanError::Pins(ObjectSceneError::WrongSnapshot)
    );
    assert!(prepare_object_paint_plan(&scene, &selector(scene.packet())).is_ok());
}

#[test]
fn a_packet_without_a_numeric_order_plan_is_unavailable_not_ordered() {
    let reference = reference_scene();
    assert!(reference.packet().numeric_order().is_none());
    assert_eq!(
        command_sequence(&reference, &selector(reference.packet())).unwrap_err(),
        ObjectPaintPlanError::Unavailable
    );
}

#[test]
fn a_dropped_command_is_unavailable_in_every_oracle_row_and_never_a_shorter_plan() {
    let mut dropped_total = 0;
    for case in cases() {
        let snap = snapshot(&document(&case), "contour-drop");
        let pk = packet(&snap);
        let pin = ScenePin::of_packet(&pk);
        let indices: Vec<usize> = pk.records().map(|r| r.source_object_index()).collect();
        assert!(sequence_of(&pk, &pin, full_output(&pk, &pin)).is_ok());
        for dropped in &indices {
            let mut output = ProducerOutput::new(PRODUCER, pin.clone());
            for index in indices.iter().filter(|index| *index != dropped) {
                output = output.with_item(*index, *index);
            }
            assert_eq!(
                sequence_of(&pk, &pin, output),
                Err(ObjectPaintPlanError::Unavailable),
                "{} dropped {dropped}",
                case.id
            );
            dropped_total += 1;
        }
    }
    assert_eq!(dropped_total, 482);
}

#[test]
fn a_producer_reported_unavailable_record_is_never_painted_silently() {
    let snap = snapshot(&paint_test_document(&[7]), "contour-reported");
    let pk = packet(&snap);
    let pin = ScenePin::of_packet(&pk);
    let indices: Vec<usize> = pk.records().map(|r| r.source_object_index()).collect();
    for reported in &indices {
        let mut output = ProducerOutput::new(PRODUCER, pin.clone());
        for index in &indices {
            output = if index == reported {
                output.with_unavailable(*index, "geometry unavailable")
            } else {
                output.with_item(*index, *index)
            };
        }
        assert_eq!(
            sequence_of(&pk, &pin, output),
            Err(ObjectPaintPlanError::Unavailable)
        );
    }
}

fn expected(case: &Case) -> &Vec<String> {
    &case.expected
}
fn natural(case: &Case) -> &Vec<String> {
    &case.natural
}
fn compact(case: &Case) -> &Vec<String> {
    &case.compact
}
fn storage(case: &Case) -> &Vec<String> {
    &case.storage
}

#[test]
fn wrong_scene_orders_disagree_with_the_oracle_on_the_recorded_rows() {
    let all = cases();
    // The real plan agrees with the baseline on every row ...
    assert!(divergences(&all, expected).is_empty());
    // ... and each wrong order is caught on exactly the rows the generator counted:
    // Order ignored (natural), hidden layers dropped before ranking (compact), and
    // document array order mistaken for paint order (storage).
    assert_eq!(divergences(&all, natural).len(), 68);
    assert_eq!(divergences(&all, compact).len(), 4);
    assert_eq!(divergences(&all, storage).len(), 87);
}

#[test]
fn out_of_range_paint_numbers_are_refused_by_the_typed_paint_and_by_the_codec() {
    for (r, g, b, a) in [
        ("1.5", "0.5", "0.5", "1"),
        ("0.5", "-0.1", "0.5", "1"),
        ("0.5", "0.5", "1.0000000000000002", "1"),
        ("0.5", "0.5", "0.5", "2"),
    ] {
        let json = format!(r#"{{"kind":"solid","r":{r},"g":{g},"b":{b},"a":{a}}}"#);
        let fill: SolidFill = serde_json::from_str(&json).expect("fill json");
        assert_eq!(
            SceneSolid::from_fill(&fill),
            Err(ScenePaintError::UnrepresentableStraightRgba),
            "{json}"
        );
        // Defense in depth: an admitted document can never carry such a fill.
        let mut value = paint_test_document(&[7]);
        value["objects"][0]["fill"] = serde_json::from_str(&json).unwrap();
        assert!(decode_storage_project(&serde_json::to_vec(&value).unwrap()).is_err());
    }
}

#[test]
fn the_compositor_converts_paint_through_the_typed_scene_solid() {
    // Text drift guard: the GPU path cannot run in this harness, so the wiring is pinned
    // by source. The conversion itself is pinned by `rgba_f32` literals and by the L07-A
    // oracle, so the compositor must not carry a second copy of the channel rule.
    // L07-I moved the conversion into object_scene_style_emit.rs, still through the kernel's rgba_f32.
    let emit = include_str!("../src/object_scene_style_emit.rs");
    assert!(emit.contains(".rgba_f32()"));
    assert!(COMPOSITOR.contains(".style()"));
    assert!(COMPOSITOR.contains("CompositorError::new(error.message())"));
    assert!(!COMPOSITOR.contains("as_f64()"));
    assert!(!COMPOSITOR.contains("(0.0..=1.0).contains"));
}

#[test]
fn the_producer_module_uses_absolute_crate_paths_only() {
    assert!(!PRODUCER_SOURCE.contains("super::"));
    assert!(PRODUCER_SOURCE.contains(
        "use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{"
    ));
}
