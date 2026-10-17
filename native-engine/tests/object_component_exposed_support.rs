//! Shared support of the override tests (L04-E): the colour-token table, the stroke-spec to
//! `ObjectRecord` builder, the record view, the scripted port and the oracle row loader.
//!
//! `fixtures/object-component-exposed/oracle.txt` was produced by `tools/gen_oracle.cjs` of the L04-E
//! leaf from the verbatim `exposed-properties.js` at baseline `fbe4b8a3` (Node `vm`). Row kinds,
//! tab separated: `E` expose scenario, `A` applyOverrides scenario, `D` applyOverrides scenario whose
//! baseline output the native kernel refuses. A stroke spec is `{id, opacity, fill, stroke, real,
//! legacy}`; `fill`/`stroke` are colour tokens (`c1..c4`, `white`), `real` is `hasRealStroke`.

use std::borrow::Cow;
use std::collections::HashMap;

use super::{
    apply_overrides_with, ExposedEntry, ExposedError, ExposedValue, ObjectRecord, OverridePort,
    OverrideRules, Presence,
};
use crate::object_document::SolidFill;
use serde_json::{json, Number, Value};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-component-exposed/oracle.txt");
pub(crate) const BASELINE: &str = "fbe4b8a33e6d5a229f014154df2ba21988a9fabc";
pub(crate) const BLOB: &str = "dfd93e88afad3d158d62c851f3bfe8238753d542";

/// Oracle rows of one kind as (scenario, result) JSON values.
pub(crate) fn rows(kind: &str) -> Vec<(Value, Value)> {
    ORACLE
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(|cols| cols[0] == kind)
        .map(|cols| {
            (
                serde_json::from_str(cols[1]).expect("scenario json"),
                serde_json::from_str(cols[2]).expect("result json"),
            )
        })
        .collect()
}

/// `n:NaN` style specials and plain JSON numbers.
pub(crate) fn jnum(value: &Value) -> f64 {
    match value {
        Value::Number(number) => number.as_f64().expect("finite json number"),
        Value::String(text) => match text.as_str() {
            "n:NaN" => f64::NAN,
            "n:Infinity" => f64::INFINITY,
            "n:-Infinity" => f64::NEG_INFINITY,
            "n:-0" => -0.0,
            other => panic!("number token {other}"),
        },
        other => panic!("not a number: {other}"),
    }
}

pub(crate) fn solid(token: &str) -> SolidFill {
    let (r, g, b) = match token {
        "c1" => (0.25, 0.5, 0.75),
        "c2" => (0.125, 0.25, 0.5),
        "c3" => (1.0, 0.0, 0.5),
        "c4" => (0.0, 1.0, 0.25),
        "white" => (1.0, 1.0, 1.0),
        other => panic!("colour token {other}"),
    };
    serde_json::from_value(json!({"kind": "solid", "r": r, "g": g, "b": b, "a": 1})).expect("fill")
}

pub(crate) fn token_of(fill: &SolidFill) -> &'static str {
    ["c1", "c2", "c3", "c4", "white"]
        .into_iter()
        .find(|token| solid(token) == *fill)
        .expect("a known colour")
}

pub(crate) fn solid_json(token: &str) -> Value {
    let fill = solid(token);
    json!({"kind": "solid", "r": fill.r, "g": fill.g, "b": fill.b, "a": fill.a})
}

/// A stroke spec as a native record: vector family unless `legacy`.
pub(crate) fn record(spec: &Value) -> ObjectRecord {
    let id = spec.get("id").and_then(Value::as_str).unwrap_or("");
    let legacy = spec.get("legacy").and_then(Value::as_bool).unwrap_or(false);
    let fill = spec
        .get("fill")
        .and_then(Value::as_str)
        .map_or(Value::Null, solid_json);
    let seg = |x: i32, y: i32| json!({"point": {"x": x, "y": y}, "handleIn": {"x": 0, "y": 0}, "handleOut": {"x": 0, "y": 0}});
    let family = if legacy {
        "closed-solid-fill-cubic-path"
    } else {
        "editable-vector-path"
    };
    let mut value = json!({
        "schemaVersion": 1,
        "family": family,
        "target": {"contextId": "comp_a", "frameScope": {"kind": "authored", "frame": 0},
                   "layerUid": "L1", "strokeId": id},
        "geometry": {"kind": "cubic-path", "closed": true, "coordinateSpace": "document-world",
                     "handleSpace": "relative", "segments": [seg(0, 0), seg(10, 5)]},
        "fill": fill,
    });
    if !legacy {
        let real = spec.get("real").and_then(Value::as_bool).unwrap_or(true);
        let stroke = if real {
            solid_json(spec["stroke"].as_str().expect("stroke token"))
        } else {
            Value::Null
        };
        value["vector"] = json!({"stroke": stroke, "hasRealStroke": real, "strokeWidth": 3,
            "strokeCap": "round", "strokeJoin": "round", "opacity": spec["opacity"].clone()});
    }
    serde_json::from_value(value).expect("record")
}

