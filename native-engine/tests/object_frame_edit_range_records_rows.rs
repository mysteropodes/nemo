//! Oracle rows for the layer range records (L05-G): the verbatim dense baseline editor (src/js/app.js
//! insertFrame, removeFrame, removeFrameSpan; src/js/timeline.js duplicateKeyframe, extendExposure,
//! splitLayerAtPlayhead at fbe4b8a3, executed in a Node vm by the leaf's generator) over three layers
//! that carry FULL range records. This file is data access only: it parses the rows and builds native
//! documents from them. It holds no test and no production symbol beyond the codec and the document.
use crate::object_codec::decode_storage_project;
use crate::object_document::ObjectDocument;
use serde_json::{json, Value};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-frame-edit-range-records/oracle.txt");

/// One input layer: its stable uid, the lock flag and the native wire `range` (`Null` = no member).
pub(crate) struct LayerRow {
    pub(crate) uid: String,
    pub(crate) locked: bool,
    pub(crate) range: Value,
}

/// What the baseline left behind: frame count, work-area end, composition markers and every layer range.
pub(crate) struct Outcome {
    pub(crate) total: u32,
    pub(crate) wa: i64,
    pub(crate) comp: Value,
    pub(crate) ranges: Vec<Value>,
}

/// One insert / remove / span / extend / dupkey run.
pub(crate) struct ShiftRow {
    pub(crate) op: String,
    pub(crate) total: u32,
    pub(crate) cursor: u32,
    /// Frames of an extend or a span; 0 for the other operations.
    pub(crate) count: u32,
    pub(crate) sel: Option<Vec<String>>,
    pub(crate) wa: i64,
    pub(crate) layers: Vec<LayerRow>,
    pub(crate) comp: Value,
    /// Insert only: `G` the layer spliced a slot, `P` it was only length-padded.
    pub(crate) roles: String,
    /// `None` when the baseline refused (it left the document untouched).
    pub(crate) out: Option<Outcome>,
    /// Span of one frame only: the baseline `removeFrame` result on the same input.
    pub(crate) as_remove: Option<Outcome>,
}

/// One splitLayerAtPlayhead run.
pub(crate) struct SplitRow {
    pub(crate) eff_in: i64,
    pub(crate) eff_out: i64,
    pub(crate) cut: i64,
    pub(crate) layer: Value,
    /// `None` = refused; else the original half, the second half and whether a live link was dropped.
    pub(crate) out: Option<(Value, Value, bool)>,
}

pub(crate) enum Row {
    Shift(ShiftRow),
    Split(SplitRow),
}

fn outcome(value: &Value) -> Option<Outcome> {
    if value.as_str() == Some("refused") {
        return None;
    }
    Some(Outcome {
        total: value["total"].as_u64().unwrap() as u32,
        wa: value["wa"].as_i64().unwrap(),
        comp: value["comp"].clone(),
        ranges: value["ranges"].as_array().unwrap().clone(),
    })
}

fn shift_row(row: &Value) -> ShiftRow {
    ShiftRow {
        op: row["op"].as_str().unwrap().to_string(),
        total: row["total"].as_u64().unwrap() as u32,
        cursor: row["cursor"].as_u64().unwrap() as u32,
        count: row["count"].as_u64().unwrap_or(0) as u32,
        sel: row["sel"].as_array().map(|list| {
            list.iter()
                .map(|uid| uid.as_str().unwrap().to_string())
                .collect()
        }),
        wa: row["wa"].as_i64().unwrap(),
        layers: row["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|layer| LayerRow {
                uid: layer["uid"].as_str().unwrap().to_string(),
                locked: layer["locked"].as_bool().unwrap(),
                range: layer["range"].clone(),
            })
            .collect(),
        comp: row["comp"].clone(),
        roles: row["roles"].as_str().unwrap().to_string(),
        out: outcome(&row["out"]),
        as_remove: if row["asRemove"].is_null() {
            None
        } else {
            outcome(&row["asRemove"])
        },
    }
}

fn split_row(row: &Value) -> SplitRow {
    let out = &row["out"];
    SplitRow {
        eff_in: row["effIn"].as_i64().unwrap(),
        eff_out: row["effOut"].as_i64().unwrap(),
        cut: row["cut"].as_i64().unwrap(),
        layer: row["layer"].clone(),
        out: if out.as_str() == Some("refused") {
            None
        } else {
            Some((
                out["original"].clone(),
                out["second"].clone(),
                out["linkDropped"].as_bool().unwrap(),
            ))
        },
    }
}

/// Every oracle row in file order.
pub(crate) fn rows() -> Vec<Row> {
    ORACLE
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let value: Value = serde_json::from_str(line).unwrap();
            if value["op"] == "split" {
                Row::Split(split_row(&value))
            } else {
                Row::Shift(shift_row(&value))
            }
        })
        .collect()
}

/// The shift rows only.
pub(crate) fn shift_rows() -> Vec<ShiftRow> {
    rows()
        .into_iter()
        .filter_map(|row| match row {
            Row::Shift(shift) => Some(shift),
            Row::Split(_) => None,
        })
        .collect()
}

/// Wire JSON of the layers: `layerUid`, `locked` only when set, `range` only when the row has one.
pub(crate) fn layers_json(layers: &[LayerRow]) -> Vec<Value> {
    layers
        .iter()
        .map(|layer| {
            let mut value = json!({ "layerUid": layer.uid });
            if layer.locked {
                value["locked"] = json!(true);
            }
            if !layer.range.is_null() {
                value["range"] = layer.range.clone();
            }
            value
        })
        .collect()
}

/// A drawing-less v1 document of `total` frames whose layers carry the row's lock flags and ranges. It
/// goes through the real codec, so every oracle range is also an admission test.
pub(crate) fn range_doc(layers: &[LayerRow], total: u32) -> ObjectDocument {
    let text = json!({
        "format": "nemo.native-object-document",
        "formatVersion": 1,
        "totalFrames": total,
        "layers": layers_json(layers),
        "objects": [],
    });
    decode_storage_project(text.to_string().as_bytes()).unwrap()
}

/// The `range` member of layer `index` as wire JSON (`Null` when the layer has none).
pub(crate) fn range_json(document: &ObjectDocument, index: usize) -> Value {
    match document.layers[index].range.value() {
        Some(range) => serde_json::to_value(range).unwrap(),
        None => Value::Null,
    }
}
