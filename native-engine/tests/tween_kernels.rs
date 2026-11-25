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

// ---- opt-in rigid swing (pivot arc) and least-turn alignment ----------------
fn swing_stroke(points: &[[f64; 2]], closed: bool) -> StrokeIn {
    serde_json::from_value(serde_json::json!({
        "segments": points.iter().map(|p| serde_json::json!({ "point": p })).collect::<Vec<_>>(),
        "closed": closed,
        "strokeColor": "#000000"
    }))
    .unwrap()
}
fn turned(points: &[[f64; 2]], degrees: f64) -> Vec<[f64; 2]> {
    let (s, c) = degrees.to_radians().sin_cos();
    points
        .iter()
        .map(|p| [p[0] * c - p[1] * s, p[0] * s + p[1] * c])
        .collect()
}
fn swing(a: &[[f64; 2]], b: &[[f64; 2]], closed: bool, improved: bool, t: f64) -> Vec<SegOut> {
    let (ra, rb) = resample_pair(
        &swing_stroke(a, closed),
        &swing_stroke(b, closed),
        25,
        false,
    );
    let rb = align_pair_with(&ra, &rb, improved);
    let pair = PreparedInterpolation::new(
        ra.segments,
        rb.segments,
        closed,
        closed,
        Some((a.len(), b.len())),
    )
    .unwrap();
    let pair = if improved {
        pair.with_pivot_arc()
    } else {
        pair
    };
    pair.evaluate(t, None).unwrap()
}
fn polyline(s: &[SegOut]) -> f64 {
    s.windows(2)
        .map(|w| (w[1].point[0] - w[0].point[0]).hypot(w[1].point[1] - w[0].point[1]))
        .sum()
}
fn arm() -> Vec<[f64; 2]> {
    (0..=12)
        .map(|i| {
            let u = f64::from(i) / 12.;
            [200. * u, 18. * (std::f64::consts::PI * u).sin()]
        })
        .collect()
}

#[test]
fn a_limb_swinging_about_its_end_keeps_its_length_and_its_pivot() {
    let a = arm();
    let rest = polyline(&swing(&a, &a, false, false, 0.5));
    for degrees in [30., 60., 90.] {
        let b = turned(&a, degrees);
        for t in [0.25, 0.5, 0.75] {
            let mid = swing(&a, &b, false, true, t);
            let radii = [mid[0].point, mid[mid.len() - 1].point].map(|p| p[0].hypot(p[1]));
            assert!((polyline(&mid) / rest - 1.).abs() < 0.01);
            assert!(radii[0] < 0.5);
            assert!((radii[1] - 200.).abs() < 1.);
            let tip = mid[mid.len() - 1].point;
            let angle = tip[1].atan2(tip[0]).to_degrees();
            assert!((angle - degrees * t).abs() < 0.5);
        }
    }
    let baseline = swing(&a, &turned(&a, 90.), false, false, 0.5);
    assert!(polyline(&baseline) / rest < 0.75);
}

#[test]
fn pairs_that_are_not_a_rigid_swing_keep_the_baseline_result() {
    let zed = vec![[0., 0.], [60., 0.], [60., 60.], [100., 60.]];
    let grown = vec![[10., 10.], [30., 30.], [90., 90.], [150., 90.]];
    let moved: Vec<[f64; 2]> = arm().iter().map(|p| [p[0] + 300., p[1] - 40.]).collect();
    for (a, b) in [(zed, grown), (arm(), moved)] {
        for t in [0.2, 0.5, 0.8] {
            let (ra, rb) = resample_pair(
                &swing_stroke(&a, false),
                &swing_stroke(&b, false),
                25,
                false,
            );
            let rb = align_pair(&ra, &rb);
            let make = || {
                PreparedInterpolation::new(
                    ra.segments.clone(),
                    rb.segments.clone(),
                    false,
                    false,
                    None,
                )
                .unwrap()
            };
            let (base, arc) = (
                make().evaluate(t, None).unwrap(),
                make().with_pivot_arc().evaluate(t, None).unwrap(),
            );
            for (x, y) in base.iter().zip(&arc) {
                assert_eq!(x.point, y.point);
                assert_eq!(x.handle_in, y.handle_in);
                assert_eq!(x.handle_out, y.handle_out);
            }
        }
    }
}

