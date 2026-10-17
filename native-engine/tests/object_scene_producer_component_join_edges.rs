//! Edge cases of the component producer join (L07-K): the placing layer's own timing and
//! its bound, an overflowing instance matrix, a failing camera owner and a placing layer
//! that also holds records of its own. Each keeps its slot as a typed outcome; the rest
//! of the scene still equals the baseline oracle. Inadmissible tables are in `inputs`.

use super::rows::{base, diff, entry_of, oracle, outcome_of, shapes_of, Prepared};
use super::support::{
    instance_mut, layer_mut, scene_with_objects, snapshot_of, table_of, with_stray_records,
    FixturePorts, Runtime,
};
use super::ComponentProduceError;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::{
    ComponentOutcome, SceneSlot,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::{
    playback_input, ComponentUnavailable, MAX_HELD_FRAME,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_shape::ShapeError;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, ProducerOutput, SceneOutcome, ScenePin, UnavailableCause,
};
use crate::object_document::object_component_record_kernels::object_component_records::ComponentRecordError;
use serde_json::json;

#[test]
fn placing_layer_timing_follows_the_baseline_scan_and_a_missing_one_is_typed() {
    let set = oracle().remove(0);
    // Interpolated samples of the placing layer are not keyframes: the key at 0 still holds
    // its componentFrame over them (the baseline scans down for `isKeyframe`).
    let mut scene = set.scene.clone();
    layer_mut(&mut scene, "ly_comp_hold")["timing"]["samples"] =
        json!([{ "frame": 5 }, { "frame": 6 }]);
    let sampled = Prepared::from_json(&set.components, &scene, set.cameras.clone());
    for frame in [4, 5, 6, 7] {
        let (inner, ids) = shapes_of(&outcome_of(&sampled, frame, "ly_comp_hold"));
        assert_eq!(
            (inner, ids[0].as_str()),
            (3.0, "s_comp_a_3"),
            "frame {frame}"
        );
    }
    // A layer with no frame key at all and no timing simply plays by its mode.
    let mut bare = set.scene.clone();
    layer_mut(&mut bare, "ly_comp_loop")
        .as_object_mut()
        .unwrap()
        .remove("timing");
    let untimed = Prepared::from_json(&set.components, &bare, set.cameras.clone());
    let row = set
        .expects
        .iter()
        .find(|e| e.uid == "ly_comp_loop" && e.frame == 7)
        .unwrap();
    let (_, joined) = untimed.scene(7);
    assert_eq!(diff(entry_of(&joined, "ly_comp_loop"), &row.want), None);
    assert_eq!(joined.unavailable().count(), 0);
    // Without timing a frame key cannot be told from a hold: the table is inadmissible
    // (its key sits on no timing key) and the production is refused; the playback input
    // alone still types the missing timing.
    let mut keyed = set.scene.clone();
    layer_mut(&mut keyed, "ly_comp_hold")
        .as_object_mut()
        .unwrap()
        .remove("timing");
    let keyed = Prepared::from_json(&set.components, &keyed, set.cameras.clone());
    let packet = keyed.packet(7);
    assert_eq!(
        keyed
            .produce_with(
                &packet,
                &mut FixturePorts::default(),
                &mut Runtime::default()
            )
            .unwrap_err(),
        ComponentProduceError::InvalidTable(ComponentRecordError::Invalid(
            "a frame key must sit on a timing key of its layer"
        ))
    );
    let host = keyed
        .snapshot
        .document()
        .layers()
        .iter()
        .find(|layer| layer.layer_uid() == "ly_comp_hold")
        .unwrap();
    let instance = keyed.table.instance("scene-root", "ly_comp_hold").unwrap();
    assert_eq!(
        playback_input(instance, host, 7).err(),
        Some(ComponentUnavailable::HostTimingMissing)
    );
    // The unedited set is the control: every slot evaluates.
    assert_eq!(base().scene(7).1.unavailable().count(), 0);
}

#[test]
fn a_held_key_beyond_the_dense_row_bound_is_typed_and_a_keyless_layer_costs_no_row() {
    let set = oracle().remove(0);
    let (mut scene, mut components) = (set.scene.clone(), set.components.clone());
    scene["totalFrames"] = json!(200_000);
    layer_mut(&mut scene, "ly_comp_hold")["timing"]["keys"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "frame": 150_000 }));
    instance_mut(&mut components, "ly_comp_hold")["frameKeys"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "frame": 150_000, "blankOverride": false }));
    let snapshot = snapshot_of(&scene);
    let table = table_of(&components);
    let host = |uid: &str| {
        snapshot
            .document()
            .layers()
            .iter()
            .find(|layer| layer.layer_uid() == uid)
            .unwrap()
            .clone()
    };
    let instance = |uid: &str| table.instance("scene-root", uid).unwrap();
    assert_eq!(MAX_HELD_FRAME, 100_000);
    assert_eq!(
        playback_input(instance("ly_comp_hold"), &host("ly_comp_hold"), 150_001).err(),
        Some(ComponentUnavailable::HostTimingTooLong { held: 150_000 })
    );
    // Before that key the held key is the one at frame 0: a normal, small row.
    let early = playback_input(instance("ly_comp_hold"), &host("ly_comp_hold"), 99_999).unwrap();
    assert_eq!(early.outer_keys.len(), 1);
    assert_eq!(early.outer_keys[0].component_frame, Some(3.0));
    // An instance without frame keys never allocates a row, however late the frame is.
    let keyless = playback_input(instance("ly_comp_loop"), &host("ly_comp_loop"), 150_001).unwrap();
    assert!(keyless.outer_keys.is_empty());
}

