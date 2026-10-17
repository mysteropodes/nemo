//! Mutation controls of the component producer join (L07-K).
//!
//! Each control breaks one rule (a mutated input or a wrong reference implementation)
//! and must be noticed by the oracle comparison. The real implementation passes the same
//! rows with zero differences (`tests`), so a control differs from it only by its flaw.
//! Pin, claim and merge controls are in `pins`.

use super::rows::{diff, entry_of, oracle, Expect, Prepared, Set, FRAMES};
use super::support::{component_mut, instance_mut, scene_with_objects};
use crate::evaluation::object_component_kernels::object_component_playback::{
    evaluate_component_playback, PlaybackInput, PlaybackOutcome,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_scene::ComponentScene;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_component_select::playback_input;
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::ScenePin;
use serde_json::{json, Value};

/// True when the baseline result `want` (blank | inner frame) differs from `outcome`.
fn playback_differs(outcome: PlaybackOutcome, want: &Value) -> bool {
    match outcome {
        PlaybackOutcome::Blank => want["blank"] != Value::Bool(true),
        PlaybackOutcome::Frame(frame) => want["inner"].as_f64() != Some(frame.raw()),
    }
}

/// The real `PlaybackInput` of an oracle row.
fn real_input(prepared: &Prepared, uid: &str, frame: u32) -> PlaybackInput {
    let instance = prepared.table.instance("scene-root", uid).unwrap();
    let host = prepared
        .snapshot
        .document()
        .layers()
        .iter()
        .find(|layer| layer.layer_uid() == uid)
        .unwrap();
    playback_input(instance, host, frame).expect("playback input")
}

/// Sorted layers with at least one row where the kernel, fed by `build`, disagrees
/// with the baseline.
fn disagreeing_layers(
    sets: &[Set],
    build: &dyn Fn(&Prepared, &str, u32) -> PlaybackInput,
) -> Vec<String> {
    let mut layers: Vec<String> = Vec::new();
    for set in sets {
        let prepared = Prepared::of(set);
        for expect in &set.expects {
            let instance = prepared.table.instance("scene-root", &expect.uid).unwrap();
            let component = prepared.table.component(instance.symbol_id()).unwrap();
            let input = build(&prepared, &expect.uid, expect.frame);
            let outcome = evaluate_component_playback(
                &input,
                f64::from(component.total_frames()),
                i64::from(expect.frame),
                None,
            );
            if playback_differs(outcome, &expect.want) && !layers.contains(&expect.uid) {
                layers.push(expect.uid.clone());
            }
        }
    }
    layers.sort();
    layers
}

#[test]
fn an_outer_key_read_at_the_exact_frame_only_disagrees_with_the_oracle() {
    let sets = oracle();
    let both = &sets[..2];
    // The real held-key rule agrees with the baseline on every row of both sets...
    assert_eq!(disagreeing_layers(both, &real_input), Vec::<String>::new());
    // ...a mutant that forgets the hold (a key counts on its own frame only) does not:
    // the componentFrame hold and the blank span both live on the frames AFTER the key.
    let exact = |prepared: &Prepared, uid: &str, frame: u32| {
        let mut input = real_input(prepared, uid, frame);
        if input.outer_keys.len() != frame as usize + 1 {
            input.outer_keys.clear();
        }
        input
    };
    assert_eq!(
        disagreeing_layers(both, &exact),
        ["ly_comp_blank", "ly_comp_hold"]
    );
}

#[test]
fn a_selection_without_the_backward_hold_disagrees_with_the_oracle() {
    let sets = oracle();
    let (mut real_bad, mut mutant_bad, mut rows) = (0, 0, 0);
    for set in sets.iter().filter(|set| set.name != "camera") {
        let prepared = Prepared::of(set);
        for expect in &set.expects {
            let Some(inner) = expect.want["inner"].as_f64() else {
                continue;
            };
            if inner.fract() != 0.0 || inner >= 12.0 {
                continue;
            }
            let instance = prepared.table.instance("scene-root", &expect.uid).unwrap();
            let component = prepared.table.component(instance.symbol_id()).unwrap();
            let want: Vec<&str> = expect.want["shapes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|shape| shape["strokeId"].as_str().unwrap())
                .collect();
            // Mutant: only the records authored exactly at the inner frame, visible layers.
            let mut exact = Vec::new();
            for layer in component.layers().iter().filter(|layer| layer.visible()) {
                for object in component.objects() {
                    let target = object.target();
                    if target.layer_uid() == layer.layer_uid() && f64::from(target.frame()) == inner
                    {
                        exact.push(target.stroke_id());
                    }
                }
            }
            rows += 1;
            mutant_bad += usize::from(exact != want);
            let (_, scene) = prepared.scene(expect.frame);
            real_bad += usize::from(diff(entry_of(&scene, &expect.uid), &expect.want).is_some());
        }
    }
    // Pinned from an independent recount of the oracle rows (inner frame integral, < 12).
    assert_eq!(rows, 457);
    assert_eq!(real_bad, 0);
    // Inner B holds its frame-0 key over all 12 frames, so the mutant loses it nearly everywhere.
    assert_eq!(mutant_bad, 393);
}

fn frame0_row<'a>(set: &'a Set, uid: &str) -> &'a Expect {
    set.expects
        .iter()
        .find(|expect| expect.uid == uid && expect.frame == 0)
        .unwrap()
}

