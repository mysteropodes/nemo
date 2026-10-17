//! Behaviour tests for the component evaluated-shape producer join (L07-K; D32, I16a).
//!
//! Every test runs the real chain: a v2 `ObjectSnapshot` and `ObjectFramePacket`, the
//! L04-G `ComponentSet`, the L04-A playback kernel, the L07-H shape kernel and the L07-C
//! pin types. The expected values are the BASELINE results of `getEffectiveStrokes`
//! stored in `oracle.txt` (see `rows`), plus literals from the fixture's own
//! `expected.json`. Inputs, ports and overrides are in `inputs` and `overrides`.

use super::rows::{
    base, diff, entry_of, oracle, outcome_of, shapes_of, Prepared, CANVAS, FRAMES, ORACLE,
};
use super::support::{component_mut, instance_mut, layer_mut, scene_with_objects, table_of};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::{
    ComponentOutcome, SceneSlot,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, ProducerOutput, SceneOutcome, ScenePin,
};
use serde_json::json;

#[test]
fn oracle_row_counts_and_provenance_are_pinned() {
    let sets = oracle();
    let names: Vec<&str> = sets.iter().map(|set| set.name.as_str()).collect();
    assert_eq!(names, ["base", "placed", "camera", "variants"]);
    let rows: Vec<usize> = sets.iter().map(|set| set.expects.len()).collect();
    assert_eq!(rows, [168, 168, 48, 168]);
    assert_eq!(FRAMES, 24);
    assert_eq!(CANVAS, (640.0, 360.0));
    assert_eq!(sets[2].cameras.len(), 1);
    assert_eq!(sets[2].cameras["sym_comp"].len(), 2);
    assert!(ORACLE.contains("Baseline commit fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    // Verbatim baseline functions: getEffectiveStrokes and resolveSymbolFrameIdx.
    assert!(ORACLE.contains("c938d85f31af331657711d3956169917c6cae3390557c615d4f6e008cc9e3295"));
    assert!(ORACLE.contains("a87c5f559553cb7700bc8d8943a1938915a2bdc0026540f3722c4b159f8ebeef"));
    assert!(ORACLE.contains("Paper.js 0.12.18"));
    assert!(ORACLE.contains("sets=4 X=552 blank=24 empty=23 shapes=983 handled=1716"));
}

#[test]
fn every_oracle_row_matches_the_baseline_scene_through_the_native_join() {
    let mut compared = 0;
    for set in oracle() {
        let prepared = Prepared::of(&set);
        // The converted table is admissible to the L04-G validator against the scene layers.
        assert_eq!(
            prepared
                .table
                .validate(prepared.snapshot.document().layers()),
            Ok(()),
            "{}",
            set.name
        );
        for frame in 0..FRAMES {
            let (packet, scene) = prepared.scene(frame);
            assert_eq!(scene.pin(), &ScenePin::of_packet(&packet));
            // One slot per placing layer: nothing is silently absent.
            assert_eq!(scene.slots().len(), packet.source_layers().len());
            assert_eq!(scene.unavailable().count(), 0, "{} {frame}", set.name);
            for expect in set.expects.iter().filter(|expect| expect.frame == frame) {
                let entry = entry_of(&scene, &expect.uid);
                assert_eq!(
                    diff(entry, &expect.want),
                    None,
                    "{} {} frame {frame}",
                    set.name,
                    expect.uid
                );
                compared += 1;
            }
        }
    }
    assert_eq!(compared, 552);
}

#[test]
fn fixture_expected_json_frames_are_pinned_literally() {
    let prepared = base();
    let ids = |frame: u32, uid: &str, inner: f64, a: &str| {
        let got = shapes_of(&outcome_of(&prepared, frame, uid));
        assert_eq!(got, (inner, vec![a.to_owned(), "s_comp_b_0".to_owned()]));
    };
    // tests/fixtures/components/expected.json, symbol-frame checks.
    ids(0, "ly_comp_loop", 0.0, "s_comp_a_0");
    ids(5, "ly_comp_loop", 1.0, "s_comp_a_1"); // symPlacedAt 4
    ids(16, "ly_comp_loop", 0.0, "s_comp_a_0");
    ids(12, "ly_comp_once", 11.0, "s_comp_a_11"); // holds at the last frame
    ids(15, "ly_comp_ping", 7.0, "s_comp_a_7");
    ids(23, "ly_comp_ping", 1.0, "s_comp_a_1");
    ids(0, "ly_comp_single", 5.0, "s_comp_a_5");
    ids(23, "ly_comp_single", 5.0, "s_comp_a_5");
    ids(3, "ly_comp_speed", 6.0, "s_comp_a_6"); // symSpeed 2
    ids(7, "ly_comp_hold", 3.0, "s_comp_a_3"); // componentFrame 3 on the outer key
    ids(11, "ly_comp_blank", 11.0, "s_comp_a_11");
}

#[test]
fn a_held_blank_is_reported_and_a_later_non_blank_key_restores_playback() {
    let set = oracle().remove(0);
    let (mut scene, mut components) = (set.scene.clone(), set.components.clone());
    layer_mut(&mut scene, "ly_comp_blank")["timing"]["keys"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "frame": 18 }));
    instance_mut(&mut components, "ly_comp_blank")["frameKeys"]
        .as_array_mut()
        .unwrap()
        .push(json!({ "frame": 18, "blankOverride": false }));
    let prepared = Prepared::from_json(&components, &scene, set.cameras.clone());
    assert_eq!(
        prepared
            .table
            .validate(prepared.snapshot.document().layers()),
        Ok(())
    );
    for frame in [12, 13, 17] {
        assert_eq!(
            outcome_of(&prepared, frame, "ly_comp_blank"),
            ComponentOutcome::Blank,
            "frame {frame}"
        );
    }
    // The key at 18 holds blankOverride:false: playback resumes at (18 - 0) % 12.
    assert_eq!(
        shapes_of(&outcome_of(&prepared, 18, "ly_comp_blank")).0,
        6.0
    );
    assert_eq!(
        shapes_of(&outcome_of(&prepared, 23, "ly_comp_blank")).0,
        11.0
    );
    // Blank is explicit and distinct from an unavailable slot.
    let (_, scene) = prepared.scene(13);
    assert_eq!(scene.unavailable().count(), 0);
    assert_eq!(scene.slots().len(), 7);
}

