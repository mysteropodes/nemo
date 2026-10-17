//! Scenario codec and runner shared by the L04-L oracle tests (reading, building
//! the real inputs and comparing; no evaluation logic of its own).
//!
//! A scenario (`scn` .. `end`, grammar in the fixture headers) is one call of
//! the verbatim baseline `getEffectiveStrokesRendered` / `applyLayerDuplicator`
//! / `resolvedEffectorsFor`. [`World`] is the in-memory provider behind the
//! two ports: it answers ONLY what the baseline was asked (Motion values, layer
//! motion, layer content) and records what the kernel asks, so a join that
//! reads a different property, another frame or an extra layer shows up as a
//! missing answer (typed `Unavailable`) or a different call list.

use super::LayerSources;
use crate::evaluation::object_component_kernels::duplicator_placement_matrix::test_codec::{
    hex, parse_list, parse_wants, Want,
};
use crate::evaluation::object_component_kernels::duplicator_placement_temporal::SpanBounds;
use crate::evaluation::object_component_kernels::object_effector_falloff::test_codec::{
    js, parse_nums,
};
use crate::evaluation::object_component_kernels::object_effector_layer::{MotionDelta, Sampled};
use crate::evaluation::object_component_kernels::object_effector_layers::{
    MotionPort, Unavailable,
};
use crate::object_document::object_component_record_kernels::object_duplicator_records::DuplicatorSet;
use crate::object_document::{ObjectLayer, ObjectRecord};
use serde_json::{json, Number, Value};
use std::cell::RefCell;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-duplicator-evaluate/oracle.txt");

/// Every scenario of the evaluation oracle.
pub(crate) fn all() -> Vec<Scenario> {
    scenarios(ORACLE)
}

#[derive(Debug, Clone)]
pub(crate) struct LayerRow {
    pub uid: String,
    pub dup: bool,
    pub effector: Option<EffectorRow>,
}

#[derive(Debug, Clone)]
pub(crate) struct EffectorRow {
    pub visible: Option<bool>,
    pub target: String,
    pub pos: Option<[f64; 2]>,
    pub falloff: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct LogRow {
    pub k: u32,
    pub pivot: [f64; 2],
    pub place: [f64; 7],
    pub dup_3d: Option<[f64; 3]>,
}

#[derive(Debug, Clone)]
pub(crate) enum Out {
    Same,
    Throws,
    Big { n: usize, first: Want, last: Want },
    List(Vec<Want>),
}

#[derive(Clone)]
pub(crate) struct Scenario {
    pub name: String,
    pub entry: String,
    pub edit: bool,
    pub three_d: bool,
    pub frame: f64,
    pub canvas: [f64; 2],
    pub can_resample: bool,
    pub span: Option<SpanBounds>,
    pub big: bool,
    pub dupj: Option<Value>,
    pub layers: Vec<LayerRow>,
    pub values: Vec<(String, String, Sampled)>,
    pub motion: Vec<(String, Option<MotionDelta>)>,
    pub strokes: Vec<(String, u64, Vec<ObjectRecord>)>,
    pub base: Vec<ObjectRecord>,
    pub calls: Vec<String>,
    pub log: Vec<LogRow>,
    pub out: Option<Out>,
    pub effs: Option<String>,
}

fn field<'a>(line: &'a str, key: &str) -> &'a str {
    line.split_whitespace()
        .find_map(|t| t.strip_prefix(key))
        .unwrap_or_else(|| panic!("field {key} in {line}"))
}

/// Every `"@hex"` string of a descriptor becomes the number it names.
pub(crate) fn revive(v: Value) -> Value {
    match v {
        Value::String(s) if s.starts_with('@') => {
            Value::Number(Number::from_f64(hex(&s[1..])).expect("finite descriptor number"))
        }
        Value::Array(a) => Value::Array(a.into_iter().map(revive).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, x)| (k, revive(x))).collect()),
        other => other,
    }
}

