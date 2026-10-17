//! Oracle tests for the exposed-property overrides (L04-E, I14/I35). The rows and the shared builders
//! are described in `object_component_exposed_support.rs`; every `A` row is the baseline
//! `applyOverrides` output (events, identity, output strokes) and every `D` row is a baseline output
//! the native kernel refuses with a typed error instead of reproducing.

use std::borrow::Cow;

use super::support::{
    record, rows, run, run_with, strings, view, ScriptedPort, BASELINE, BLOB, ORACLE,
};
use super::{apply_overrides, ExposedError, BASELINE_OVERRIDE_RULES};
use serde_json::{json, Value};

fn id_of(scenario: &Value) -> &str {
    scenario["id"].as_str().expect("id")
}

#[test]
fn fixture_pins_the_baseline_and_its_row_counts() {
    assert!(ORACLE.contains(BASELINE));
    assert!(ORACLE.contains(&format!(
        "src/js/domain/component/exposed-properties.js blob {BLOB}"
    )));
    assert_eq!(rows("A").len(), 51);
    assert_eq!(rows("D").len(), 8);
    assert_eq!(rows("E").len(), 21);
}

#[test]
fn every_oracle_row_is_reproduced() {
    for (scenario, expected) in rows("A") {
        let ran = run(BASELINE_OVERRIDE_RULES, &scenario);
        let (out, same) = ran
            .result
            .unwrap_or_else(|e| panic!("{}: {e:?}", id_of(&scenario)));
        assert_eq!(
            ran.events,
            strings(&expected["events"]),
            "{} events",
            id_of(&scenario)
        );
        assert_eq!(
            same,
            expected["same"].as_bool().unwrap(),
            "{} identity",
            id_of(&scenario)
        );
        assert_eq!(
            Value::Array(out),
            expected["out"],
            "{} output",
            id_of(&scenario)
        );
    }
}

#[test]
fn opacity_pins_are_hand_written_literals() {
    // value -> stored opacity: clamp(value / 100, 0, 1), written as the JSON number JS would write
    let cases: [(f64, Value); 8] = [
        (-10.0, json!(0)),
        (0.0, json!(0)),
        (25.0, json!(0.25)),
        (50.0, json!(0.5)),
        (100.0, json!(1)),
        (150.0, json!(1)),
        (f64::INFINITY, json!(1)),
        (f64::NEG_INFINITY, json!(0)),
    ];
    for (value, opacity) in cases {
        let encoded = if value.is_finite() {
            json!(value)
        } else if value > 0.0 {
            json!("n:Infinity")
        } else {
            json!("n:-Infinity")
        };
        let scenario = json!({"id": "pin", "frame": 4,
            "props": [{"key": "k0", "stroke": "a", "field": "opacity"}],
            "strokes": [{"id": "a", "opacity": 0.7, "fill": "c1", "stroke": "c2"}],
            "reads": {"k0": [encoded]}});
        let (out, same) = run(BASELINE_OVERRIDE_RULES, &scenario).result.unwrap();
        assert!(!same);
        assert_eq!(out[0]["opacity"], opacity, "value {value}");
    }
}

#[test]
fn the_hide_threshold_is_exactly_fifty() {
    for (value, hidden) in [
        (49.0, true),
        (49.999_999, true),
        (50.0, false),
        (50.000_001, false),
        (f64::NEG_INFINITY, true),
    ] {
        let encoded = if value.is_finite() {
            json!(value)
        } else {
            json!("n:-Infinity")
        };
        let scenario = json!({"id": "t", "frame": 0,
            "props": [{"key": "k0", "stroke": "a", "field": "__visible"}],
            "strokes": [{"id": "a", "opacity": 1, "fill": "c1", "stroke": "c2"}],
            "reads": {"k0": [encoded]}});
        let (out, _) = run(BASELINE_OVERRIDE_RULES, &scenario).result.unwrap();
        assert_eq!(out.is_empty(), hidden, "visibility {value}");
    }
    // NaN compares false: the stroke stays
    let nan = json!({"id": "t", "frame": 0,
        "props": [{"key": "k0", "stroke": "a", "field": "__visible"}],
        "strokes": [{"id": "a", "opacity": 1, "fill": "c1", "stroke": "c2"}],
        "reads": {"k0": ["n:NaN"]}});
    assert_eq!(
        run(BASELINE_OVERRIDE_RULES, &nan).result.unwrap().0.len(),
        1
    );
}

