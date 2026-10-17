//! Hand-written literal controls for the reorder and delete plans (L02-E).
//!
//! Every expectation below is computed by hand from the fixture documented in
//! `object_layer_lifecycle_plan.rs` (`build`): layer `L<i>` has natural slot `i`
//! in context 0 and `n-1-i` in context 1. They complement the oracle replay with
//! values the baseline rows cannot show (order-plan slots, objects, holders,
//! scope elements) and with the typed refusals, each of which must leave the input
//! document byte for byte untouched.

use super::fixture::{
    admitted, build, build_value, json_of, layer_uids, parse, plain, slot_of, text_of,
};
use super::{
    plan_delete, plan_reorder, LayerSelection, LifecycleError, UnavailableLayerEditPolicy as Nobody,
};
use serde_json::json;

fn pick(active: &str, selected: &[&str], anchor: Option<&str>) -> LayerSelection {
    LayerSelection {
        active: active.to_string(),
        selected: selected.iter().map(|s| s.to_string()).collect(),
        anchor: anchor.map(str::to_string),
    }
}

fn names(plan_layers: &[String]) -> Vec<&str> {
    plan_layers.iter().map(String::as_str).collect()
}

#[test]
fn reorder_block_move_reseats_every_context() {
    let before = build(&plain(4), 2);
    let selection = pick("L0", &["L0", "L2"], Some("L2"));
    let plan = plan_reorder(&before, &[0], 3.0, &selection, &Nobody).expect("reorder");
    assert!(plan.changed && admitted(&plan.document));
    assert_eq!(names(&layer_uids(&plan.document)), ["L1", "L2", "L0", "L3"]);
    let want = [("L1", 0, 3), ("L2", 1, 2), ("L0", 2, 1), ("L3", 3, 0)];
    for (layer, base, mirror) in want {
        assert_eq!(
            slot_of(&plan.document, 0, layer),
            base,
            "scene-root slot of {layer}"
        );
        assert_eq!(
            slot_of(&plan.document, 1, layer),
            mirror,
            "mirror slot of {layer}"
        );
    }
    let outcome = plan.selection.expect("selection");
    assert_eq!(
        (outcome.active, outcome.selected, outcome.anchor),
        (2, vec![2, 1], Some(1))
    );
    assert_eq!(
        json_of(&plan.document)["objects"],
        json_of(&before)["objects"]
    );
    assert_eq!(
        text_of(&before),
        text_of(&build(&plain(4), 2)),
        "input untouched"
    );
}

#[test]
fn reorder_selection_follows_the_layer_not_the_slot() {
    let before = build(&plain(3), 2);
    let selection = pick("L2", &["L0", "L2"], Some("L0"));
    let plan = plan_reorder(&before, &[2], 0.0, &selection, &Nobody).expect("reorder");
    assert_eq!(names(&layer_uids(&plan.document)), ["L2", "L0", "L1"]);
    let outcome = plan.selection.expect("selection");
    assert_eq!(
        (outcome.active, outcome.selected, outcome.anchor),
        (0, vec![1, 0], Some(1))
    );
}

#[test]
fn reorder_into_its_own_gap_is_a_byte_identical_noop() {
    let before = build(&plain(4), 2);
    let cases: [(&[i64], f64); 7] = [
        (&[1, 2], 1.0),
        (&[1, 2], 2.0),
        (&[1, 2], 3.0),
        (&[0], 0.0),
        (&[0], 1.0),
        (&[3], 4.0),
        (&[0, 1, 2, 3], 2.0),
    ];
    for (moving, gap) in cases {
        let selection = pick("L1", &["L1"], None);
        let plan = plan_reorder(&before, moving, gap, &selection, &Nobody).expect("noop");
        assert!(!plan.changed, "{moving:?} at {gap}");
        assert_eq!(
            text_of(&plan.document),
            text_of(&before),
            "{moving:?} at {gap}"
        );
        let outcome = plan.selection.expect("selection");
        assert_eq!(
            (outcome.active, outcome.selected),
            (1, vec![1]),
            "{moving:?} at {gap}"
        );
    }
}

