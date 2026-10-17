//! Island rings of a boolean result against the baseline (L02-I).
//!
//! The `ring` rows of `fixtures/object-boolean-ops/oracle.txt` are the VERBATIM
//! `flattenBooleanResult(_polygonsToPaperItem(polys), preserve)` of
//! `src/js/tools.js` at fbe4b8a3 run on the real Paper.js: the island paths it
//! returned, point and handle by point and handle. Rows with `preserve` are
//! replayed exactly (every point is an input point); refit rows to rounding.
//! Literal rings and areas below are derived by hand, independently of the rows.

use super::*;
use crate::object_document::object_group_kernels::object_boolean_topology::{
    IslandBuild, Polygon, TopologyError,
};
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

fn ring(v: &Value) -> Vec<[f64; 2]> {
    v.as_array()
        .expect("ring")
        .iter()
        .map(|p| [num(&p[0]), num(&p[1])])
        .collect()
}

fn polygons(v: &Value) -> Vec<Polygon> {
    v.as_array()
        .expect("polys")
        .iter()
        .map(|p| Polygon {
            exterior: ring(&p["exterior"]),
            holes: p["holes"].as_array().unwrap().iter().map(ring).collect(),
        })
        .collect()
}

fn flat(island: &Island) -> Vec<[f64; 6]> {
    island
        .segments
        .iter()
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

fn points(island: &Island) -> Vec<[f64; 2]> {
    island.segments.iter().map(|s| s.point).collect()
}

fn square(x: f64, y: f64, s: f64) -> Vec<[f64; 2]> {
    vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s]]
}

fn poly(exterior: Vec<[f64; 2]>, holes: Vec<Vec<[f64; 2]>>) -> Polygon {
    Polygon { exterior, holes }
}

#[test]
fn the_ring_rows_are_pinned() {
    let all = rows("ring");
    assert_eq!(all.len(), 34);
    assert_eq!(rows("orient").len(), 1);
}

#[test]
fn every_row_matches_the_baseline_island_by_island() {
    for row in rows("ring") {
        let name = row["n"].as_str().unwrap();
        let preserve = row["in"]["preserve"].as_bool().unwrap();
        let islands = flatten_polygons(&polygons(&row["in"]["polys"]), preserve).expect(name);
        let want = row["o"]["islands"].as_array().unwrap();
        assert_eq!(islands.len(), want.len(), "{name}: island count");
        for (k, (got, want)) in islands.iter().zip(want).enumerate() {
            assert_eq!(want["closed"], Value::Bool(true), "{name}");
            let got = flat(got);
            let want = want["segs"].as_array().unwrap();
            assert_eq!(got.len(), want.len(), "{name}: island {k} segments");
            for (i, (g, w)) in got.iter().zip(want).enumerate() {
                for m in 0..6 {
                    let w = num(&w[m]);
                    let tolerance = if preserve {
                        0.0
                    } else {
                        1e-9 * w.abs().max(1.0)
                    };
                    assert!(
                        (g[m] - w).abs() <= tolerance,
                        "{name}: island {k} segment {i} member {m}: {} vs {w}",
                        g[m]
                    );
                }
            }
        }
    }
}

#[test]
fn the_clockwise_rule_is_papers() {
    let probe = &rows("orient")[0]["o"];
    let cw = square(0.0, 0.0, 10.0);
    let mut ccw = cw.clone();
    ccw.reverse();
    assert_eq!(signed_area(&cw), num(&probe["cwArea"]));
    assert_eq!(signed_area(&ccw), num(&probe["ccwArea"]));
    assert_eq!(is_clockwise(&cw), probe["cwFlag"].as_bool().unwrap());
    assert_eq!(is_clockwise(&ccw), probe["ccwFlag"].as_bool().unwrap());
    // a zero-area ring counts as clockwise
    assert_eq!(
        is_clockwise(&[[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]]),
        probe["zeroFlag"].as_bool().unwrap()
    );
    let mut ring = ccw.clone();
    set_clockwise(&mut ring, true);
    assert_eq!(ring, cw);
    set_clockwise(&mut ring, false);
    assert_eq!(ring, ccw);
}