#[test]
fn reads_follow_the_stroke_grouping_not_the_declaration_order() {
    let scenario = json!({"id": "o", "frame": 4,
        "props": [{"key": "k0", "stroke": "b", "field": "opacity"},
                  {"key": "k1", "stroke": "a", "field": "opacity"},
                  {"key": "k2", "stroke": "b", "field": "fillColor"}],
        "strokes": [{"id": "a", "opacity": 1, "fill": "c1", "stroke": "c2"},
                    {"id": "b", "opacity": 1, "fill": "c1", "stroke": "c2"}],
        "reads": {"k0": [10], "k1": [20], "k2": ["c3"]}});
    let ran = run(BASELINE_OVERRIDE_RULES, &scenario);
    assert_eq!(
        ran.events,
        [
            "available",
            "meta:k0",
            "meta:k1",
            "meta:k2",
            "read:k1@4",
            "read:k0@4",
            "read:k2@4"
        ]
    );
}

#[test]
fn nothing_declared_or_unavailable_returns_the_very_input_slice() {
    let strokes = vec![
        record(&json!({"id": "a", "opacity": 1, "fill": "c1", "stroke": "c2"})),
        record(&json!({"id": "b", "opacity": 1, "fill": "c1", "stroke": "c2"})),
    ];
    let mut port = ScriptedPort::of(&json!({}));
    let out = apply_overrides(&[], &strokes, "inst", 0, &mut port).unwrap();
    assert!(matches!(out, Cow::Borrowed(_)));
    assert!(std::ptr::eq(out.as_ptr(), strokes.as_ptr()));
    assert!(port.events.is_empty(), "an empty list calls no port");

    let scenario = json!({"frame": 0, "available": false,
        "props": [{"key": "k0", "stroke": "a", "field": "opacity"}], "reads": {"k0": [10]}});
    let props = super::support::props_of(&scenario);
    let mut port = ScriptedPort::of(&scenario);
    let out = apply_overrides(&props, &strokes, "inst", 0, &mut port).unwrap();
    assert!(std::ptr::eq(out.as_ptr(), strokes.as_ptr()));
    assert_eq!(
        port.events,
        ["available"],
        "unavailable stops before any registration"
    );
}

#[test]
fn an_active_list_returns_a_detached_copy_even_when_nothing_matches() {
    let strokes = vec![record(
        &json!({"id": "a", "opacity": 0.5, "fill": "c1", "stroke": "c2"}),
    )];
    let scenario = json!({"frame": 0, "props": [{"key": "k0", "stroke": "zz", "field": "opacity"}],
        "reads": {"k0": [10]}});
    let props = super::support::props_of(&scenario);
    let mut port = ScriptedPort::of(&scenario);
    let out = apply_overrides(&props, &strokes, "inst", 0, &mut port).unwrap();
    assert!(matches!(out, Cow::Owned(_)));
    assert!(!std::ptr::eq(out.as_ptr(), strokes.as_ptr()));
    assert_eq!(&*out, strokes.as_slice());
    assert_eq!(
        port.events,
        ["available", "meta:k0"],
        "no read for a declaration nothing matches"
    );
}

#[test]
fn legacy_records_take_fill_and_visibility_only() {
    let legacy = json!({"id": "a", "legacy": true, "fill": "c1"});
    let run_field = |field: &str, read: Value| {
        let scenario = json!({"id": "l", "frame": 0, "props": [{"key": "k0", "stroke": "a", "field": field}],
            "strokes": [legacy.clone()], "reads": {"k0": [read]}});
        run(BASELINE_OVERRIDE_RULES, &scenario)
    };
    let fill = run_field("fillColor", json!("c3"));
    assert_eq!(fill.result.unwrap().0, [json!({"id": "a", "fill": "c3"})]);
    let opacity = run_field("opacity", json!(50));
    assert_eq!(
        opacity.result,
        Err(ExposedError::NeedsVectorBody {
            key: "k0".into(),
            field: "opacity".into()
        })
    );
    assert_eq!(
        opacity.events,
        ["available", "meta:k0"],
        "refused before any read"
    );
    let stroke = run_field("strokeColor", json!("c3"));
    assert_eq!(
        stroke.result,
        Err(ExposedError::NeedsVectorBody {
            key: "k0".into(),
            field: "strokeColor".into()
        })
    );
    let no_fill = run_field("fillColor", Value::Null);
    assert_eq!(
        no_fill.result,
        Err(ExposedError::FillRequired { key: "k0".into() })
    );
}

