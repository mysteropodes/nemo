//! L01-G test support: the oracle rows (fixtures/object-parametric-edit/oracle.txt, Paper.js output as hex f64
//! cells), record/document builders over the L01-F support, the real dispatch caller and result readers.
//! Nothing here decides an expectation: every expected number is an oracle cell or a literal in a test.
use super::vsupport::{dispatch, target};
use crate::history::NativeObjectHistory;
use serde_json::{json, Map, Value};

pub(super) const ORACLE: &str = include_str!("fixtures/object-parametric-edit/oracle.txt");
pub(super) const BASELINE: &str = "fbe4b8a33e6d5a229f014154df2ba21988a9fabc";
pub(super) const OP_PARAM: &str = "command.document.object.parametric.edit";
pub(super) const OP_FX: &str = "command.document.object.path.effects.edit";

/// One anchor per entry: point x y, handleIn x y, handleOut x y.
pub(super) type Segs = Vec<[f64; 6]>;

pub(super) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split('\t').collect::<Vec<&'static str>>())
        .filter(|cols| cols[0] == kind)
        .collect()
}

pub(super) fn meta(key: &str) -> &'static str {
    rows("meta")
        .into_iter()
        .find(|cols| cols[1] == key)
        .map(|cols| cols[2])
        .unwrap_or_else(|| panic!("oracle meta {key}"))
}

pub(super) fn hexf(cell: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(cell, 16).expect("hex f64 cell"))
}

pub(super) fn pair(cell: &str) -> [f64; 2] {
    let (first, second) = cell.split_once(',').expect("pair cell");
    [hexf(first), hexf(second)]
}

pub(super) fn segs(cell: &str) -> Segs {
    cell.split(';')
        .map(|anchor| {
            let values: Vec<f64> = anchor.split(',').map(hexf).collect();
            assert_eq!(values.len(), 6, "{anchor}");
            [
                values[0], values[1], values[2], values[3], values[4], values[5],
            ]
        })
        .collect()
}

pub(super) fn frame(cell: &str) -> Option<[f64; 4]> {
    if cell == "-" {
        return None;
    }
    let values: Vec<f64> = cell.split(',').map(hexf).collect();
    assert_eq!(values.len(), 4, "{cell}");
    Some([values[0], values[1], values[2], values[3]])
}

fn segment_json(anchor: &[f64; 6]) -> Value {
    json!({
        "point": {"x": anchor[0], "y": anchor[1]},
        "handleIn": {"x": anchor[2], "y": anchor[3]},
        "handleOut": {"x": anchor[4], "y": anchor[5]}
    })
}

/// `R:tl:tr:br:bl`, `E:startAngle:sweep:innerRadius` or `S:pointCount:innerRatio:cornerRadius` (`!` = absent)
/// as the storage `paramShape` object without a box.
pub(super) fn spec_json(cell: &str) -> Value {
    let parts: Vec<&str> = cell.split(':').collect();
    let (kind, keys): (&str, &[&str]) = match parts[0] {
        "R" => ("rect", &["tl", "tr", "br", "bl"]),
        "E" => ("ellipse", &["startAngle", "sweep", "innerRadius"]),
        "S" => ("star", &["pointCount", "innerRatio", "cornerRadius"]),
        other => panic!("unknown spec kind {other}"),
    };
    let mut shape = Map::new();
    shape.insert("kind".to_string(), json!(kind));
    for (index, key) in keys.iter().enumerate() {
        if parts[index + 1] != "!" {
            shape.insert((*key).to_string(), json!(hexf(parts[index + 1])));
        }
    }
    Value::Object(shape)
}

/// The same spec with every member absent (only the kind remains).
pub(super) fn blank_spec(cell: &str) -> String {
    let members = if cell.starts_with('R') { 4 } else { 3 };
    format!("{}{}", &cell[..1], ":!".repeat(members))
}

/// A `vectorExt` slot carrying a `paramShape` (and an optional box and `boxAngle`).
pub(super) fn ext_json(spec: &str, frame: Option<[f64; 4]>, angle: f64) -> Value {
    let mut shape = spec_json(spec);
    if let Some(b) = frame {
        shape["box"] = json!({"x1": b[0], "y1": b[1], "x2": b[2], "y2": b[3]});
    }
    let mut ext = json!({ "paramShape": shape });
    if angle != 0.0 {
        ext["boxAngle"] = json!(angle);
    }
    ext
}

/// An admitted `editable-vector-path` record (solid fill, no stroke) at frame 0.
pub(super) fn record(
    stroke: &str,
    layer: &str,
    segments: &Segs,
    closed: bool,
    ext: Option<Value>,
) -> Value {
    let anchors: Vec<Value> = segments.iter().map(segment_json).collect();
    let mut record = json!({
        "schemaVersion": 1,
        "family": "editable-vector-path",
        "target": target(layer, 0, stroke),
        "geometry": {
            "kind": "cubic-path",
            "closed": closed,
            "coordinateSpace": "document-world",
            "handleSpace": "relative",
            "segments": anchors
        },
        "fill": {"kind": "solid", "r": 0.2, "g": 0.4, "b": 0.6, "a": 1},
        "vector": {
            "stroke": null,
            "hasRealStroke": false,
            "strokeWidth": 4,
            "strokeCap": "round",
            "strokeJoin": "round",
            "opacity": 1
        }
    });
    if let Some(ext) = ext {
        record["vectorExt"] = ext;
    }
    record
}

