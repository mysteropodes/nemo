//! Oracle tests for the effector layers of a context (L04-L, I09).
//!
//! `fixtures/object-effector-layers/oracle.txt` was produced by
//! `tools/gen_oracle.cjs` of the L04-L leaf, which runs the WHOLE of
//! `src/js/effector-layer.js` at baseline `fbe4b8a3` verbatim in a Node `vm`
//! and calls `window.SMEffectorLayer.resolvedEffectorsFor` over a
//! `state.layers` array built from each scenario. The Motion lookups answer
//! from the scenario and are logged, so the rows cover the layer filter
//! (record, target, visibility, layer order), the origin, the property
//! defaults, the channel assembly AND the order and the set of Motion
//! properties the baseline asked for (a layer with no nonzero offset never
//! asks for its radius, strength or angle).
//!
//! The join runs over a real `DuplicatorSet`, real `ObjectLayer`s and a
//! provider that answers only what the baseline was asked.

use super::{
    effector_layers_for, resolved_effectors, EffectorScope, Unavailable, KEY_ANGLE,
    KEY_OFF_OPACITY, KEY_OFF_POSITION, KEY_OFF_POSITION_Z, KEY_OFF_ROTATION, KEY_OFF_ROTATION_X,
    KEY_OFF_ROTATION_Y, KEY_OFF_SCALE, KEY_RADIUS, KEY_STRENGTH,
};
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::test_codec::hex;
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::object_duplicator_evaluate::test_codec::{
    document_of, named, scenarios, Scenario, World,
};
use crate::evaluation::object_component_kernels::object_effector_falloff::Effector;
use crate::evaluation::object_component_kernels::object_effector_layer::test_codec::format_resolved_list;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-effector-layers/oracle.txt");

pub(crate) fn all() -> Vec<Scenario> {
    scenarios(ORACLE)
}

/// The join over a scenario's own document and provider.
pub(crate) fn resolve(sc: &Scenario) -> (Result<Vec<Effector>, Unavailable>, Vec<String>) {
    let (set, layers) = document_of(sc);
    let world = World::new(sc);
    let scope = EffectorScope {
        context_id: "scene-root",
        layers: &layers,
        set: &set,
        canvas_w: sc.canvas[0],
        canvas_h: sc.canvas[1],
    };
    let result = resolved_effectors(&scope, "host", &world);
    (result, world.distinct_calls())
}

#[test]
fn every_scenario_matches_resolved_effectors_for() {
    let scenarios = all();
    assert_eq!(scenarios.len(), 9);
    for sc in &scenarios {
        let (result, calls) = resolve(sc);
        let list = result.unwrap_or_else(|e| panic!("{}: {e:?}", sc.name));
        assert_eq!(
            Some(format_resolved_list(&list)),
            sc.effs,
            "{}: the resolved effectors",
            sc.name
        );
        assert_eq!(calls, sc.calls, "{}: what the provider was asked", sc.name);
    }
}

#[test]
fn the_property_keys_are_the_motion_keys_of_effector_layer_js() {
    assert_eq!(
        [KEY_RADIUS, KEY_STRENGTH, KEY_ANGLE],
        ["effRadius", "effStrength", "effAngle"]
    );
    assert_eq!(
        [
            KEY_OFF_POSITION,
            KEY_OFF_ROTATION,
            KEY_OFF_SCALE,
            KEY_OFF_OPACITY,
            KEY_OFF_POSITION_Z,
            KEY_OFF_ROTATION_X,
            KEY_OFF_ROTATION_Y
        ],
        [
            "effOffPosition",
            "effOffRotation",
            "effOffScale",
            "effOffOpacity",
            "effOffPositionZ",
            "effOffRotationX",
            "effOffRotationY"
        ]
    );
}

fn first_channel(e: &Effector) -> (String, Vec<f64>) {
    let c = &e.channels.as_ref().expect("channels")[0];
    let values = c.value.iter().map(|n| match n {
        JsNumber::Value(v) => *v,
        other => panic!("{other:?}"),
    });
    (c.prop.clone(), values.collect())
}