fn parse_layer(line: &str) -> LayerRow {
    let uid = line.split_whitespace().nth(1).expect("lay uid").to_string();
    let effector = if field(line, "eff=") == "1" {
        let pos = field(line, "pos=");
        let fall = field(line, "fall=");
        Some(EffectorRow {
            visible: match field(line, "vis=") {
                "u" => None,
                v => Some(v == "1"),
            },
            target: field(line, "tgt=").to_string(),
            pos: if pos == "-" {
                None
            } else {
                let (x, y) = pos.split_once(',').expect("x,y");
                Some([hex(x), hex(y)])
            },
            falloff: if fall == "-" {
                None
            } else {
                Some(fall.to_string())
            },
        })
    } else {
        None
    };
    LayerRow {
        uid,
        dup: field(line, "dup=") == "1",
        effector,
    }
}

fn parse_log(t: &[&str]) -> LogRow {
    let n: Vec<f64> = t[3..10].iter().map(|h| hex(h)).collect();
    LogRow {
        k: t[0].parse().expect("k"),
        pivot: [hex(t[1]), hex(t[2])],
        place: [n[0], n[1], n[2], n[3], n[4], n[5], n[6]],
        dup_3d: if t[10] == "-" {
            None
        } else {
            let v: Vec<f64> = t[10].split(':').map(hex).collect();
            Some([v[0], v[1], v[2]])
        },
    }
}

/// Every scenario of an oracle, in file order.
pub(crate) fn scenarios(oracle: &'static str) -> Vec<Scenario> {
    let mut out = Vec::new();
    let mut cur: Option<Scenario> = None;
    for line in oracle.lines().filter(|l| !l.starts_with('#')) {
        let (kind, rest) = line.split_once(' ').unwrap_or((line, ""));
        let t: Vec<&str> = rest.split_whitespace().collect();
        if kind == "scn" {
            cur = Some(Scenario {
                name: rest.to_string(),
                entry: String::new(),
                edit: false,
                three_d: false,
                frame: 0.0,
                canvas: [0.0; 2],
                can_resample: false,
                span: None,
                big: false,
                dupj: None,
                layers: Vec::new(),
                values: Vec::new(),
                motion: Vec::new(),
                strokes: Vec::new(),
                base: Vec::new(),
                calls: Vec::new(),
                log: Vec::new(),
                out: None,
                effs: None,
            });
            continue;
        }
        if kind == "end" {
            out.push(cur.take().expect("scn before end"));
            continue;
        }
        let sc = cur.as_mut().expect("row outside a scenario");
        match kind {
            "cfg" => {
                sc.entry = field(line, "entry=").to_string();
                sc.edit = field(line, "edit=") == "1";
                sc.three_d = field(line, "three_d=") == "1";
                sc.frame = hex(field(line, "frame="));
                sc.canvas = [hex(field(line, "cw=")), hex(field(line, "ch="))];
                sc.can_resample = field(line, "can_resample=") == "1";
                sc.big = field(line, "big=") == "1";
                let span = field(line, "span=");
                if span != "-" {
                    let (a, b) = span.split_once('/').expect("start/len");
                    sc.span = Some(SpanBounds {
                        start: hex(a),
                        len: hex(b),
                    });
                }
            }
            "dupj" => {
                sc.dupj = if t[0] == "-" {
                    None
                } else {
                    Some(revive(serde_json::from_str(t[0]).expect("descriptor json")))
                }
            }
            "lay" => sc.layers.push(parse_layer(line)),
            "mv" => {
                let sampled = if t[2] == "null" {
                    None
                } else {
                    Some(parse_nums(t[2]))
                };
                sc.values
                    .push((t[0].to_string(), t[1].to_string(), sampled));
            }
            "mm" => {
                let delta = if t[1] == "-" {
                    None
                } else {
                    Some(MotionDelta {
                        dx: js(t[1]),
                        dy: js(t[2]),
                    })
                };
                sc.motion.push((t[0].to_string(), delta));
            }
            "st" => sc
                .strokes
                .push((t[0].to_string(), hex(t[1]).to_bits(), parse_list(t[2]))),
            "base" => sc.base = parse_list(t[0]),
            "pl" => sc.calls.push(rest.to_string()),
            "log" => sc.log.push(parse_log(&t)),
            "out" => {
                sc.out = Some(match t[0] {
                    "same" => Out::Same,
                    "throws" => Out::Throws,
                    "big" => Out::Big {
                        n: t[1].parse().expect("count"),
                        first: parse_wants(t[2]).remove(0),
                        last: parse_wants(t[3]).remove(0),
                    },
                    list => Out::List(parse_wants(list)),
                })
            }
            "effs" => sc.effs = Some(t[0].to_string()),
            other => panic!("unknown row {other}"),
        }
    }
    assert!(cur.is_none(), "unterminated scenario");
    out
}

