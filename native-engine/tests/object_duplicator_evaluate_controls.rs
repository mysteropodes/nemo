//! Negative and mutation controls of the duplicator evaluation join (L04-L).
//!
//! A control changes ONE input of a scenario the oracle sweep passes (the seed,
//! a random flag, a Motion value, an effector layer's visibility or order, the
//! canvas, the frame, the entry shape, the pool order) and proves that the
//! real join then produces a different result, or fails with the exact typed
//! error. The sweep compares everything against the baseline, so a join that
//! ignored one of these inputs could not pass both the sweep and its control.

use super::test_codec::{all, named, Scenario};
use super::test_compare::{copies_of, mismatch, run};
use super::{EvaluateError, Evaluation};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::object_duplicator_evaluate_spec::describe;
use crate::evaluation::object_component_kernels::object_effector_falloff::Falloff;
use crate::evaluation::object_component_kernels::object_effector_layers::Unavailable;
use crate::object_document::object_component_record_kernels::object_duplicator_records::DuplicatorDescriptor;
use serde_json::{json, Number, Value};

fn changed(sc: &Scenario, edit: impl FnOnce(&mut Scenario)) -> Scenario {
    let mut copy = sc.clone();
    edit(&mut copy);
    copy
}

fn number(x: f64) -> Value {
    Value::Number(Number::from_f64(x).expect("finite"))
}

/// The scenario passes the sweep, the changed one does not give the same result.
fn must_differ(sc: &Scenario, mutated: &Scenario) {
    assert!(
        mismatch(sc, &run(sc)).is_none(),
        "{}: the original must pass",
        sc.name
    );
    assert_ne!(
        run(sc).result,
        run(mutated).result,
        "{}: the change was not noticed",
        sc.name
    );
    assert!(mismatch(sc, &run(mutated)).is_some(), "{}", sc.name);
}

#[test]
fn another_seed_moves_the_random_clones() {
    let all = all();
    let sc = named(&all, "grid_all_random_6_clones");
    let other = changed(sc, |s| {
        s.dupj.as_mut().expect("descriptor")["seed"] = number(78.0)
    });
    must_differ(sc, &other);
}

#[test]
fn a_random_flag_turned_off_changes_the_pattern() {
    let all = all();
    let sc = named(&all, "grid_all_random_6_clones");
    let other = changed(sc, |s| {
        s.dupj.as_mut().expect("descriptor")["staggerRandom"]["position"] = json!(false)
    });
    must_differ(sc, &other);
}

#[test]
fn a_provider_that_reports_zero_offsets_is_noticed() {
    let all = all();
    let sc = named(&all, "grid_offsets_all_channels");
    let other = changed(sc, |s| {
        for (uid, _, value) in s.values.iter_mut() {
            if uid == "host" {
                *value = Some(vec![JsNumber::Value(0.0); 2]);
            }
        }
    });
    must_differ(sc, &other);
}

#[test]
fn a_hidden_effector_layer_is_skipped_and_never_sampled() {
    let all = all();
    let sc = named(&all, "el_two_layers_in_layer_order");
    let other = changed(sc, |s| {
        let layer = s.layers.iter_mut().find(|l| l.uid == "e1").expect("e1");
        layer.effector.as_mut().expect("effector").visible = Some(false);
    });
    must_differ(sc, &other);
    assert!(!run(&other).calls.iter().any(|c| c.contains("e1")));
}

#[test]
fn a_target_elsewhere_removes_the_effector() {
    let all = all();
    let sc = named(&all, "el_position_offset");
    let other = changed(sc, |s| {
        s.layers[1].effector.as_mut().expect("effector").target = "elsewhere".to_string();
    });
    must_differ(sc, &other);
    assert!(!run(&other).calls.iter().any(|c| c.contains("e1")));
}

