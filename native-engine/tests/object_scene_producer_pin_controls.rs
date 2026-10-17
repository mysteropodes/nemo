//! Negative and mutation controls for the pinned multi-producer scene join (L07-C).
//!
//! Every refusal is exercised on a real packet and real producer outputs built by
//! `super::rows`. The mutation controls re-implement three WRONG joins (producer
//! encounter order, ascending source-object index, natural order that ignores Order)
//! over the same producer outputs and require each to disagree with the baseline
//! oracle on the exact row counts the generator recorded in the oracle header
//! (`mutantMismatches`: natural 232, encounter 258, sourceIndex 258 of 300), so the
//! oracle demonstrably discriminates what the real join gets right.

use super::rows::{
    case_of, cases, divergences, document, label_of, outputs, packet, snapshot, Label,
};
use super::{
    join_scene, PinField, ProducerEntry, ProducerOutput, SceneJoinError, SceneOutcome, ScenePin,
    UnavailableCause,
};
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::ObjectFramePacket;
use crate::revision::ObjectSnapshot;

const FIELDS: [PinField; 7] = [
    PinField::Instance,
    PinField::Document,
    PinField::Revision,
    PinField::Snapshot,
    PinField::Context,
    PinField::Scope,
    PinField::Frame,
];

fn corrupt(pin: &mut ScenePin, field: PinField) {
    match field {
        PinField::Instance => pin.instance_id.push_str("-other"),
        PinField::Document => pin.document_id.push_str("-other"),
        PinField::Revision => pin.content_revision += 1,
        PinField::Snapshot => pin.document_snapshot_id.push_str("-other"),
        PinField::Context => pin.context_id = "other-context".to_owned(),
        PinField::Scope => pin.scope_kind = FrameScopeKind::Reference,
        PinField::Frame => pin.frame += 1,
    }
}

/// Mixed scene: ranks and element Order values, four records, three layers.
fn fixture(hidden: &[usize]) -> (ObjectSnapshot, ObjectFramePacket) {
    let case = case_of(vec![
        (None, vec![None, Some(2.0)]),
        (Some(1.0), vec![None]),
        (None, vec![None]),
    ]);
    let snap = snapshot(&document(&case, hidden), "scene-pin");
    let pk = packet(&snap);
    (snap, pk)
}

fn first_item(out: &ProducerOutput<Label>) -> (usize, Label) {
    match &out.entries[0] {
        ProducerEntry::Item {
            source_object_index,
            payload,
        } => (*source_object_index, payload.clone()),
        ProducerEntry::Unavailable { .. } => panic!("first entry is an item"),
    }
}

#[test]
fn a_single_mismatching_pin_field_in_any_producer_refuses_the_whole_join_with_that_field() {
    let (_snap, pk) = fixture(&[]);
    let pin = ScenePin::of_packet(&pk);
    assert!(
        join_scene(&pk, &pin, outputs(&pk)).is_ok(),
        "control: unmodified join passes"
    );
    for field in FIELDS {
        for index in 0..3 {
            let mut outs = outputs(&pk);
            let name = outs[index].producer.clone();
            corrupt(&mut outs[index].pin, field);
            assert_eq!(
                join_scene(&pk, &pin, outs).unwrap_err(),
                SceneJoinError::WrongPin {
                    producer: Some(name),
                    field
                },
                "{field:?} in output {index}"
            );
        }
    }
}

#[test]
fn a_packet_that_disagrees_with_the_callers_expected_pin_is_refused_per_field() {
    let (_snap, pk) = fixture(&[]);
    for field in FIELDS {
        let mut expected = ScenePin::of_packet(&pk);
        corrupt(&mut expected, field);
        assert_eq!(
            expected.first_mismatch(&ScenePin::of_packet(&pk)),
            Some(field)
        );
        assert_eq!(
            join_scene(&pk, &expected, outputs(&pk)).unwrap_err(),
            SceneJoinError::WrongPin {
                producer: None,
                field
            },
            "{field:?}"
        );
    }
}

#[test]
fn the_first_mismatching_field_is_reported_in_the_fixed_order() {
    let (_snap, pk) = fixture(&[]);
    let pin = ScenePin::of_packet(&pk);
    let mut outs = outputs(&pk);
    corrupt(&mut outs[0].pin, PinField::Frame);
    corrupt(&mut outs[0].pin, PinField::Snapshot);
    corrupt(&mut outs[0].pin, PinField::Instance);
    let err = join_scene(&pk, &pin, outs).unwrap_err();
    assert!(matches!(
        err,
        SceneJoinError::WrongPin {
            field: PinField::Instance,
            ..
        }
    ));
}

