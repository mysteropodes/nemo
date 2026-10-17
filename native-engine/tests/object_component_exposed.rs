//! Oracle tests for `expose()` (L04-E, I14/I35). `E` rows of
//! `fixtures/object-component-exposed/oracle.txt` (tools/gen_oracle.cjs of the leaf, verbatim
//! `exposed-properties.js` at baseline `fbe4b8a3` in a Node `vm`) hold the scenario (component
//! present or missing, declaration list absent/null/empty/listed, target, label, default, clock,
//! random, metadata port or none) and the baseline result: the entry, the whole list after the call,
//! the port events in call order and the metadata call arguments.

use super::{expose, ExposeRequest, ExposedEntry, ExposedError, ExposedOutcome};
use crate::evaluation::object_component_kernels::object_component_exposed_key::RadixError;
use crate::object_document::Presence;
use serde_json::{json, Number, Value};

const ORACLE: &str = include_str!("fixtures/object-component-exposed/oracle.txt");

fn rows() -> Vec<(Value, Value)> {
    ORACLE
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(|cols| cols[0] == "E")
        .map(|cols| {
            (
                serde_json::from_str(cols[1]).expect("scenario"),
                serde_json::from_str(cols[2]).expect("result"),
            )
        })
        .collect()
}

fn jnum(value: &Value) -> f64 {
    match value {
        Value::Number(number) => number.as_f64().expect("finite"),
        Value::String(text) => match text.as_str() {
            "n:NaN" => f64::NAN,
            "n:Infinity" => f64::INFINITY,
            "n:-Infinity" => f64::NEG_INFINITY,
            "n:-0" => -0.0,
            other => panic!("number token {other}"),
        },
        other => panic!("not a number {other}"),
    }
}

fn default_of(value: Option<&Value>) -> Presence<Number> {
    match value {
        Some(Value::Number(number)) => Presence::Present(number.clone()),
        None => Presence::Missing,
        Some(other) => panic!("unsupported default {other}"),
    }
}

fn entry_of(value: &Value) -> ExposedEntry {
    ExposedEntry {
        key: value["key"].as_str().unwrap().to_string(),
        label: value["label"].as_str().unwrap().to_string(),
        target_stroke_id: value["targetStrokeId"].as_str().unwrap().to_string(),
        target_field: value["targetField"].as_str().unwrap().to_string(),
        default_value: default_of(value.get("default")),
    }
}

/// The baseline JSON shape of an entry; an absent default has no member.
fn entry_json(entry: &ExposedEntry) -> Value {
    let mut value = json!({"key": entry.key, "label": entry.label,
        "targetStrokeId": entry.target_stroke_id, "targetField": entry.target_field});
    if let Presence::Present(default) = &entry.default_value {
        value["default"] = Value::Number(default.clone());
    }
    value
}

fn slot_of(scenario: &Value) -> Option<Presence<Vec<ExposedEntry>>> {
    if scenario["sym"] == "missing" {
        return None;
    }
    Some(match scenario.get("props") {
        None | Some(Value::Null) => Presence::Missing,
        Some(list) => Presence::Present(list.as_array().unwrap().iter().map(entry_of).collect()),
    })
}

struct Ran {
    result: Result<Option<ExposedOutcome>, ExposedError>,
    events: Vec<String>,
    calls: Vec<Value>,
}

fn run(scenario: &Value) -> Ran {
    let slot = slot_of(scenario);
    let request = ExposeRequest {
        target_stroke_id: scenario["stroke"].as_str().unwrap(),
        target_field: scenario["field"].as_str().unwrap(),
        label: scenario["label"].as_str().unwrap(),
        default_value: default_of(scenario.get("default")),
    };
    let (clock_value, random_value) = (jnum(&scenario["clock"]), jnum(&scenario["random"]));
    let events = std::cell::RefCell::new(Vec::<String>::new());
    let mut calls: Vec<Value> = Vec::new();
    let mut clock = || {
        events.borrow_mut().push("clock".to_string());
        clock_value
    };
    let mut random = || {
        events.borrow_mut().push("random".to_string());
        random_value
    };
    let mut metadata = |entry: &ExposedEntry| {
        events.borrow_mut().push("metadata".to_string());
        let default = match &entry.default_value {
            Presence::Present(number) => Value::Number(number.clone()),
            Presence::Missing => Value::Null,
        };
        calls.push(json!([entry.key, entry.label, default]));
    };
    let port: Option<&mut dyn FnMut(&ExposedEntry)> = if scenario.get("noMeta").is_some() {
        None
    } else {
        Some(&mut metadata)
    };
    let result = expose(slot.as_ref(), &request, &mut clock, &mut random, port);
    let events = events.into_inner();
    Ran {
        result,
        events,
        calls,
    }
}

