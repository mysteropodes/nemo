//! Paint-bucket commit join (G18, L02-I) against the baseline.
//!
//! `fixtures/object-fill-trace-ops/oracle.txt` (generator `tools/gen_oracle_fill.js`): the VERBATIM
//! `fillVectorFind`, `fillFindExistingMatch` and `fillInsertIndexFor` of `src/js/tools.js` at fbe4b8a3 on the
//! real Paper.js 0.12.18. The graph tracer and the raster tracer that `fillVectorFind` calls are scripted from
//! outside, so what is replayed is the step walk, the area floor, the early stop and the raster fallback. The
//! providers below are the PORTS of the join (tracer, touch probe, curved union): the join itself is the real
//! code, the production providers report `Unavailable` and are tested as such.

use std::collections::HashMap;

use super::*;
use crate::object_document::object_group_kernels::object_boolean_topology::{Bounds, MergeError};
use serde_json::Value;

const ORACLE: &str = include_str!("fixtures/object-fill-trace-ops/oracle.txt");

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

fn bounds(v: &Value) -> Bounds {
    let a = v.as_array().expect("bounds");
    Bounds {
        x: num(&a[0]),
        y: num(&a[1]),
        width: num(&a[2]),
        height: num(&a[3]),
    }
}

#[derive(Clone)]
enum Spec {
    Hit(TraceHit),
    Throw,
}

fn spec(v: &Value) -> Spec {
    if v.as_str() == Some("throw") {
        return Spec::Throw;
    }
    Spec::Hit(TraceHit {
        ring: Vec::new(),
        area: num(&v["area"]),
        wall_ids: v["walls"]
            .as_array()
            .map(|w| {
                w.iter()
                    .map(|id| id.as_str().unwrap().to_string())
                    .collect()
            })
            .unwrap_or_default(),
        used_gap: v["usedGap"].as_bool().unwrap(),
        gap_px: v.get("gapPx").and_then(Value::as_u64).unwrap_or(0) as u32,
    })
}

/// The tracer port, scripted per gap; it records every call like the oracle does.
struct Scripted {
    script: HashMap<u32, Spec>,
    raster: Option<Spec>,
    calls: Vec<String>,
}

impl FillTraceProvider for Scripted {
    fn trace(&mut self, gap: u32) -> Result<Option<TraceHit>, TraceFailure> {
        self.calls.push(format!("g{gap}"));
        match self.script.get(&gap) {
            None => Ok(None),
            Some(Spec::Throw) => Err(TraceFailure::Failed),
            Some(Spec::Hit(hit)) => Ok(Some(hit.clone())),
        }
    }
    fn raster(&mut self) -> Result<Option<TraceHit>, TraceFailure> {
        self.calls.push("raster".to_string());
        match &self.raster {
            None => Ok(None),
            Some(Spec::Throw) => Err(TraceFailure::Failed),
            Some(Spec::Hit(hit)) => Ok(Some(hit.clone())),
        }
    }
}

fn scripted(row: &Value) -> Scripted {
    Scripted {
        script: row["in"]["script"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(gap, v)| (gap.parse().unwrap(), spec(v)))
            .collect(),
        raster: if row["in"]["raster"].is_null() {
            None
        } else {
            Some(spec(&row["in"]["raster"]))
        },
        calls: Vec::new(),
    }
}

#[test]
fn the_oracle_rows_are_pinned() {
    assert_eq!(
        (
            rows("find").len(),
            rows("exist").len(),
            rows("insert").len()
        ),
        (16, 12, 6)
    );
}

