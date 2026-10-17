//! Admission, pins, one-producer and typed-unavailability tests of the component evaluation join
//! (L04-K; I15, D33). The documents are the oracle sets of `support` (real `ObjectSnapshot`s);
//! missing providers are the ports of the producer join, never defaults.

use super::support::{entry_of, selector, set, Ports, Prepared, Runtime, SCENE};
use super::{
    evaluate_component_frame, evaluate_packet, ComponentFrame, ComponentFrameError,
    CANVAS_UNAVAILABLE,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::{
    ComponentJoinError, ComponentOutcome,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::ComponentUnavailable;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    PinField, ScenePin,
};
use crate::object_codec::encode_storage_project;
use crate::object_document::FrameScopeKind;
use crate::object_frame_packet::{
    prepare_object_exposure_frame, prepare_object_frame, ObjectFrameError, ObjectFramePacket,
    ObjectFrameSelector,
};
use serde_json::json;

fn frame_error(error: ObjectFrameError) -> Option<ComponentFrameError> {
    Some(ComponentFrameError::Frame(error))
}

fn outcome(
    prepared: &Prepared,
    ports: &mut Ports,
    runtime: &mut Runtime,
    uid: &str,
) -> ComponentOutcome {
    match prepared
        .query_with(SCENE, 0, ports, runtime)
        .expect("evaluates")
    {
        ComponentFrame::Scene(scene) => entry_of(&scene, uid).outcome.clone(),
        other => panic!("{other:?}"),
    }
}

#[test]
fn the_viewport_packet_and_the_export_packet_give_one_output() {
    let prepared = Prepared::of("base");
    for frame in [0u32, 4, 12, 23] {
        let sel = selector(&prepared.snapshot, SCENE, FrameScopeKind::Authored, frame);
        let viewport = prepare_object_frame(&prepared.snapshot, &sel).expect("viewport packet");
        let export =
            prepare_object_exposure_frame(&prepared.snapshot, &sel).expect("export packet");
        let pin = ScenePin::of_packet(&viewport);
        assert_eq!(pin, ScenePin::of_packet(&export));
        let evaluate = |packet: &ObjectFramePacket| {
            evaluate_packet(
                packet,
                &pin,
                None,
                &mut prepared.ports(),
                &mut prepared.runtime(),
            )
            .expect("evaluates")
            .expect("the document declares the table")
        };
        let from_viewport = evaluate(&viewport);
        assert_eq!(from_viewport.slots().len(), 7);
        assert_eq!(from_viewport, evaluate(&export));
        // The entry point is the same producer: its scene is that scene.
        assert_eq!(from_viewport, prepared.scene(frame));
    }
}

#[test]
fn evaluation_never_edits_the_snapshot_and_repeats_exactly() {
    let prepared = Prepared::of("overrides");
    let document = prepared.snapshot.document();
    let bytes = encode_storage_project(document).expect("encodes");
    let table = document.component_set().cloned();
    assert!(table.is_some());
    let first = prepared.scene(3);
    let second = prepared.scene(3);
    assert_eq!(first, second);
    let document = prepared.snapshot.document();
    assert_eq!(encode_storage_project(document).expect("encodes"), bytes);
    assert_eq!(document.component_set().cloned(), table);
    assert_eq!(prepared.snapshot.content_revision(), 0);
}

#[test]
fn a_wrong_pin_in_any_selector_field_is_refused_before_any_work() {
    let prepared = Prepared::of("remap");
    let attempt = |sel: ObjectFrameSelector<'_>| {
        let mut ports = prepared.ports();
        let mut runtime = prepared.runtime();
        let got =
            evaluate_component_frame(&prepared.snapshot, &sel, None, &mut ports, &mut runtime);
        if got.is_err() {
            assert!(ports.remap_asks.is_empty() && runtime.reads.is_empty());
        }
        got.err()
    };
    let base = || selector(&prepared.snapshot, SCENE, FrameScopeKind::Authored, 9);
    let mut wrong = base();
    wrong.instance_id = "another-instance";
    assert_eq!(attempt(wrong), frame_error(ObjectFrameError::WrongInstance));
    let mut wrong = base();
    wrong.document_id = "another-document";
    assert_eq!(attempt(wrong), frame_error(ObjectFrameError::WrongDocument));
    let mut wrong = base();
    wrong.content_revision += 1;
    assert_eq!(attempt(wrong), frame_error(ObjectFrameError::WrongRevision));
    let mut wrong = base();
    wrong.document_snapshot_id = "another-snapshot";
    assert_eq!(attempt(wrong), frame_error(ObjectFrameError::WrongSnapshot));
    assert_eq!(attempt(base()), None, "the unmutated pins are admitted");
}

