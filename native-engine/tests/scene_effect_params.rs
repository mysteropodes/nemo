//! Oracle-driven tests for the effect-stack pass planner (L08-C). The hand
//! written literal cases live in `scene_effect_params_cases.rs`, the mutation
//! and negative controls in `scene_effect_params_controls.rs`.
//!
//! `fixtures/scene-effect-params/oracle.txt` (`ORACLE`) was produced by
//! `tools/gen_scene_effect_params_oracle.js` against baseline commit
//! fbe4b8a33e6d5a229f014154df2ba21988a9fabc. Its `#` header pins the SHA-256
//! of the extracted `geometry-wasm/src/engine.rs` regions (EffectIn,
//! run_one_effect, apply_effect_stack, apply_effect_stack_into, the zoom
//! floor) and of the verbatim `sceneEffectsOf` / `paramValueAt` JS that ran in
//! a Node `vm`. The Rust control flow was executed through a hand-written Node
//! transliteration (no compiler was available when the oracle was cut) whose
//! number tables (simple_fx ids, defaults, zoom-scaled set, glow / hqBloom /
//! colorAdjust / vignette / blur defaults, composite mode) are PARSED out of
//! the extracted Rust text.
//!
//! One JSON row per line: `zoom` (effect zoom, f64), `registered` (custom
//! effect names with a pipeline), `effects` (wire `EffectIn` entries:
//! `effectType`, `enabled`, `p1..p8`; absent or null parameters are `None`),
//! and the expected `stack` (apply_effect_stack) and `into`
//! (apply_effect_stack_into) plans as `{landing, steps}`. Each step is
//! `<effect index>:<source>><target>:<pass>` with every f32 rendered as its
//! 8-digit hex bit pattern (`describe`). Bridge rows add `wire`: the exact JSON
//! the verbatim JS `sceneEffectsOf` emitted for the scenario.

use super::*;
use serde_json::Value;

pub(crate) const ORACLE: &str = include_str!("fixtures/scene-effect-params/oracle.txt");

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Expect {
    pub(crate) landing: String,
    pub(crate) steps: Vec<String>,
}

pub(crate) struct Row {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) zoom: f64,
    pub(crate) registered: Vec<String>,
    pub(crate) effects: Vec<EffectSpec>,
    pub(crate) stack: Expect,
    pub(crate) into: Expect,
    pub(crate) wire: Option<String>,
}

pub(crate) fn bits(x: f32) -> String {
    format!("{:08x}", x.to_bits())
}

fn fx_text(p: &FxParams) -> String {
    let list: Vec<String> = p.params.iter().map(|x| bits(*x)).collect();
    let bounds = match p.bounds {
        FxBounds::Attached => "attached",
        FxBounds::FullCanvas => "full",
    };
    format!("id={};{};{}", bits(p.effect_id), list.join(","), bounds)
}

pub(crate) fn pass_text(pass: &EffectPass) -> String {
    match pass {
        EffectPass::ColorAdjust { p1, p2 } => format!("color({},{})", bits(*p1), bits(*p2)),
        EffectPass::Vignette { strength, radius } => {
            format!("vig({},{})", bits(*strength), bits(*radius))
        }
        EffectPass::Blur { radius_px } => format!("blur({})", bits(*radius_px)),
        EffectPass::Glow {
            radius_px,
            composite_mode,
        } => format!("glow({};m{})", bits(*radius_px), composite_mode),
        EffectPass::HqBloom {
            extract,
            radius_px,
            composite_mode,
        } => format!(
            "hq({};r={};m{})",
            fx_text(extract),
            bits(*radius_px),
            composite_mode
        ),
        EffectPass::SimpleFx(p) => format!("fx({})", fx_text(p)),
        EffectPass::Custom {
            name,
            registered,
            params,
        } => format!(
            "custom({};reg{};{})",
            name,
            u8::from(*registered),
            fx_text(params)
        ),
    }
}

fn step_text(step: &PassStep) -> String {
    let source = match step.source {
        PassSource::Initial => "init",
        PassSource::StackA => "A",
        PassSource::StackB => "B",
    };
    let target = match step.target {
        PassTarget::StackA => "A",
        PassTarget::StackB => "B",
        PassTarget::Final => "final",
    };
    format!(
        "{}:{}>{}:{}",
        step.effect_index,
        source,
        target,
        pass_text(&step.pass)
    )
}