#[test]
fn reorder_gap_rounds_like_math_round_and_clamps() {
    let before = build(&plain(4), 2);
    let run = |moving: &[i64], gap: f64| {
        let plan = plan_reorder(&before, moving, gap, &pick("L0", &[], None), &Nobody);
        layer_uids(&plan.expect("reorder").document)
    };
    assert_eq!(
        names(&run(&[3], 0.49999999999999994)),
        ["L3", "L0", "L1", "L2"]
    );
    assert_eq!(names(&run(&[3], -0.5)), ["L3", "L0", "L1", "L2"]);
    assert_eq!(names(&run(&[0], 1.5)), ["L1", "L0", "L2", "L3"]);
    assert_eq!(names(&run(&[0], 2.5)), ["L1", "L2", "L0", "L3"]);
    assert_eq!(names(&run(&[0], f64::INFINITY)), ["L1", "L2", "L3", "L0"]);
    assert_eq!(
        names(&run(&[3], f64::NEG_INFINITY)),
        ["L3", "L0", "L1", "L2"]
    );
    assert_eq!(
        names(&run(&[2, 0, 2, 9, -1], 4.0)),
        ["L1", "L3", "L0", "L2"]
    );
}

#[test]
fn reorder_refusals_name_the_cause_and_leave_the_input_alone() {
    let before = build(&plain(3), 2);
    let bytes = text_of(&before);
    let unknown = |id: &str| LifecycleError::UnknownLayer(id.to_string());
    let cases = [
        (pick("zz", &[], None), 1.0, unknown("zz")),
        (pick("L0", &["L1", "zz"], None), 1.0, unknown("zz")),
        (pick("L0", &[], Some("zz")), 1.0, unknown("zz")),
        (
            pick("L0", &[], None),
            f64::NAN,
            LifecycleError::NonFiniteGap,
        ),
    ];
    for (selection, gap, want) in cases {
        let got = plan_reorder(&before, &[2], gap, &selection, &Nobody);
        assert_eq!(got.expect_err("refusal"), want);
        assert_eq!(text_of(&before), bytes);
    }
}

#[test]
fn reorder_needs_every_context_to_list_every_layer() {
    let mut value = build_value(&plain(3), 2);
    let layers = value["compositionOrder"]["contexts"][1]["layers"]
        .as_array_mut()
        .expect("layers");
    layers.retain(|entry| entry["layerUid"] != "L1");
    let broken = parse(value);
    let got = plan_reorder(&broken, &[2], 0.0, &pick("L0", &[], None), &Nobody);
    let want = LifecycleError::OrderPlanIncomplete {
        context_id: "unused-symbol".to_string(),
        layer_uid: "L1".to_string(),
    };
    assert_eq!(got.expect_err("refusal"), want);
}

#[test]
fn delete_removes_layer_content_and_compacts_every_context() {
    let before = build(&plain(4), 2);
    let selection = pick("L3", &["L1", "L2"], None);
    let plan = plan_delete(&before, &selection, &Nobody).expect("delete");
    assert_eq!(names(&layer_uids(&plan.document)), ["L0", "L3"]);
    assert!(admitted(&plan.document));
    assert_eq!(plan.removed, ["L1", "L2"]);
    assert_eq!(plan.spared, None);
    let outcome = plan.selection.expect("selection");
    assert_eq!(
        (outcome.active, outcome.selected, outcome.anchor),
        (1, vec![1], Some(1))
    );
    assert_eq!(
        plan.document.objects.len(),
        3,
        "L0 frame 0, L3 frames 0 and 1"
    );
    let after = json_of(&plan.document);
    assert_eq!(
        after["propertyHolders"].as_array().expect("holders").len(),
        3
    );
    let want = [("L0", 0, 1), ("L3", 1, 0)];
    for (layer, base, mirror) in want {
        assert_eq!(
            slot_of(&plan.document, 0, layer),
            base,
            "scene-root slot of {layer}"
        );
        assert_eq!(
            slot_of(&plan.document, 1, layer),
            mirror,
            "mirror slot of {layer}"
        );
    }
    let scopes = &after["compositionOrder"]["contexts"][0]["scopes"];
    let frame0: Vec<&str> = scopes[0]["elements"]
        .as_array()
        .expect("elements")
        .iter()
        .map(|e| e["layerUid"].as_str().expect("uid"))
        .collect();
    let frame1: Vec<&str> = scopes[1]["elements"]
        .as_array()
        .expect("elements")
        .iter()
        .map(|e| e["layerUid"].as_str().expect("uid"))
        .collect();
    assert_eq!((frame0, frame1), (vec!["L0", "L3"], vec!["L3"]));
    assert_eq!(
        text_of(&before),
        text_of(&build(&plain(4), 2)),
        "input untouched"
    );
}