#[test]
fn outputs_from_another_real_snapshot_are_refused_before_any_ordering() {
    let (_snap, pk) = fixture(&[]);
    let case = case_of(vec![
        (None, vec![None, Some(2.0)]),
        (Some(1.0), vec![None]),
        (None, vec![None]),
    ]);
    let other_snap = snapshot(&document(&case, &[]), "another-instance");
    let foreign = outputs(&packet(&other_snap));
    // Same shape and same record indices: only the pin tells them apart.
    assert_eq!(foreign.len(), 3);
    let err = join_scene(&pk, &ScenePin::of_packet(&pk), foreign).unwrap_err();
    assert_eq!(
        err,
        SceneJoinError::WrongPin {
            producer: Some("text".to_owned()),
            field: PinField::Instance
        }
    );
    // A second admission of the same JSON under the same instance id is a new
    // document incarnation (document_id comes from a sequence): refused as Document.
    let again = snapshot(&document(&case, &[]), "scene-pin");
    let err = join_scene(&pk, &ScenePin::of_packet(&pk), outputs(&packet(&again))).unwrap_err();
    assert_eq!(
        err,
        SceneJoinError::WrongPin {
            producer: Some("text".to_owned()),
            field: PinField::Document
        }
    );
}

#[test]
fn a_record_claimed_twice_refuses_whether_by_one_producer_or_two() {
    let (_snap, pk) = fixture(&[]);
    let pin = ScenePin::of_packet(&pk);
    let (index, label) = first_item(&outputs(&pk)[0]);
    let duplicate = SceneJoinError::DuplicateClaim {
        source_object_index: index,
    };
    // same producer twice
    let mut outs = outputs(&pk);
    outs[0].entries.push(ProducerEntry::Item {
        source_object_index: index,
        payload: label.clone(),
    });
    assert_eq!(join_scene(&pk, &pin, outs).unwrap_err(), duplicate);
    // two producers
    let mut outs = outputs(&pk);
    outs[1].entries.push(ProducerEntry::Item {
        source_object_index: index,
        payload: label,
    });
    assert_eq!(join_scene(&pk, &pin, outs).unwrap_err(), duplicate);
    // an unavailable report is a claim as well
    let mut outs = outputs(&pk);
    outs[2].entries.push(ProducerEntry::Unavailable {
        source_object_index: index,
        reason: "late".to_owned(),
    });
    assert_eq!(join_scene(&pk, &pin, outs).unwrap_err(), duplicate);
}

#[test]
fn indices_outside_the_packet_or_in_an_invisible_layer_are_refused_atomically() {
    let (_snap, pk) = fixture(&[]);
    let pin = ScenePin::of_packet(&pk);
    let mut outs = outputs(&pk);
    outs[2].entries.push(ProducerEntry::Unavailable {
        source_object_index: 9999,
        reason: "x".to_owned(),
    });
    assert_eq!(
        join_scene(&pk, &pin, outs).unwrap_err(),
        SceneJoinError::UnknownSource {
            producer: "contour".to_owned(),
            source_object_index: 9999
        }
    );
    // Layer L1 hidden: its record is outside the selected set.
    let (snap, pk) = fixture(&[1]);
    let pin = ScenePin::of_packet(&pk);
    let hidden_index = snap
        .document()
        .objects()
        .iter()
        .position(|r| r.target().layer_uid() == "L1")
        .unwrap();
    assert!(
        join_scene(&pk, &pin, outputs(&pk)).is_ok(),
        "control: without the stray claim"
    );
    let mut outs = outputs(&pk);
    outs[0].entries.push(ProducerEntry::Item {
        source_object_index: hidden_index,
        payload: ("L1".to_owned(), "a".to_owned(), 0),
    });
    assert_eq!(
        join_scene(&pk, &pin, outs).unwrap_err(),
        SceneJoinError::UnknownSource {
            producer: "text".to_owned(),
            source_object_index: hidden_index
        }
    );
}

#[test]
fn a_dropped_item_never_disappears_in_any_oracle_row() {
    for case in cases() {
        let snap = snapshot(&document(&case, &[]), "scene-pin");
        let pk = packet(&snap);
        let mut outs = outputs(&pk);
        let victim = outs.iter_mut().find(|o| !o.entries.is_empty()).unwrap();
        let dropped = match victim.entries.remove(0) {
            ProducerEntry::Item {
                source_object_index,
                ..
            } => source_object_index,
            ProducerEntry::Unavailable { .. } => unreachable!(),
        };
        let scene = join_scene(&pk, &ScenePin::of_packet(&pk), outs).unwrap();
        assert_eq!(scene.entries().len(), pk.records().len(), "{}", case.id);
        let missing: Vec<_> = scene.unavailable().collect();
        assert_eq!(missing.len(), 1, "{}", case.id);
        assert_eq!(missing[0].source_object_index(), dropped, "{}", case.id);
        assert_eq!(
            missing[0].outcome(),
            &SceneOutcome::Unavailable(UnavailableCause::Unclaimed),
            "{}",
            case.id
        );
        assert!(!scene.is_complete());
    }
}

