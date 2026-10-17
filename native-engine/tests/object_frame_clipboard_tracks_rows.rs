//! Test support for the Motion-track repeat: the baseline oracle rows (fixtures/object-frame-clipboard-tracks/oracle.txt,
//! the verbatim repeatSelection at fbe4b8a3 over dense layers with `ld.motion` tables), native documents built from a row's
//! tables in two forms, and the reading of a document back into the oracle's vocabulary. No test lives here.
use crate::object_codec::decode_storage_project;
use crate::object_document::ObjectDocument;
use serde_json::{json, Map, Value};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-frame-clipboard-tracks/oracle.txt");

/// One oracle row. `input`/`output` hold, per layer, the `ld.motion` table (`null` when the layer has none).
pub(crate) struct Row {
    pub(crate) id: usize,
    pub(crate) min_layer: usize,
    pub(crate) max_layer: usize,
    pub(crate) first: u32,
    pub(crate) last: u32,
    pub(crate) times: i64,
    pub(crate) eff: u32,
    pub(crate) total: u32,
    pub(crate) order: bool,
    pub(crate) dup: bool,
    pub(crate) changed: bool,
    pub(crate) input: Vec<Value>,
    pub(crate) output: Vec<Value>,
    pub(crate) mutants: Vec<bool>,
}
impl Row {
    pub(crate) fn label(&self) -> String {
        format!("row {}", self.id)
    }
    /// The layers the cycle covers, by stable id, first to last selected.
    pub(crate) fn layers(&self) -> Vec<String> {
        (self.min_layer..=self.max_layer).map(uid).collect()
    }
    /// The forms this row can be built in: an `order` table lives in the composition-order plan, so a version-1
    /// document cannot hold it.
    pub(crate) fn forms(&self) -> Vec<Form> {
        if self.order {
            vec![Form::V2]
        } else {
            vec![Form::V1, Form::V2]
        }
    }
}

fn field(value: &Value, name: &str) -> Value {
    value[name].clone()
}
fn number(value: &Value, name: &str) -> i64 {
    value[name].as_i64().unwrap_or_else(|| panic!("{name}"))
}
fn flag(value: &Value, name: &str) -> bool {
    value[name].as_bool().unwrap_or_else(|| panic!("{name}"))
}
fn list(value: &Value, name: &str) -> Vec<Value> {
    value[name]
        .as_array()
        .cloned()
        .unwrap_or_else(|| panic!("{name}"))
}

pub(crate) fn rows() -> Vec<Row> {
    ORACLE
        .lines()
        .filter(|line| line.starts_with("row "))
        .map(|line| {
            let mut parts = line.splitn(3, ' ');
            parts.next();
            let id: usize = parts.next().unwrap().parse().unwrap();
            let value: Value = serde_json::from_str(parts.next().unwrap()).unwrap();
            let bounds = field(&value, "bounds");
            Row {
                id,
                min_layer: number(&bounds, "minL") as usize,
                max_layer: number(&bounds, "maxL") as usize,
                first: number(&bounds, "minF") as u32,
                last: number(&bounds, "maxF") as u32,
                times: number(&value, "times"),
                eff: number(&value, "eff") as u32,
                total: number(&value, "total") as u32,
                order: flag(&value, "order"),
                dup: flag(&value, "dup"),
                changed: flag(&value, "chg"),
                input: list(&value, "in"),
                output: list(&value, "out"),
                mutants: value["mut"]
                    .as_str()
                    .unwrap()
                    .chars()
                    .map(|c| c == '1')
                    .collect(),
            }
        })
        .collect()
}

/// Index of a mutant bit, in the order of the oracle header.
pub(crate) const T_MINUS: usize = 0;
pub(crate) const T_PLUS: usize = 1;
pub(crate) const F_MINUS: usize = 2;
pub(crate) const F_ZERO: usize = 3;
pub(crate) const L_MINUS: usize = 4;
pub(crate) const NO_SORT: usize = 5;
pub(crate) const WIDE: usize = 6;
pub(crate) const NARROW: usize = 7;
pub(crate) const SHIFT: usize = 8;

// ---- native documents

