//! Curve refit (`Path#simplify(2.5)`) against the vendored Paper.js (L02-I).
//!
//! `fixtures/object-boolean-ops/oracle.txt` holds the `fit` rows: a closed
//! polygon fed to the REAL `Path#simplify(2.5)` of Paper.js 0.12.18 (generator
//! `tools/gen_oracle.js`, baseline fbe4b8a3). Every row is replayed here, then a
//! few literal values and negative controls pin what the replay alone could not.

use super::*;
use serde_json::Value;

const ORACLE: &str = include_str!("fixtures/object-boolean-ops/oracle.txt");

fn rows(kind: &str) -> Vec<Value> {
    ORACLE
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).expect("oracle row is JSON"))
        .filter(|row| row["k"].as_str() == Some(kind))
        .collect()
}

fn num(v: &Value) -> f64 {
    v.as_f64().expect("number")
}

fn points(v: &Value) -> Vec<[f64; 2]> {
    v.as_array()
        .expect("points")
        .iter()
        .map(|p| [num(&p[0]), num(&p[1])])
        .collect()
}

fn flat(segs: &[RingSeg]) -> Vec<[f64; 6]> {
    segs.iter()
        .map(|s| {
            [
                s.point[0],
                s.point[1],
                s.handle_in[0],
                s.handle_in[1],
                s.handle_out[0],
                s.handle_out[1],
            ]
        })
        .collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

#[test]
fn the_fit_rows_are_pinned() {
    assert_eq!(rows("fit").len(), 11);
    let names: Vec<String> = rows("fit")
        .iter()
        .map(|row| row["n"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(names[0], "square");
    assert_eq!(names[10], "keyhole-ring");
}

#[test]
fn every_row_matches_paper_simplify_to_rounding() {
    for row in rows("fit") {
        let name = row["n"].as_str().unwrap();
        let input = points(&row["in"]["pts"]);
        let fitted = simplify_closed(&input, 2.5).expect(name);
        let got = flat(&fitted);
        let want: Vec<Vec<f64>> = row["o"]["segs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|seg| seg.as_array().unwrap().iter().map(num).collect())
            .collect();
        assert_eq!(got.len(), want.len(), "{name}: segment count");
        for (i, (g, w)) in got.iter().zip(&want).enumerate() {
            for k in 0..6 {
                assert!(
                    close(g[k], w[k]),
                    "{name}: segment {i} member {k}: {} vs {}",
                    g[k],
                    w[k]
                );
            }
        }
        assert_eq!(row["o"]["ok"], Value::Bool(true), "{name}");
    }
}

#[test]
fn a_square_collapses_to_three_curved_anchors_exactly_like_paper() {
    let square = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
    let fitted = flat(&simplify_closed(&square, 2.5).unwrap());
    assert_eq!(fitted.len(), 3);
    // Literal first anchor of the baseline run: point (0,0), handleIn and handleOut.
    assert!(close(fitted[0][2], -23.570226039551585));
    assert!(close(fitted[0][3], 23.570226039551585));
    assert!(close(fitted[0][4], 66.66666666666666));
    assert!(close(fitted[0][5], -66.66666666666666));
    assert_eq!([fitted[1][0], fitted[1][1]], [100.0, 100.0]);
    assert_eq!([fitted[2][0], fitted[2][1]], [0.0, 100.0]);
}

#[test]
fn no_points_means_no_path() {
    assert!(simplify_closed(&[], 2.5).is_none());
}

#[test]
fn exact_duplicate_points_collapse_before_the_fit() {
    let with = [
        [0.0, 0.0],
        [100.0, 0.0],
        [100.0, 0.0],
        [100.0, 100.0],
        [0.0, 100.0],
    ];
    let without = [[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]];
    assert_eq!(
        flat(&simplify_closed(&with, 2.5).unwrap()),
        flat(&simplify_closed(&without, 2.5).unwrap())
    );
}

#[test]
fn the_tolerance_matters() {
    let circle: Vec<[f64; 2]> = (0..16)
        .map(|i| {
            let t = std::f64::consts::TAU * f64::from(i) / 16.0;
            [100.0 + 50.0 * t.cos(), 100.0 + 50.0 * t.sin()]
        })
        .collect();
    let loose = simplify_closed(&circle, 2.5).unwrap().len();
    let tight = simplify_closed(&circle, 0.001).unwrap().len();
    assert!(
        tight > loose,
        "a tighter tolerance keeps more anchors ({tight} vs {loose})"
    );
}

#[test]
fn every_fitted_value_is_finite() {
    for row in rows("fit") {
        let input = points(&row["in"]["pts"]);
        for seg in flat(&simplify_closed(&input, 2.5).unwrap()) {
            assert!(seg.iter().all(|v| v.is_finite()), "{}", row["n"]);
        }
    }
}

#[test]
fn a_wrong_expectation_is_caught() {
    // Negative control for the replay itself: the baseline triangle fitted with a
    // different point order does not reproduce the oracle segments.
    let row = rows("fit")
        .into_iter()
        .find(|r| r["n"] == "triangle")
        .unwrap();
    let mut input = points(&row["in"]["pts"]);
    input.reverse();
    let got = flat(&simplify_closed(&input, 2.5).unwrap());
    let want: Vec<Vec<f64>> = row["o"]["segs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|seg| seg.as_array().unwrap().iter().map(num).collect())
        .collect();
    let same = got.len() == want.len()
        && got
            .iter()
            .zip(&want)
            .all(|(g, w)| (0..6).all(|k| close(g[k], w[k])));
    assert!(!same, "a reversed ring must not replay the forward oracle");
}
