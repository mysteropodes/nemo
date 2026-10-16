//! Oracle replay for the boolean result topology kernel (G19 + G18, L02-D).
//!
//! `fixtures/object-boolean-topology/oracle.txt` (`ORACLE`, 156 JSON lines) was
//! produced by running the verbatim `_polygonsToPaperItem`,
//! `flattenBooleanResult`, `insertBooleanResult`, `carryBooleanData`,
//! `_isNearIdenticalArea`, `fillMergeSameColor` and `eraseAtPoint` of
//! `src/js/tools.js` (and `colorHex8` of `src/js/app.js`) at fbe4b8a3 against
//! the REAL vendored Paper.js 0.12.18 in a Node `vm` context (generator
//! `tools/gen_oracle.js`). Recorders wrap three internal collaborators from
//! outside; no function under test is edited. A stroke id the baseline minted
//! is printed as `FRESH`.
//!
//! Rows replayed here: `poly` (`_polygonsToPaperItem` result shapes), `flat`
//! (`flattenBooleanResult` island plan), `insert` (`insertBooleanResult` into a
//! real layer) and `carry` (`carryBooleanData`). `merge` and `near` rows are
//! replayed by `object_boolean_topology_merge.rs`, `erase` rows by
//! `object_boolean_topology_erase.rs`.

use super::{
    admit_polygons, carry_boolean_data, classify, plan_insert, Bounds, DataCarry, DataValue,
    InsertRequest, IslandBuild, PaintArg, PathFacts, PlacedIsland, Polygon, ResultShape, Ring,
    SourceData, StrokeIdAction, StrokeInfo, TopologyError, KEEP_DATA_ALL,
};
use serde_json::Value;

pub(crate) const ORACLE: &str = include_str!("fixtures/object-boolean-topology/oracle.txt");

pub(crate) fn rows(kind: &str) -> Vec<Value> {
    ORACLE
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str::<Value>(line).expect("oracle row is JSON"))
        .filter(|row| row["k"].as_str() == Some(kind))
        .collect()
}

pub(crate) fn name(row: &Value) -> String {
    row["n"].as_str().expect("row name").to_string()
}

pub(crate) fn f(v: &Value) -> f64 {
    v.as_f64().expect("number")
}

pub(crate) fn idx(v: &Value) -> usize {
    v.as_u64().expect("index") as usize
}

pub(crate) fn idx_list(v: &Value) -> Vec<usize> {
    v.as_array().expect("array").iter().map(idx).collect()
}

pub(crate) fn bounds(v: &Value) -> Bounds {
    let a = v.as_array().expect("bounds");
    Bounds {
        x: f(&a[0]),
        y: f(&a[1]),
        width: f(&a[2]),
        height: f(&a[3]),
    }
}

pub(crate) fn shape(v: &Value) -> ResultShape {
    match v["k"].as_str().expect("shape kind") {
        "null" => ResultShape::Empty,
        "single" => ResultShape::Single(PathFacts {
            bounds: bounds(&v["b"]),
            segments: idx(&v["n"]),
        }),
        "compound" => ResultShape::Compound(
            v["rings"]
                .as_array()
                .expect("rings")
                .iter()
                .map(|r| Ring {
                    clockwise: r["cw"].as_bool().expect("cw"),
                    bounds: bounds(&r["b"]),
                    segments: idx(&r["n"]),
                })
                .collect(),
        ),
        other => panic!("unknown shape kind {other}"),
    }
}

pub(crate) fn points(v: &Value) -> Vec<[f64; 2]> {
    v.as_array()
        .expect("points")
        .iter()
        .map(|p| [f(&p[0]), f(&p[1])])
        .collect()
}

pub(crate) fn polygons(v: &Value) -> Vec<Polygon> {
    v.as_array()
        .expect("polygons")
        .iter()
        .map(|p| Polygon {
            exterior: points(&p["exterior"]),
            holes: p["holes"]
                .as_array()
                .expect("holes")
                .iter()
                .map(points)
                .collect(),
        })
        .collect()
}

