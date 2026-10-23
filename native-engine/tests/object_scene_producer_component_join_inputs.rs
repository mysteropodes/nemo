//! Input-side tests of the component producer join (L07-K): unresolved instances, the
//! Time Remap and camera ports, inner-layer features without an evaluator and a packet
//! without an order plan. Expected values come from the baseline oracle (see `rows`).

use super::rows::{base, diff, entry_of, oracle, outcome_of, shapes_of, Prepared, CANVAS};
use super::support::{
    component_mut, instance_mut, packet_of, snapshot_of, table_of, FixturePorts, Runtime,
};
use super::{produce, ComponentInputs, ComponentProduceError, NoPorts};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::{
    ComponentJoinError, ComponentLayerEntry, ComponentOutcome,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::ComponentUnavailable;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::ScenePin;
use crate::object_document::object_component_record_kernels::object_component_records::ComponentRecordError;
use serde_json::{json, Value};

#[test]
fn unresolved_symbols_are_typed_unavailable_slots_and_unresolved_instances_are_refused() {
    let set = oracle().remove(0);
    // An unknown symbol keeps its plan slot as a typed unavailable entry.
    let mut components = set.components.clone();
    instance_mut(&mut components, "ly_comp_loop")["symbolId"] = json!("sym_missing");
    let prepared = Prepared::from_json(&components, &set.scene, set.cameras.clone());
    let (_, scene) = prepared.scene(3);
    assert_eq!(scene.slots().len(), 7);
    assert_eq!(scene.unavailable().count(), 1);
    assert_eq!(
        entry_of(&scene, "ly_comp_loop").outcome,
        ComponentOutcome::Unavailable(ComponentUnavailable::UnresolvedComponent {
            symbol_id: "sym_missing".to_owned()
        })
    );
    assert_eq!(scene.slots()[0].entry().layer_uid, "ly_comp_loop");
    assert_eq!(
        prepared
            .table
            .validate(prepared.snapshot.document().layers()),
        Err(ComponentRecordError::Unresolved(
            "instance symbolId names no component"
        ))
    );
    let once = set
        .expects
        .iter()
        .find(|e| e.uid == "ly_comp_once" && e.frame == 3)
        .unwrap();
    assert_eq!(diff(entry_of(&scene, "ly_comp_once"), &once.want), None);

    // An instance on a layer the packet does not have, in a component context, or twice.
    let refused = |edit: &dyn Fn(&mut Value)| {
        let mut components = set.components.clone();
        edit(&mut components);
        let prepared = Prepared::from_json(&components, &set.scene, set.cameras.clone());
        let packet = prepared.packet(0);
        prepared
            .produce_with(
                &packet,
                &mut FixturePorts::default(),
                &mut Runtime::default(),
            )
            .unwrap_err()
    };
    assert_eq!(
        refused(&|c| instance_mut(c, "ly_comp_hold")["layerUid"] = json!("ly_nope")),
        ComponentProduceError::UnresolvedInstance {
            context_id: "scene-root".to_owned(),
            layer_uid: "ly_nope".to_owned()
        }
    );
    assert_eq!(
        refused(&|c| instance_mut(c, "ly_comp_hold")["contextId"] = json!("sym_comp")),
        ComponentProduceError::UnresolvedInstance {
            context_id: "sym_comp".to_owned(),
            layer_uid: "ly_comp_hold".to_owned()
        }
    );
    assert_eq!(
        refused(&|c| {
            let copy = instance_mut(c, "ly_comp_hold").clone();
            c["instances"].as_array_mut().unwrap().push(copy);
        }),
        ComponentProduceError::DuplicateInstance {
            layer_uid: "ly_comp_hold".to_owned()
        }
    );
    // The L04-G validator names the nested placement the producer cannot place.
    let mut nested = set.components.clone();
    instance_mut(&mut nested, "ly_comp_hold")["contextId"] = json!("sym_comp");
    assert_eq!(
        table_of(&nested).validate(base().snapshot.document().layers()),
        Err(ComponentRecordError::NestedComponent)
    );
}

#[test]
fn an_inadmissible_table_is_refused_before_any_instance_is_evaluated() {
    let set = oracle().remove(0);
    let refusal = |edit: &dyn Fn(&mut Value)| {
        let mut components = set.components.clone();
        edit(&mut components);
        let prepared = Prepared::from_json(&components, &set.scene, set.cameras.clone());
        let packet = prepared.packet(0);
        let mut ports = FixturePorts::default();
        let error = prepared
            .produce_with(&packet, &mut ports, &mut Runtime::default())
            .unwrap_err();
        // Refused before any evaluation: no provider was asked.
        assert!(ports.camera_asks.is_empty() && ports.remap_asks.is_empty());
        error
    };
    let invalid =
        |why: &'static str| ComponentProduceError::InvalidTable(ComponentRecordError::Invalid(why));
    assert_eq!(
        refusal(&|c| instance_mut(c, "ly_comp_loop")["symMatrix"] = json!([1, 0, 0, 1, 0])),
        invalid("symMatrix needs six finite numbers")
    );
    assert_eq!(
        refusal(&|c| instance_mut(c, "ly_comp_hold")["frameKeys"][0]["frame"] = json!(3)),
        invalid("a frame key must sit on a timing key of its layer")
    );
    assert_eq!(
        refusal(&|c| c["formatVersion"] = json!(9)),
        ComponentProduceError::InvalidTable(ComponentRecordError::Unsupported(
            "unsupported component set format or formatVersion"
        ))
    );
    // An unknown component stays a typed slot, but it does not hide another defect.
    assert_eq!(
        refusal(&|c| {
            instance_mut(c, "ly_comp_loop")["symbolId"] = json!("sym_missing");
            instance_mut(c, "ly_comp_once")["symMatrix"] = json!([1]);
        }),
        invalid("symMatrix needs six finite numbers")
    );
}

#[test]
fn time_remap_comes_from_the_port_and_an_unwired_sampler_is_typed_unavailable() {
    let set = oracle().remove(0);
    let mut components = set.components.clone();
    instance_mut(&mut components, "ly_comp_loop")["timeRemap"] = json!({ "keys": [] });
    let prepared = Prepared::from_json(&components, &set.scene, set.cameras.clone());
    let packet = prepared.packet(3);
    let run = |remap: Option<Result<Option<f64>, &'static str>>| {
        let mut ports = FixturePorts {
            remap,
            ..FixturePorts::default()
        };
        let production = prepared
            .produce_with(&packet, &mut ports, &mut Runtime::default())
            .unwrap();
        let scene = production
            .join(&packet, &ScenePin::of_packet(&packet))
            .unwrap();
        (
            entry_of(&scene, "ly_comp_loop").outcome.clone(),
            ports.remap_asks,
        )
    };
    // The sampled value names the inner frame directly (Math.round, clamped).
    for (value, inner) in [(7.4, 7.0), (7.5, 8.0), (99.0, 11.0), (-3.0, 0.0)] {
        let (outcome, asks) = run(Some(Ok(Some(value))));
        let (got, ids) = shapes_of(&outcome);
        assert_eq!(got, inner, "remap {value}");
        assert_eq!(ids[0], format!("s_comp_a_{}", inner as u32));
        // Asked once, for the remapped layer only, at the packet frame.
        assert_eq!(asks, [("ly_comp_loop".to_owned(), 3)]);
    }
    // The sampler's null plays the layer by its mode: loop, placed at 4, frame 3 -> 0.
    let (outcome, _) = run(Some(Ok(None)));
    assert_eq!(shapes_of(&outcome).0, 0.0);
    // No sampler: the slot stays, typed.
    let (outcome, asks) = run(None);
    assert_eq!(
        outcome,
        ComponentOutcome::Unavailable(ComponentUnavailable::TimeRemap(
            "time remap sampler is not wired"
        ))
    );
    assert_eq!(asks.len(), 1);
    // The default NoPorts has no sampler and no camera member.
    let mut ports = NoPorts;
    let production = prepared
        .produce_with(&packet, &mut ports, &mut Runtime::default())
        .unwrap();
    assert_eq!(
        production
            .entries
            .iter()
            .filter(|e| matches!(e.outcome, ComponentOutcome::Unavailable(_)))
            .count(),
        1
    );
    // A layer decided by a held componentFrame or a held blank never asks the sampler.
    let mut decided = set.components.clone();
    for uid in ["ly_comp_hold", "ly_comp_blank"] {
        instance_mut(&mut decided, uid)["timeRemap"] = json!({ "keys": [] });
    }
    let held = Prepared::from_json(&decided, &set.scene, set.cameras.clone());
    let packet = held.packet(15);
    let mut ports = FixturePorts::default();
    let production = held
        .produce_with(&packet, &mut ports, &mut Runtime::default())
        .unwrap();
    assert!(ports.remap_asks.is_empty());
    let by_uid = |uid: &str| {
        production
            .entries
            .iter()
            .find(|entry| entry.layer_uid == uid)
            .unwrap()
            .outcome
            .clone()
    };
    assert_eq!(shapes_of(&by_uid("ly_comp_hold")).0, 3.0);
    assert_eq!(by_uid("ly_comp_blank"), ComponentOutcome::Blank);
    // Instances without Time Remap never ask the sampler.
    let plain = base();
    let mut ports = FixturePorts::default();
    let packet = plain.packet(3);
    plain
        .produce_with(&packet, &mut ports, &mut Runtime::default())
        .unwrap();
    assert!(ports.remap_asks.is_empty());
}