#[test]
fn fixture_pins_the_baseline_and_its_row_count() {
    assert!(ORACLE.contains("fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert_eq!(rows().len(), 21);
}

#[test]
fn every_oracle_row_is_reproduced_except_the_duplicate_key() {
    for (scenario, expected) in rows() {
        let id = scenario["id"].as_str().unwrap();
        let ran = run(&scenario);
        if id == "duplicate-key-appended" {
            continue;
        }
        let events: Vec<String> = serde_json::from_value(expected["events"].clone()).unwrap();
        assert_eq!(ran.events, events, "{id} events");
        let outcome = ran.result.unwrap_or_else(|e| panic!("{id}: {e:?}"));
        if expected["entry"].is_null() {
            assert!(outcome.is_none(), "{id}: no component, no entry");
            assert!(expected["props"].is_null(), "{id}");
            continue;
        }
        let outcome = outcome.unwrap();
        assert_eq!(entry_json(&outcome.entry), expected["entry"], "{id} entry");
        let props: Vec<Value> = outcome.props.iter().map(entry_json).collect();
        assert_eq!(Value::Array(props), expected["props"], "{id} list");
        assert_eq!(
            Value::Array(ran.calls),
            expected["calls"],
            "{id} metadata calls"
        );
    }
}

#[test]
fn the_duplicate_key_is_refused_where_the_baseline_appended_it() {
    let (scenario, expected) = rows()
        .into_iter()
        .find(|(s, _)| s["id"] == "duplicate-key-appended")
        .unwrap();
    // baseline: two entries with the same key (the symbol keeps both)
    let list = expected["props"].as_array().unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0]["key"], list[1]["key"]);
    assert_eq!(list[0]["key"], "ep_100_i");
    let ran = run(&scenario);
    assert_eq!(
        ran.result,
        Err(ExposedError::DuplicateKey {
            key: "ep_100_i".to_string()
        })
    );
    // the ports ran (clock, random) but no metadata registration happened for a refused entry
    assert_eq!(ran.events, ["clock", "random"]);
    assert!(ran.calls.is_empty());
}

#[test]
fn no_component_means_no_entry_and_no_port_call() {
    let scenario = json!({"sym": "missing", "stroke": "s", "field": "opacity", "label": "L",
        "clock": 1296, "random": 0.5, "default": 100});
    let ran = run(&scenario);
    assert_eq!(ran.result, Ok(None));
    assert!(ran.events.is_empty());
}

#[test]
fn pinned_keys_and_lists_are_hand_written_literals() {
    let scenario = json!({"sym": "present", "stroke": "st1", "field": "opacity", "label": "Opacity",
        "clock": 1296, "random": 0.5, "default": 100,
        "props": [{"key": "ep_a_b", "label": "A", "targetStrokeId": "st0", "targetField": "__visible", "default": 100}]});
    let ran = run(&scenario);
    let outcome = ran.result.unwrap().unwrap();
    assert_eq!(outcome.entry.key, "ep_100_i");
    assert_eq!(outcome.props.len(), 2);
    assert_eq!(
        outcome.props[0].key, "ep_a_b",
        "existing entries stay first"
    );
    assert_eq!(outcome.props[1], outcome.entry);
    assert_eq!(
        outcome.entry.default_value,
        Presence::Present(Number::from(100u64))
    );
    assert_eq!(ran.events, ["clock", "random", "metadata"]);
}

#[test]
fn an_absent_default_stays_absent_and_zero_stays_zero() {
    let mut scenario = json!({"sym": "present", "stroke": "s", "field": "opacity", "label": "L",
        "clock": 7, "random": 0.6});
    let absent = run(&scenario).result.unwrap().unwrap();
    assert_eq!(absent.entry.default_value, Presence::Missing);
    scenario["default"] = json!(0);
    let zero = run(&scenario).result.unwrap().unwrap();
    assert_eq!(
        zero.entry.default_value,
        Presence::Present(Number::from(0u64))
    );
    assert_ne!(absent.entry, zero.entry);
}

#[test]
fn engine_dependent_ports_are_refused_after_both_ports_ran() {
    let scenario = json!({"sym": "present", "stroke": "s", "field": "opacity", "label": "L",
        "clock": 9007199254740992.0, "random": 0.5, "default": 1});
    let ran = run(&scenario);
    assert_eq!(
        ran.result,
        Err(ExposedError::Key(RadixError::EngineDependent))
    );
    assert_eq!(ran.events, ["clock", "random"]);
}

#[test]
fn a_refusal_publishes_nothing_so_the_input_list_is_unchanged() {
    let existing = Presence::Present(vec![entry_of(&json!({"key": "ep_100_i", "label": "A",
        "targetStrokeId": "x", "targetField": "opacity"}))]);
    let request = ExposeRequest {
        target_stroke_id: "s",
        target_field: "opacity",
        label: "L",
        default_value: Presence::Missing,
    };
    let result = expose(Some(&existing), &request, &mut || 1296.0, &mut || 0.5, None);
    assert_eq!(
        result,
        Err(ExposedError::DuplicateKey {
            key: "ep_100_i".to_string()
        })
    );
    let Presence::Present(list) = &existing else {
        panic!()
    };
    assert_eq!(
        list.len(),
        1,
        "the caller's list is a value, never edited in place"
    );
}