pub(crate) fn data_value(v: &Value) -> DataValue {
    match v {
        Value::Null => DataValue::Null,
        Value::Bool(b) => DataValue::Bool(*b),
        Value::Number(n) => DataValue::Number(n.as_f64().expect("finite number")),
        Value::String(s) => DataValue::Text(s.clone()),
        other => DataValue::Opaque(other.to_string()),
    }
}

pub(crate) fn data_json(v: &DataValue) -> Value {
    match v {
        DataValue::Null => Value::Null,
        DataValue::Bool(b) => Value::Bool(*b),
        DataValue::Number(n) => serde_json::json!(*n),
        DataValue::Text(s) => Value::String(s.clone()),
        DataValue::Opaque(text) => serde_json::from_str(text).expect("opaque data is JSON"),
    }
}

/// `None` for a JS `null` / `undefined` source; an object becomes a [`SourceData`].
pub(crate) fn source_data(v: &Value) -> Option<SourceData> {
    v.as_object().map(|map| {
        map.iter()
            .map(|(k, val)| (k.clone(), data_value(val)))
            .collect()
    })
}

/// JSON equality that treats `5` and `5.0` as the same number.
pub(crate) fn same(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(p, q)| same(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).map_or(false, |w| same(v, w)))
        }
        _ => a == b,
    }
}

/// The `data` object a bare island ends up with: carried keys plus the stroke id
/// (`FRESH` stands for a newly minted one).
pub(crate) fn expected_data(carry: &DataCarry) -> Value {
    let mut map = serde_json::Map::new();
    for (key, value) in &carry.fields {
        map.insert(key.clone(), data_json(value));
    }
    match &carry.stroke_id {
        StrokeIdAction::Absent => {}
        StrokeIdAction::Carry(value) => {
            map.insert("strokeId".to_string(), data_json(value));
        }
        StrokeIdAction::Fresh => {
            map.insert("strokeId".to_string(), Value::String("FRESH".to_string()));
        }
    }
    Value::Object(map)
}

/// Strict: the island data equals the plan exactly (bare provider rings).
/// Lenient (Paper-sourced rows): only the carried keys and the stroke id are
/// compared, because a Paper.js `unite` / `subtract` result already holds a
/// cloned copy of an operand's data that a native provider ring does not.
pub(crate) fn check_data(
    label: &str,
    observed: &Value,
    carry: &DataCarry,
    first: bool,
    src_id: Option<&str>,
    strict: bool,
) {
    if strict {
        assert!(
            same(observed, &expected_data(carry)),
            "{label}: data {observed} != {}",
            expected_data(carry)
        );
        return;
    }
    let object = observed.as_object().expect("island data");
    for key in KEEP_DATA_ALL.iter() {
        match (object.get(*key), carry.fields.get(*key)) {
            (None, None) => {}
            (Some(got), Some(want)) => {
                assert!(
                    same(got, &data_json(want)),
                    "{label}: {key} {got} != {want:?}"
                )
            }
            (got, want) => panic!("{label}: {key} observed {got:?} planned {want:?}"),
        }
    }
    let got = object.get("strokeId");
    match (&carry.stroke_id, first) {
        (StrokeIdAction::Carry(value), _) => {
            assert!(
                same(got.expect("strokeId"), &data_json(value)),
                "{label}: strokeId"
            )
        }
        (StrokeIdAction::Fresh, _) => {
            let id = got.and_then(Value::as_str).expect("fresh strokeId");
            assert!(
                id == "FRESH" || Some(id) != src_id,
                "{label}: strokeId {id}"
            );
        }
        (StrokeIdAction::Absent, true) => assert!(got.is_none(), "{label}: strokeId"),
        (StrokeIdAction::Absent, false) => {}
    }
}