#[test]
fn the_camera_port_is_asked_per_component_and_composes_before_the_instance_matrix() {
    let sets = oracle();
    let camera = Prepared::of(&sets[2]);
    let packet = camera.packet(9);
    let mut ports = camera.ports();
    let production = camera
        .produce_with(&packet, &mut ports, &mut Runtime::default())
        .unwrap();
    // Two instances of one component: the port is asked once per evaluated instance.
    assert_eq!(ports.camera_asks, ["sym_comp", "sym_comp"]);
    let scene = production
        .join(&packet, &ScenePin::of_packet(&packet))
        .unwrap();
    for expect in sets[2].expects.iter().filter(|e| e.frame == 9) {
        assert_eq!(diff(entry_of(&scene, &expect.uid), &expect.want), None);
    }
    // Without the port's keys the same instances are NOT the camera result.
    let bare = Prepared::from_json(&sets[2].components, &sets[2].scene, Default::default());
    let packet = bare.packet(9);
    let scene = bare
        .production(&packet)
        .join(&packet, &ScenePin::of_packet(&packet))
        .unwrap();
    let want = sets[2].expects.iter().find(|e| e.frame == 9).unwrap();
    assert_ne!(diff(entry_of(&scene, &want.uid), &want.want), None);
}

#[test]
fn inner_layers_with_unevaluated_features_are_typed_refusals_not_skips() {
    let set = oracle().remove(0);
    assert_eq!(
        (set.expects[2].uid.as_str(), set.expects[2].frame),
        ("ly_comp_loop", 2)
    );
    let edited = |edit: &dyn Fn(&mut Value)| {
        let mut components = set.components.clone();
        edit(&mut components);
        let prepared = Prepared::from_json(&components, &set.scene, set.cameras.clone());
        outcome_of(&prepared, 2, "ly_comp_loop")
    };
    let with_fx =
        |fx: Value| edited(&|c| component_mut(c, "sym_comp")["layers"][0]["pathFx"] = fx.clone());
    assert_eq!(
        with_fx(json!([{ "type": "zigzag" }])),
        ComponentOutcome::Unavailable(ComponentUnavailable::InnerFeature {
            layer_uid: "ly_comp_ia".to_owned(),
            feature: "path effects"
        })
    );
    // A disabled effect and an empty stack evaluate nothing and are not refused.
    for fx in [json!([{ "type": "zigzag", "enabled": false }]), json!([])] {
        assert_eq!(
            diff(
                &ComponentLayerEntry {
                    source_layer_index: 0,
                    layer_uid: "ly_comp_loop".to_owned(),
                    symbol_id: "sym_comp".to_owned(),
                    outcome: with_fx(fx),
                },
                &set.expects[2].want
            ),
            None
        );
    }
    assert_eq!(
        edited(&|c| {
            component_mut(c, "sym_comp")["layers"][1]
                .as_object_mut()
                .unwrap()
                .remove("timing");
        }),
        ComponentOutcome::Unavailable(ComponentUnavailable::InnerTimingMissing {
            layer_uid: "ly_comp_ib".to_owned()
        })
    );
}

#[test]
fn a_packet_without_a_numeric_order_plan_is_refused_not_ordered() {
    let set = oracle().remove(0);
    let mut v1 = set.scene.clone();
    v1["formatVersion"] = json!(1);
    v1.as_object_mut().unwrap().remove("compositionOrder");
    let snapshot = snapshot_of(&v1);
    let table = table_of(&set.components);
    let packet = packet_of(&snapshot, 0);
    assert!(packet.numeric_order().is_none());
    let inputs = ComponentInputs {
        components: &table,
        canvas: CANVAS,
    };
    let production = produce(
        &packet,
        &inputs,
        &mut FixturePorts::default(),
        &mut Runtime::default(),
    )
    .unwrap();
    let error = production
        .join(&packet, &ScenePin::of_packet(&packet))
        .unwrap_err();
    assert_eq!(error, ComponentJoinError::NoNumericOrder);
}
