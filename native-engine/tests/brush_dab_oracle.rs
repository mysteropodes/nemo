//! G16 option C (#2361): the native dab stamper against the baseline stamper run over the shipped Paper.js
//! (`tests/native-object-vector-fixtures/brush-dab-oracle.json`, #2416): every built-in preset on a straight and a
//! curved path with fixed seeds, plus a pressure profile. The first eight dabs of each case are compared anchor and
//! handle for handle (the oracle rounds to 1e-6), and every dab by its anchor centroid, extent, opacity, value delta,
//! closure and segment count (rounded to 1e-3). Two project catalog presets with captured stamps (`customCases`) go
//! through the catalog mapping and are compared on every dab.
use super::presets::{builtin, from_catalog, DabPreset};
use crate::object_document::BrushPresetRecord;
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
    stamp_with(o, case, &builtin(case["preset"].as_str().unwrap()).unwrap())
}
fn stamp_with(o: &Value, case: &Value, preset: &DabPreset) -> Vec<Dab> {
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
    let mut rng = SeededRng::new(case["seed"].as_u64().unwrap() as u32);
    build_brush_dabs(
        &segments,
        false,
        preset,
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

#[test]
fn every_custom_catalog_preset_matches_the_baseline_dab_for_dab() {
    let o = oracle();
    let cases = o["customCases"].as_array().unwrap();
    assert_eq!(cases.len(), 2);
    for case in cases {
        let id = case["id"].as_str().unwrap();
        let record: BrushPresetRecord = serde_json::from_value(case["preset"].clone()).unwrap();
        let dabs = stamp_with(&o, case, &from_catalog(&record).unwrap());
        let expected = case["dabs"].as_array().unwrap();
        assert_eq!(dabs.len(), expected.len(), "{id}");
        for (i, (dab, e)) in dabs.iter().zip(expected).enumerate() {
            let part = &e["parts"][0];
            let segments = part["segments"].as_array().unwrap();
            assert_eq!(part["closed"].as_bool(), Some(dab.path.closed), "{id} dab {i}");
            assert_eq!(segments.len(), dab.path.segments.len(), "{id} dab {i}");
            for (s, e) in dab.path.segments.iter().zip(segments) {
                let got = [s.p[0], s.p[1], s.h_in[0], s.h_in[1], s.h_out[0], s.h_out[1]];
                let same = got.iter().zip(e.as_array().unwrap()).all(|(g, e)| near(*g, f(e), 1.5e-6));
                assert!(same, "{id} dab {i}: {got:?} != {e}");
            }
            assert!(near(dab.opacity, f(&e["opacity"]), 1.5e-6), "{id} dab {i} opacity");
            assert!(near(dab.value_delta, f(&e["valueDelta"]), 1.5e-6), "{id} dab {i} value");
        }
    }
}

#[test]
fn a_captured_stamp_without_its_handles_has_no_catalog_preset() {
    let o = oracle();
    let mut preset = o["customCases"][0]["preset"].clone();
    preset["customStamp"]["segments"][1].as_object_mut().unwrap().remove("handleOut");
    let record: BrushPresetRecord = serde_json::from_value(preset).unwrap();
    assert_eq!(from_catalog(&record), None, "the baseline reads s.handleOut[0] and throws");
}

/// #2361 draw-texture-symmetry-trim: a mirrored stroke's copy is an anchor of its own group and seed (the seed the native
/// symmetry adapter derives from the drawn stroke's). Native stamps the drawn curve and its reflection, each with its own
/// seed, as the baseline stamper does over the same two paths: every dab of both, dab for dab on the first eight and by
/// summary on the rest; the copy's dabs are its own, not the drawn stroke's mirrored.
#[test]
fn a_mirrored_seeded_stroke_stamps_the_drawn_curve_and_its_copy_each_from_its_own_seed() {
    let o = oracle();
    let cases = o["mirrorCases"].as_array().unwrap();
    assert_eq!(cases.len(), 8);
    let full = o["full"].as_u64().unwrap() as usize;
    for pair in cases.chunks(2) {
        let (drawn, copy) = (&pair[0], &pair[1]);
        assert_eq!((drawn["path"].as_str(), copy["path"].as_str()), (Some("curve"), Some("curveMirror")));
        assert_ne!(drawn["seed"], copy["seed"], "{}: the copy has its own seed", copy["id"]);
        let mut stamped = Vec::new();
        for case in pair {
            let id = case["id"].as_str().unwrap();
            let dabs = stamp(&o, case);
            assert_eq!(dabs.len() as u64, case["count"].as_u64().unwrap(), "{id}");
            for (i, dab) in dabs.iter().enumerate() {
                let row = case["summary"][i].as_array().unwrap();
                let n = dab.path.segments.len() as f64;
                let centre = [
                    dab.path.segments.iter().map(|s| s.p[0]).sum::<f64>() / n,
                    dab.path.segments.iter().map(|s| s.p[1]).sum::<f64>() / n,
                ];
                assert!(
                    near(centre[0], f(&row[0]), 1.5e-3)
                        && near(centre[1], f(&row[1]), 1.5e-3)
                        && near(dab.opacity, f(&row[7]), 1.5e-3)
                        && near(dab.value_delta, f(&row[6]), 1.5e-3),
                    "{id} dab {i}: {centre:?} {} {} != {row:?}",
                    dab.opacity,
                    dab.value_delta
                );
                if i < full {
                    let segments = case["dabs"][i]["parts"][0]["segments"].as_array().unwrap();
                    assert_eq!(segments.len(), dab.path.segments.len(), "{id} dab {i}");
                    for (s, e) in dab.path.segments.iter().zip(segments) {
                        let got = [s.p[0], s.p[1], s.h_in[0], s.h_in[1], s.h_out[0], s.h_out[1]];
                        assert!(
                            got.iter().zip(e.as_array().unwrap()).all(|(g, e)| near(*g, f(e), 1.5e-6)),
                            "{id} dab {i}: {got:?} != {e}"
                        );
                    }
                }
            }
            stamped.push(dabs);
        }
        // The same seed over the mirror image is not the copy: the drawn stroke's own seed gives other dabs.
        let mut reseeded = copy.clone();
        reseeded["seed"] = drawn["seed"].clone();
        let same_seed = stamp(&o, &reseeded);
        let moved = same_seed.len() != stamped[1].len()
            || same_seed.iter().zip(&stamped[1]).any(|(a, b)| !near(a.value_delta, b.value_delta, 1e-9) || !near(a.opacity, b.opacity, 1e-9));
        assert!(moved, "{}: a copy stamped with the drawn seed would repeat the drawn stroke's jitter", copy["id"]);
    }
}