pub(crate) fn describe(plan: &EffectStackPlan) -> Expect {
    let landing = match plan.landing {
        Landing::Empty => "empty",
        Landing::StackA => "A",
        Landing::StackB => "B",
        Landing::Final => "final",
    };
    Expect {
        landing: landing.to_string(),
        steps: plan.steps.iter().map(step_text).collect(),
    }
}

/// Wire entry to spec with the serde defaults of `EffectIn`: missing or
/// non-string type is empty, missing or non-bool `enabled` is false, missing
/// or null parameters are `None`; numbers narrow `f64 as f32` as serde does.
pub(crate) fn spec_of(v: &Value) -> EffectSpec {
    let mut params: [Option<f32>; 8] = [None; 8];
    for (i, slot) in params.iter_mut().enumerate() {
        let key = format!("p{}", i + 1);
        *slot = v
            .get(key.as_str())
            .and_then(Value::as_f64)
            .map(|x| x as f32);
    }
    EffectSpec {
        effect_type: v
            .get("effectType")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        enabled: v.get("enabled").and_then(Value::as_bool).unwrap_or(false),
        params,
    }
}

fn expect_of(v: &Value) -> Expect {
    Expect {
        landing: v["landing"].as_str().expect("landing").to_string(),
        steps: v["steps"]
            .as_array()
            .expect("steps")
            .iter()
            .map(|s| s.as_str().expect("step").to_string())
            .collect(),
    }
}

pub(crate) fn rows() -> Vec<Row> {
    ORACLE
        .lines()
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| {
            let v: Value = serde_json::from_str(l).expect("oracle row is JSON");
            Row {
                id: v["id"].as_str().expect("id").to_string(),
                kind: v["kind"].as_str().expect("kind").to_string(),
                zoom: v["zoom"].as_f64().expect("zoom"),
                registered: v["registered"]
                    .as_array()
                    .expect("registered")
                    .iter()
                    .map(|s| s.as_str().expect("name").to_string())
                    .collect(),
                effects: v["effects"]
                    .as_array()
                    .expect("effects")
                    .iter()
                    .map(spec_of)
                    .collect(),
                stack: expect_of(&v["stack"]),
                into: expect_of(&v["into"]),
                wire: v.get("wire").and_then(Value::as_str).map(str::to_owned),
            }
        })
        .collect()
}

pub(crate) fn registry(names: &[String]) -> impl Fn(&str) -> bool + '_ {
    move |n| names.iter().any(|r| r.as_str() == n)
}

fn header_lines() -> Vec<&'static str> {
    ORACLE.lines().filter(|l| l.starts_with('#')).collect()
}

#[test]
fn oracle_header_pins_the_baseline_commit_and_sources() {
    let header = header_lines().join("\n");
    assert!(header.contains("# baseline commit: fbe4b8a33e6d5a229f014154df2ba21988a9fabc"));
    assert!(header.contains("# command: node tools/gen_scene_effect_params_oracle.js"));
    for pin in PINS {
        assert!(header.contains(pin), "missing pin {pin}");
    }
}

const PINS: [&str; 7] = [
    "# sha256 effect_in = 8954723ab173798844f4ecf99d265903e0edf34123860a5984bbfca456f8ee33",
    "# sha256 run_one_effect = 73dd19b62c113e0448e227eddf0adb09a09d1bd5dd698923f756e761dcd1caaf",
    "# sha256 apply_effect_stack = 428b448a7a6922e29b1c1596b4e7df8abca47bd08c17473c99429a58645f94b1",
    "# sha256 apply_effect_stack_into = 98864ec21aa3e5c9949c44faab6b5a9b232d0bb2f9ca0f888c05f090b278e9e9",
    "# sha256 effect_zoom_floor = 5ddaf6398e075c3fb6181c8e33e73d20506926cf9542a57ebb071df537fb9d99",
    "# sha256 js:sceneEffectsOf = c40f29b9708d788447518c35fd838a24568d6b047a6296095dc51d1984420645",
    "# sha256 js:paramValueAt = f793a840dfb70c6691062d42f35b633d3ee0d55521d1ab8b581ef819d3e36730",
];

