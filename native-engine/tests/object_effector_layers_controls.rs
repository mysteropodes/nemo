//! Negative and mutation controls of the effector layer lookup (L04-L).
//!
//! Each control changes one input of a scenario the oracle test passes and
//! proves the real join then differs, or reports the exact typed gap.

use super::tests::{all, resolve};
use super::Unavailable;
use crate::evaluation::object_component_kernels::object_duplicator_evaluate::test_codec::named;

#[test]
fn a_hidden_layer_is_dropped_before_it_is_sampled() {
    let all = all();
    let sc = named(&all, "ef_two_layers_and_a_hidden_one");
    let mut edited = sc.clone();
    edited
        .layers
        .iter_mut()
        .find(|l| l.uid == "e1")
        .and_then(|l| l.effector.as_mut())
        .expect("e1")
        .visible = Some(false);
    let (before, _) = resolve(sc);
    let (after, calls) = resolve(&edited);
    assert_eq!(before.expect("resolved").len(), 2);
    assert_eq!(after.expect("resolved").len(), 1);
    assert!(calls.iter().all(|c| !c.contains(" e1")), "{calls:?}");
}

#[test]
fn an_effector_aimed_elsewhere_is_ignored() {
    let all = all();
    let sc = named(&all, "ef_one_layer");
    let mut edited = sc.clone();
    edited.layers[1].effector.as_mut().expect("effector").target = "elsewhere".to_string();
    let (result, calls) = resolve(&edited);
    assert_eq!(result, Ok(Vec::new()));
    assert!(calls.is_empty());
}

#[test]
fn layer_order_is_the_sampling_order() {
    let all = all();
    let sc = named(&all, "ef_two_layers_and_a_hidden_one");
    let mut edited = sc.clone();
    edited.layers.reverse();
    let (_, calls) = resolve(&edited);
    let first_motion = calls
        .iter()
        .find(|c| c.starts_with("M "))
        .expect("a motion call");
    assert_eq!(first_motion, "M e3");
    assert_eq!(
        sc.calls
            .iter()
            .find(|c| c.starts_with("M "))
            .map(String::as_str),
        Some("M e1")
    );
}

#[test]
fn a_provider_without_the_layer_motion_or_a_property_is_a_typed_gap() {
    let all = all();
    let sc = named(&all, "ef_one_layer");
    let mut no_motion = sc.clone();
    no_motion.motion.clear();
    assert_eq!(resolve(&no_motion).0, Err(Unavailable::LayerMotion));
    let mut no_scale = sc.clone();
    no_scale.values.retain(|(_, key, _)| key != "effOffScale");
    assert_eq!(
        resolve(&no_scale).0,
        Err(Unavailable::Property("effOffScale"))
    );
    let mut no_radius = sc.clone();
    no_radius.values.retain(|(_, key, _)| key != "effRadius");
    assert_eq!(
        resolve(&no_radius).0,
        Err(Unavailable::Property("effRadius"))
    );
}

#[test]
fn the_canvas_center_is_the_origin_of_an_anchorless_layer() {
    let all = all();
    let sc = named(&all, "ef_motion_and_defaults");
    let mut edited = sc.clone();
    edited.canvas = [2000.0, 1000.0];
    let before = resolve(sc).0.expect("resolved");
    let after = resolve(&edited).0.expect("resolved");
    assert_ne!(before[0].pos, after[0].pos);
    let p = after[0].pos.expect("origin");
    assert_eq!((p.x, p.y), (1012.5, 496.75));
}
