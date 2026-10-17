//! L10-H oracle rows: the parser of the baseline serP oracle, the production inputs built from its
//! input column and the INDEPENDENT derivation of the expected native record from its serP column.
use super::support::ORACLE;
use super::svg::StrokeExtras;
use crate::history::object_history::object_shot_kernels::native_import_svg_explode::{
    Contour, ExplodedPath, LeafStyle, PathOrigin, Seg,
};
use crate::object_document::{FrameScope, FrameScopeKind, ObjectTarget};
use serde_json::{json, Value};

pub(super) struct Pair {
    pub(super) id: String,
    pub(super) input: Value,
    pub(super) serp: Value,
}
type Meta = Vec<(String, String)>;

pub(super) fn oracle() -> (Meta, Meta, Vec<Pair>) {
    let (mut meta, mut facts, mut pairs) = (Vec::new(), Vec::new(), Vec::new());
    for line in ORACLE.lines().filter(|l| !l.starts_with('#')) {
        let cols: Vec<&str> = line.split('\t').collect();
        match cols[0] {
            "meta" => meta.push((cols[1].to_owned(), cols[2].to_owned())),
            "fact" => facts.push((cols[1].to_owned(), cols[2].to_owned())),
            "pair" => {
                assert_eq!(cols.len(), 4, "{line}");
                pairs.push(Pair {
                    id: cols[1].to_owned(),
                    input: serde_json::from_str(cols[2]).unwrap(),
                    serp: serde_json::from_str(cols[3]).unwrap(),
                });
            }
            other => panic!("unknown oracle row kind {other}"),
        }
    }
    (meta, facts, pairs)
}

fn pt(v: &Value) -> [f64; 2] {
    [v[0].as_f64().unwrap(), v[1].as_f64().unwrap()]
}
/// The production input of one oracle row: the kernel's exploded path plus the adapter's extras.
pub(super) fn production_input(input: &Value) -> (ExplodedPath, StrokeExtras) {
    let segments = input["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| Seg {
            point: pt(&s["point"]),
            handle_in: pt(&s["handleIn"]),
            handle_out: pt(&s["handleOut"]),
        })
        .collect();
    let token = |key: &str| input[key].as_str().map(str::to_owned);
    let path = ExplodedPath {
        origin: PathOrigin::Passthrough,
        contour: Contour {
            segments,
            closed: input["closed"].as_bool().unwrap(),
        },
        style: LeafStyle {
            fill: token("fill"),
            stroke: token("stroke"),
            stroke_width: input["strokeWidth"].as_f64().unwrap(),
            opacity: input["opacity"].as_f64().unwrap(),
        },
    };
    let extras = StrokeExtras {
        cap: input["cap"].as_str().unwrap().to_owned(),
        join: input["join"].as_str().unwrap().to_owned(),
        miter_limit: input["miterLimit"].as_f64(),
        dash_array: input["dashArray"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d.as_f64().unwrap())
            .collect(),
        dash_offset: input["dashOffset"].as_f64(),
    };
    (path, extras)
}

pub(super) fn target() -> ObjectTarget {
    ObjectTarget {
        context_id: "scene-root".into(),
        frame_scope: FrameScope {
            kind: FrameScopeKind::Authored,
            frame: 0,
        },
        layer_uid: "L0".into(),
        stroke_id: String::new(),
    }
}
fn target_json() -> Value {
    json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},
        "layerUid":"L0","strokeId":""})
}

fn channels(hex: &str) -> [f64; 4] {
    let byte =
        |i: usize| f64::from(u8::from_str_radix(&hex[1 + 2 * i..3 + 2 * i], 16).unwrap()) / 255.0;
    [
        byte(0),
        byte(1),
        byte(2),
        if hex.len() == 9 { byte(3) } else { 1.0 },
    ]
}
fn solid(hex: &str) -> Value {
    let c = channels(hex);
    json!({"kind":"solid","r":c[0],"g":c[1],"b":c[2],"a":c[3]})
}

/// The native record the baseline serP column implies, derived independently of production.
pub(super) fn expected_from_serp(serp: &Value) -> Value {
    let anchor = |p: &Value| json!({"x":p[0],"y":p[1]});
    let segments: Vec<Value> = serp["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            json!({"point":anchor(&s["point"]),"handleIn":anchor(&s["handleIn"]),
                "handleOut":anchor(&s["handleOut"])})
        })
        .collect();
    let real = serp["hasRealStroke"].as_bool().unwrap();
    let mut vector = serde_json::Map::new();
    vector.insert(
        "stroke".into(),
        if real {
            solid(serp["strokeColor"].as_str().unwrap())
        } else {
            Value::Null
        },
    );
    vector.insert("hasRealStroke".into(), json!(real));
    for key in [
        "strokeWidth",
        "strokeCap",
        "strokeJoin",
        "miterLimit",
        "dashArray",
        "dashOffset",
        "opacity",
    ] {
        if !serp[key].is_null() {
            vector.insert(key.into(), serp[key].clone());
        }
    }
    json!({"schemaVersion":1,"family":"editable-vector-path","target":target_json(),
        "geometry":{"kind":"cubic-path","closed":serp["closed"],"coordinateSpace":"document-world",
            "handleSpace":"relative","segments":segments},
        "fill":serp["fillColor"].as_str().map_or(Value::Null, solid),
        "vector":Value::Object(vector)})
}

/// Structural equality with numbers compared by value (spelling is checked separately).
pub(super) fn same_json(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same_json(x, y))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, x)| y.get(k).is_some_and(|y| same_json(x, y)))
        }
        _ => a == b,
    }
}
/// `JSON.stringify` never writes `10.0`: an integral value below 2^53 must be an integer.
pub(super) fn spelled_like_json_stringify(value: &Value) -> bool {
    match value {
        Value::Number(n) => {
            !n.is_f64()
                || n.as_f64()
                    .is_some_and(|f| f.fract() != 0.0 || f.abs() >= 9_007_199_254_740_992.0)
        }
        Value::Array(items) => items.iter().all(spelled_like_json_stringify),
        Value::Object(map) => map.values().all(spelled_like_json_stringify),
        _ => true,
    }
}