#[test]
fn layer_order_is_the_order_the_provider_is_asked_in() {
    let all = all();
    let sc = named(&all, "el_two_layers_in_layer_order");
    let reversed = changed(sc, |s| s.layers.reverse());
    let asked = run(&reversed).calls;
    assert_ne!(asked, sc.calls);
    let mut a = asked.clone();
    let mut b = sc.calls.clone();
    a.sort();
    b.sort();
    assert_eq!(a, b, "the same calls, in another order");
    let first = |calls: &[String]| calls.iter().find(|c| c.starts_with("M ")).cloned();
    assert_eq!(first(&sc.calls).as_deref(), Some("M e1"));
    assert_eq!(first(&asked).as_deref(), Some("M e2"));
}

#[test]
fn an_effector_origin_without_an_anchor_follows_the_canvas_size() {
    let all = all();
    let sc = named(&all, "el_origin_defaults_to_the_canvas_centre");
    let other = changed(sc, |s| s.canvas = [1920.0, 1080.0]);
    must_differ(sc, &other);
}

#[test]
fn the_temporal_stagger_reads_the_frame_it_was_given() {
    let all = all();
    let sc = named(&all, "tmp_forward");
    let other = changed(sc, |s| s.frame += 1.0);
    must_differ(sc, &other);
}

#[test]
fn without_a_resample_port_the_stagger_is_off() {
    let all = all();
    let sc = named(&all, "direct_resample_span");
    let other = changed(sc, |s| s.can_resample = false);
    must_differ(sc, &other);
    assert!(!run(&other).calls.iter().any(|c| c.starts_with("S host")));
}

#[test]
fn the_edit_source_flag_returns_the_base_on_the_rendered_entry_only() {
    let all = all();
    let rendered = changed(named(&all, "grid_1x4_spacing"), |s| s.edit = true);
    assert!(matches!(
        run(&rendered).result,
        Ok(Evaluation::Unchanged(super::UnchangedReason::EditingSource))
    ));
    assert!(run(&rendered).calls.is_empty());
    // the direct entry has no such guard: the same flag changes nothing there
    let direct = named(&all, "direct_ignores_the_edit_flag");
    assert!(direct.edit);
    assert!(mismatch(direct, &run(direct)).is_none());
    assert!(!copies_of(&run(direct)).copies.is_empty());
}

#[test]
fn the_pool_order_decides_which_member_fills_a_slot() {
    let all = all();
    let sc = named(&all, "pool_two_members");
    let other = changed(sc, |s| {
        s.dupj.as_mut().expect("descriptor")["sourceLayerUids"] = json!(["m2", "m1"])
    });
    must_differ(sc, &other);
    let first = |s: &Scenario| {
        copies_of(&run(s)).copies[0]
            .record
            .target
            .stroke_id()
            .to_string()
    };
    assert_eq!(first(sc), "m1a");
    assert_eq!(first(&other), "m2a");
}

#[test]
fn a_mode_label_selects_the_placement_branch() {
    let all = all();
    let sc = named(&all, "grid_2x3");
    let other = changed(sc, |s| {
        s.dupj.as_mut().expect("descriptor")["mode"] = json!("radial")
    });
    must_differ(sc, &other);
}

#[test]
fn a_property_the_provider_cannot_serve_is_a_typed_gap_not_a_zero() {
    let all = all();
    let sc = named(&all, "grid_2x3");
    let other = changed(sc, |s| {
        s.values
            .retain(|(uid, key, _)| !(uid == "host" && key == "dupOffsetRot"))
    });
    assert_eq!(
        run(&other).result,
        Err(EvaluateError::Unavailable(Unavailable::Property(
            "dupOffsetRot"
        )))
    );
    // nothing after the missing key was read: the keys before it were, in the baseline order
    let asked = run(&other).calls;
    assert_eq!(
        asked.last().map(String::as_str),
        Some("V host dupOffsetRot")
    );
    assert!(asked.contains(&"V host dupOffsetPos".to_string()));
}

#[test]
fn a_null_host_value_is_the_baselines_type_error() {
    let all = all();
    let sc = named(&all, "grid_2x3");
    let other = changed(sc, |s| {
        for (uid, key, value) in s.values.iter_mut() {
            if uid == "host" && key == "dupOffsetHue" {
                *value = None;
            }
        }
    });
    assert_eq!(
        run(&other).result,
        Err(EvaluateError::NullValue("dupOffsetHue"))
    );
}