#[test]
fn an_inner_frame_that_names_no_frame_is_an_empty_slot_not_an_unavailable_one() {
    let sets = oracle();
    let variants = Prepared::of(&sets[3]);
    for frame in 0..=10 {
        match outcome_of(&variants, frame, "ly_v_frac") {
            ComponentOutcome::Shapes(placed) => {
                assert_eq!(placed.inner_frame.raw(), f64::from(frame) + 0.5);
                assert!(placed.shapes.is_empty(), "frame {frame}");
            }
            other => panic!("frame {frame}: {other:?}"),
        }
    }
    assert_eq!(
        shapes_of(&outcome_of(&variants, 11, "ly_v_frac")).1,
        ["s_comp_a_11", "s_comp_b_0"]
    );
    for frame in 12..FRAMES {
        let (inner, ids) = shapes_of(&outcome_of(&variants, frame, "ly_v_range"));
        assert_eq!((inner, ids.len()), (f64::from(frame), 0), "frame {frame}");
    }
    let (_, scene) = variants.scene(20);
    assert_eq!(scene.unavailable().count(), 0);
}

#[test]
fn component_slots_share_the_packet_pin_and_the_plan_slot_of_the_placing_layer() {
    let set = oracle().remove(0);
    let frame = 5;
    let label = |slot: &SceneSlot<'_, String>| match slot {
        SceneSlot::Object(entry) => match entry.outcome() {
            SceneOutcome::Item { payload, .. } => format!("obj:{payload}"),
            other => panic!("unexpected {other:?}"),
        },
        SceneSlot::Component(slot) => format!("cmp:{}", slot.entry().layer_uid),
    };
    for (explicit, want) in [
        (
            None,
            vec![
                "obj:ly_bg",
                "cmp:ly_comp_loop",
                "cmp:ly_comp_once",
                "cmp:ly_comp_ping",
                "cmp:ly_comp_single",
                "cmp:ly_comp_speed",
                "cmp:ly_comp_hold",
                "cmp:ly_comp_blank",
                "obj:ly_fg",
            ],
        ),
        (
            // Order rank 1 is the front-most position: the layer moves to the end.
            Some(("ly_comp_once", 1.0)),
            vec![
                "obj:ly_bg",
                "cmp:ly_comp_loop",
                "cmp:ly_comp_ping",
                "cmp:ly_comp_single",
                "cmp:ly_comp_speed",
                "cmp:ly_comp_hold",
                "cmp:ly_comp_blank",
                "obj:ly_fg",
                "cmp:ly_comp_once",
            ],
        ),
    ] {
        let scene_json = scene_with_objects(&set.scene, frame, explicit);
        let prepared = Prepared::from_json(&set.components, &scene_json, set.cameras.clone());
        let packet = prepared.packet(frame);
        let pin = ScenePin::of_packet(&packet);
        let mut objects = ProducerOutput::new("contour", pin.clone());
        for record in packet.records() {
            objects = objects.with_item(
                record.source_object_index(),
                record.record().target().layer_uid().to_owned(),
            );
        }
        let objects = join_scene(&packet, &pin, vec![objects]).expect("object scene");
        let components = prepared
            .production(&packet)
            .join(&packet, &pin)
            .expect("component scene");
        let merged = components.merge(&packet, &objects).expect("merged");
        let got: Vec<String> = merged.iter().map(&label).collect();
        assert_eq!(got, want, "{explicit:?}");
        // The merged order is the packet's plan, read independently of the join.
        let plan: Vec<&str> = packet
            .numeric_order()
            .unwrap()
            .layers()
            .iter()
            .map(|layer| packet.source_layers()[layer.source_layer_index()].layer_uid())
            .collect();
        let merged_layers: Vec<&str> = got
            .iter()
            .map(|label| label.split(':').nth(1).unwrap())
            .collect();
        assert_eq!(merged_layers, plan);
        for slot in components.slots() {
            let in_plan = &packet.numeric_order().unwrap().layers()[slot.plan_position()];
            assert_eq!(
                packet.source_layers()[in_plan.source_layer_index()].layer_uid(),
                slot.entry().layer_uid
            );
        }
        // Every shape of the merged scene still agrees with the baseline rows.
        for slot in components.slots() {
            let want = set
                .expects
                .iter()
                .find(|e| e.frame == frame && e.uid == slot.entry().layer_uid)
                .unwrap();
            assert_eq!(diff(slot.entry(), &want.want), None);
        }
    }
}

