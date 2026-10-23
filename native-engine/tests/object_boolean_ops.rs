//! Polygon boolean / eraser engine against the baseline (G19 + G18, L02-I).
//!
//! The `fold` and `erase` rows of `fixtures/object-boolean-ops/oracle.txt` were
//! produced by running the VERBATIM `foldBooleanOp` and `_polygonsToPaperItem`
//! of `src/js/tools.js` at fbe4b8a3 on the real Paper.js 0.12.18 (the fallback
//! `window.GeometryWasm` is absent), generator `tools/gen_oracle.js`. A row is
//! the operation, its operand polygons and what Paper produced: for every child
//! of the result its bounds, area and nesting depth (even = exterior, odd =
//! hole; Paper's own clockwise flag is not an exterior marker for `exclude`)
//! and the even-odd area. The engine here is a different clipper, so vertices
//! and ring order are NOT compared: islands, holes, bounds and areas are.

use super::*;
use crate::object_document::object_group_kernels::object_boolean_topology::Polygon;
use serde_json::Value;

const ORACLE: &str = include_str!("fixtures/object-boolean-ops/oracle.txt");

pub(crate) fn rows(kind: &str) -> Vec<Value> {
    ORACLE
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).expect("oracle row is JSON"))
        .filter(|row| row["k"].as_str() == Some(kind))
        .collect()
}

pub(crate) fn num(v: &Value) -> f64 {
    v.as_f64().expect("number")
}

pub(crate) fn ring(v: &Value) -> Vec<Point> {
    v.as_array()
        .expect("ring")
        .iter()
        .map(|p| [num(&p[0]), num(&p[1])])
        .collect()
}

pub(crate) fn multi(v: &Value) -> MultiPolygon {
    v.as_array()
        .expect("polygons")
        .iter()
        .map(|p| Polygon {
            exterior: ring(&p["exterior"]),
            holes: p["holes"].as_array().unwrap().iter().map(ring).collect(),
        })
        .collect()
}

pub(crate) fn op_of(name: &str) -> BoolOp {
    BoolOp::from_name(name).expect("operation name")
}

/// `[x, y, width, height]` and absolute area of a ring.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Facts {
    pub(crate) bounds: [f64; 4],
    pub(crate) area: f64,
}

pub(crate) fn facts_of(points: &[Point]) -> Facts {
    let (mut min_x, mut min_y) = (f64::INFINITY, f64::INFINITY);
    let (mut max_x, mut max_y) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for p in points {
        min_x = min_x.min(p[0]);
        min_y = min_y.min(p[1]);
        max_x = max_x.max(p[0]);
        max_y = max_y.max(p[1]);
    }
    Facts {
        bounds: [min_x, min_y, max_x - min_x, max_y - min_y],
        area: ring_area(points),
    }
}

fn sorted(mut facts: Vec<Facts>) -> Vec<Facts> {
    facts.sort_by(|a, b| {
        for k in 0..4 {
            let order = a.bounds[k]
                .partial_cmp(&b.bounds[k])
                .unwrap_or(std::cmp::Ordering::Equal);
            if order != std::cmp::Ordering::Equal {
                return order;
            }
        }
        std::cmp::Ordering::Equal
    });
    facts
}

fn oracle_facts(row: &Value) -> (Vec<Facts>, Vec<Facts>) {
    let o = &row["o"];
    let kids: Vec<Value> = match o["k"].as_str().unwrap() {
        "compound" => o["rings"].as_array().unwrap().clone(),
        _ if num(&o["n"]) == 0.0 => Vec::new(),
        _ => vec![o.clone()],
    };
    let mut exteriors = Vec::new();
    let mut holes = Vec::new();
    for kid in &kids {
        let b = kid["b"].as_array().unwrap();
        let facts = Facts {
            bounds: [num(&b[0]), num(&b[1]), num(&b[2]), num(&b[3])],
            area: num(&kid["a"]),
        };
        // a lone path has no depth member: it is an exterior
        if kid.get("depth").map_or(0, |d| d.as_u64().unwrap()) % 2 == 0 {
            exteriors.push(facts);
        } else {
            holes.push(facts);
        }
    }
    (sorted(exteriors), sorted(holes))
}

pub(crate) fn engine_facts(result: &MultiPolygon) -> (Vec<Facts>, Vec<Facts>) {
    let exteriors = result.iter().map(|p| facts_of(&p.exterior)).collect();
    let holes = result
        .iter()
        .flat_map(|p| p.holes.iter().map(|h| facts_of(h)))
        .collect();
    (sorted(exteriors), sorted(holes))
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-6 * b.abs().max(1.0)
}

fn assert_points_near(got: &[[f64; 2]], want: &[[f64; 2]]) {
    assert_eq!(got.len(), want.len(), "point count");
    for (g, w) in got.iter().zip(want) {
        assert!(near(g[0], w[0]) && near(g[1], w[1]), "{g:?} vs {w:?}");
    }
}