#[test]
fn a_missing_layer_motion_or_effector_property_is_a_typed_gap() {
    let all = all();
    let sc = named(&all, "el_position_offset");
    let no_motion = changed(sc, |s| s.motion.retain(|(uid, _)| uid != "e1"));
    assert_eq!(
        run(&no_motion).result,
        Err(EvaluateError::Unavailable(Unavailable::LayerMotion))
    );
    let no_key = changed(sc, |s| {
        s.values
            .retain(|(uid, key, _)| !(uid == "e1" && key == "effOffRotation"))
    });
    assert_eq!(
        run(&no_key).result,
        Err(EvaluateError::Unavailable(Unavailable::Property(
            "effOffRotation"
        )))
    );
}

fn descriptor(value: Value) -> DuplicatorDescriptor {
    serde_json::from_value(value).expect("a decodable descriptor")
}

#[test]
fn the_descriptor_view_keeps_absent_null_and_explicit_values_apart() {
    let absent = descriptor(json!({}));
    let null = descriptor(json!({ "endAngle": null }));
    let zero = descriptor(json!({ "endAngle": 0, "seed": 0, "rows": 0 }));
    assert_eq!(describe(&absent).spec.end_angle, JsNumber::Undefined);
    assert_eq!(describe(&null).spec.end_angle, JsNumber::Null);
    assert_eq!(describe(&zero).spec.end_angle, JsNumber::Value(0.0));
    assert_eq!(describe(&absent).seed, JsNumber::Undefined);
    assert_eq!(describe(&zero).seed, JsNumber::Value(0.0));
    assert_eq!(describe(&zero).spec.rows, JsNumber::Value(0.0));
    assert_eq!(describe(&absent).spec.rows, JsNumber::Undefined);
    assert_eq!(describe(&absent).spec.mode, None);
    assert!(!describe(&absent).objects_mode);
    assert!(describe(&absent).temporal.is_none());
    assert!(describe(&absent).pool_uids.is_empty());
}

#[test]
fn the_descriptor_view_reads_the_temporal_and_random_blocks() {
    let d = descriptor(json!({
        "timeOffset": { "enabled": true, "direction": "centerOut" },
        "staggerRandom": { "rotation": true },
        "sourceMode": "objects",
        "sourceLayerUids": ["a", "b"],
    }));
    let view = describe(&d);
    let t = view.temporal.expect("a time offset block");
    assert!(t.enabled);
    assert!(
        t.offset_frames.is_nan(),
        "an absent frame count reads as undefined"
    );
    assert_eq!(t.direction, Some("centerOut"));
    assert!(view.rand.rotation && !view.rand.position && !view.rand.hue);
    assert!(view.objects_mode);
    assert_eq!(view.pool_uids, ["a".to_string(), "b".to_string()]);
}

#[test]
fn inline_effectors_keep_null_and_empty_channels_apart() {
    let d = descriptor(json!({ "effectors": [
        { "channels": null, "offsetRot": 5 },
        { "channels": [], "offsetRot": 5 },
        { "channels": [{ "prop": "position" }], "falloff": "step" },
    ]}));
    let view = describe(&d);
    let e = &view.inline_effectors;
    assert_eq!(e.len(), 3);
    assert!(
        e[0].channels.is_none(),
        "null channels migrate the legacy fields"
    );
    assert_eq!(e[1].channels.as_ref().map(Vec::len), Some(0));
    let channel = &e[2].channels.as_ref().expect("channels")[0];
    assert_eq!(channel.prop, "position");
    assert!(
        channel.value.is_empty(),
        "an absent value is the empty list"
    );
    assert_eq!(e[2].falloff, Falloff::Step);
    assert!(e.iter().all(|x| !x.from_layer));
    assert_eq!(e[0].offset_rot, JsNumber::Value(5.0));
}