/// `{id, opacity, fill, stroke}` of a vector record, `{id, fill}` of a legacy one; the stroke is
/// the effective one (a stroke without `hasRealStroke` is not shown).
pub(crate) fn view(record: &ObjectRecord) -> Value {
    let fill = match record.fill.value() {
        Some(Some(fill)) => json!(token_of(fill)),
        _ => Value::Null,
    };
    let id = record.target.stroke_id.clone();
    match record.vector.value() {
        None => json!({"id": id, "fill": fill}),
        Some(body) => {
            let stroke = match body.solid_stroke() {
                Some(stroke) if body.has_real_stroke() => json!(token_of(stroke)),
                _ => Value::Null,
            };
            json!({"id": id, "opacity": body.opacity.clone(), "fill": fill, "stroke": stroke})
        }
    }
}

pub(crate) fn decode_value(value: &Value) -> ExposedValue {
    match value {
        Value::Null => ExposedValue::Paint(None),
        Value::String(text) if !text.starts_with("n:") => ExposedValue::Paint(Some(solid(text))),
        other => ExposedValue::Number(jnum(other)),
    }
}

pub(crate) fn props_of(scenario: &Value) -> Vec<ExposedEntry> {
    scenario
        .get("props")
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .map(|prop| ExposedEntry {
                    key: prop["key"].as_str().expect("key").to_string(),
                    label: format!("L {}", prop["key"].as_str().expect("key")),
                    target_stroke_id: prop["stroke"].as_str().expect("stroke").to_string(),
                    target_field: prop["field"].as_str().expect("field").to_string(),
                    default_value: Presence::Present(Number::from(0u64)),
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Scripted port: records every call as the oracle spells it (`available`, `meta:k`, `read:k@f`).
pub(crate) struct ScriptedPort {
    pub(crate) events: Vec<String>,
    pub(crate) available: bool,
    pub(crate) reads: HashMap<String, Vec<Value>>,
    pub(crate) fail_key: Option<String>,
}

impl ScriptedPort {
    pub(crate) fn of(scenario: &Value) -> ScriptedPort {
        let reads = scenario["reads"]
            .as_object()
            .map(|map| {
                map.iter()
                    .map(|(key, list)| (key.clone(), list.as_array().expect("list").clone()))
                    .collect()
            })
            .unwrap_or_default();
        ScriptedPort {
            events: Vec::new(),
            available: scenario.get("available").and_then(Value::as_bool) != Some(false),
            reads,
            fail_key: None,
        }
    }
}

impl OverridePort for ScriptedPort {
    fn available(&mut self) -> bool {
        self.events.push("available".to_string());
        self.available
    }
    fn register(&mut self, prop: &ExposedEntry) {
        self.events.push(format!("meta:{}", prop.key));
    }
    fn read(
        &mut self,
        layer_uid: &str,
        key: &str,
        frame: u32,
    ) -> Result<Vec<ExposedValue>, String> {
        assert_eq!(layer_uid, "inst", "the instance layer is forwarded");
        self.events.push(format!("read:{key}@{frame}"));
        if self.fail_key.as_deref() == Some(key) {
            return Err("value port failed".to_string());
        }
        Ok(self
            .reads
            .get(key)
            .map(|list| list.iter().map(decode_value).collect())
            .unwrap_or_default())
    }
}

pub(crate) struct Ran {
    pub(crate) events: Vec<String>,
    /// Views of the output records and whether the input slice itself came back.
    pub(crate) result: Result<(Vec<Value>, bool), ExposedError>,
}

pub(crate) fn run_with(rules: OverrideRules, scenario: &Value, port: &mut ScriptedPort) -> Ran {
    let strokes: Vec<ObjectRecord> = scenario["strokes"]
        .as_array()
        .expect("strokes")
        .iter()
        .map(record)
        .collect();
    let before = strokes.clone();
    let frame = scenario["frame"].as_u64().expect("frame") as u32;
    let result = apply_overrides_with(rules, &props_of(scenario), &strokes, "inst", frame, port)
        .map(|out| {
            let same = matches!(out, Cow::Borrowed(_));
            (out.iter().map(view).collect::<Vec<_>>(), same)
        });
    assert_eq!(strokes, before, "the input records are never mutated");
    Ran {
        events: std::mem::take(&mut port.events),
        result,
    }
}

pub(crate) fn run(rules: OverrideRules, scenario: &Value) -> Ran {
    run_with(rules, scenario, &mut ScriptedPort::of(scenario))
}

pub(crate) fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("array")
        .iter()
        .map(|item| item.as_str().expect("string").to_string())
        .collect()
}