#[test]
fn the_refused_baseline_outputs_are_pinned_and_refused() {
    let s = |text: &str| text.to_string();
    for (scenario, expected) in rows("D") {
        let id = id_of(&scenario).to_string();
        let ran = run(BASELINE_OVERRIDE_RULES, &scenario);
        let base = &expected["out"][0];
        // what the baseline produced (the reason for the typed refusal)
        let (want_err, baseline_note) = match id.as_str() {
            "d-empty-read-opacity" => (
                ExposedError::NoFirstComponent { key: s("k0") },
                base["opacity"].is_null(),
            ),
            "d-empty-read-visible" => (
                ExposedError::NoFirstComponent { key: s("k0") },
                base["id"] == "a",
            ),
            "d-nan-opacity" => (
                ExposedError::NonFiniteOpacity { key: s("k0") },
                base["opacity"].is_null(),
            ),
            "d-number-into-fill" => (
                ExposedError::ValueKind {
                    key: s("k0"),
                    field: s("fillColor"),
                },
                base["fill"] == 42,
            ),
            "d-colour-into-opacity" => (
                ExposedError::ValueKind {
                    key: s("k0"),
                    field: s("opacity"),
                },
                base["opacity"].is_null(),
            ),
            "d-colour-into-visible" => (
                ExposedError::ValueKind {
                    key: s("k0"),
                    field: s("__visible"),
                },
                base["id"] == "a",
            ),
            "d-stroke-null-on-real-stroke" => (
                ExposedError::NoStrokePaint { key: s("k0") },
                base["stroke"] == "#fff",
            ),
            "d-unsupported-field" => (
                ExposedError::UnsupportedField {
                    key: s("k0"),
                    field: s("strokeWidth"),
                },
                base["id"] == "a",
            ),
            other => panic!("unexpected divergence row {other}"),
        };
        assert!(
            baseline_note,
            "{id}: the baseline output is what the note says"
        );
        assert_eq!(ran.result, Err(want_err), "{id}");
    }
}

#[test]
fn a_failing_value_port_is_a_typed_error_and_stops_the_pass() {
    let scenario = json!({"id": "f", "frame": 2,
        "props": [{"key": "k0", "stroke": "a", "field": "opacity"},
                  {"key": "k1", "stroke": "b", "field": "opacity"}],
        "strokes": [{"id": "a", "opacity": 1, "fill": "c1", "stroke": "c2"},
                    {"id": "b", "opacity": 1, "fill": "c1", "stroke": "c2"}],
        "reads": {"k0": [10], "k1": [20]}});
    let mut port = ScriptedPort::of(&scenario);
    port.fail_key = Some("k0".to_string());
    let ran = run_with(BASELINE_OVERRIDE_RULES, &scenario, &mut port);
    assert_eq!(
        ran.result,
        Err(ExposedError::Port {
            key: "k0".into(),
            message: "value port failed".into()
        })
    );
    assert_eq!(ran.events, ["available", "meta:k0", "meta:k1", "read:k0@2"]);
}

#[test]
fn views_round_trip_the_builder() {
    let spec = json!({"id": "a", "opacity": 0.25, "fill": "c4", "stroke": "c1"});
    assert_eq!(
        view(&record(&spec)),
        json!({"id": "a", "opacity": 0.25, "fill": "c4", "stroke": "c1"})
    );
    let hidden_stroke = json!({"id": "a", "opacity": 1, "fill": "c4", "real": false});
    assert_eq!(view(&record(&hidden_stroke))["stroke"], Value::Null);
}
