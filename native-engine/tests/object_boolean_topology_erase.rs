//! Oracle replay for the eraser admission (`eraseAtPoint`, L02-D). Each `erase`
//! row ran the verbatim function on a real Paper layer in one of two provider
//! modes: `paper` (its own `target.subtract` fallback) or `wasm` (canned
//! `erase_at_point` polygons). The kernel is fed the same target facts and
//! result and must reproduce the insert slot, the island order, the style, the
//! redrawn outline, the data carry and the delete-on-empty rule.

use super::tests::{check_data, check_style, f, idx, name, polygons, rows, shape, source_data};
use super::{
    admit_polygons, erase_stroke_info, plan_erase_polygons, plan_erase_shape,
    wasm_result_plausible, EraseOutcome, ErasePlan, EraseTarget, ResultShape, SourceData,
    StrokeInfo,
};
use serde_json::Value;

fn target_row(row: &Value) -> (usize, &Value) {
    let items = row["in"]["items"].as_array().expect("items");
    let at = items
        .iter()
        .position(|s| s["target"] == true)
        .expect("target");
    (at, &items[at])
}

fn target_of<'a>(row: &Value, data: &'a Option<SourceData>) -> EraseTarget<'a> {
    let (at, spec) = target_row(row);
    let stroke = spec["stroke"].as_str().map(|color| StrokeInfo {
        color: color.to_string(),
        width: spec["sw"].as_f64().unwrap_or(1.0),
        cap: "butt".to_string(),
        join: "miter".to_string(),
    });
    let is_vector_brush = spec["data"]["isVectorBrush"] == true;
    // `eraseAtPoint` borrows the stroke colour as the fill of a closed, fill-less loop.
    let fill = spec["fill"].as_str().or_else(|| spec["stroke"].as_str());
    EraseTarget {
        index: at,
        layer_len: row["in"]["items"].as_array().expect("items").len(),
        fill: fill.map(str::to_string),
        opacity: spec["op"].as_f64().unwrap_or(1.0),
        stroke: erase_stroke_info(is_vector_brush, stroke),
        data: data.as_ref(),
    }
}

fn check_plan(label: &str, row: &Value, target: &EraseTarget, plan: &ErasePlan) {
    let observed = row["o"]["items"].as_array().expect("items");
    let calls = row["o"]["calls"].as_array().expect("calls");
    let sentinels = row["in"]["items"].as_array().expect("items").len() - 1;
    let src_id = row["in"]["items"][target.index]["data"]["strokeId"].as_str();
    match &plan.outcome {
        EraseOutcome::Unchanged => panic!("{label}: every row has a fill"),
        EraseOutcome::DeleteTarget => {
            assert!(calls.is_empty(), "{label}: nothing inserted");
            assert_eq!(
                observed.len(),
                sentinels,
                "{label}: only the target is gone"
            );
        }
        EraseOutcome::Replace(insert) => {
            let call = &calls[0];
            assert_eq!(
                idx(&call["at"]),
                insert.islands[0].index,
                "{label}: insert slot"
            );
            assert_eq!(
                observed.len(),
                sentinels + insert.islands.len(),
                "{label}: layer size"
            );
            assert_eq!(
                call["preserve"],
                Value::Null,
                "{label}: preserveGeometry is not passed"
            );
            let mut next = 0;
            for (position, item) in observed.iter().enumerate() {
                if item.get("s").is_some() {
                    continue;
                }
                let placed = &insert.islands[next];
                let at = format!("{label} island {next}");
                assert_eq!(placed.index, position, "{at}: slot");
                check_style(&at, item, placed);
                check_data(&at, &item["data"], &placed.carry, next == 0, src_id, false);
                next += 1;
            }
            assert_eq!(next, insert.islands.len(), "{label}: every island observed");
            let outline = &call["si"];
            match &insert.islands[0].stroke {
                Some(info) => {
                    assert_eq!(
                        outline["color"].as_str(),
                        Some(info.color.as_str()),
                        "{label}: outline"
                    );
                    assert_eq!(f(&outline["width"]), info.width, "{label}: outline width");
                }
                None => assert!(outline.is_null(), "{label}: no outline redrawn"),
            }
        }
    }
}

