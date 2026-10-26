//! G16 option C (#2361): the native dab stamper against the baseline stamper run over the shipped Paper.js
//! (`tests/native-object-vector-fixtures/brush-dab-oracle.json`, #2416): every built-in preset on a straight and a
//! curved path with fixed seeds, plus a pressure profile. The first eight dabs of each case are compared anchor and
//! handle for handle (the oracle rounds to 1e-6), and every dab by its anchor centroid, extent, opacity, value delta,
//! closure and segment count (rounded to 1e-3).
use super::presets::builtin;
use super::sampling::{jitter_color_value, width_at_frac, SeededRng};
use super::stamp::{build_brush_dabs, Dab};
use serde_json::Value;

const ORACLE: &str =
    include_str!("../../tests/native-object-vector-fixtures/brush-dab-oracle.json");

fn oracle() -> Value {
    serde_json::from_str(ORACLE).unwrap()
}
fn f(v: &Value) -> f64 {
    v.as_f64().unwrap()
}
fn near(a: f64, b: f64, tolerance: f64) -> bool {
    (a - b).abs() <= tolerance
}
fn stamp(o: &Value, case: &Value) -> Vec<Dab> {
    let segments: Vec<[f64; 6]> = o["paths"][case["path"].as_str().unwrap()]["segments"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            [
                f(&s[0][0]),
                f(&s[0][1]),
                f(&s[1][0]),
                f(&s[1][1]),
                f(&s[2][0]),
                f(&s[2][1]),
            ]
        })
        .collect();
    let profile: Option<Vec<(f64, f64)>> = case["profile"]
        .as_array()
        .map(|p| p.iter().map(|s| (f(&s["t"]), f(&s["width"]))).collect());
    let preset = builtin(case["preset"].as_str().unwrap()).unwrap();
    let mut rng = SeededRng::new(case["seed"].as_u64().unwrap() as u32);
    build_brush_dabs(
        &segments,
        false,
        &preset,
        f(&case["width"]),
        &mut rng,
        profile.as_deref(),
    )
    .unwrap()
}

#[test]
fn the_seeded_generator_is_the_baseline_mulberry32() {
    let o = oracle();
    let mut rng = SeededRng::new(1);
    for expected in o["seededRng1"].as_array().unwrap() {
        assert_eq!(rng.next(), f(expected));
    }
}

#[test]
fn every_built_in_preset_matches_the_baseline_dab_for_dab() {
    let o = oracle();
    let full = o["full"].as_u64().unwrap() as usize;
    let cases = o["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 57);
    let mut failures = Vec::new();
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let dabs = stamp(&o, case);
        if dabs.len() as u64 != case["count"].as_u64().unwrap() {
            failures.push(format!(
                "{id}: {} dabs, baseline {}",
                dabs.len(),
                case["count"]
            ));
            continue;
        }
        'dab: for (i, dab) in dabs.iter().enumerate() {
            if i < full {
                let expected = &case["dabs"][i];
                let part = &expected["parts"][0];
                let segments = part["segments"].as_array().unwrap();
                if segments.len() != dab.path.segments.len()
                    || part["closed"].as_bool() != Some(dab.path.closed)
                {
                    failures.push(format!(
                        "{id} dab {i}: shape {} segments",
                        dab.path.segments.len()
                    ));
                    break 'dab;
                }
                for (k, (s, e)) in dab.path.segments.iter().zip(segments).enumerate() {
                    let got = [s.p[0], s.p[1], s.h_in[0], s.h_in[1], s.h_out[0], s.h_out[1]];
                    if got
                        .iter()
                        .zip(e.as_array().unwrap())
                        .any(|(g, e)| !near(*g, f(e), 1.5e-6))
                    {
                        failures.push(format!("{id} dab {i} segment {k}: {got:?} != {e}"));
                        break 'dab;
                    }
                }
                if !near(dab.opacity, f(&expected["opacity"]), 1.5e-6)
                    || !near(dab.value_delta, f(&expected["valueDelta"]), 1.5e-6)
                {
                    failures.push(format!(
                        "{id} dab {i}: opacity {} value {}",
                        dab.opacity, dab.value_delta
                    ));
                    break 'dab;
                }
            }
            let row = case["summary"][i].as_array().unwrap();
            let points: Vec<[f64; 2]> = dab.path.segments.iter().map(|s| s.p).collect();
            let n = points.len() as f64;
            let fold = |pick: fn([f64; 2]) -> f64, min: bool| {
                points.iter().map(|p| pick(*p)).fold(
                    if min {
                        f64::INFINITY
                    } else {
                        f64::NEG_INFINITY
                    },
                    |a, b| if min { a.min(b) } else { a.max(b) },
                )
            };
            let got = [
                points.iter().map(|p| p[0]).sum::<f64>() / n,
                points.iter().map(|p| p[1]).sum::<f64>() / n,
                fold(|p| p[0], true),
                fold(|p| p[1], true),
                fold(|p| p[0], false),
                fold(|p| p[1], false),
                dab.value_delta,
                dab.opacity,
            ];
            if got.iter().zip(row).any(|(g, e)| !near(*g, f(e), 1.5e-3))
                || f(&row[8]) != if dab.path.closed { 1.0 } else { 0.0 }
                || f(&row[9]) != n
            {
                failures.push(format!("{id} dab {i}: summary {got:?} != {row:?}"));
                break;
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of 57 cases differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn width_profile_and_colour_jitter_follow_the_baseline_helpers() {
    let profile = [(0.0, 2.0), (0.5, 20.0), (1.0, 6.0)];
    assert_eq!(
        [
            width_at_frac(&profile, -1.0),
            width_at_frac(&profile, 0.25),
            width_at_frac(&profile, 0.75),
            width_at_frac(&profile, 2.0)
        ],
        [2.0, 11.0, 13.0, 6.0]
    );
    assert_eq!(
        jitter_color_value([0.5, 0.25, 1.0, 0.4], 0.0),
        [0.5, 0.25, 1.0, 0.4]
    );
    assert_eq!(
        jitter_color_value([0.5, 0.25, 1.0, 0.4], -0.5),
        [0.25, 0.125, 0.5, 0.4]
    );
    assert_eq!(
        jitter_color_value([0.5, 0.0, 1.0, 1.0], 2.0),
        [1.0, 1.0, 1.0, 1.0]
    );
    assert!(builtin("none").is_none() && builtin("charcoal-smooth").is_some());
}

#[test]
fn the_comparison_catches_another_seed() {
    let o = oracle();
    let case = &o["cases"][0];
    let expected = &case["dabs"][0]["parts"][0]["segments"][0];
    let mut other = case.clone();
    other["seed"] = serde_json::json!(2);
    let first = stamp(&o, &other)[0].path.segments[0];
    assert!(!near(first.p[0], f(&expected[0]), 1.5e-6) || !near(first.p[1], f(&expected[1]), 1.5e-6));
    let same = stamp(&o, case)[0].path.segments[0];
    assert!(near(same.p[0], f(&expected[0]), 1.5e-6) && near(same.p[1], f(&expected[1]), 1.5e-6));
}