fn same_facts(name: &str, got: &[Facts], want: &[Facts], what: &str) {
    assert_eq!(got.len(), want.len(), "{name}: {what} count");
    for (g, w) in got.iter().zip(want) {
        for k in 0..4 {
            assert!(
                near(g.bounds[k], w.bounds[k]),
                "{name}: {what} bounds {g:?} vs {w:?}"
            );
        }
        assert!(near(g.area, w.area), "{name}: {what} area {g:?} vs {w:?}");
    }
}

pub(crate) fn square(x: f64, y: f64, s: f64) -> Vec<Point> {
    vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s]]
}

pub(crate) fn one(exterior: Vec<Point>) -> MultiPolygon {
    vec![Polygon {
        exterior,
        holes: Vec::new(),
    }]
}

#[test]
fn the_oracle_corpus_shape_is_pinned() {
    let counts: Vec<usize> = ["fold", "erase", "ring", "fit", "bake", "orient"]
        .iter()
        .map(|kind| rows(kind).len())
        .collect();
    assert_eq!(counts, vec![22, 7, 34, 11, 9, 1]);
    assert_eq!(ORACLE.lines().count(), 84);
}

#[test]
fn every_fold_row_matches_the_baseline_topology_bounds_and_area() {
    for row in rows("fold") {
        let name = row["n"].as_str().unwrap();
        let operands: Vec<MultiPolygon> = row["in"]["operands"]
            .as_array()
            .unwrap()
            .iter()
            .map(multi)
            .collect();
        let result = fold_multi(op_of(row["in"]["op"].as_str().unwrap()), &operands).expect(name);
        let (want_ext, want_holes) = oracle_facts(&row);
        let (got_ext, got_holes) = engine_facts(&result);
        same_facts(name, &got_ext, &want_ext, "exterior");
        same_facts(name, &got_holes, &want_holes, "hole");
        assert!(
            near(multi_area(&result), num(&row["o"]["area"])),
            "{name}: area"
        );
    }
}

#[test]
fn every_erase_row_matches_the_baseline_subtract() {
    for row in rows("erase") {
        let name = row["n"].as_str().unwrap();
        let operands: Vec<MultiPolygon> = row["in"]["operands"]
            .as_array()
            .unwrap()
            .iter()
            .map(multi)
            .collect();
        let target = &operands[0];
        let plain_target = target.len() == 1 && target[0].holes.is_empty();
        let result = if plain_target {
            // the eraser's own entry point: cleaning, subtract, plausibility guard
            erase_difference(&target[0].exterior, &operands[1][0].exterior).expect(name)
        } else {
            fold_multi(BoolOp::Subtract, &operands).expect(name)
        };
        let (want_ext, want_holes) = oracle_facts(&row);
        let (got_ext, got_holes) = engine_facts(&result);
        same_facts(name, &got_ext, &want_ext, "exterior");
        same_facts(name, &got_holes, &want_holes, "hole");
        assert!(
            near(multi_area(&result), num(&row["o"]["area"])),
            "{name}: area"
        );
    }
}

#[test]
fn three_disjoint_operands_keep_every_piece_in_one_fold() {
    let operands = vec![
        one(square(0.0, 0.0, 50.0)),
        one(square(100.0, 0.0, 50.0)),
        one(square(200.0, 0.0, 50.0)),
    ];
    let result = fold_multi(BoolOp::Unite, &operands).unwrap();
    assert_eq!(result.len(), 3);
    assert!(near(multi_area(&result), 7_500.0));
}

#[test]
fn islands_and_holes_come_back_in_a_canonical_order() {
    // operands listed right to left: the result is left to right
    let operands = vec![
        one(square(200.0, 0.0, 10.0)),
        one(square(0.0, 100.0, 10.0)),
        one(square(0.0, 0.0, 10.0)),
    ];
    let result = fold_multi(BoolOp::Unite, &operands).unwrap();
    let starts: Vec<[f64; 2]> = result
        .iter()
        .map(|p| facts_of(&p.exterior).bounds)
        .map(|b| [b[0], b[1]])
        .collect();
    assert_points_near(&starts, &[[0.0, 0.0], [0.0, 100.0], [200.0, 0.0]]);
    let donut = fold_multi(
        BoolOp::Subtract,
        &[
            one(square(0.0, 0.0, 100.0)),
            vec![
                Polygon {
                    exterior: square(60.0, 60.0, 10.0),
                    holes: vec![],
                },
                Polygon {
                    exterior: square(10.0, 10.0, 10.0),
                    holes: vec![],
                },
            ],
        ],
    )
    .unwrap();
    assert_eq!(donut.len(), 1);
    let hole_starts: Vec<[f64; 2]> = donut[0]
        .holes
        .iter()
        .map(|h| facts_of(h).bounds)
        .map(|b| [b[0], b[1]])
        .collect();
    assert_points_near(&hole_starts, &[[10.0, 10.0], [60.0, 60.0]]);
}

#[test]
fn a_single_operand_returns_itself_cleaned() {
    let mut ring = square(0.0, 0.0, 10.0);
    ring.push(ring[0]); // closing duplicate
    ring.insert(1, ring[1]); // consecutive duplicate
    let result = fold_multi(BoolOp::Unite, &[one(ring)]).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].exterior.len(), 4);
    assert!(near(multi_area(&result), 100.0));
}

