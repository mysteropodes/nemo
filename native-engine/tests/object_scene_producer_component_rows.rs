//! Oracle row parser for the component evaluated-shape kernel (L07-H).
//!
//! `fixtures/object-scene-producer-component/oracle.txt` was written by
//! `tools/gen_oracle.js` from baseline fbe4b8a3: `cloneStrokeForTransform`,
//! `applyMatrixToStrokeData` and `symMatrixOf` (app.js) plus `bezierEase`, `snap`,
//! `cameraAtFrameFor` and `cameraMatrixAtFrame` (camera.js) run verbatim against the
//! real Paper.js 0.12.18 `Matrix`. TAB separated rows (`#` lines are provenance):
//! `A id in m out`, `D id in m factor k out`, `I ii strokes`, `L uid frame ii|-`,
//! `P name sym|- cam|- w h`, `O name ii cam|- out`. `out` is JSON, `!throws` or
//! `!nonfinite`. JSON numbers are shortest round-trip text, so parsing them recovers
//! the exact doubles; `-0` prints as `0`, hence numbers compare with `==`.

use crate::evaluation::object_component_kernels::object_component_playback::{
    OuterKey, PlayMode, PlaybackInput,
};
use crate::evaluation::property_camera_eval::CameraKey;
use serde_json::Value;
use std::collections::BTreeMap;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-scene-producer-component/oracle.txt");

#[derive(Clone, Debug)]
pub(crate) enum Expect {
    Json(Value),
    Throws,
    NonFinite,
}

pub(crate) struct ApplyRow {
    pub(crate) id: String,
    pub(crate) input: Value,
    pub(crate) matrix: [f64; 6],
    pub(crate) out: Expect,
}

pub(crate) struct DupRow {
    pub(crate) id: String,
    pub(crate) input: Value,
    pub(crate) matrix: [f64; 6],
    pub(crate) factor: f64,
    pub(crate) k: u32,
    pub(crate) out: Expect,
}

pub(crate) struct LayerRow {
    pub(crate) uid: String,
    pub(crate) frame: i64,
    pub(crate) inner: Option<f64>,
}