/// V1 plain; V2 adds the composition-order plan (where a layer's `order` table lives).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Form {
    V1,
    V2,
}
pub(crate) fn uid(layer: usize) -> String {
    format!("L{layer}")
}
fn target(layer: &str) -> Value {
    json!({"contextId":"scene-root","layerUid":layer})
}
/// The holder of one layer: every table but `order`, in the table's own key order; the `xc_amp` table needs its control.
fn holder(layer: &str, table: &Map<String, Value>) -> Option<Value> {
    let properties: Vec<Value> = table
        .iter()
        .filter(|(name, _)| name.as_str() != "order")
        .map(|(name, track)| json!({"property":name,"track":track}))
        .collect();
    if properties.is_empty() {
        return None;
    }
    let mut holder = json!({"target":target(layer),"properties":properties});
    if table.contains_key("xc_amp") {
        holder["controls"] = json!([{"key":"xc_amp","name":"Amp","type":"number"}]);
    }
    Some(holder)
}
/// The composition-order plan of `layers` layers whose scene-root layer `index` carries `order[index]`; `extra` adds a
/// second context `ctx-x` with the same stack, whose layer 0 carries the given track.
pub(crate) fn plan_json(orders: &[Option<Value>], extra: Option<Value>) -> Value {
    let stack = |tracks: &[Option<Value>]| -> Vec<Value> {
        tracks
            .iter()
            .enumerate()
            .map(|(index, track)| match track {
                Some(track) => {
                    json!({"layerUid":uid(index),"naturalPosition":index,"order":{"track":track}})
                }
                None => json!({"layerUid":uid(index),"naturalPosition":index,"order":{}}),
            })
            .collect()
    };
    let mut contexts = vec![json!({"contextId":"scene-root","layers":stack(orders),"scopes":[]})];
    if let Some(track) = extra {
        let mut other: Vec<Option<Value>> = vec![None; orders.len()];
        other[0] = Some(track);
        contexts.push(json!({"contextId":"ctx-x","layers":stack(&other),"scopes":[]}));
    }
    json!({"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":contexts})
}
/// A native document with `tables.len()` bare layers (`L0`...), no cells, and the given `ld.motion` tables.
pub(crate) fn document_json(total: u32, tables: &[Value], form: Form) -> Value {
    let layers: Vec<Value> = (0..tables.len())
        .map(|i| json!({"layerUid":uid(i)}))
        .collect();
    let mut holders = Vec::new();
    let mut orders: Vec<Option<Value>> = Vec::new();
    for (index, table) in tables.iter().enumerate() {
        orders.push(table.get("order").cloned());
        if let Some(table) = table.as_object() {
            if let Some(made) = holder(&uid(index), table) {
                holders.push(made);
            }
        }
    }
    let version = if form == Form::V2 { 2 } else { 1 };
    let mut value = json!({"format":"nemo.native-object-document","formatVersion":version,
        "totalFrames":total,"layers":layers,"objects":[]});
    if !holders.is_empty() {
        value["propertyHolders"] = Value::Array(holders);
    }
    if form == Form::V2 {
        value["compositionOrder"] = plan_json(&orders, None);
    }
    value
}
pub(crate) fn decode(value: &Value) -> ObjectDocument {
    decode_storage_project(value.to_string().as_bytes()).unwrap_or_else(|error| panic!("{error}"))
}
/// True when the storage codec admits the document (the strict drawing admission, track order included).
pub(crate) fn decodes(value: &Value) -> bool {
    decode_storage_project(value.to_string().as_bytes()).is_ok()
}
pub(crate) fn document(total: u32, tables: &[Value], form: Form) -> ObjectDocument {
    decode(&document_json(total, tables, form))
}

/// The tables a document holds, per layer, in the oracle's vocabulary: scene-root layer-level holders (no stroke id)
/// plus the layer's `order` track of the scene-root composition-order plan; `null` when a layer has none.
pub(crate) fn tables(document: &ObjectDocument) -> Vec<Value> {
    let wire = serde_json::to_value(document).unwrap();
    let layers = wire["layers"].as_array().unwrap().len();
    let mut out: Vec<Map<String, Value>> = vec![Map::new(); layers];
    for holder in wire["propertyHolders"].as_array().into_iter().flatten() {
        let target = &holder["target"];
        if target["contextId"] != "scene-root" || !target["strokeId"].is_null() {
            continue;
        }
        let index = wire["layers"]
            .as_array()
            .unwrap()
            .iter()
            .position(|layer| layer["layerUid"] == target["layerUid"])
            .unwrap();
        for record in holder["properties"].as_array().unwrap() {
            if !record["track"].is_null() {
                out[index].insert(
                    record["property"].as_str().unwrap().into(),
                    record["track"].clone(),
                );
            }
        }
    }
    for context in wire["compositionOrder"]["contexts"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if context["contextId"] != "scene-root" {
            continue;
        }
        for (index, layer) in context["layers"].as_array().unwrap().iter().enumerate() {
            if !layer["order"]["track"].is_null() {
                out[index].insert("order".into(), layer["order"]["track"].clone());
            }
        }
    }
    out.into_iter()
        .map(|map| {
            if map.is_empty() {
                Value::Null
            } else {
                Value::Object(map)
            }
        })
        .collect()
}

/// A whole document compared with the one the row's expected output builds: every field, number representation and
/// holder included, not only the tables.
pub(crate) fn matches_expected(row: &Row, form: Form, got: &ObjectDocument) -> Result<(), String> {
    let expected = document(row.total, &row.output, form);
    if *got == expected {
        return Ok(());
    }
    Err(format!(
        "{} {form:?}: tables {:?} expected {:?}",
        row.label(),
        tables(got),
        row.output
    ))
}