#[test]
fn an_empty_accumulator_ends_the_walk_empty() {
    let operands = vec![
        one(square(10.0, 10.0, 10.0)),
        one(square(0.0, 0.0, 100.0)),
        one(square(500.0, 500.0, 10.0)),
    ];
    // (small - big) is empty; the third operand is never folded in
    assert!(fold_multi(BoolOp::Subtract, &operands).unwrap().is_empty());
    // intersect with a disjoint operand ends empty too
    let apart = vec![
        one(square(0.0, 0.0, 10.0)),
        one(square(50.0, 50.0, 10.0)),
        one(square(0.0, 0.0, 10.0)),
    ];
    assert!(fold_multi(BoolOp::Intersect, &apart).unwrap().is_empty());
}

#[test]
fn degenerate_operand_rings_act_as_an_empty_operand() {
    let degenerate = one(vec![[0.0, 0.0], [5.0, 5.0]]);
    let solid = one(square(0.0, 0.0, 10.0));
    let united = fold_multi(BoolOp::Unite, &[degenerate.clone(), solid.clone()]).unwrap();
    assert!(near(multi_area(&united), 100.0));
    assert!(
        fold_multi(BoolOp::Subtract, &[degenerate.clone(), solid.clone()])
            .unwrap()
            .is_empty()
    );
    let kept = fold_multi(BoolOp::Subtract, &[solid.clone(), degenerate.clone()]).unwrap();
    assert!(near(multi_area(&kept), 100.0));
    assert!(fold_multi(BoolOp::Intersect, &[solid, degenerate])
        .unwrap()
        .is_empty());
}

#[test]
fn bad_input_is_a_typed_refusal_naming_the_operand() {
    assert_eq!(
        fold_multi(BoolOp::Unite, &[]).err(),
        Some(OpsError::NoOperands)
    );
    let nan = one(vec![[0.0, 0.0], [f64::NAN, 1.0], [2.0, 2.0]]);
    assert_eq!(
        fold_multi(BoolOp::Unite, &[one(square(0.0, 0.0, 1.0)), nan]).err(),
        Some(OpsError::NonFinite { operand: 1 })
    );
    let far = one(square(0.0, 0.0, 2.0e12));
    assert_eq!(
        fold_multi(BoolOp::Unite, &[far, one(square(0.0, 0.0, 1.0))]).err(),
        Some(OpsError::OutOfRange { operand: 0 })
    );
}

#[test]
fn the_eraser_cleans_its_rings_like_the_retained_geometry() {
    let dirty = vec![
        [0.0, 0.0],
        [0.0005, 0.0],
        [10.0, 0.0],
        [10.0, 10.0],
        [0.0, 10.0],
        [0.0001, 0.0002],
    ];
    assert_eq!(
        dedupe_ring(&dirty),
        vec![[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]
    );
    // a ring that collapses below three points is a typed refusal, not a panic
    let collapsed = vec![[0.0, 0.0], [0.0004, 0.0], [0.0, 0.0004]];
    assert_eq!(
        erase_difference(&collapsed, &square(0.0, 0.0, 10.0)).err(),
        Some(OpsError::Degenerate)
    );
    assert_eq!(
        erase_difference(&square(0.0, 0.0, 10.0), &collapsed).err(),
        Some(OpsError::Degenerate)
    );
}

#[test]
fn the_eraser_guard_is_typed() {
    // 10 000 target, 1 000 footprint: removing 6 001 of it is implausible, 6 000 is allowed
    assert_eq!(
        guard_result(10_000.0, 3_999.0, 1_000.0),
        Err(OpsError::Implausible)
    );
    assert_eq!(guard_result(10_000.0, 4_000.0, 1_000.0), Ok(()));
    // a small target may lose everything
    assert_eq!(guard_result(3_000.0, 0.0, 1_000.0), Ok(()));
    // a NaN result counts as an empty one
    assert_eq!(
        guard_result(10_000.0, f64::NAN, 1_000.0),
        Err(OpsError::Implausible)
    );
}

#[test]
fn modes_map_to_operations() {
    use crate::object_document::object_group_kernels::object_group_hierarchy::CombineMode;
    assert_eq!(BoolOp::from_mode(CombineMode::None), None);
    assert_eq!(BoolOp::from_mode(CombineMode::Unite), Some(BoolOp::Unite));
    assert_eq!(
        BoolOp::from_mode(CombineMode::Subtract),
        Some(BoolOp::Subtract)
    );
    assert_eq!(
        BoolOp::from_mode(CombineMode::Intersect),
        Some(BoolOp::Intersect)
    );
    assert_eq!(
        BoolOp::from_mode(CombineMode::Exclude),
        Some(BoolOp::Exclude)
    );
    assert_eq!(BoolOp::from_name("unite"), Some(BoolOp::Unite));
    assert_eq!(BoolOp::from_name("union"), None);
    assert_eq!(BoolOp::from_name(""), None);
}