/// Fill, outline and opacity of one observed island against its plan.
pub(crate) fn check_style(label: &str, item: &Value, placed: &PlacedIsland) {
    let fill = match &placed.fill {
        PaintArg::Set(hex) => Value::String(hex.clone()),
        PaintArg::Leave | PaintArg::Clear => Value::Null,
    };
    assert_eq!(item["fill"], fill, "{label}: fill");
    match &placed.stroke {
        Some(info) => {
            assert_eq!(
                item["stroke"].as_str(),
                Some(info.color.as_str()),
                "{label}: stroke"
            );
            assert_eq!(f(&item["sw"]), info.width, "{label}: stroke width");
            assert_eq!(
                item["cap"].as_str(),
                Some(info.cap.as_str()),
                "{label}: cap"
            );
            assert_eq!(
                item["join"].as_str(),
                Some(info.join.as_str()),
                "{label}: join"
            );
        }
        None => assert!(item["stroke"].is_null(), "{label}: stroke must be cleared"),
    }
    assert_eq!(
        f(&item["op"]),
        placed.opacity.unwrap_or(1.0),
        "{label}: opacity"
    );
}

fn build_of(text: &str) -> IslandBuild {
    match text {
        "single" => IslandBuild::Single,
        "flat" => IslandBuild::Flat,
        "merged" => IslandBuild::Merged,
        other => panic!("unknown build {other}"),
    }
}

#[test]
fn oracle_corpus_shape_is_pinned() {
    let count = |kind: &str| rows(kind).len();
    assert_eq!(count("poly"), 12);
    assert_eq!(count("flat"), 17);
    assert_eq!(count("insert"), 18);
    assert_eq!(count("carry"), 22);
    assert_eq!(count("near"), 19);
    assert_eq!(count("merge"), 45);
    assert_eq!(count("erase"), 23);
    assert_eq!(ORACLE.lines().count(), 156);
}

#[test]
fn poly_rows_match_the_baseline() {
    for row in rows("poly") {
        let label = name(&row);
        let input = polygons(&row["in"]["polys"]);
        let kept = input.iter().filter(|p| p.exterior.len() >= 3).count();
        assert_eq!(kept, idx(&row["o"]["kept"]), "{label}: kept polygons");
        let degenerate_hole = input
            .iter()
            .filter(|p| p.exterior.len() >= 3)
            .any(|p| p.holes.iter().any(|h| h.len() < 3));
        let outcome = admit_polygons(&input);
        if degenerate_hole {
            // The baseline turns a 2-point hole into a bogus exterior ring.
            let rings = row["o"]["shape"]["rings"].as_array().expect("rings");
            assert!(
                rings.iter().any(|r| idx(&r["n"]) < 3 && r["cw"] == true),
                "{label}: baseline bogus ring"
            );
            assert!(
                matches!(outcome, Err(TopologyError::DegenerateHole { .. })),
                "{label}: {outcome:?}"
            );
            continue;
        }
        let admission = outcome.expect(&label);
        assert_eq!(admission.shape, shape(&row["o"]["shape"]), "{label}: shape");
        assert_eq!(
            admission.dropped.len(),
            input.len() - kept,
            "{label}: dropped"
        );
    }
}

