//! Element Motion bake against the baseline (L02-I).
//!
//! The `bake` rows of `fixtures/object-boolean-ops/oracle.txt` are the VERBATIM
//! `elementMotionBakedClone` of `src/js/tools.js` at fbe4b8a3 on the real
//! Paper.js, with `SMMotion.elementMotionAt` returning the row's matrix. A row
//! whose clone is the path itself (`same`) is the no-op, no-matrix and
//! no-stroke-id cases, which never reach this module. The literal cases below
//! are derived by hand from the three Paper steps.

use super::*;
use crate::object_document::object_group_kernels::object_boolean_topology::Polygon;
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

fn mat(v: &Value) -> ElementMat {
    ElementMat {
        dx: num(&v["dx"]),
        dy: num(&v["dy"]),
        rot: num(&v["rot"]),
        sx: num(&v["sx"]),
        sy: num(&v["sy"]),
        ax: num(&v["ax"]),
        ay: num(&v["ay"]),
    }
}

fn ring(v: &Value) -> Vec<[f64; 2]> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|p| [num(&p[0]), num(&p[1])])
        .collect()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * b.abs().max(1.0)
}

fn unit(x: f64, y: f64) -> Vec<[f64; 2]> {
    vec![[x, y], [x + 10.0, y], [x + 10.0, y + 10.0], [x, y + 10.0]]
}

#[test]
fn the_bake_rows_are_pinned() {
    assert_eq!(rows("bake").len(), 9);
    let same = rows("bake")
        .iter()
        .filter(|row| row["o"]["same"].as_bool().unwrap())
        .count();
    assert_eq!(same, 3);
}