/// The two ports over one scenario's tables; `calls` is what the kernel asked.
pub(crate) struct World<'a> {
    pub sc: &'a Scenario,
    pub calls: RefCell<Vec<String>>,
}

impl<'a> World<'a> {
    pub(crate) fn new(sc: &'a Scenario) -> World<'a> {
        World {
            sc,
            calls: RefCell::new(Vec::new()),
        }
    }

    /// The distinct calls in first-call order (the oracle's `pl` rows).
    pub(crate) fn distinct_calls(&self) -> Vec<String> {
        let mut seen: Vec<String> = Vec::new();
        for call in self.calls.borrow().iter() {
            if !seen.contains(call) {
                seen.push(call.clone());
            }
        }
        seen
    }
}

impl MotionPort for World<'_> {
    fn value(&self, layer_uid: &str, key: &'static str) -> Result<Sampled, Unavailable> {
        self.calls.borrow_mut().push(format!("V {layer_uid} {key}"));
        self.sc
            .values
            .iter()
            .find(|(uid, k, _)| uid == layer_uid && k == key)
            .map(|(_, _, v)| v.clone())
            .ok_or(Unavailable::Property(key))
    }

    fn motion_delta(&self, layer_uid: &str) -> Result<Option<MotionDelta>, Unavailable> {
        self.calls.borrow_mut().push(format!("M {layer_uid}"));
        self.sc
            .motion
            .iter()
            .find(|(uid, _)| uid == layer_uid)
            .map(|(_, d)| *d)
            .ok_or(Unavailable::LayerMotion)
    }
}

impl LayerSources for World<'_> {
    fn has_layer(&self, uid: &str) -> bool {
        self.calls.borrow_mut().push(format!("H {uid}"));
        self.sc.layers.iter().any(|l| l.uid == uid)
    }

    fn strokes_at(&self, uid: &str, frame: f64) -> Vec<ObjectRecord> {
        self.calls
            .borrow_mut()
            .push(format!("S {uid} {:016x}", frame.to_bits()));
        self.sc
            .strokes
            .iter()
            .find(|(u, bits, _)| u == uid && *bits == frame.to_bits())
            .map(|(_, _, list)| list.clone())
            .unwrap_or_default()
    }
}

/// The document side of a scenario: the duplicator set and the context's layers.
pub(crate) fn document_of(sc: &Scenario) -> (DuplicatorSet, Vec<ObjectLayer>) {
    let mut duplicators = Vec::new();
    let mut effector_layers = Vec::new();
    let mut layers = Vec::new();
    for row in &sc.layers {
        let mut layer = json!({ "layerUid": row.uid });
        if let Some(e) = &row.effector {
            if let Some(v) = e.visible {
                layer["visible"] = json!(v);
            }
            let mut target = json!({ "targetLayerUid": e.target });
            if let Some(p) = e.pos {
                let n = |x: f64| Value::Number(Number::from_f64(x).expect("finite"));
                target["pos"] = Value::Array(vec![n(p[0]), n(p[1])]);
            }
            if let Some(f) = &e.falloff {
                target["falloff"] = json!(f);
            }
            effector_layers.push(
                json!({ "contextId": "scene-root", "layerUid": row.uid, "effector": target }),
            );
        }
        if row.uid == "host" {
            if let Some(d) = &sc.dupj {
                duplicators.push(json!({
                    "contextId": "scene-root", "layerUid": "host", "duplicator": d,
                }));
            }
        } else if row.dup {
            duplicators.push(json!({
                "contextId": "scene-root", "layerUid": row.uid, "duplicator": {},
            }));
        }
        layers.push(serde_json::from_value(layer).expect("a decodable layer"));
    }
    let set = json!({
        "format": "nemo.native-duplicator-set",
        "formatVersion": 1,
        "duplicators": duplicators,
        "effectorLayers": effector_layers,
    });
    (
        serde_json::from_value(set).expect("a decodable duplicator set"),
        layers,
    )
}

/// The scenario with this name.
pub(crate) fn named<'a>(all: &'a [Scenario], name: &str) -> &'a Scenario {
    all.iter()
        .find(|s| s.name == name)
        .unwrap_or_else(|| panic!("scenario {name}"))
}