#[test]
fn visible_layers_aimed_at_the_host_resolve_in_layer_order() {
    let all = all();
    let sc = named(&all, "ef_two_layers_and_a_hidden_one");
    let (result, _) = resolve(sc);
    let list = result.expect("resolved");
    assert_eq!(list.len(), 2, "e2 is hidden");
    let origin = |e: &Effector| {
        let p = e.pos.expect("origin");
        (p.x, p.y)
    };
    assert_eq!(origin(&list[0]), (10.0, 20.0));
    assert_eq!(origin(&list[1]), (3.0, 4.0));
    assert_eq!(
        first_channel(&list[0]),
        ("position".to_string(), vec![5.0, 6.0])
    );
    assert_eq!(first_channel(&list[1]), ("rotation".to_string(), vec![9.0]));
    assert!(list.iter().all(|e| e.from_layer));
    let (set, layers) = document_of(sc);
    let scope = EffectorScope {
        context_id: "scene-root",
        layers: &layers,
        set: &set,
        canvas_w: 1920.0,
        canvas_h: 1080.0,
    };
    let found: Vec<&str> = effector_layers_for(&scope, "host")
        .iter()
        .map(|(layer, _)| layer.layer_uid())
        .collect();
    assert_eq!(found, ["e1", "e3"]);
    assert!(
        effector_layers_for(&scope, "").is_empty(),
        "an empty uid names nobody"
    );
    assert!(effector_layers_for(&scope, "someone-else").is_empty());
}

#[test]
fn the_origin_is_the_canvas_center_plus_the_motion_delta_with_the_baseline_defaults() {
    let all = all();
    let (result, _) = resolve(named(&all, "ef_motion_and_defaults"));
    let e = &result.expect("resolved")[0];
    let p = e.pos.expect("origin");
    // canvas 1000 x 500: (500, 250) + (12.5, -3.25)
    assert_eq!((p.x, p.y), (512.5, 246.75));
    let [radius, strength, angle] = [e.radius, e.strength, e.angle];
    let (v200, v100, v0) = (
        JsNumber::Value(200.0),
        JsNumber::Value(100.0),
        JsNumber::Value(0.0),
    );
    assert_eq!((radius, strength, angle), (v200, v100, v0));
    assert_eq!(first_channel(e), ("opacity".to_string(), vec![-5.0]));
    assert_eq!(hex("4069000000000000"), 200.0);
}

#[test]
fn a_layer_without_a_nonzero_offset_never_asks_for_its_radius_strength_or_angle() {
    let all = all();
    let sc = named(&all, "ef_no_channel");
    let (result, calls) = resolve(sc);
    assert_eq!(result, Ok(Vec::new()));
    assert_eq!(
        calls.len(),
        8,
        "the Motion position and the seven offsets: {calls:?}"
    );
    assert!(calls.iter().all(|c| !c.contains("effRadius")
        && !c.contains("effStrength")
        && !c.contains("effAngle")));
    // while one nonzero offset asks for them, in the baseline order
    let (_, asked) = resolve(named(&all, "ef_one_layer"));
    let tail: Vec<&str> = asked
        .iter()
        .rev()
        .take(3)
        .map(String::as_str)
        .rev()
        .collect();
    assert_eq!(
        tail,
        ["V e1 effRadius", "V e1 effStrength", "V e1 effAngle"]
    );
}

#[test]
fn a_zero_sample_is_kept_not_replaced_by_the_default() {
    let all = all();
    let (result, _) = resolve(named(&all, "ef_zero_samples_are_kept"));
    let e = &result.expect("resolved")[0];
    assert_eq!(
        (e.radius, e.strength),
        (JsNumber::Value(0.0), JsNumber::Value(0.0))
    );
}

#[test]
fn a_scope_in_another_context_finds_no_effector() {
    let all = all();
    let sc = named(&all, "ef_one_layer");
    let (set, layers) = document_of(sc);
    let world = World::new(sc);
    let scope = EffectorScope {
        context_id: "comp-1",
        layers: &layers,
        set: &set,
        canvas_w: 1920.0,
        canvas_h: 1080.0,
    };
    assert_eq!(resolved_effectors(&scope, "host", &world), Ok(Vec::new()));
    assert!(
        world.distinct_calls().is_empty(),
        "nothing is sampled for a foreign context"
    );
}
