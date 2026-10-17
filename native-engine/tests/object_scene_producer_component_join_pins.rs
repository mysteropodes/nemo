//! Pin, claim and merge controls of the component producer join (L07-K): every pin
//! field, forged claims, unavailable slots that must not vanish, and scenes pinned to
//! different frames.

use super::rows::{diff, entry_of, oracle, Prepared, Set};
use super::support::{
    component_mut, instance_mut, layer_mut, packet_of, scene_with_objects, Runtime,
};
use super::NoPorts;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::{
    ComponentJoinError, ComponentLayerEntry, ComponentOutcome, SceneSlot,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::ComponentUnavailable;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, PinField, ProducerOutput, ScenePin,
};
use crate::object_codec::decode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::revision::ObjectSnapshot;
use serde_json::json;

fn foreign_snapshot(set: &Set, name: &str) -> ObjectSnapshot {
    ObjectSnapshot::new(
        name,
        decode_storage_project(&serde_json::to_vec(&set.scene).unwrap()).unwrap(),
    )
    .unwrap()
}

#[test]
fn a_mismatch_in_any_pin_field_refuses_the_whole_join_with_that_field() {
    let set = oracle().remove(0);
    let prepared = Prepared::of(&set);
    let packet = prepared.packet(4);
    let pin = ScenePin::of_packet(&packet);
    let edits: [(&dyn Fn(&mut ScenePin), PinField); 7] = [
        (
            &|p: &mut ScenePin| p.instance_id = "other".to_owned(),
            PinField::Instance,
        ),
        (
            &|p: &mut ScenePin| p.document_id = "other".to_owned(),
            PinField::Document,
        ),
        (
            &|p: &mut ScenePin| p.content_revision += 1,
            PinField::Revision,
        ),
        (
            &|p: &mut ScenePin| p.document_snapshot_id = "other".to_owned(),
            PinField::Snapshot,
        ),
        (
            &|p: &mut ScenePin| p.context_id = "other".to_owned(),
            PinField::Context,
        ),
        (
            &|p: &mut ScenePin| p.scope_kind = FrameScopeKind::Reference,
            PinField::Scope,
        ),
        (&|p: &mut ScenePin| p.frame += 1, PinField::Frame),
    ];
    for (edit, field) in edits {
        let mut production = prepared.production(&packet);
        edit(&mut production.pin);
        assert_eq!(
            production.join(&packet, &pin).unwrap_err(),
            ComponentJoinError::WrongPin {
                producer: Some("component".to_owned()),
                field
            },
            "{field:?}"
        );
        let mut expected = pin.clone();
        edit(&mut expected);
        assert_eq!(
            prepared
                .production(&packet)
                .join(&packet, &expected)
                .unwrap_err(),
            ComponentJoinError::WrongPin {
                producer: None,
                field
            },
            "{field:?}"
        );
    }
    // The first differing field in the fixed order wins.
    let mut production = prepared.production(&packet);
    production.pin.frame += 1;
    production.pin.instance_id = "other".to_owned();
    assert_eq!(
        production.join(&packet, &pin).unwrap_err(),
        ComponentJoinError::WrongPin {
            producer: Some("component".to_owned()),
            field: PinField::Instance
        }
    );
    // A real production of another admitted snapshot is refused as a foreign instance.
    let foreign = foreign_snapshot(&set, "foreign-instance");
    let foreign_packet = packet_of(&foreign, 4);
    let foreign_production = prepared.production(&foreign_packet);
    assert_eq!(
        foreign_production.join(&packet, &pin).unwrap_err(),
        ComponentJoinError::WrongPin {
            producer: Some("component".to_owned()),
            field: PinField::Instance
        }
    );
}

#[test]
fn bad_claims_are_refused_atomically() {
    let set = oracle().remove(0);
    let prepared = Prepared::of(&set);
    let packet = prepared.packet(2);
    let pin = ScenePin::of_packet(&packet);
    let tampered = |edit: &dyn Fn(&mut Vec<ComponentLayerEntry>)| {
        let mut production = prepared.production(&packet);
        edit(&mut production.entries);
        production.join(&packet, &pin).unwrap_err()
    };
    assert_eq!(
        tampered(&|e: &mut Vec<ComponentLayerEntry>| e.push(e[0].clone())),
        ComponentJoinError::DuplicateClaim {
            source_layer_index: 0
        }
    );
    assert_eq!(
        tampered(&|e: &mut Vec<ComponentLayerEntry>| e[0].source_layer_index = 99),
        ComponentJoinError::UnknownLayer {
            source_layer_index: 99
        }
    );
    assert_eq!(
        tampered(&|e: &mut Vec<ComponentLayerEntry>| e[0].layer_uid = "ly_other".to_owned()),
        ComponentJoinError::UnknownLayer {
            source_layer_index: 0
        }
    );
    // A claim on an invisible layer: the producer never makes one, a forged one is refused.
    let mut hidden_scene = set.scene.clone();
    layer_mut(&mut hidden_scene, "ly_comp_once")["visible"] = json!(false);
    let hidden = Prepared::from_json(&set.components, &hidden_scene, set.cameras.clone());
    let hidden_packet = hidden.packet(2);
    let hidden_pin = ScenePin::of_packet(&hidden_packet);
    let mut production = hidden.production(&hidden_packet);
    assert_eq!(production.entries.len(), 6);
    assert!(production
        .entries
        .iter()
        .all(|entry| entry.layer_uid != "ly_comp_once"));
    let joined = production
        .clone()
        .join(&hidden_packet, &hidden_pin)
        .unwrap();
    assert_eq!(joined.slots().len(), 6);
    production.entries.push(ComponentLayerEntry {
        source_layer_index: 1,
        layer_uid: "ly_comp_once".to_owned(),
        symbol_id: "sym_comp".to_owned(),
        outcome: ComponentOutcome::Blank,
    });
    assert_eq!(
        production.join(&hidden_packet, &hidden_pin).unwrap_err(),
        ComponentJoinError::UnknownLayer {
            source_layer_index: 1
        }
    );
}