#[test]
fn a_consumer_pin_that_differs_from_the_packet_is_a_typed_join_error() {
    let prepared = Prepared::of("base");
    let sel = selector(&prepared.snapshot, SCENE, FrameScopeKind::Authored, 5);
    let packet = prepare_object_frame(&prepared.snapshot, &sel).expect("packet");
    let asked = |pin: &ScenePin| {
        evaluate_packet(
            &packet,
            pin,
            None,
            &mut prepared.ports(),
            &mut prepared.runtime(),
        )
        .err()
    };
    let mut other = ScenePin::of_packet(&packet);
    assert_eq!(asked(&other), None);
    other.frame = 6;
    let wrong = |field| {
        Some(ComponentFrameError::Join(ComponentJoinError::WrongPin {
            producer: None,
            field,
        }))
    };
    assert_eq!(asked(&other), wrong(PinField::Frame));
    other.frame = 5;
    other.context_id = "sym_comp".to_owned();
    assert_eq!(asked(&other), wrong(PinField::Context));
    other.context_id = SCENE.to_owned();
    other.content_revision = 9;
    assert_eq!(asked(&other), wrong(PinField::Revision));
}

#[test]
fn contexts_scopes_and_frames_are_admitted_against_their_own_context() {
    let prepared = Prepared::of("variants");
    let at = |context: &str, frame| prepared.query(context, frame);
    assert_eq!(
        at("nope", 0).err(),
        frame_error(ObjectFrameError::InvalidContext)
    );
    assert_eq!(
        at("", 0).err(),
        frame_error(ObjectFrameError::InvalidContext)
    );
    // The scene has 24 frames, every component 12: the range is the context's own.
    assert!(at(SCENE, 23).is_ok());
    assert_eq!(
        at(SCENE, 24).err(),
        frame_error(ObjectFrameError::FrameOutOfRange)
    );
    assert!(at(SCENE, 12).is_ok());
    assert!(at("sym_comp", 11).is_ok());
    assert_eq!(
        at("sym_comp", 12).err(),
        frame_error(ObjectFrameError::FrameOutOfRange)
    );
    // Reference-scoped frames have no component or v2 scene records.
    for context in [SCENE, "sym_comp"] {
        let sel = selector(&prepared.snapshot, context, FrameScopeKind::Reference, 0);
        let got = evaluate_component_frame(
            &prepared.snapshot,
            &sel,
            None,
            &mut prepared.ports(),
            &mut prepared.runtime(),
        );
        assert_eq!(
            got.err(),
            frame_error(ObjectFrameError::Unavailable),
            "{context}"
        );
    }
    match at("sym_late", 5).expect("inside") {
        ComponentFrame::Inside(inside) => {
            assert_eq!(inside.symbol_id, "sym_late");
            assert_eq!(inside.frame, 5);
            assert_eq!(inside.pin.context_id, "sym_late");
            assert_eq!(inside.pin.frame, 5);
            assert_eq!(
                inside.pin.document_snapshot_id,
                prepared.snapshot.snapshot_id()
            );
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_document_without_the_table_is_not_an_empty_table() {
    let mut bare = set("base");
    bare.document
        .as_object_mut()
        .unwrap()
        .remove("componentTables");
    let bare = Prepared::from_set(bare);
    assert_eq!(bare.query(SCENE, 3), Ok(ComponentFrame::NoComponents));
    assert_eq!(
        bare.query("sym_comp", 0).err(),
        frame_error(ObjectFrameError::InvalidContext)
    );
    // The member present without the component table is still no table.
    let mut hollow = set("base");
    hollow.document["componentTables"] = json!({});
    assert_eq!(
        Prepared::from_set(hollow).query(SCENE, 3),
        Ok(ComponentFrame::NoComponents)
    );
    let mut empty = set("base");
    empty.document["componentTables"] = json!({"components": {
        "format": "nemo.native-component-set", "formatVersion": 1, "components": [], "instances": []
    }});
    let empty = Prepared::from_set(empty);
    match empty.query(SCENE, 3).expect("evaluates") {
        ComponentFrame::Scene(scene) => assert!(scene.slots().is_empty()),
        other => panic!("{other:?}"),
    }
}

#[test]
fn motion_property_holders_keep_the_scene_unavailable_not_flat() {
    let mut set = set("base");
    set.document["propertyHolders"] = json!([{
        "target": {"contextId": "scene-root", "layerUid": "ly_comp_loop"},
        "properties": [{"property": "position", "static": [3, 4]}]
    }]);
    let prepared = Prepared::from_set(set);
    assert_eq!(
        prepared.query(SCENE, 3).err(),
        frame_error(ObjectFrameError::Unavailable)
    );
    // Inside a component nothing of the scene's Motion applies.
    assert!(prepared.query("sym_comp", 3).is_ok());
}

/// The "variants" set with `range` set on one inner layer of one component.
fn variants_with_range(symbol: usize, layer: usize, range: serde_json::Value) -> Prepared {
    let mut set = set("variants");
    set.document["componentTables"]["components"]["components"][symbol]["layers"][layer]["range"] =
        range;
    Prepared::from_set(set)
}

#[test]
fn a_visible_inner_layer_with_a_gating_range_makes_the_entered_view_unavailable() {
    // The entered view shows inner layers as ordinary layers, which obey their own window
    // (`getEffectiveStrokes` -> `layerHasTimeRange`); until the D27 evaluator reads in points, out points
    // and time links, painting every frame of such a layer would silently lose the window.
    let want = Some(ComponentFrameError::Inside(
        ComponentUnavailable::InnerFeature {
            layer_uid: "ly_comp_ia".into(),
            feature: "layer range",
        },
    ));
    for range in [
        json!({"inPoint": 2}),
        json!({"outPoint": 0}),
        json!({"timeLink": {"uid": "ly_comp_ib"}}),
    ] {
        let prepared = variants_with_range(0, 0, range.clone());
        for frame in [0, 3, 11] {
            assert_eq!(
                prepared.query("sym_comp", frame).err(),
                want,
                "{range} @{frame}"
            );
        }
        // The placed instance and the scene are unaffected: the baseline's symbol branch never reads an
        // inner layer's range, and the root packet gate only looks at scene layers.
        assert!(prepared.query(SCENE, 3).is_ok(), "{range}");
        assert!(
            prepared.query("sym_late", 5).is_ok(),
            "{range}: another component"
        );
    }
}

#[test]
fn a_range_that_gates_nothing_or_sits_on_a_hidden_layer_does_not_block_the_entered_view() {
    // Markers never gate, explicit nulls are no window, and a hidden layer shows nothing to lose.
    for range in [
        json!({"markers": [{"frame": 2}]}),
        json!({"inPoint": null, "outPoint": null, "timeLink": null}),
        json!({}),
    ] {
        let prepared = variants_with_range(0, 0, range.clone());
        assert!(prepared.query("sym_comp", 3).is_ok(), "{range}");
    }
    // `sym_hidden`'s second layer is hidden.
    let hidden = variants_with_range(1, 1, json!({"inPoint": 2}));
    assert!(hidden.query("sym_hidden", 3).is_ok());
    // The same range on a visible layer of that component does block.
    let visible = variants_with_range(1, 0, json!({"inPoint": 2}));
    assert!(visible.query("sym_hidden", 3).is_err());
}

#[test]
fn an_unwired_time_remap_sampler_is_a_typed_slot_and_the_others_evaluate() {
    let prepared = Prepared::of("remap");
    let mut ports = Ports {
        remap: None,
        ..prepared.ports()
    };
    let scene = match prepared
        .query_with(SCENE, 7, &mut ports, &mut prepared.runtime())
        .expect("evaluates")
    {
        ComponentFrame::Scene(scene) => scene,
        other => panic!("{other:?}"),
    };
    assert_eq!(scene.slots().len(), 2);
    assert_eq!(scene.unavailable().count(), 1);
    let looped = entry_of(&scene, "ly_comp_loop");
    assert_eq!(
        looped.outcome,
        ComponentOutcome::Unavailable(ComponentUnavailable::TimeRemap(
            "time remap sampler is not wired"
        ))
    );
    // The held componentFrame decides before Time Remap: that layer needs no sampler.
    let held = entry_of(&scene, "ly_comp_hold");
    assert!(matches!(held.outcome, ComponentOutcome::Shapes(_)));
    assert_eq!(ports.remap_asks, [("ly_comp_loop".to_owned(), 7)]);
}

#[test]
fn camera_keys_without_a_canvas_size_are_a_typed_slot_never_a_guessed_size() {
    let mut prepared = Prepared::of("camera");
    prepared.set.canvas = None;
    for uid in ["ly_comp_loop", "ly_comp_ping"] {
        let got = outcome(
            &prepared,
            &mut prepared.ports(),
            &mut prepared.runtime(),
            uid,
        );
        assert_eq!(
            got,
            ComponentOutcome::Unavailable(ComponentUnavailable::Camera(CANVAS_UNAVAILABLE)),
            "{uid}"
        );
    }
    // A component without camera keys never needs the canvas.
    let mut base = Prepared::of("base");
    base.set.canvas = None;
    let got = outcome(
        &base,
        &mut base.ports(),
        &mut base.runtime(),
        "ly_comp_loop",
    );
    assert!(matches!(got, ComponentOutcome::Shapes(_)));
}

#[test]
fn an_unwired_property_runtime_is_a_typed_slot_per_instance() {
    let prepared = Prepared::of("overrides");
    // Each call evaluates the whole frame (three instances), so three calls ask nine times.
    let mut unwired = Runtime::default();
    for uid in ["ly_comp_loop", "ly_comp_ping", "ly_comp_speed"] {
        let got = outcome(&prepared, &mut prepared.ports(), &mut unwired, uid);
        assert_eq!(
            got,
            ComponentOutcome::Unavailable(ComponentUnavailable::OverridesUnavailable),
            "{uid}"
        );
    }
    assert_eq!(unwired.availability_asks, 9);
    assert!(unwired.reads.is_empty());
}