#[test]
fn a_plain_result_stays_as_given_and_a_multi_island_result_goes_clockwise() {
    let single = flatten_polygons(
        &[poly(
            vec![[0.0, 0.0], [0.0, 10.0], [10.0, 10.0], [10.0, 0.0]],
            vec![],
        )],
        true,
    )
    .unwrap();
    assert_eq!(single[0].build, IslandBuild::Single);
    assert_eq!(
        points(&single[0]),
        vec![[0.0, 0.0], [0.0, 10.0], [10.0, 10.0], [10.0, 0.0]]
    );
    let ccw_island = poly(
        vec![[0.0, 0.0], [0.0, 50.0], [50.0, 50.0], [50.0, 0.0]],
        vec![],
    );
    let flat_two =
        flatten_polygons(&[ccw_island, poly(square(100.0, 0.0, 50.0), vec![])], true).unwrap();
    assert_eq!(flat_two.len(), 2);
    assert!(flat_two
        .iter()
        .all(|i| i.build == IslandBuild::Flat && !i.refit));
    assert_eq!(
        points(&flat_two[0]),
        vec![[50.0, 0.0], [50.0, 50.0], [0.0, 50.0], [0.0, 0.0]]
    );
}

#[test]
fn the_slit_joins_the_first_exterior_vertex_to_the_first_hole_vertex() {
    let donut = poly(square(0.0, 0.0, 200.0), vec![square(75.0, 75.0, 50.0)]);
    let islands = flatten_polygons(&[donut], true).unwrap();
    assert_eq!(islands.len(), 1);
    assert_eq!(islands[0].build, IslandBuild::Merged);
    // Baseline quirk kept on purpose (module note): the pair is always (0, 0).
    assert_eq!(
        points(&islands[0]),
        vec![
            [0.0, 0.0],
            [75.0, 125.0],
            [125.0, 125.0],
            [125.0, 75.0],
            [75.0, 75.0],
            [75.0, 125.0],
            [0.0, 0.0],
            [200.0, 0.0],
            [200.0, 200.0],
            [0.0, 200.0]
        ]
    );
    // The zero-width slit adds no area: 200 x 200 minus the 50 x 50 hole.
    assert_eq!(signed_area(&points(&islands[0])), 37_500.0);
}

#[test]
fn a_hole_belongs_to_its_own_exterior_even_inside_a_nested_island() {
    // A donut whose hole holds an island that has a hole of its own. The bounds of the island's
    // hole sit inside the donut's bounds too, which cut it into the donut as well in the baseline.
    let donut = poly(square(0.0, 0.0, 300.0), vec![square(50.0, 50.0, 200.0)]);
    let island = poly(
        square(100.0, 100.0, 100.0),
        vec![square(120.0, 120.0, 20.0)],
    );
    let islands = flatten_polygons(&[donut, island], true).unwrap();
    assert_eq!(islands.len(), 2);
    assert_eq!(signed_area(&points(&islands[0])), 90_000.0 - 40_000.0);
    assert_eq!(signed_area(&points(&islands[1])), 10_000.0 - 400.0);
    assert_eq!(islands[0].segments.len(), 10);
    assert_eq!(islands[1].segments.len(), 10);
}

#[test]
fn every_hole_of_a_polygon_is_kept() {
    let polygon = poly(
        square(0.0, 0.0, 300.0),
        vec![square(20.0, 20.0, 60.0), square(200.0, 20.0, 60.0)],
    );
    let islands = flatten_polygons(&[polygon], true).unwrap();
    let area = signed_area(&points(&islands[0]));
    assert_eq!(area, 90_000.0 - 3_600.0 - 3_600.0);
    assert_eq!(islands[0].segments.len(), 4 + 2 * 6);
}