#[test]
fn an_unavailable_instance_never_disappears_from_the_scene() {
    let set = oracle().remove(0);
    // One stored record whose handles are not relative: instances that show it are unavailable.
    let mut components = set.components.clone();
    component_mut(&mut components, "sym_comp")["objects"][0]["geometry"]["handleSpace"] =
        json!("absolute");
    let prepared = Prepared::from_json(&components, &set.scene, set.cameras.clone());
    let (packet, scene) = prepared.scene(0);
    assert_eq!(scene.slots().len(), 7);
    let unavailable: Vec<&str> = scene
        .unavailable()
        .map(|slot| slot.entry().layer_uid.as_str())
        .collect();
    // Inner frame 0 at outer frame 0: loop, once, ping, speed, blank (single shows 5, hold 3).
    assert_eq!(
        unavailable,
        [
            "ly_comp_loop",
            "ly_comp_once",
            "ly_comp_ping",
            "ly_comp_speed",
            "ly_comp_blank"
        ]
    );
    assert_eq!(
        entry_of(&scene, "ly_comp_loop").outcome,
        ComponentOutcome::Unavailable(ComponentUnavailable::Geometry {
            stroke_id: "s_comp_a_0".to_owned(),
            reason: "geometry is not a relative-handle cubic path"
        })
    );
    for expect in set.expects.iter().filter(|e| e.frame == 0) {
        let entry = entry_of(&scene, &expect.uid);
        if matches!(entry.outcome, ComponentOutcome::Shapes(_)) {
            assert_eq!(diff(entry, &expect.want), None, "{}", expect.uid);
        }
    }
    // The unavailable slots stay in the merged sequence, at their plan position.
    let pin = ScenePin::of_packet(&packet);
    let objects = join_scene(
        &packet,
        &pin,
        vec![ProducerOutput::<String>::new("contour", pin.clone())],
    )
    .unwrap();
    let merged = scene.merge(&packet, &objects).unwrap();
    assert_eq!(merged.len(), 7);
    let unavailable_slots: Vec<bool> = merged
        .iter()
        .map(|slot| match slot {
            SceneSlot::Component(slot) => {
                matches!(slot.entry().outcome, ComponentOutcome::Unavailable(_))
            }
            SceneSlot::Object(_) => false,
        })
        .collect();
    assert_eq!(
        unavailable_slots,
        [true, true, true, false, true, false, true]
    );

    // A Time Remap with no sampler is the same: typed, in its slot.
    let mut remap = set.components.clone();
    instance_mut(&mut remap, "ly_comp_loop")["timeRemap"] = json!({ "keys": [] });
    let unwired = Prepared::from_json(&remap, &set.scene, set.cameras.clone());
    let packet = unwired.packet(0);
    let production = unwired
        .produce_with(&packet, &mut NoPorts, &mut Runtime::default())
        .unwrap();
    assert_eq!(production.entries.len(), 7);
    let loop_entry = production
        .entries
        .iter()
        .find(|e| e.layer_uid == "ly_comp_loop")
        .unwrap();
    assert!(matches!(
        loop_entry.outcome,
        ComponentOutcome::Unavailable(ComponentUnavailable::TimeRemap(_))
    ));
}

#[test]
fn scenes_pinned_to_different_frames_are_not_merged() {
    let set = oracle().remove(0);
    let doc = scene_with_objects(&set.scene, 5, None);
    let prepared = Prepared::from_json(&set.components, &doc, set.cameras.clone());
    let objects_at = |frame: u32| {
        let packet = prepared.packet(frame);
        let pin = ScenePin::of_packet(&packet);
        let mut output = ProducerOutput::new("contour", pin.clone());
        for record in packet.records() {
            output = output.with_item(record.source_object_index(), frame);
        }
        join_scene(&packet, &pin, vec![output]).unwrap()
    };
    let components_at = |frame: u32| {
        let packet = prepared.packet(frame);
        let scene = prepared
            .production(&packet)
            .join(&packet, &ScenePin::of_packet(&packet))
            .unwrap();
        (packet, scene)
    };
    let (packet4, components4) = components_at(4);
    let (packet5, components5) = components_at(5);
    assert_eq!(
        objects_at(4).entries().len(),
        0,
        "records are authored at frame 5"
    );
    assert_eq!(objects_at(5).entries().len(), 2);
    assert_eq!(
        components4.merge(&packet5, &objects_at(5)).unwrap_err(),
        ComponentJoinError::WrongPin {
            producer: Some("component".to_owned()),
            field: PinField::Frame
        }
    );
    assert_eq!(
        components5.merge(&packet5, &objects_at(4)).unwrap_err(),
        ComponentJoinError::ScenePinMismatch {
            field: PinField::Frame
        }
    );
    assert!(components4.merge(&packet4, &objects_at(4)).is_ok());
}