#[test]
fn ignoring_inner_layer_visibility_and_dropping_the_instance_matrix_are_detected() {
    let sets = oracle();
    let variants = &sets[3];
    let mut components = variants.components.clone();
    component_mut(&mut components, "sym_hidden")["layers"][1]["visible"] = json!(true);
    let shown = Prepared::from_json(&components, &variants.scene, variants.cameras.clone());
    let real = Prepared::of(variants);
    let mut differing = 0;
    for expect in variants.expects.iter().filter(|e| e.uid == "ly_v_hidden") {
        let (_, scene) = real.scene(expect.frame);
        assert_eq!(diff(entry_of(&scene, &expect.uid), &expect.want), None);
        let (_, mutated) = shown.scene(expect.frame);
        differing += usize::from(diff(entry_of(&mutated, &expect.uid), &expect.want).is_some());
    }
    assert_eq!(differing, FRAMES as usize);

    let placed = &sets[1];
    let mut bare = placed.components.clone();
    instance_mut(&mut bare, "ly_comp_loop")
        .as_object_mut()
        .unwrap()
        .remove("symMatrix");
    let without = Prepared::from_json(&bare, &placed.scene, placed.cameras.clone());
    let (_, scene) = without.scene(0);
    let moved = frame0_row(placed, "ly_comp_loop");
    assert!(diff(entry_of(&scene, "ly_comp_loop"), &moved.want).is_some());
    // Only the edited instance moved: the others still match.
    let kept = frame0_row(placed, "ly_comp_once");
    assert_eq!(diff(entry_of(&scene, "ly_comp_once"), &kept.want), None);
}

fn slot_uids(scene: &ComponentScene) -> Vec<String> {
    scene
        .slots()
        .iter()
        .map(|slot| slot.entry().layer_uid.clone())
        .collect()
}

#[test]
fn slots_follow_the_packet_plan_and_never_the_production_or_source_order() {
    let set = oracle().remove(0);
    let frame = 5;
    let doc = scene_with_objects(&set.scene, frame, Some(("ly_comp_once", 1.0)));
    let prepared = Prepared::from_json(&set.components, &doc, set.cameras.clone());
    let packet = prepared.packet(frame);
    let pin = ScenePin::of_packet(&packet);
    let production = prepared.production(&packet);
    let produced: Vec<String> = production
        .entries
        .iter()
        .map(|entry| entry.layer_uid.clone())
        .collect();
    let mut reversed = production.clone();
    reversed.entries.reverse();
    let scene = production.join(&packet, &pin).unwrap();
    let scene_reversed = reversed.join(&packet, &pin).unwrap();
    // Production order is source order (once is second); the plan puts the ranked layer last.
    assert_eq!(produced[1], "ly_comp_once");
    assert_eq!(
        slot_uids(&scene),
        [
            "ly_comp_loop",
            "ly_comp_ping",
            "ly_comp_single",
            "ly_comp_speed",
            "ly_comp_hold",
            "ly_comp_blank",
            "ly_comp_once"
        ]
    );
    assert_ne!(slot_uids(&scene), produced);
    assert_eq!(
        scene, scene_reversed,
        "entry order never changes the joined scene"
    );
}

/// Expected segments (baseline rows of the set) whose two handles are both non-zero.
fn handled_segments(set: &Set) -> usize {
    let non_zero = |v: &Value| v[0].as_f64().unwrap() != 0.0 || v[1].as_f64().unwrap() != 0.0;
    set.expects
        .iter()
        .flat_map(|expect| expect.want["shapes"].as_array().into_iter().flatten())
        .flat_map(|shape| shape["segments"].as_array().unwrap())
        .filter(|segment| non_zero(&segment["handleIn"]) && non_zero(&segment["handleOut"]))
        .count()
}

