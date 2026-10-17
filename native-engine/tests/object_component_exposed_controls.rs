//! Controls for the exposed-property overrides: each baseline rule of the real kernel is switched
//! off through `OverrideRules` and the oracle must notice, naming the rows that catch it. The
//! unmutated rules reproduce every row first, so a miss is caused by the mutation alone.

use super::support::{record, rows, run, strings, view};
use super::{apply_overrides, OverrideRules, BASELINE_OVERRIDE_RULES};
use serde_json::{json, Value};
use std::borrow::Cow;

/// Ids of the `A` rows whose output, events or identity differ from the baseline under `rules`.
fn missed(rules: OverrideRules) -> Vec<String> {
    rows("A")
        .into_iter()
        .filter(|(scenario, expected)| {
            let ran = run(rules, scenario);
            let Ok((out, same)) = ran.result else {
                return true;
            };
            ran.events != strings(&expected["events"])
                || same != expected["same"].as_bool().unwrap()
                || Value::Array(out) != expected["out"]
        })
        .map(|(scenario, _)| scenario["id"].as_str().unwrap().to_string())
        .collect()
}

fn catches(rules: OverrideRules, row: &str) -> bool {
    missed(rules).iter().any(|id| id == row)
}

#[test]
fn the_unmutated_rules_reproduce_every_row() {
    assert_eq!(missed(BASELINE_OVERRIDE_RULES), Vec::<String>::new());
}

#[test]
fn threshold_fifty_versus_fifty_one_and_forty_nine() {
    let up = OverrideRules {
        hide_below: 51.0,
        ..BASELINE_OVERRIDE_RULES
    };
    let down = OverrideRules {
        hide_below: 49.0,
        ..BASELINE_OVERRIDE_RULES
    };
    assert!(catches(up, "visible-50"), "50 must stay visible");
    assert!(catches(down, "visible-49"), "49 must be hidden");
}

#[test]
fn a_wrong_opacity_divisor_is_caught() {
    for divisor in [10.0, 1000.0] {
        let rules = OverrideRules {
            opacity_divisor: divisor,
            ..BASELINE_OVERRIDE_RULES
        };
        assert!(catches(rules, "opacity-25"), "divisor {divisor}");
    }
}

#[test]
fn removing_the_clamp_is_caught_on_both_sides() {
    let rules = OverrideRules {
        clamp_opacity: false,
        ..BASELINE_OVERRIDE_RULES
    };
    assert!(catches(rules, "opacity-150"));
    assert!(catches(rules, "opacity--10"));
    assert!(
        !catches(rules, "opacity-25"),
        "inside the range the clamp is invisible"
    );
}

#[test]
fn a_hide_that_a_later_visible_value_can_undo_is_caught() {
    let rules = OverrideRules {
        sticky_hide: false,
        ..BASELINE_OVERRIDE_RULES
    };
    assert!(catches(rules, "hide-wins-visible-after"));
    assert!(
        !catches(rules, "hide-wins-visible-before"),
        "the order that happens to agree"
    );
}

#[test]
fn keeping_hidden_strokes_is_caught() {
    let rules = OverrideRules {
        drop_hidden: false,
        ..BASELINE_OVERRIDE_RULES
    };
    assert!(catches(rules, "visible-0"));
    assert!(catches(rules, "all-hidden"));
    assert!(catches(rules, "hide-wins-over-opacity-and-fill"));
}

#[test]
fn only_the_stable_stroke_id_addresses_a_record() {
    // layerUid and contextId equal to the declared id must not match; only target.strokeId does.
    let mut impostor = record(&json!({"id": "zz", "opacity": 0.5, "fill": "c1", "stroke": "c2"}));
    impostor.target.layer_uid = "a".to_string();
    impostor.target.context_id = "a".to_string();
    let real = record(&json!({"id": "a", "opacity": 0.5, "fill": "c1", "stroke": "c2"}));
    let scenario = json!({"frame": 0, "props": [{"key": "k0", "stroke": "a", "field": "opacity"}],
        "reads": {"k0": [10]}});
    let props = super::support::props_of(&scenario);
    let strokes = vec![impostor.clone(), real];
    let mut port = super::support::ScriptedPort::of(&scenario);
    let out = apply_overrides(&props, &strokes, "inst", 0, &mut port).unwrap();
    assert_eq!(
        out[0], impostor,
        "a record that only looks like the target is untouched"
    );
    assert_eq!(view(&out[1])["opacity"], json!(0.1));
    assert_eq!(
        port.events,
        ["available", "meta:k0", "read:k0@0"],
        "one read, for the real target"
    );
}

#[test]
fn the_stored_records_are_never_the_ones_that_change() {
    // The component's stored strokes are inputs by shared reference; the output is a copy.
    let stored = vec![record(
        &json!({"id": "a", "opacity": 0.5, "fill": "c1", "stroke": "c2"}),
    )];
    let before = stored.clone();
    let scenario = json!({"frame": 0, "props": [{"key": "k0", "stroke": "a", "field": "opacity"}],
        "reads": {"k0": [100]}});
    let props = super::support::props_of(&scenario);
    for _ in 0..2 {
        let mut port = super::support::ScriptedPort::of(&scenario);
        let out = apply_overrides(&props, &stored, "inst", 0, &mut port).unwrap();
        assert!(
            matches!(out, Cow::Owned(_)),
            "a matched override never borrows"
        );
        assert_eq!(view(&out[0])["opacity"], json!(1));
        assert_eq!(
            stored, before,
            "the stored record keeps its authored opacity"
        );
    }
    assert_eq!(view(&stored[0])["opacity"], json!(0.5));
}