pub(super) fn straight_rect() -> Segs {
    vec![
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [100.0, 0.0, 0.0, 0.0, 0.0, 0.0],
        [100.0, 60.0, 0.0, 0.0, 0.0, 0.0],
        [0.0, 60.0, 0.0, 0.0, 0.0, 0.0],
    ]
}

pub(super) fn stored_segments(object: &Value) -> Segs {
    object["geometry"]["segments"]
        .as_array()
        .expect("segments")
        .iter()
        .map(|anchor| {
            let at = |part: &str, axis: &str| anchor[part][axis].as_f64().expect("coordinate");
            [
                at("point", "x"),
                at("point", "y"),
                at("handleIn", "x"),
                at("handleIn", "y"),
                at("handleOut", "x"),
                at("handleOut", "y"),
            ]
        })
        .collect()
}

pub(super) fn stored_frame(object: &Value) -> Option<[f64; 4]> {
    let b = &object["vectorExt"]["paramShape"]["box"];
    if b.is_null() {
        return None;
    }
    let at = |axis: &str| b[axis].as_f64().expect("box coordinate");
    Some([at("x1"), at("y1"), at("x2"), at("y2")])
}

/// Bit-for-bit equality (`-0` and `0` differ).
pub(super) fn same_bits(got: &Segs, want: &Segs) -> Result<(), String> {
    if got.len() != want.len() {
        return Err(format!("anchor count {} != {}", got.len(), want.len()));
    }
    for (index, (g, w)) in got.iter().zip(want).enumerate() {
        for slot in 0..6 {
            if g[slot].to_bits() != w[slot].to_bits() {
                return Err(format!(
                    "anchor {index} slot {slot}: {} != {}",
                    g[slot], w[slot]
                ));
            }
        }
    }
    Ok(())
}

/// The trigonometric bound: values that went through `cos`/`sin` may differ from V8 by a few ulps, so they are
/// compared with an absolute bound scaled by the magnitude (1e-11 per unit; a real mistake is many orders larger).
pub(super) fn within_bound(got: &Segs, want: &Segs) -> Result<(), String> {
    within_tolerance(got, want, 1e-11)
}

/// The same comparison with an explicit per-unit tolerance. The roughen hash multiplies `sin` by 43758.5453, so
/// its results carry that amplified last-bit difference and use the wider 1e-9 (still far below any real mistake).
pub(super) fn within_tolerance(got: &Segs, want: &Segs, tolerance: f64) -> Result<(), String> {
    if got.len() != want.len() {
        return Err(format!("anchor count {} != {}", got.len(), want.len()));
    }
    for (index, (g, w)) in got.iter().zip(want).enumerate() {
        for slot in 0..6 {
            if (g[slot] - w[slot]).abs() > tolerance * (1.0 + w[slot].abs()) {
                return Err(format!(
                    "anchor {index} slot {slot}: {} vs {}",
                    g[slot], w[slot]
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn request(
    owner: &NativeObjectHistory,
    id: &str,
    operation: &str,
    payload: Value,
) -> Value {
    json!({
        "apiVersion": 2,
        "requestId": id,
        "instanceId": owner.instance_id(),
        "documentId": owner.document_id(),
        "expectedRevision": owner.content_revision(),
        "operation": operation,
        "payload": payload
    })
}

/// Send one payload with the owner's current revision; returns the response.
pub(super) fn send(
    owner: &mut NativeObjectHistory,
    id: &str,
    operation: &str,
    payload: Value,
) -> Value {
    let envelope = request(owner, id, operation, payload);
    dispatch(owner, &envelope)
}

pub(super) fn applied(response: &Value) -> bool {
    assert_eq!(response["ok"], true, "{response}");
    response["result"]["applied"].as_bool().expect("applied")
}

pub(super) fn refusal(response: &Value) -> String {
    assert_eq!(response["ok"], false, "{response}");
    response["error"]["code"]
        .as_str()
        .expect("error code")
        .to_string()
}

pub(super) fn set(stable_target: Value, shape: Value) -> Value {
    json!({"command": "object.parametric.set", "stableTarget": stable_target, "paramShape": shape})
}

pub(super) fn translate(stable_target: Value, dx: f64, dy: f64) -> Value {
    json!({"command": "object.parametric.translate", "stableTarget": stable_target, "delta": [dx, dy]})
}

pub(super) fn scale(stable_target: Value, factors: [f64; 2], pivot: [f64; 2]) -> Value {
    json!({
        "command": "object.parametric.scale",
        "stableTarget": stable_target,
        "scale": factors,
        "anchor": pivot
    })
}

pub(super) fn effects(layer: &str, stack: Value) -> Value {
    json!({"command": "object.path.effects.set", "layerUid": layer, "stack": stack})
}
