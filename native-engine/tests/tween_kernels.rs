//! #2358: independent pinned JavaScript baseline, produced before the port.
use super::*;
use serde_json::Value;
fn cases() -> Vec<Value> {
    serde_json::from_str::<Value>(include_str!("fixtures/tween-baseline/oracle.json")).unwrap()
        ["cases"]
        .as_array()
        .unwrap()
        .clone()
}
#[test]
fn tween_resampling_matches_independent_baseline() {
    for c in cases().into_iter().filter(|c| c["operation"] == "resample") {
        let s: StrokeIn = serde_json::from_value(c["input"]["stroke"].clone()).unwrap();
        let out = serde_json::to_value(resample_stroke_inner(
            &s,
            c["input"]["count"].as_u64().unwrap() as usize,
        ))
        .unwrap();
        assert_eq!(
            out["segments"].as_array().unwrap().len(),
            c["expected"]["segments"].as_array().unwrap().len()
        );
        for (a, b) in out["segments"]
            .as_array()
            .unwrap()
            .iter()
            .zip(c["expected"]["segments"].as_array().unwrap())
        {
            for field in ["point", "handleIn", "handleOut"] {
                for i in 0..2 {
                    assert!(
                        (a[field][i].as_f64().unwrap() - b[field][i].as_f64().unwrap()).abs()
                            < 1e-8,
                        "{field} {i}: {a} != {b}"
                    );
                }
            }
        }
    }
}

#[test]
fn tween_matching_preserves_baseline_partners() {
    for c in cases().into_iter().filter(|c| c["operation"] == "match") {
        let a: Vec<StrokeIn> = serde_json::from_value(c["input"]["a"].clone()).unwrap();
        let b: Vec<StrokeIn> = serde_json::from_value(c["input"]["b"].clone()).unwrap();
        let actual: Vec<_> = auto_match_inner(&a, &b)
            .iter()
            .map(|m| (m.a, m.b))
            .collect();
        let expected: Vec<_> = c["expected"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| {
                (
                    m["a"].as_u64().unwrap() as usize,
                    m["b"].as_u64().unwrap() as usize,
                )
            })
            .collect();
        assert_eq!(actual, expected, "{}", c["input"]["name"]);
        for (actual, expected) in auto_match_inner(&a, &b)
            .iter()
            .zip(c["expected"].as_array().unwrap())
        {
            assert!(
                (actual.score - expected["score"].as_f64().unwrap()).abs() < 1e-8,
                "score {} != {}",
                actual.score,
                expected
            );
        }
    }
}
#[test]
fn tween_assignment_preserves_independent_tie_and_trap_oracles() {
    assert_eq!(
        hungarian(&[vec![1., 1., 1.], vec![1., 1., 1.], vec![1., 1., 1.]]),
        vec![0, 1, 2]
    );
    assert_eq!(
        hungarian(&[vec![1., 2., 100.], vec![2., 100., 1.], vec![100., 1., 2.]]),
        vec![0, 2, 1]
    );
}

#[test]
fn tween_partition_excludes_dabs_and_ghosts_from_hungarian() {
    for c in cases()
        .into_iter()
        .filter(|c| c["operation"] == "partition")
    {
        let strokes = c["input"]["strokes"].as_array().unwrap();
        let actual = split_tweenables(strokes, c["input"]["manual"].as_bool().unwrap());
        assert_eq!(serde_json::to_value(actual).unwrap(), c["expected"]);
    }
}

#[test]
fn tween_rng_matches_original_seeded_rng_including_wraparound() {
    for c in cases().into_iter().filter(|c| c["operation"] == "rng") {
        let mut rng = TweenRng::new(c["input"]["seed"].as_u64().unwrap() as u32);
        let actual: Vec<_> = (0..16).map(|_| rng.next()).collect();
        assert_eq!(serde_json::to_value(actual).unwrap(), c["expected"]);
    }
}
#[test]
fn tween_alignment_preserves_open_direction_and_hairpin_endpoints() {
    for c in cases().into_iter().filter(|c| c["operation"] == "align") {
        let from = |v: &Value| ResampledOut {
            segments: serde_json::from_value(v["segments"].clone()).unwrap(),
            widths: None,
            is_vector_brush: false,
            stroke_color: Some("#112233".into()),
            fill_color: None,
        };
        let actual =
            serde_json::to_value(align_pair(&from(&c["input"]["a"]), &from(&c["input"]["b"])))
                .unwrap();
        let actual: Vec<SegOut> = serde_json::from_value(actual["segments"].clone()).unwrap();
        let expected: Vec<SegOut> =
            serde_json::from_value(c["expected"]["segments"].clone()).unwrap();
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.iter().zip(expected) {
            // Same coordinates; serde_json distinguishes integer and float encodings.
            assert_eq!(a.point, b.point);
            assert_eq!(a.handle_in, b.handle_in);
            assert_eq!(a.handle_out, b.handle_out);
        }
    }
}