#[test]
fn manual_arc_handles_still_steer_the_centre_of_a_rigid_swing() {
    let a = arm();
    let b = turned(&a, 60.);
    let (ra, rb) = resample_pair(
        &swing_stroke(&a, false),
        &swing_stroke(&b, false),
        25,
        false,
    );
    let rb = align_pair(&ra, &rb);
    let mean = |s: &[SegOut]| {
        let n = s.len() as f64;
        s.iter().fold([0., 0.], |c, s| {
            [c[0] + s.point[0] / n, c[1] + s.point[1] / n]
        })
    };
    let (ca, cb) = (mean(&ra.segments), mean(&rb.segments));
    let pair = PreparedInterpolation::new(ra.segments, rb.segments, false, false, None)
        .unwrap()
        .with_pivot_arc();
    let controls = ([ca[0] + 40., ca[1] - 90.], [cb[0] + 40., cb[1] - 90.]);
    let centre = mean(&pair.evaluate(0.5, Some(controls)).unwrap());
    let expected = [
        (ca[0] + 3. * controls.0[0] + 3. * controls.1[0] + cb[0]) / 8.,
        (ca[1] + 3. * controls.0[1] + 3. * controls.1[1] + cb[1]) / 8.,
    ];
    assert!((centre[0] - expected[0]).hypot(centre[1] - expected[1]) < 1e-6);
}
#[test]
#[rustfmt::skip]
fn an_open_stroke_swinging_wide_is_not_read_as_drawn_backwards() {
    let hook: Vec<_> = arm().iter().map(|p| [p[0], 40. * (p[0] / 200.).powi(3)]).collect();
    for (stroke, widest) in [(arm(), 150.), (hook, 120.)] {
        let rest = polyline(&swing(&stroke, &stroke, false, false, 0.5));
        for degrees in [100., 120., 135., 150.].into_iter().filter(|d| *d <= widest) {
            let mid = swing(&stroke, &turned(&stroke, degrees), false, true, 0.5);
            assert!(mid[0].point[0].hypot(mid[0].point[1]) < 0.5, "{degrees}");
            assert!((polyline(&mid) / rest - 1.).abs() < 0.02, "{degrees}");
        }
        let redrawn: Vec<[f64; 2]> = stroke.iter().rev().map(|p| [p[0] + 20., p[1] + 10.]).collect();
        let first = swing(&stroke, &redrawn, false, true, 0.5)[0].point;
        assert!((first[0] - 10.).hypot(first[1] - 5.) < 0.5, "{first:?}");
    }
}

#[test]
#[rustfmt::skip]
fn a_bending_elbow_keeps_its_length_and_its_still_half() {
    let straight: Vec<[f64; 2]> = (0..=12).map(|i| [200. * f64::from(i) / 12., 0.]).collect();
    for bend in [45_f64, 90., 135.] {
        let (sin, cos) = bend.to_radians().sin_cos();
        for travel in [[0., 0.], [60., 30.], [250., -120.]] {
            let bent: Vec<[f64; 2]> = (0..=12).map(|i| { let reach = 100. * f64::from(i - 6) / 6.; let p = if i <= 6 { [100. * f64::from(i) / 6., 0.] } else { [100. + reach * cos, reach * sin] }; [p[0] + travel[0], p[1] + travel[1]] }).collect();
            for t in [0.25, 0.5, 0.75] {
                let mid = swing(&straight, &bent, false, true, t);
                assert!((polyline(&mid) / 200. - 1.).abs() < 0.005, "{bend} {travel:?} at {t}");
                let (shoulder, want) = (mid[0].point, [travel[0] * t, travel[1] * t]);
                assert!((shoulder[0] - want[0]).hypot(shoulder[1] - want[1]) < 0.5);
            }
        }
    }
}