#[test]
fn paper_fallback_rows_match_the_baseline() {
    let mut replayed = 0;
    for row in rows("erase") {
        if row["in"]["mode"] != "paper" {
            continue;
        }
        let label = name(&row);
        let data = source_data(&row["in"]["items"][target_row(&row).0]["data"]);
        let target = target_of(&row, &data);
        let calls = row["o"]["calls"].as_array().expect("calls");
        assert!(
            row["o"]["threw"].is_null(),
            "{label}: the baseline does not throw here"
        );
        let result = match calls.first() {
            Some(call) => shape(&call["shape"]),
            None => ResultShape::Empty,
        };
        let plan = plan_erase_shape(&target, &result).expect(&label);
        check_plan(&label, &row, &target, &plan);
        replayed += 1;
    }
    assert_eq!(replayed, 12);
}

#[test]
fn wasm_provider_rows_match_the_baseline() {
    let mut replayed = 0;
    for row in rows("erase") {
        if row["in"]["mode"] != "wasm" {
            continue;
        }
        let label = name(&row);
        let data = source_data(&row["in"]["items"][target_row(&row).0]["data"]);
        let target = target_of(&row, &data);
        let polys = polygons(&row["in"]["polys"]);
        let pre = &row["o"]["pre"];
        let calls = row["o"]["calls"].as_array().expect("calls");
        let fell_back = row["o"]["warn"].as_array().expect("warn").len() == 1;
        if pre["wasm"].is_null() {
            // Baseline: `wasmRes.item.area` on a null item throws, the target stays.
            assert!(
                row["o"]["threw"]
                    .as_str()
                    .expect("threw")
                    .contains("reading 'area'"),
                "{label}"
            );
            assert!(calls.is_empty(), "{label}");
            let plan = plan_erase_polygons(&target, &polys).expect(&label);
            assert_eq!(
                plan.outcome,
                EraseOutcome::DeleteTarget,
                "{label}: native deletes the target"
            );
            replayed += 1;
            continue;
        }
        let plausible = wasm_result_plausible(f(&pre["ta"]), f(&pre["wasm"]), f(&pre["eraser"]));
        assert_eq!(!plausible, fell_back, "{label}: plausibility guard");
        assert!(row["o"]["threw"].is_null(), "{label}");
        if plausible {
            let plan = plan_erase_polygons(&target, &polys).expect(&label);
            let admission = admit_polygons(&polys).expect(&label);
            assert_eq!(
                admission.shape,
                shape(&calls[0]["shape"]),
                "{label}: admitted shape"
            );
            assert_eq!(plan.dropped, admission.dropped, "{label}");
            let kept = polys.iter().filter(|p| p.exterior.len() >= 3).count();
            assert_eq!(plan.dropped.len(), polys.len() - kept, "{label}");
            check_plan(&label, &row, &target, &plan);
        } else {
            let plan = plan_erase_shape(&target, &shape(&calls[0]["shape"])).expect(&label);
            check_plan(&label, &row, &target, &plan);
        }
        replayed += 1;
    }
    assert_eq!(replayed, 11);
}

#[test]
fn the_empty_provider_result_is_the_one_documented_divergence() {
    // eraseAtPoint's WASM branch throws on `item.area` when nothing is left,
    // keeping the target; the contract (and the Paper fallback) delete it.
    let rows = rows("erase");
    let row = rows
        .iter()
        .find(|r| name(r) == "erase-wasm-empty-polygon-list")
        .expect("row");
    assert!(row["o"]["threw"].as_str().is_some());
    assert_eq!(row["o"]["items"].as_array().expect("items").len(), 3);
    let paper = rows
        .iter()
        .find(|r| name(r) == "erase-paper-fully-erased-deletes-target")
        .expect("row");
    assert_eq!(paper["o"]["items"].as_array().expect("items").len(), 2);
}
