//! The text scene reported through the pinned scene registry (L07-C) against a
//! real `ObjectFramePacket`: the join keeps the pin, the claim and the typed
//! unavailable entry, and a snapshot is byte-identical before and after.

use super::oracle::read;
use super::tests::run;
use super::{scene_refusal, text_producer_output, GeometryAdmission, TextSceneError, PRODUCER};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, PinField, ProducerEntry, SceneJoinError, SceneOutcome, ScenePin, UnavailableCause,
};
use crate::object_codec::{decode_storage_project, encode_storage_project};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{prepare_object_frame, ObjectFramePacket, ObjectFrameSelector};
use crate::object_render_scene::{paint_test_document, ObjectSceneError};
use crate::revision::ObjectSnapshot;

const FRAME: u32 = 7;

fn fixture() -> (ObjectSnapshot, ObjectFramePacket) {
    let value = paint_test_document(&[FRAME]);
    let snapshot = ObjectSnapshot::new(
        "text-pin",
        decode_storage_project(&serde_json::to_vec(&value).unwrap()).unwrap(),
    )
    .unwrap();
    let packet = prepare_object_frame(
        &snapshot,
        &ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame: FRAME,
        },
    )
    .unwrap();
    (snapshot, packet)
}

fn text_scene() -> super::TextGlyphScene {
    let oracle = read();
    let case = oracle
        .cases
        .iter()
        .find(|c| c.name == "translate_layer")
        .unwrap();
    run(case, &oracle.fonts, GeometryAdmission::DropCoincidentCurves)
        .0
        .expect("the oracle text is admitted")
}

#[test]
fn a_text_item_is_one_claim_on_a_real_packet_and_unclaimed_records_stay_unavailable() {
    let (_snapshot, packet) = fixture();
    let pin = ScenePin::of_packet(&packet);
    let index = packet.records().next().unwrap().source_object_index();
    let scene = text_scene();
    assert_eq!(scene.contours().len(), 11);
    let output = text_producer_output(pin.clone(), index, Ok(scene));
    assert_eq!(output.producer, PRODUCER);
    assert_eq!(output.pin, pin);
    assert!(output.resources.is_empty());
    assert!(matches!(
        output.entries.as_slice(),
        [ProducerEntry::Item { source_object_index, payload }]
            if *source_object_index == index && payload.contours().len() == 11
    ));
    let joined = join_scene(&packet, &pin, vec![output]).expect("pin and claim agree");
    assert_eq!(joined.entries().len(), packet.records().len());
    let mut items = 0;
    for entry in joined.entries() {
        match entry.outcome() {
            SceneOutcome::Item { producer, .. } => {
                items += 1;
                assert_eq!(producer, PRODUCER);
                assert_eq!(entry.source_object_index(), index);
            }
            SceneOutcome::Unavailable(UnavailableCause::Unclaimed) => {}
            _ => panic!("every other record is unclaimed"),
        }
    }
    assert_eq!(items, 1);
    assert!(!joined.is_complete(), "four records no producer claimed");
}

#[test]
fn a_refused_block_is_a_typed_unavailable_entry_and_the_frame_refusal_names_it() {
    let (_snapshot, packet) = fixture();
    let pin = ScenePin::of_packet(&packet);
    let index = packet.records().next().unwrap().source_object_index();
    let error = TextSceneError::FontUnavailable;
    let output = text_producer_output(pin.clone(), index, Err(error.clone()));
    let joined = join_scene(&packet, &pin, vec![output]).unwrap();
    let entry = joined
        .entries()
        .iter()
        .find(|e| e.source_object_index() == index)
        .unwrap();
    assert!(matches!(
        entry.outcome(),
        SceneOutcome::Unavailable(UnavailableCause::Reported { producer, reason })
            if producer == PRODUCER && reason == "no glyph outline provider is installed"
    ));
    assert_eq!(
        scene_refusal(&error, index),
        ObjectSceneError::UnsupportedGeometry {
            source_object_index: index,
            reason: "no glyph outline provider is installed",
        }
    );
}

#[test]
fn an_output_for_another_frame_is_refused_by_the_join() {
    let (_snapshot, packet) = fixture();
    let pin = ScenePin::of_packet(&packet);
    let index = packet.records().next().unwrap().source_object_index();
    let mut stale = pin.clone();
    stale.frame += 1;
    let output = text_producer_output(stale, index, Ok(text_scene()));
    assert_eq!(
        join_scene(&packet, &pin, vec![output]).unwrap_err(),
        SceneJoinError::WrongPin {
            producer: Some(PRODUCER.to_owned()),
            field: PinField::Frame,
        }
    );
}

#[test]
fn preparing_a_text_scene_leaves_the_pinned_snapshot_byte_identical() {
    let (snapshot, packet) = fixture();
    let before = encode_storage_project(snapshot.document()).unwrap();
    let pin = ScenePin::of_packet(&packet);
    let index = packet.records().next().unwrap().source_object_index();
    let output = text_producer_output(pin.clone(), index, Ok(text_scene()));
    let _ = join_scene(&packet, &pin, vec![output]).unwrap();
    assert_eq!(encode_storage_project(snapshot.document()).unwrap(), before);
    assert_eq!(ScenePin::of_packet(&packet), pin);
}