#[test]
fn flat_rows_match_the_baseline() {
    for row in rows("flat") {
        let label = name(&row);
        let result = shape(&row["in"]["shape"]);
        let preserve = row["in"]["preserve"].as_bool().expect("preserve");
        let islands = row["o"]["islands"].as_array().expect("islands");
        let dropped = idx_list(&row["o"]["dropped"]);
        if !dropped.is_empty() {
            assert_eq!(
                classify(&result),
                Err(TopologyError::UnpairedHoles(dropped)),
                "{label}: the baseline silently dropped these holes"
            );
            continue;
        }
        let plan = classify(&result).expect(&label);
        assert_eq!(plan.islands.len(), islands.len(), "{label}: island count");
        for (k, (got, want)) in plan.islands.iter().zip(islands.iter()).enumerate() {
            let at = format!("{label} island {k}");
            assert_eq!(
                got.build,
                build_of(want["b"].as_str().expect("build")),
                "{at}"
            );
            assert_eq!(got.ring, idx(&want["ring"]), "{at}: ring");
            assert_eq!(got.holes, idx_list(&want["holes"]), "{at}: holes");
            assert_eq!(got.cleanup, want["cleanup"] == true, "{at}: cleanup");
            if got.cleanup {
                assert_eq!(want["skip"].as_bool(), Some(preserve), "{at}: skipRefit");
            }
        }
        assert_eq!(
            plan.shared_holes,
            idx_list(&row["o"]["shared"]),
            "{label}: shared"
        );
        let request = InsertRequest {
            insert_at: 0,
            layer_len: 0,
            fill: PaintArg::Leave,
            opacity: None,
            stroke: None,
            src: None,
            preserve_geometry: preserve,
        };
        let planned = plan_insert(&result, &request).expect(&label);
        assert_eq!(planned.skip_refit, preserve, "{label}: skip_refit");
    }
}

fn insert_request<'a>(
    row: &Value,
    stroke: &'a Option<StrokeInfo>,
    src: &'a Option<SourceData>,
) -> InsertRequest<'a> {
    let input = &row["in"];
    let fill = if input["fill"].is_null() {
        PaintArg::Clear
    } else if input["fill"] == "undef" {
        PaintArg::Leave
    } else {
        PaintArg::Set(input["fill"].as_str().expect("fill").to_string())
    };
    InsertRequest {
        insert_at: idx(&input["at"]),
        layer_len: idx(&input["sentinels"]),
        fill,
        opacity: input["op"].as_f64(),
        stroke: stroke.as_ref(),
        src: src.as_ref(),
        preserve_geometry: input["preserve"] == true,
    }
}

#[test]
fn insert_rows_match_the_baseline() {
    for row in rows("insert") {
        let label = name(&row);
        let input = &row["in"];
        let stroke = if input["stroke"].is_null() {
            None
        } else {
            Some(StrokeInfo {
                color: input["stroke"]["color"].as_str().unwrap_or("").to_string(),
                width: f(&input["stroke"]["width"]),
                cap: input["stroke"]["cap"].as_str().expect("cap").to_string(),
                join: input["stroke"]["join"].as_str().expect("join").to_string(),
            })
        };
        let src = source_data(&input["src"]);
        let plan = plan_insert(
            &shape(&input["shape"]),
            &insert_request(&row, &stroke, &src),
        )
        .expect(&label);
        let src_id = input["src"]["strokeId"].as_str();
        let items = row["o"]["items"].as_array().expect("items");
        let at = idx_list(&row["o"]["at"]);
        let planned: Vec<usize> = plan.islands.iter().map(|i| i.index).collect();
        assert_eq!(planned, at, "{label}: insert positions");
        let mut next = 0;
        for (position, item) in items.iter().enumerate() {
            if item.get("s").is_some() {
                continue;
            }
            let placed = &plan.islands[next];
            assert_eq!(placed.index, position, "{label}: island {next} slot");
            check_style(&format!("{label} island {next}"), item, placed);
            check_data(
                &format!("{label} island {next}"),
                &item["data"],
                &placed.carry,
                next == 0,
                src_id,
                true,
            );
            next += 1;
        }
        assert_eq!(next, plan.islands.len(), "{label}: every island observed");
    }
}

#[test]
fn carry_rows_match_the_baseline() {
    for row in rows("carry") {
        let label = name(&row);
        let src = source_data(&row["in"]["src"]);
        let first = row["in"]["first"].as_bool().expect("first");
        let carry = carry_boolean_data(src.as_ref(), first);
        assert!(
            same(&expected_data(&carry), &row["o"]["data"]),
            "{label}: {} != {}",
            expected_data(&carry),
            row["o"]["data"]
        );
    }
}