#[test]
fn every_find_row_matches_the_step_walk_floor_early_stop_and_raster_fallback() {
    for row in rows("find") {
        let name = row["n"].as_str().unwrap();
        let mut provider = scripted(&row);
        let cap = row["in"]["maxGap"].as_u64().map(|gap| gap as u32);
        let best = find_fill_region(&mut provider, cap).expect(name);
        // the baseline's second, stale JS tracer has no native twin: its calls are not compared
        let want: Vec<String> = row["o"]["calls"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|c| c.get("js").is_none())
            .map(|c| match c.get("g") {
                Some(gap) => format!("g{}", gap.as_u64().unwrap()),
                None => "raster".to_string(),
            })
            .collect();
        assert_eq!(provider.calls, want, "{name}: calls");
        match (&best, &row["o"]["result"]) {
            (None, Value::Null) => {}
            (Some(hit), result) if !result.is_null() => {
                assert_eq!(
                    u64::from(hit.gap_px),
                    result["gapPx"].as_u64().unwrap(),
                    "{name}: gap"
                );
                assert_eq!(
                    hit.used_gap,
                    result["usedGap"].as_bool().unwrap(),
                    "{name}: usedGap"
                );
                assert_eq!(hit.area.abs(), num(&result["area"]), "{name}: area");
                let walls: Vec<&str> = result["walls"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|w| w.as_str().unwrap())
                    .collect();
                assert_eq!(hit.wall_ids, walls, "{name}: walls");
            }
            other => panic!("{name}: {other:?}"),
        }
    }
}

fn child_facts(item: &Value, covers: bool) -> ChildFacts {
    let r = item["r"].as_array().unwrap();
    let data = &item["data"];
    ChildFacts {
        filled_shape: item.get("fill").map_or(true, |f| !f.is_null()),
        excluded: data["isLinkedFillCompanion"].as_bool().unwrap_or(false)
            || data["isBrushTextureCopy"].as_bool().unwrap_or(false),
        bounds: Bounds {
            x: num(&r[0]),
            y: num(&r[1]),
            width: num(&r[2]),
            height: num(&r[3]),
        },
        area: (num(&r[2]) * num(&r[3])).abs(),
        covers_click: covers,
    }
}

#[test]
fn every_existing_match_row_matches_the_baseline() {
    for row in rows("exist") {
        let name = row["n"].as_str().unwrap();
        let children: Vec<ChildFacts> = row["in"]["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| child_facts(c, false))
            .collect();
        let got = find_existing_match(&children, num(&row["o"]["area"]), &bounds(&row["o"]["b"]));
        assert_eq!(got.map(|i| i as u64), row["o"]["index"].as_u64(), "{name}");
    }
}

#[test]
fn every_insert_row_matches_the_baseline() {
    for row in rows("insert") {
        let name = row["n"].as_str().unwrap();
        let pt = row["in"]["pt"].as_array().unwrap();
        let (px, py) = (num(&pt[0]), num(&pt[1]));
        let children: Vec<ChildFacts> = row["in"]["children"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| {
                let r = c["r"].as_array().unwrap();
                let inside = px >= num(&r[0])
                    && px <= num(&r[0]) + num(&r[2])
                    && py >= num(&r[1])
                    && py <= num(&r[1]) + num(&r[3]);
                child_facts(c, inside && c.get("fill").map_or(true, |f| !f.is_null()))
            })
            .collect();
        assert_eq!(
            fill_insert_index(&children) as u64,
            row["o"]["index"].as_u64().unwrap(),
            "{name}"
        );
    }
}

#[test]
fn the_raster_fallback_keeps_the_gap_it_was_found_at_and_a_failed_step_finds_nothing() {
    let mut p = Scripted {
        script: HashMap::from([(0, Spec::Throw), (24, Spec::Hit(hit(900.0, true, &["w"])))]),
        raster: None,
        calls: Vec::new(),
    };
    let best = find_fill_region(&mut p, None).unwrap().unwrap();
    assert_eq!(best.gap_px, 24);
    assert_eq!(
        p.calls.len(),
        7,
        "a thrown step does not stop the escalation"
    );
}

pub(super) fn hit(area: f64, used_gap: bool, walls: &[&str]) -> TraceHit {
    TraceHit {
        ring: Vec::new(),
        area,
        wall_ids: walls.iter().map(|w| w.to_string()).collect(),
        used_gap,
        gap_px: 0,
    }
}

#[test]
fn the_production_ports_report_unavailable_and_commit_nothing() {
    assert_eq!(
        find_fill_region(&mut UnavailableTrace, None),
        Err(FillTraceError::Unavailable)
    );
    let mut raster_only = Scripted {
        script: HashMap::new(),
        raster: Some(Spec::Hit(hit(500.0, false, &[]))),
        calls: Vec::new(),
    };
    assert!(find_fill_region(&mut raster_only, None).unwrap().is_some());
    assert_eq!(
        UnavailableUnion.unite(1),
        Err(MergeError::GeometryUnavailable)
    );
}