pub(crate) struct Scenario {
    pub(crate) sym: Option<[f64; 6]>,
    pub(crate) cam: Option<Vec<CameraKey>>,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

pub(crate) struct OutRow {
    pub(crate) name: String,
    pub(crate) inner: u32,
    pub(crate) cam: Option<[f64; 6]>,
    pub(crate) out: Vec<Value>,
}

#[derive(Default)]
pub(crate) struct Oracle {
    pub(crate) apply: Vec<ApplyRow>,
    pub(crate) dup: Vec<DupRow>,
    pub(crate) inputs: BTreeMap<u32, Vec<Value>>,
    pub(crate) layers: Vec<LayerRow>,
    pub(crate) scenarios: BTreeMap<String, Scenario>,
    pub(crate) outs: Vec<OutRow>,
}

fn json(text: &str) -> Value {
    serde_json::from_str(text).expect("oracle json")
}

fn six(text: &str) -> [f64; 6] {
    let v: Vec<f64> = json(text)
        .as_array()
        .expect("matrix array")
        .iter()
        .map(|n| n.as_f64().expect("matrix number"))
        .collect();
    v.try_into().expect("six numbers")
}

fn expect(text: &str) -> Expect {
    match text {
        "!throws" => Expect::Throws,
        "!nonfinite" => Expect::NonFinite,
        t => Expect::Json(json(t)),
    }
}

fn num(v: &Value, key: &str) -> f64 {
    v[key].as_f64().expect("number field")
}

fn pair(v: &Value) -> [f64; 2] {
    [
        v[0].as_f64().expect("pair 0"),
        v[1].as_f64().expect("pair 1"),
    ]
}

fn camera_key(v: &Value) -> CameraKey {
    CameraKey {
        frame: num(v, "frame"),
        x: num(v, "x"),
        y: num(v, "y"),
        w: num(v, "w"),
        rot: Some(num(v, "rot")),
        h_out: Some(pair(&v["hOut"])),
        h_in: Some(pair(&v["hIn"])),
        ease: v["ease"].as_array().map(|e| {
            let n: Vec<f64> = e.iter().map(|x| x.as_f64().expect("ease")).collect();
            [n[0], n[1], n[2], n[3]]
        }),
    }
}

pub(crate) fn oracle() -> Oracle {
    let mut o = Oracle::default();
    for line in ORACLE
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
    {
        let c: Vec<&str> = line.split('\t').collect();
        match c[0] {
            "A" => o.apply.push(ApplyRow {
                id: c[1].to_string(),
                input: json(c[2]),
                matrix: six(c[3]),
                out: expect(c[4]),
            }),
            "D" => o.dup.push(DupRow {
                id: c[1].to_string(),
                input: json(c[2]),
                matrix: six(c[3]),
                factor: c[4].parse().expect("factor"),
                k: c[5].parse().expect("dup index"),
                out: expect(c[6]),
            }),
            "I" => {
                let strokes = json(c[2]).as_array().expect("strokes").clone();
                o.inputs.insert(c[1].parse().expect("inner frame"), strokes);
            }
            "L" => o.layers.push(LayerRow {
                uid: c[1].to_string(),
                frame: c[2].parse().expect("frame"),
                inner: (c[3] != "-").then(|| c[3].parse().expect("inner")),
            }),
            "P" => {
                let cam = (c[3] != "-").then(|| {
                    json(c[3])
                        .as_array()
                        .expect("keys")
                        .iter()
                        .map(camera_key)
                        .collect()
                });
                o.scenarios.insert(
                    c[1].to_string(),
                    Scenario {
                        sym: (c[2] != "-").then(|| six(c[2])),
                        cam,
                        w: c[4].parse().expect("canvas w"),
                        h: c[5].parse().expect("canvas h"),
                    },
                );
            }
            "O" => o.outs.push(OutRow {
                name: c[1].to_string(),
                inner: c[2].parse().expect("inner frame"),
                cam: (c[3] != "-").then(|| six(c[3])),
                out: json(c[4]).as_array().expect("outputs").clone(),
            }),
            other => panic!("unknown oracle row kind {other}"),
        }
    }
    o
}

/// Numeric JSON equality on `f64` (`0 == -0`, integer text equals float text).
pub(crate) fn same_json(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| same_json(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| same_json(v, w)))
        }
        _ => a == b,
    }
}

/// The fixture layers of `tests/fixtures/components/project.json` as playback inputs
/// (`corpus.cjs` `inst(...)` defaults: speed 1, placedAt 0, singleFrame 0, 24 outer
/// frames with a keyframe at 0; `ly_comp_hold` carries `componentFrame` 3 on frame 0,
/// `ly_comp_blank` a `blankOverride` keyframe at 12).
pub(crate) fn fixture_playback(uid: &str) -> PlaybackInput {
    let mut outer = vec![OuterKey::default(); 24];
    outer[0].is_keyframe = true;
    let base = |play_mode| PlaybackInput {
        play_mode,
        speed: Some(1.0),
        placed_at: Some(0.0),
        single_frame: Some(0.0),
        ..PlaybackInput::default()
    };
    let mut input = match uid {
        "ly_comp_loop" => PlaybackInput {
            placed_at: Some(4.0),
            ..base(PlayMode::Loop)
        },
        "ly_comp_once" => base(PlayMode::Once),
        "ly_comp_ping" => base(PlayMode::PingPong),
        "ly_comp_single" => PlaybackInput {
            single_frame: Some(5.0),
            ..base(PlayMode::Single)
        },
        "ly_comp_speed" => PlaybackInput {
            speed: Some(2.0),
            ..base(PlayMode::Loop)
        },
        "ly_comp_hold" | "ly_comp_blank" => base(PlayMode::Loop),
        other => panic!("unknown fixture layer {other}"),
    };
    if uid == "ly_comp_hold" {
        outer[0].component_frame = Some(3.0);
    }
    if uid == "ly_comp_blank" {
        outer[12] = OuterKey {
            is_keyframe: true,
            component_frame: None,
            blank_override: Some(true),
        };
    }
    input.outer_keys = outer;
    input
}