#[test]
#[rustfmt::skip]
fn far_travel_reversed_lines_dots_and_steered_bends_are_handled() {
    let mean = |s: &[SegOut]| s.iter().fold([0., 0.], |c, p| [c[0] + p.point[0] / s.len() as f64, c[1] + p.point[1] / s.len() as f64]);
    let apart = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
    let line: Vec<[f64; 2]> = (0..=12).map(|i| [200. * f64::from(i) / 12., 0.]).collect();
    let far: Vec<[f64; 2]> = turned(&arm(), 40.).iter().map(|p| [p[0] + 600., p[1]]).collect();
    let (old, new) = (swing(&arm(), &far, false, false, 0.5), swing(&arm(), &far, false, true, 0.5));
    assert!(apart(mean(&old), mean(&new)) < 1e-6);
    let across = |dx: f64| mean(&swing(&arm(), &turned(&arm(), 90.).iter().map(|p| [p[0] + dx, p[1]]).collect::<Vec<_>>(), false, true, 0.5));
    assert!(apart(across(500.), [across(520.)[0] - 10., across(520.)[1]]) < 12.);
    for bulge in [18., 30., 80.] {
        let curve = |h: f64| line.iter().map(|p| [p[0], h * (std::f64::consts::PI * p[0] / 200.).sin()]).collect::<Vec<_>>();
        for shift in [0., 40., 150.] {
            let moved: Vec<[f64; 2]> = curve(-bulge).iter().map(|p| [p[0] + shift, p[1] + shift / 2.]).collect();
            let (mid, base) = (swing(&curve(bulge), &moved, false, true, 0.25), swing(&curve(bulge), &moved, false, false, 0.25));
            assert!(apart(mid[0].point, base[0].point) < 1e-9 && apart(mid[0].point, [shift / 4., shift / 8.]) < 6., "{bulge} {shift}");
        }
    }
    for degrees in [30., 50., 60., 80.] {
        let back: Vec<[f64; 2]> = turned(&line, degrees).into_iter().rev().collect();
        let (tip, root) = { let m = swing(&line, &back, false, true, 0.5); (m[m.len() - 1].point, m[0].point) };
        let angle = (tip[1] - root[1]).atan2(tip[0] - root[0]).to_degrees();
        assert!((angle.rem_euclid(180.) - degrees / 2.).abs() < 1., "{degrees}: {angle}");
    }
    for shift in [195., 200., 205.] {
        let slid: Vec<[f64; 2]> = line.iter().map(|p| [p[0] + shift, p[1]]).collect();
        let mid = swing(&line, &slid, false, true, 0.5);
        assert!(mid.iter().all(|p| p.point[1].abs() < 0.5), "a slide stays level at {shift}");
    }
    let (ra, rb) = resample_pair(&swing_stroke(&line, false), &swing_stroke(&line[..1], false), 25, false);
    assert!(rigid_turn(&ra, &rb).is_none());
    let bent: Vec<[f64; 2]> = (0..=12).map(|i| if i <= 6 { line[i] } else { [100., line[i][0] - 100.] }).collect();
    let (ra, rb) = resample_pair(&swing_stroke(&line, false), &swing_stroke(&bent, false), 25, false);
    let pair = PreparedInterpolation::new(ra.segments, rb.segments, false, false, None).unwrap().with_pivot_arc();
    let (free, pulled) = (pair.evaluate(0.5, None).unwrap(), pair.evaluate(0.5, Some(([0., -300.], [0., -300.]))).unwrap());
    assert!(apart(mean(&free), mean(&pulled)) > 150.);
}