/// The set with every stored segment of `sym_comp` edited in the component table.
fn with_segments_edited(set: &Set, edit: &dyn Fn(&mut Value)) -> Prepared {
    let mut components = set.components.clone();
    let objects = component_mut(&mut components, "sym_comp")["objects"]
        .as_array_mut()
        .unwrap();
    for object in objects {
        for segment in object["geometry"]["segments"].as_array_mut().unwrap() {
            edit(segment);
        }
    }
    Prepared::from_json(&components, &set.scene, set.cameras.clone())
}

#[test]
fn handle_mapping_is_pinned_by_asymmetric_handles_and_a_swap_drop_or_absolute_handle_is_detected() {
    let sets = oracle();
    // The oracle is informative: the sets that move shapes carry non-zero asymmetric handles
    // (the fixture's own strokes have none), counted from the baseline rows themselves.
    let counts: Vec<usize> = sets.iter().map(handled_segments).collect();
    assert_eq!(counts, [0, 1248, 384, 84]);
    let swap = |segment: &mut Value| {
        let handle_in = segment["handleIn"].take();
        segment["handleIn"] = segment["handleOut"].take();
        segment["handleOut"] = handle_in;
    };
    let zero = |segment: &mut Value| {
        segment["handleIn"] = json!({ "x": 0, "y": 0 });
        segment["handleOut"] = json!({ "x": 0, "y": 0 });
    };
    let absolute = |segment: &mut Value| {
        for field in ["handleIn", "handleOut"] {
            for axis in ["x", "y"] {
                let sum = segment["point"][axis].as_f64().unwrap()
                    + segment[field][axis].as_f64().unwrap();
                segment[field][axis] = json!(sum);
            }
        }
    };
    let mutants: [(&str, &dyn Fn(&mut Value)); 3] =
        [("swap", &swap), ("zero", &zero), ("absolute", &absolute)];
    // placed: skewed instance matrix; camera: component camera, matrix on the second layer.
    for (set, uids) in [
        (&sets[1], vec!["ly_comp_loop"]),
        (&sets[2], vec!["ly_comp_loop", "ly_comp_ping"]),
    ] {
        let real = Prepared::of(set);
        for uid in uids {
            let want = &frame0_row(set, uid).want;
            assert_eq!(want["shapes"][0]["strokeId"], "s_comp_a_0");
            assert_eq!(diff(entry_of(&real.scene(0).1, uid), want), None);
            for (label, edit) in mutants {
                let mutated = with_segments_edited(set, edit);
                assert_eq!(
                    diff(entry_of(&mutated.scene(0).1, uid), want),
                    Some("geometry of s_comp_a_0".to_owned()),
                    "{} {label} {uid}",
                    set.name
                );
            }
        }
    }
}

#[test]
fn a_corrupted_oracle_expectation_is_detected() {
    let set = oracle().remove(0);
    let prepared = Prepared::of(&set);
    let expect = set
        .expects
        .iter()
        .find(|e| e.uid == "ly_comp_loop" && e.frame == 5)
        .unwrap();
    let (_, scene) = prepared.scene(5);
    let entry = entry_of(&scene, "ly_comp_loop");
    assert_eq!(diff(entry, &expect.want), None);
    // One ulp on a coordinate, a changed stroke id, a dropped shape, a changed inner frame.
    let mut ulp = expect.want.clone();
    let x = ulp["shapes"][0]["segments"][0]["point"][0]
        .as_f64()
        .unwrap();
    ulp["shapes"][0]["segments"][0]["point"][0] = json!(f64::from_bits(x.to_bits() + 1));
    assert_eq!(diff(entry, &ulp), Some("geometry of s_comp_a_1".to_owned()));
    let mut renamed = expect.want.clone();
    renamed["shapes"][1]["strokeId"] = json!("s_comp_x");
    assert!(diff(entry, &renamed).is_some());
    let mut dropped = expect.want.clone();
    dropped["shapes"].as_array_mut().unwrap().pop();
    assert_eq!(diff(entry, &dropped), Some("2 shapes != 1".to_owned()));
    let mut inner = expect.want.clone();
    inner["inner"] = json!(2);
    assert!(diff(entry, &inner).is_some());
    assert!(diff(entry, &json!({ "blank": true })).is_some());
}