#[test]
fn an_overflowing_instance_matrix_is_a_typed_slot_and_the_others_still_match() {
    let set = oracle().remove(0);
    let mut components = set.components.clone();
    instance_mut(&mut components, "ly_comp_once")["symMatrix"] = json!([1e308, 0, 0, 1e308, 0, 0]);
    let prepared = Prepared::from_json(&components, &set.scene, set.cameras.clone());
    let (_, scene) = prepared.scene(0);
    // 40 * 1e308 overflows: the shape kernel refuses and nothing of the instance is published.
    assert_eq!(
        entry_of(&scene, "ly_comp_once").outcome,
        ComponentOutcome::Unavailable(ComponentUnavailable::Shape {
            stroke_id: "s_comp_a_0".to_owned(),
            cause: ShapeError::NonFiniteResult
        })
    );
    assert_eq!(scene.unavailable().count(), 1);
    for expect in set.expects.iter().filter(|e| e.frame == 0) {
        if expect.uid != "ly_comp_once" {
            assert_eq!(diff(entry_of(&scene, &expect.uid), &expect.want), None);
        }
    }
}

#[test]
fn a_placing_layer_keeps_its_component_slot_before_its_own_object_entries() {
    let set = oracle().remove(0);
    let frame = 5;
    let doc = scene_with_objects(&set.scene, frame, None);
    let doc = with_stray_records(&doc, "ly_comp_loop", &["st1", "st2"], frame);
    let prepared = Prepared::from_json(&set.components, &doc, set.cameras.clone());
    let packet = prepared.packet(frame);
    let pin = ScenePin::of_packet(&packet);
    // A contour producer claims every record but `st2`, which stays an Unclaimed entry.
    let mut contour = ProducerOutput::new("contour", pin.clone());
    for record in packet.records() {
        let stroke = record.record().target().stroke_id().to_owned();
        if stroke != "st2" {
            contour = contour.with_item(record.source_object_index(), stroke);
        }
    }
    let objects = join_scene(&packet, &pin, vec![contour]).expect("object scene");
    let components = prepared
        .production(&packet)
        .join(&packet, &pin)
        .expect("component scene");
    let merged = components.merge(&packet, &objects).expect("merged");
    let label = |slot: &SceneSlot<'_, String>| match slot {
        SceneSlot::Component(slot) => format!("cmp:{}", slot.entry().layer_uid),
        SceneSlot::Object(entry) => match entry.outcome() {
            SceneOutcome::Item { payload, .. } => format!("obj:{payload}"),
            SceneOutcome::Unavailable(UnavailableCause::Unclaimed) => "unclaimed".to_owned(),
            other => panic!("unexpected {other:?}"),
        },
    };
    let got: Vec<String> = merged.iter().map(label).collect();
    // The component slot of the layer comes first, then the layer's own entries in plan
    // order (the unclaimed record keeps its place); every other layer is untouched.
    assert_eq!(
        got,
        [
            "obj:bg1",
            "cmp:ly_comp_loop",
            "obj:st1",
            "unclaimed",
            "cmp:ly_comp_once",
            "cmp:ly_comp_ping",
            "cmp:ly_comp_single",
            "cmp:ly_comp_speed",
            "cmp:ly_comp_hold",
            "cmp:ly_comp_blank",
            "obj:fg1"
        ]
    );
    // The stray records never change the placed shapes of the layer.
    let want = set
        .expects
        .iter()
        .find(|e| e.frame == frame && e.uid == "ly_comp_loop")
        .unwrap();
    assert_eq!(
        diff(entry_of(&components, "ly_comp_loop"), &want.want),
        None
    );
}

#[test]
fn a_failing_camera_owner_is_a_typed_slot_per_instance() {
    let sets = oracle();
    let camera = Prepared::of(&sets[2]);
    let packet = camera.packet(3);
    let mut ports = camera.ports();
    ports.camera_failure = Some("camera owner offline");
    let production = camera
        .produce_with(&packet, &mut ports, &mut Runtime::default())
        .unwrap();
    let scene = production
        .join(&packet, &ScenePin::of_packet(&packet))
        .unwrap();
    assert_eq!(scene.slots().len(), 2);
    for slot in scene.slots() {
        assert_eq!(
            slot.entry().outcome,
            ComponentOutcome::Unavailable(ComponentUnavailable::Camera("camera owner offline"))
        );
    }
    assert_eq!(ports.camera_asks.len(), 2);
}