#[test]
fn delete_keeps_the_active_slot_number_and_clamps_it() {
    let before = build(&plain(4), 2);
    let lower = plan_delete(&before, &pick("L1", &[], None), &Nobody).expect("delete");
    assert_eq!(names(&layer_uids(&lower.document)), ["L0", "L2", "L3"]);
    let outcome = lower.selection.expect("selection");
    assert_eq!(
        (outcome.active, outcome.selected, outcome.anchor),
        (1, vec![1], Some(1))
    );
    let below = plan_delete(&before, &pick("L2", &["L0"], None), &Nobody).expect("delete");
    assert_eq!(names(&layer_uids(&below.document)), ["L1", "L2", "L3"]);
    assert_eq!(
        below.selection.expect("selection").active,
        2,
        "slot 2 now holds L3"
    );
    let top = plan_delete(&before, &pick("L3", &[], None), &Nobody).expect("delete");
    assert_eq!(names(&layer_uids(&top.document)), ["L0", "L1", "L2"]);
    assert_eq!(
        top.selection.expect("selection").active,
        2,
        "clamped to the last slot"
    );
}

#[test]
fn delete_selecting_every_layer_spares_the_lowest_slot() {
    let before = build(&plain(3), 2);
    let plan =
        plan_delete(&before, &pick("L1", &["L0", "L1", "L2"], None), &Nobody).expect("delete");
    assert_eq!(names(&layer_uids(&plan.document)), ["L0"]);
    assert_eq!(plan.removed, ["L1", "L2"]);
    assert_eq!(plan.spared.as_deref(), Some("L0"));
    assert_eq!(plan.selection.expect("selection").active, 0);
}

#[test]
fn delete_counts_a_repeated_identity_once() {
    let before = build(&plain(3), 2);
    let plan = plan_delete(&before, &pick("L0", &["L1", "L1"], None), &Nobody).expect("delete");
    assert_eq!(names(&layer_uids(&plan.document)), ["L0", "L2"]);
    assert_eq!(plan.removed, ["L1"]);
}

#[test]
fn delete_refusals_leave_the_input_alone() {
    let single = build(&plain(1), 2);
    let many = build(&plain(3), 2);
    let (single_bytes, many_bytes) = (text_of(&single), text_of(&many));
    let last = plan_delete(&single, &pick("L0", &[], None), &Nobody);
    assert_eq!(last.expect_err("refusal"), LifecycleError::LastLayer);
    let gone = LifecycleError::UnknownLayer("zz".to_string());
    for selection in [pick("zz", &[], None), pick("L0", &["L1", "zz"], None)] {
        let got = plan_delete(&many, &selection, &Nobody);
        assert_eq!(got.expect_err("refusal"), gone);
    }
    assert_eq!(
        (text_of(&single), text_of(&many)),
        (single_bytes, many_bytes)
    );
}

#[test]
fn plans_without_an_order_plan_leave_it_absent() {
    let before = build(&plain(3), 1);
    assert!(before.composition_order.value().is_none());
    let deleted = plan_delete(&before, &pick("L2", &["L1"], None), &Nobody).expect("delete");
    assert!(deleted.document.composition_order.value().is_none());
    assert_eq!(names(&layer_uids(&deleted.document)), ["L0", "L2"]);
    let moved = plan_reorder(&before, &[0], 3.0, &pick("L0", &[], None), &Nobody).expect("reorder");
    assert!(moved.document.composition_order.value().is_none());
    assert_eq!(names(&layer_uids(&moved.document)), ["L1", "L2", "L0"]);
    let wire = json_of(&moved.document);
    assert!(wire.get("compositionOrder").is_none(), "{wire}");
    assert_eq!(wire["formatVersion"], json!(1));
}