#[test]
fn tween_interpolation_geometry_matches_independent_baseline() {
    for c in cases()
        .into_iter()
        .filter(|c| c["operation"] == "interpolate")
    {
        let input = &c["input"];
        let a = serde_json::from_value(input["a"]["segments"].clone()).unwrap();
        let b = serde_json::from_value(input["b"]["segments"].clone()).unwrap();
        let pair = PreparedInterpolation::new(
            a,
            b,
            input["a"]["closed"].as_bool().unwrap_or(false),
            input["b"]["closed"].as_bool().unwrap_or(false),
            None,
        )
        .unwrap();
        let actual =
            serde_json::to_value(pair.evaluate(input["t"].as_f64().unwrap(), None).unwrap())
                .unwrap();
        let actual = actual.as_array().unwrap();
        let expected = c["expected"]["segments"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len());
        for (a, b) in actual.iter().zip(expected) {
            for field in ["point", "handleIn", "handleOut"] {
                for i in 0..2 {
                    assert!(
                        (a[field][i].as_f64().unwrap() - b[field][i].as_f64().unwrap()).abs()
                            < 1e-8,
                        "{} t={} {field}[{i}]: {a} != {b}",
                        input["name"],
                        input["t"]
                    );
                }
            }
        }
    }
}

#[test]
fn tween_feature_pair_sampling_matches_independent_baseline() {
    for c in cases().into_iter().filter(|c| c["operation"] == "pair") {
        let input = &c["input"];
        let a = serde_json::from_value(input["a"].clone()).unwrap();
        let b = serde_json::from_value(input["b"].clone()).unwrap();
        let (a, b) = resample_pair(
            &a,
            &b,
            input["n"].as_u64().unwrap() as usize,
            input["vb"].as_bool().unwrap(),
        );
        for (i, actual) in [a, b].into_iter().enumerate() {
            let expected: ResampledOut = serde_json::from_value(c["expected"][i].clone()).unwrap();
            assert_eq!(actual.segments.len(), expected.segments.len());
            for (j, (a, b)) in actual.segments.iter().zip(&expected.segments).enumerate() {
                for (a, b) in a
                    .point
                    .iter()
                    .chain(&a.handle_in)
                    .chain(&a.handle_out)
                    .zip(b.point.iter().chain(&b.handle_in).chain(&b.handle_out))
                {
                    assert!(
                        (a - b).abs() < 1e-8,
                        "{} n={} side={i} vertex={j}: {a} != {b}",
                        input["name"],
                        input["n"]
                    );
                }
            }
            match (actual.widths, expected.widths) {
                (Some(a), Some(b)) => {
                    assert_eq!(a.len(), b.len());
                    for (a, b) in a.iter().zip(b) {
                        assert!((a - b).abs() < 1e-8, "pressure {a} != {b}");
                    }
                }
                (None, None) => {}
                _ => panic!("missing pressure samples"),
            }
        }
    }
}

#[test]
fn tween_vector_dabs_match_every_original_preset_and_seeded_pressure_custom_stamp() {
    for c in cases().into_iter().filter(|c| c["operation"] == "dabs") {
        let i = &c["input"];
        let shape: StrokeIn = serde_json::from_value(i["rec"].clone()).unwrap();
        let preset = serde_json::from_value(i["preset"].clone()).unwrap();
        let actual = build_dabs(
            &shape,
            &preset,
            i["width"].as_f64().unwrap(),
            i["seed"].as_u64().unwrap() as u32,
            i["mul"].as_f64().unwrap(),
        )
        .unwrap();
        let expected = c["expected"].as_array().unwrap();
        assert_eq!(actual.len(), expected.len(), "{}", i["name"]);
        for (di, (a, e)) in actual.iter().zip(expected).enumerate() {
            assert!(
                (a.opacity - e["opacity"].as_f64().unwrap()).abs() < 1e-8,
                "{} dab {di} opacity",
                i["name"]
            );
            let e: Vec<SegOut> = serde_json::from_value(e["segments"].clone()).unwrap();
            assert_eq!(a.segments.len(), e.len());
            for (vi, (a, b)) in a.segments.iter().zip(e).enumerate() {
                for (a, b) in a
                    .point
                    .iter()
                    .chain(&a.handle_in)
                    .chain(&a.handle_out)
                    .zip(b.point.iter().chain(&b.handle_in).chain(&b.handle_out))
                {
                    assert!(
                        (a - b).abs() < 1e-8,
                        "{} dab {di} vertex {vi}: {a} != {b}",
                        i["name"]
                    );
                }
            }
        }
    }
}

#[test]
fn tween_pressure_outline_matches_original_caps_and_rings() {
    for c in cases().into_iter().filter(|c| c["operation"] == "outline") {
        let centers: Vec<CenterSegIn> =
            serde_json::from_value(c["input"]["center"].clone()).unwrap();
        let segments: Vec<_> = centers
            .iter()
            .map(|s| SegOut {
                point: s.point,
                handle_in: s.handle_in,
                handle_out: s.handle_out,
            })
            .collect();
        let widths: Vec<_> = centers.iter().map(|s| s.width).collect();
        let actual = pressure_outline(&segments, &widths)
            .unwrap_or_else(|e| panic!("{}: {e:?}", c["input"]["name"]));
        let expected: Vec<SegOut> = serde_json::from_value(c["expected"].clone()).unwrap();
        assert_eq!(actual.len(), expected.len(), "{}", c["input"]["name"]);
        for (i, (a, b)) in actual.iter().zip(expected).enumerate() {
            for (a, b) in a
                .point
                .iter()
                .chain(&a.handle_in)
                .chain(&a.handle_out)
                .zip(b.point.iter().chain(&b.handle_in).chain(&b.handle_out))
            {
                assert!(
                    (a - b).abs() < 1e-8,
                    "{} vertex {i}: {a} != {b}",
                    c["input"]["name"]
                );
            }
        }
    }
}