#[test]
fn oracle_rows_are_well_formed() {
    let all = rows();
    assert_eq!(all.len(), ROW_COUNT);
    assert_eq!(all.iter().filter(|r| r.kind == "hand").count(), HAND_COUNT);
    assert_eq!(
        all.iter().filter(|r| r.kind == "bridge").count(),
        BRIDGE_COUNT
    );
    let mut ids: Vec<&str> = all.iter().map(|r| r.id.as_str()).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), all.len(), "row ids are unique");
    for r in &all {
        assert_eq!(r.wire.is_some(), r.kind == "bridge", "{}", r.id);
        assert!(
            ["empty", "A", "B"].contains(&r.stack.landing.as_str()),
            "{}",
            r.id
        );
        assert!(
            ["empty", "final"].contains(&r.into.landing.as_str()),
            "{}",
            r.id
        );
        assert_eq!(r.stack.steps.len(), r.into.steps.len(), "{}", r.id);
    }
}

const ROW_COUNT: usize = 209;
const HAND_COUNT: usize = 198;
const BRIDGE_COUNT: usize = 11;

#[test]
fn oracle_stack_plans_are_reproduced() {
    let mut bad = Vec::new();
    for r in rows() {
        let got = describe(&plan_effect_stack(
            &r.effects,
            r.zoom,
            &registry(&r.registered),
        ));
        if got != r.stack {
            bad.push(r.id.clone());
        }
    }
    assert!(bad.is_empty(), "stack plans differ: {bad:?}");
}

#[test]
fn oracle_into_plans_are_reproduced() {
    let mut bad = Vec::new();
    for r in rows() {
        let got = describe(&plan_effect_stack_into(
            &r.effects,
            r.zoom,
            &registry(&r.registered),
        ));
        if got != r.into {
            bad.push(r.id.clone());
        }
    }
    assert!(bad.is_empty(), "into plans differ: {bad:?}");
}

#[test]
fn oracle_bridge_wire_text_parses_to_the_planned_entries() {
    let mut seen = 0;
    for r in rows().into_iter().filter(|r| r.kind == "bridge") {
        let wire: Value = serde_json::from_str(&r.wire.clone().expect("wire")).expect("wire JSON");
        let parsed: Vec<EffectSpec> = wire
            .as_array()
            .expect("array")
            .iter()
            .map(spec_of)
            .collect();
        assert_eq!(parsed, r.effects, "{}", r.id);
        seen += 1;
    }
    assert_eq!(seen, BRIDGE_COUNT);
}

#[test]
fn oracle_coverage_spans_every_effect_family_and_landing() {
    let all = rows();
    let mut types: Vec<&str> = Vec::new();
    for r in &all {
        for e in &r.effects {
            if !types.contains(&e.effect_type.as_str()) {
                types.push(e.effect_type.as_str());
            }
        }
    }
    for needed in [
        "colorAdjust",
        "vignette",
        "glow",
        "hqBloom",
        "sepia",
        "invert",
        "grayscale",
        "posterize",
        "pixelate",
        "chromaticAberration",
        "scanlines",
        "grain",
        "sharpen",
        "edgeDetect",
        "groundShadow",
        "contourBrut",
        "threshold",
        "halftone",
        "blur",
        "bogus",
        "",
        "Blur",
        "custom:a",
        "custom:b",
        "custom:",
    ] {
        assert!(types.contains(&needed), "no oracle row uses {needed:?}");
    }
    for landing in ["empty", "A", "B"] {
        assert!(all.iter().any(|r| r.stack.landing == landing), "{landing}");
    }
    assert!(all.iter().any(|r| r.into.landing == "final"));
    assert!(all.iter().any(|r| r.into.landing == "empty"));
    assert!(all.iter().any(|r| r.effects.iter().any(|e| !e.enabled)));
    assert!(all
        .iter()
        .any(|r| r.effects.iter().any(|e| e.params.contains(&Some(0.0)))));
    assert!(all.iter().any(|r| r.zoom <= 0.0001));
    assert!(all.iter().any(|r| r.zoom > 1.0));
}