#[test]
fn evaluation_is_immutable_pinned_and_detached_from_the_stored_table() {
    let set = oracle().remove(1); // placed: every instance carries symMatrix
    let prepared = Prepared::of(&set);
    let document = prepared.snapshot.document().clone();
    let table = prepared.table.clone();
    let packet = prepared.packet(7);
    let pin = ScenePin::of_packet(&packet);
    assert_eq!(pin.instance_id, "component-join");
    assert_eq!(pin.frame, 7);
    let first = prepared.production(&packet);
    let second = prepared.production(&packet);
    assert_eq!(first, second, "the same pin gives the same production");
    assert_eq!(first.pin, pin);
    let scene = first.join(&packet, &pin).unwrap();
    assert_eq!(prepared.snapshot.document(), &document);
    assert_eq!(prepared.table, table);
    // Rewrite the stored geometry afterwards: the evaluated copies do not move.
    let mut edited = set.components.clone();
    let objects = component_mut(&mut edited, "sym_comp")["objects"]
        .as_array_mut()
        .unwrap();
    let held_b = objects
        .iter_mut()
        .find(|object| object["target"]["strokeId"] == "s_comp_b_0")
        .unwrap();
    held_b["geometry"]["segments"][0]["point"]["x"] = json!(9999);
    // Same snapshot, so the pins are equal: only the stored geometry differs.
    let moved = Prepared {
        snapshot: prepared.snapshot.clone(),
        table: table_of(&edited),
        cameras: prepared.cameras.clone(),
    };
    let other = moved.scene(7).1;
    assert_eq!(other.pin(), scene.pin());
    assert_ne!(
        other, scene,
        "the edit is visible in a scene built after it"
    );
    let looped = set
        .expects
        .iter()
        .find(|e| e.frame == 7 && e.uid == "ly_comp_loop")
        .unwrap();
    assert_eq!(
        diff(entry_of(&other, "ly_comp_loop"), &looped.want),
        Some("geometry of s_comp_b_0".to_owned())
    );
    // The earlier scene still equals the baseline rows: nothing reached it.
    for expect in set.expects.iter().filter(|e| e.frame == 7) {
        assert_eq!(diff(entry_of(&scene, &expect.uid), &expect.want), None);
    }
}