#[test]
fn every_row_matches_the_baseline_clone_to_rounding() {
    for row in rows("bake") {
        let name = row["n"].as_str().unwrap();
        let input = ring(&row["in"]["pts"]);
        let want: Vec<[f64; 2]> = row["o"]["segs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| [num(&s[0]), num(&s[1])])
            .collect();
        if row["o"]["same"].as_bool().unwrap() {
            // no matrix, no stroke id or a no-op matrix: the baseline hands the path back itself
            assert_eq!(want, input, "{name}");
            if !row["in"]["mat"].is_null() && mat(&row["in"]["mat"]).is_noop() {
                assert_eq!(bake_ring(&input, &mat(&row["in"]["mat"])), input, "{name}");
            }
            continue;
        }
        let got = bake_ring(&input, &mat(&row["in"]["mat"]));
        assert_eq!(got.len(), want.len(), "{name}");
        for (g, w) in got.iter().zip(&want) {
            assert!(
                close(g[0], w[0]) && close(g[1], w[1]),
                "{name}: {g:?} vs {w:?}"
            );
        }
    }
}

#[test]
fn a_no_op_matrix_returns_the_geometry_untouched_whatever_the_anchor() {
    let m = ElementMat {
        dx: 0.0,
        dy: 0.0,
        rot: 0.0,
        sx: 1.0,
        sy: 1.0,
        ax: 3.0,
        ay: 4.0,
    };
    assert!(m.is_noop());
    let ring = unit(10.0, 20.0);
    assert_eq!(bake_ring(&ring, &m), ring);
    let multi = vec![Polygon {
        exterior: ring.clone(),
        holes: vec![],
    }];
    assert_eq!(bake_multi(&multi, &m), multi);
    assert!(!ElementMat { dx: 0.5, ..m }.is_noop());
    assert!(!ElementMat { sx: 1.5, ..m }.is_noop());
    assert!(!ElementMat { rot: 1.0, ..m }.is_noop());
}

#[test]
fn translation_moves_every_point_by_the_delta() {
    let m = ElementMat {
        dx: 299.0,
        dy: -11.0,
        rot: 0.0,
        sx: 1.0,
        sy: 1.0,
        ax: 0.0,
        ay: 0.0,
    };
    assert_eq!(
        bake_ring(&unit(10.0, 20.0), &m),
        vec![[309.0, 9.0], [319.0, 9.0], [319.0, 19.0], [309.0, 19.0]]
    );
}

#[test]
fn scale_pivots_on_the_bounds_centre_plus_the_anchor_offset() {
    let about_centre = ElementMat {
        dx: 0.0,
        dy: 0.0,
        rot: 0.0,
        sx: 2.0,
        sy: 0.5,
        ax: 0.0,
        ay: 0.0,
    };
    // centre (15, 25): x' = 15 + 2 (x - 15), y' = 25 + 0.5 (y - 25)
    assert_eq!(
        bake_ring(&unit(10.0, 20.0), &about_centre),
        vec![[5.0, 22.5], [25.0, 22.5], [25.0, 27.5], [5.0, 27.5]]
    );
    // the anchor offset (+5, -5) moves the pivot to (20, 20): x' = 20 + 2 (x - 20), y' = 20 + 0.5 (y - 20)
    let shifted = ElementMat {
        ax: 5.0,
        ay: -5.0,
        ..about_centre
    };
    assert_eq!(
        bake_ring(&unit(10.0, 20.0), &shifted),
        vec![[0.0, 20.0], [20.0, 20.0], [20.0, 25.0], [0.0, 25.0]]
    );
}

#[test]
fn a_quarter_turn_swaps_the_axes_around_the_centre() {
    let m = ElementMat {
        dx: 0.0,
        dy: 0.0,
        rot: 90.0,
        sx: 1.0,
        sy: 1.0,
        ax: 0.0,
        ay: 0.0,
    };
    let got = bake_ring(&unit(10.0, 20.0), &m);
    // real Paper.js (elementMotionBakedClone at fbe4b8a3): the square (10,20)-(20,30) turned about (15,25)
    let want = [[20.0, 20.0], [20.0, 30.0], [10.0, 30.0], [10.0, 20.0]];
    for (g, w) in got.iter().zip(&want) {
        assert!(close(g[0], w[0]) && close(g[1], w[1]), "{g:?} vs {w:?}");
    }
}

#[test]
fn one_operand_has_one_pivot_for_all_its_rings() {
    // Two squares of one operand: the pivot is the centre of BOTH, (15, 5).
    let multi = vec![
        Polygon {
            exterior: unit(0.0, 0.0),
            holes: vec![],
        },
        Polygon {
            exterior: unit(20.0, 0.0),
            holes: vec![],
        },
    ];
    let m = ElementMat {
        dx: 0.0,
        dy: 0.0,
        rot: 0.0,
        sx: 2.0,
        sy: 1.0,
        ax: 0.0,
        ay: 0.0,
    };
    let baked = bake_multi(&multi, &m);
    // x' = 15 + 2 (x - 15)
    assert_eq!(baked[0].exterior[0], [-15.0, 0.0]);
    assert_eq!(baked[0].exterior[1], [5.0, 0.0]);
    assert_eq!(baked[1].exterior[0], [25.0, 0.0]);
    assert_eq!(baked[1].exterior[1], [45.0, 0.0]);
    // baking each ring on its own pivot would give another answer
    assert_ne!(baked[0].exterior, bake_ring(&multi[0].exterior, &m));
}

#[test]
fn a_hole_is_baked_with_its_polygon() {
    let multi = vec![Polygon {
        exterior: vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]],
        holes: vec![vec![[40.0, 40.0], [60.0, 40.0], [60.0, 60.0], [40.0, 60.0]]],
    }];
    let m = ElementMat {
        dx: 10.0,
        dy: 5.0,
        rot: 0.0,
        sx: 1.0,
        sy: 1.0,
        ax: 0.0,
        ay: 0.0,
    };
    let baked = bake_multi(&multi, &m);
    assert_eq!(
        baked[0].holes[0],
        vec![[50.0, 45.0], [70.0, 45.0], [70.0, 65.0], [50.0, 65.0]]
    );
    assert_eq!(baked[0].exterior[2], [110.0, 105.0]);
}