#[test]
fn noise_loops_are_spliced_but_a_real_slit_is_not() {
    let spur = flatten_polygons(
        &[poly(
            vec![
                [0.0, 0.0],
                [100.0, 0.0],
                [100.0, 100.0],
                [60.0, 100.0],
                [63.0, 104.0],
                [66.0, 100.5],
                [60.8, 100.2],
                [40.0, 100.0],
                [0.0, 100.0],
            ],
            vec![],
        )],
        true,
    )
    .unwrap();
    assert_eq!(
        points(&spur[0]),
        vec![
            [0.0, 0.0],
            [100.0, 0.0],
            [100.0, 100.0],
            [60.0, 100.0],
            [40.0, 100.0],
            [0.0, 100.0]
        ]
    );
    let near = flatten_polygons(
        &[poly(
            vec![
                [0.0, 0.0],
                [100.0, 0.0],
                [100.0, 0.5],
                [100.0, 100.0],
                [0.0, 100.0],
            ],
            vec![],
        )],
        true,
    )
    .unwrap();
    assert_eq!(
        points(&near[0]),
        vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]]
    );
    // a real loop above 0.2 % of the box keeps its revisit and blocks the refit
    let kept = flatten_polygons(
        &[poly(
            vec![
                [0.0, 0.0],
                [200.0, 0.0],
                [200.0, 200.0],
                [100.0, 200.0],
                [100.0, 100.0],
                [150.0, 100.0],
                [150.0, 200.5],
                [100.5, 199.5],
                [0.0, 200.0],
            ],
            vec![],
        )],
        false,
    )
    .unwrap();
    assert_eq!(kept[0].segments.len(), 9);
    assert!(!kept[0].refit);
    // a slit's loop (the 50 x 50 hole in a 200 x 200 square) is above 0.2 % of the box: kept, never refit
    let donut = flatten_polygons(
        &[poly(
            square(0.0, 0.0, 200.0),
            vec![square(75.0, 75.0, 50.0)],
        )],
        false,
    )
    .unwrap();
    assert!(!donut[0].refit);
    assert_eq!(donut[0].segments.len(), 10);
    assert!(donut[0]
        .segments
        .iter()
        .all(|s| s.handle_in == [0.0, 0.0] && s.handle_out == [0.0, 0.0]));
}

#[test]
fn the_refit_runs_only_without_preserve_and_never_after_a_kept_slit() {
    let shape = poly(square(0.0, 0.0, 100.0), vec![]);
    let kept = flatten_polygons(&[shape.clone()], true).unwrap();
    assert!(!kept[0].refit);
    assert_eq!(kept[0].segments.len(), 4);
    let refit = flatten_polygons(&[shape], false).unwrap();
    assert!(refit[0].refit);
    assert_eq!(refit[0].segments.len(), 3);
    assert!(refit[0].segments.iter().any(|s| s.handle_out != [0.0, 0.0]));
    // islands without holes are never cleaned or refit
    let two = flatten_polygons(
        &[
            poly(square(0.0, 0.0, 50.0), vec![]),
            poly(square(100.0, 0.0, 50.0), vec![]),
        ],
        false,
    )
    .unwrap();
    assert!(two.iter().all(|i| !i.refit && i.segments.len() == 4));
}

#[test]
fn degenerate_polygons_are_dropped_and_bad_holes_refused() {
    assert!(flatten_polygons(&[], false).unwrap().is_empty());
    assert!(
        flatten_polygons(&[poly(vec![[0.0, 0.0], [5.0, 5.0]], vec![])], false)
            .unwrap()
            .is_empty()
    );
    let kept = flatten_polygons(
        &[
            poly(vec![[0.0, 0.0], [5.0, 5.0]], vec![]),
            poly(square(0.0, 0.0, 50.0), vec![]),
        ],
        true,
    )
    .unwrap();
    assert_eq!(kept.len(), 1);
    assert_eq!(kept[0].build, IslandBuild::Single);
    assert_eq!(
        flatten_polygons(
            &[poly(
                square(0.0, 0.0, 100.0),
                vec![vec![[10.0, 10.0], [20.0, 20.0]]]
            )],
            true
        )
        .err(),
        Some(TopologyError::DegenerateHole {
            polygon: 0,
            hole: 0
        })
    );
    assert_eq!(
        flatten_polygons(
            &[poly(vec![[0.0, 0.0], [f64::NAN, 1.0], [2.0, 2.0]], vec![])],
            true
        )
        .err(),
        Some(TopologyError::NonFinitePoint { polygon: 0 })
    );
}