#[test]
fn encounter_order_of_outputs_and_entries_never_changes_the_joined_scene() {
    for case in cases() {
        let snap = snapshot(&document(&case, &[]), "scene-pin");
        let pk = packet(&snap);
        let pin = ScenePin::of_packet(&pk);
        let reference = join_scene(&pk, &pin, outputs(&pk)).unwrap();
        let mut reversed = outputs(&pk);
        reversed.reverse();
        let mut rotated = outputs(&pk);
        rotated.rotate_left(1);
        let mut entries_reversed = outputs(&pk);
        for out in entries_reversed.iter_mut() {
            out.entries.reverse();
        }
        for variant in [reversed, rotated, entries_reversed] {
            assert_eq!(
                join_scene(&pk, &pin, variant).unwrap(),
                reference,
                "{}",
                case.id
            );
        }
    }
}

fn encounter(outs: &[ProducerOutput<Label>]) -> Vec<(usize, Label)> {
    outs.iter()
        .flat_map(|out| out.entries.iter())
        .filter_map(|entry| match entry {
            ProducerEntry::Item {
                source_object_index,
                payload,
            } => Some((*source_object_index, payload.clone())),
            ProducerEntry::Unavailable { .. } => None,
        })
        .collect()
}

fn render(items: Vec<(usize, Label)>) -> Vec<String> {
    items
        .into_iter()
        .map(|(_, label)| label_of(&label.0, &label.1))
        .collect()
}

fn mismatches(mutant: impl Fn(Vec<(usize, Label)>) -> Vec<(usize, Label)>) -> usize {
    cases()
        .iter()
        .filter(|case| {
            let snap = snapshot(&document(case, &[]), "scene-pin");
            let pk = packet(&snap);
            render(mutant(encounter(&outputs(&pk)))) != case.expected
        })
        .count()
}

#[test]
fn three_wrong_joins_disagree_with_the_baseline_oracle_on_the_recorded_row_counts() {
    // Producer encounter order (outputs reversed, entries descending).
    assert_eq!(mismatches(|items| items), 258);
    // Ascending source-object index (objects are stored reversed).
    assert_eq!(
        mismatches(|mut items| {
            items.sort_by_key(|item| item.0);
            items
        }),
        258
    );
    // Natural order, ignoring every Order value: (layer, element) ascending.
    assert_eq!(
        mismatches(|mut items| {
            items.sort_by_key(|(_, label)| {
                (label.0[1..].parse::<usize>().unwrap(), label.1.clone())
            });
            items
        }),
        232
    );
    // The real join agrees on every row (control for the three counts above).
    assert_eq!(divergences(&cases()), Vec::<String>::new());
}

#[test]
fn a_corrupted_oracle_expectation_is_detected_exactly_once() {
    let mut few: Vec<_> = cases().into_iter().take(20).collect();
    assert_eq!(few[15].id, "layer-and-elements");
    assert_eq!(divergences(&few), Vec::<String>::new());
    few[15].expected.swap(0, 1);
    assert_eq!(divergences(&few), vec!["layer-and-elements".to_owned()]);
}

#[test]
fn resources_merge_identical_pairs_and_refuse_conflicts_and_empties() {
    let (_snap, pk) = fixture(&[]);
    let pin = ScenePin::of_packet(&pk);
    // Identical (id, version) pairs from three producers collapse to one entry.
    let scene = join_scene(&pk, &pin, outputs(&pk)).unwrap();
    assert_eq!(scene.declared_resource_versions().len(), 4);
    assert_eq!(
        scene
            .declared_resource_versions()
            .get("shared")
            .map(String::as_str),
        Some("7")
    );
    // Same id, different version inside one producer.
    let mut outs = outputs(&pk);
    outs[0]
        .resources
        .push(("shared".to_owned(), "8".to_owned()));
    assert_eq!(
        join_scene(&pk, &pin, outs).unwrap_err(),
        SceneJoinError::ResourceConflict {
            resource_id: "shared".to_owned()
        }
    );
    // Same id, different version across producers.
    let mut outs = outputs(&pk);
    outs[2]
        .resources
        .push(("res:component".to_owned(), "2".to_owned()));
    assert_eq!(
        join_scene(&pk, &pin, outs).unwrap_err(),
        SceneJoinError::ResourceConflict {
            resource_id: "res:component".to_owned()
        }
    );
    // Empty id and empty version.
    for pair in [("", "v"), ("id", "")] {
        let mut outs = outputs(&pk);
        outs[0]
            .resources
            .push((pair.0.to_owned(), pair.1.to_owned()));
        assert_eq!(
            join_scene(&pk, &pin, outs).unwrap_err(),
            SceneJoinError::InvalidResource {
                producer: "text".to_owned()
            },
            "{pair:?}"
        );
    }
}

#[test]
fn producer_names_must_be_present_and_unique() {
    let (_snap, pk) = fixture(&[]);
    let pin = ScenePin::of_packet(&pk);
    let mut outs = outputs(&pk);
    outs[1].producer = String::new();
    assert_eq!(
        join_scene(&pk, &pin, outs).unwrap_err(),
        SceneJoinError::EmptyProducerName
    );
    let mut outs = outputs(&pk);
    outs[1].producer = outs[0].producer.clone();
    assert_eq!(
        join_scene(&pk, &pin, outs).unwrap_err(),
        SceneJoinError::DuplicateProducer {
            producer: "text".to_owned()
        }
    );
}
