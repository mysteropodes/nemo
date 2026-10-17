//! Oracle replay for the same-colour fill merge (`fillMergeSameColor`,
//! `_isNearIdenticalArea`, L02-D). Each `merge` row ran the verbatim function on
//! a real Paper layer; the kernel is fed the same facts and must reach the same
//! absorbed set, union steps, removals, insert slot, part order, ids and tags.
//! The four geometric answers per candidate were queried from the Paper objects
//! before the call and are replayed through the [`TouchProbe`] port.

use super::tests::{
    bounds, check_data, check_style, f, idx, idx_list, name, rows, shape, source_data,
};
use super::{
    is_near_identical_area, merge_candidates, plan_merge_commit, plan_union, resolve_union,
    select_absorbed, Absorbed, AreaFacts, FillFacts, MergeCommit, MergeCommitInput, MergeDecision,
    ParticipantTags, ProbeError, TouchProbe,
};
use serde_json::Value;

fn truthy(v: &Value) -> bool {
    match v {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Number(n) => n.as_f64().map_or(false, |x| x != 0.0 && !x.is_nan()),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

fn facts_of(spec: &Value, info: &Value) -> FillFacts {
    let data = &spec["data"];
    FillFacts {
        is_path: info["isPath"] == true,
        closed: info["closed"] == true,
        fill_hex8: info["fill8"].as_str().map(str::to_string),
        stroked: info["stroked"] == true,
        is_vector_brush: truthy(&data["isVectorBrush"]),
        is_linked_fill_companion: truthy(&data["isLinkedFillCompanion"]),
        is_brush_texture_copy: truthy(&data["isBrushTextureCopy"]),
        is_revision_ghost: truthy(&data["isRevisionGhost"]),
        ghost_frame_defined: data.get("ghostFrame").is_some(),
        is_fill_shape: truthy(&data["isFillShape"]),
        stroke_id: data["strokeId"]
            .as_str()
            .filter(|id| !id.is_empty())
            .map(str::to_string),
        fill_walls: data["fillWalls"].as_array().map(|walls| {
            walls
                .iter()
                .map(|w| w.as_str().expect("wall").to_string())
                .collect()
        }),
    }
}

fn seed(v: &Value) -> [f64; 2] {
    [f(&v[0]), f(&v[1])]
}

fn tags_of(spec: &Value) -> ParticipantTags {
    let data = &spec["data"];
    ParticipantTags {
        fill_seeds: data["fillSeeds"]
            .as_array()
            .map(|seeds| seeds.iter().map(seed).collect()),
        fill_seed: data.get("fillSeed").map(seed),
        fill_walls: data["fillWalls"].as_array().map(|walls| {
            walls
                .iter()
                .map(|w| w.as_str().expect("wall").to_string())
                .collect()
        }),
        fill_gap_px: data["fillGapPx"].as_f64(),
    }
}

fn area_of(info: &Value) -> AreaFacts {
    AreaFacts {
        area: f(&info["area"]),
        bounds: bounds(&info["b"]),
    }
}

/// Replays the four recorded answers; remembers what was asked, in order.
pub(crate) struct Replay {
    pub(crate) probes: Vec<Value>,
    pub(crate) asked: Vec<(&'static str, usize)>,
}

impl Replay {
    fn answer(&mut self, what: &'static str, key: &str, c: usize) -> Result<bool, ProbeError> {
        self.asked.push((what, c));
        Ok(self.probes[c][key] == true)
    }
}

impl TouchProbe for Replay {
    fn bounds_intersect(&mut self, c: usize) -> Result<bool, ProbeError> {
        self.answer("bounds", "bi", c)
    }
    fn stroke_between(&mut self, c: usize) -> Result<bool, ProbeError> {
        self.answer("divider", "sb", c)
    }
    fn strict_touch(&mut self, c: usize) -> Result<bool, ProbeError> {
        self.answer("strict", "st", c)
    }
    fn near_miss(&mut self, c: usize) -> Result<bool, ProbeError> {
        self.answer("near", "nm", c)
    }
}

#[test]
fn merge_rows_match_the_baseline() {
    for row in rows("merge") {
        let label = name(&row);
        let specs = row["in"]["items"].as_array().expect("items");
        let infos = row["o"]["info"].as_array().expect("info");
        let new_index = idx(&row["o"]["newId"]);
        let layer: Vec<FillFacts> = specs
            .iter()
            .zip(infos.iter())
            .map(|(s, i)| facts_of(s, i))
            .collect();
        let allow = row["in"]["allow"] == true;
        let candidates = merge_candidates(new_index, &layer, allow);
        let mut replay = Replay {
            probes: infos.iter().map(|i| i["probe"].clone()).collect(),
            asked: Vec::new(),
        };
        let absorbed = select_absorbed(&candidates, &mut replay).expect(&label);
        let ret = &row["o"]["ret"];
        let removed = idx_list(&row["o"]["removed"]);
        let unions = row["o"]["unions"].as_array().expect("unions");
        let calls = row["o"]["calls"].as_array().expect("calls");
        if absorbed.is_empty() {
            assert!(
                unions.is_empty() && removed.is_empty() && calls.is_empty(),
                "{label}"
            );
            assert_eq!(
                ret["k"], "new",
                "{label}: the new fill is returned untouched"
            );
            continue;
        }
        let facts: Vec<Absorbed> = absorbed
            .iter()
            .map(|&i| Absorbed {
                index: i,
                area: area_of(&infos[i]),
            })
            .collect();
        let union_plan = plan_union(&area_of(&infos[new_index]), &facts);
        let observed_steps: Vec<usize> = unions.iter().map(|u| idx(&u["id"])).collect();
        assert_eq!(union_plan.steps, observed_steps, "{label}: union order");
        let outcomes: Vec<bool> = unions.iter().map(|u| u["ok"] == true).collect();
        let decision = resolve_union(&union_plan, &outcomes).expect(&label);
        match decision {
            MergeDecision::NoChange => {
                assert!(removed.is_empty() && calls.is_empty(), "{label}");
                assert_eq!(ret["k"], "new", "{label}");
            }
            MergeDecision::KeepExisting(child) => {
                assert_eq!(removed, vec![new_index], "{label}: only the new fill goes");
                assert_eq!(ret["k"], "existing", "{label}");
                assert_eq!(
                    idx(&ret["id"]),
                    child,
                    "{label}: the near-identical child is returned"
                );
                assert!(calls.is_empty(), "{label}");
            }
            MergeDecision::Unite(unite) => {
                let call = &calls[0];
                let mut participants = vec![tags_of(&specs[new_index])];
                participants.extend(absorbed.iter().map(|&i| tags_of(&specs[i])));
                let data = source_data(&specs[new_index]["data"]);
                let fill = call["fill"].as_str().expect("fill").to_string();
                let commit = plan_merge_commit(&MergeCommitInput {
                    layer_len: specs.len(),
                    new_index,
                    absorbed: &absorbed,
                    unite: &unite,
                    shape: &shape(&call["shape"]),
                    fill_hex8: &fill,
                    opacity: f(&call["op"]),
                    data: data.as_ref(),
                    participants: &participants,
                })
                .expect(&label);
                assert_eq!(commit.removed, removed, "{label}: removed children");
                assert_eq!(
                    commit.insert.skip_refit,
                    call["preserve"] == true,
                    "{label}"
                );
                assert_eq!(
                    Some(commit.insert.islands[0].index),
                    commit.first_index,
                    "{label}"
                );
                assert_eq!(
                    idx(&call["at"]),
                    commit.insert.islands[0].index,
                    "{label}: insert slot"
                );
                let lost: Vec<usize> = unions
                    .iter()
                    .filter(|u| u["ok"] == false)
                    .map(|u| idx(&u["id"]))
                    .collect();
                assert_eq!(
                    commit.lost, lost,
                    "{label}: children removed without a union"
                );
                check_parts(&label, &row, &commit, &specs[new_index]);
                assert_eq!(ret["k"], "part", "{label}");
                assert_eq!(idx(&ret["i"]), 0, "{label}: parts[0] is returned");
            }
        }
    }
}

fn check_parts(label: &str, row: &Value, commit: &MergeCommit, new_spec: &Value) {
    let finals = row["o"]["final"].as_array().expect("final");
    let src_id = new_spec["data"]["strokeId"].as_str();
    let parts: Vec<(usize, &Value)> = finals
        .iter()
        .enumerate()
        .filter(|(_, item)| item.get("id").is_none())
        .collect();
    assert_eq!(
        parts.len(),
        commit.insert.islands.len(),
        "{label}: part count"
    );
    for (k, ((position, item), placed)) in
        parts.iter().zip(commit.insert.islands.iter()).enumerate()
    {
        let at = format!("{label} part {k}");
        assert_eq!(*position, placed.index, "{at}: final slot");
        check_style(&at, item, placed);
        check_data(&at, &item["data"], &placed.carry, k == 0, src_id, false);
        match &commit.tags {
            Some(tags) => {
                let seeds: Vec<[f64; 2]> = item["seeds"]
                    .as_array()
                    .expect("seeds")
                    .iter()
                    .map(seed)
                    .collect();
                assert_eq!(seeds, tags.seeds, "{at}: fillSeeds");
                assert_eq!(f(&item["gap"]), tags.gap_px, "{at}: fillGapPx");
                if let Some(walls) = &tags.walls {
                    let got: Vec<String> = item["walls"]
                        .as_array()
                        .expect("walls")
                        .iter()
                        .map(|w| w.as_str().expect("wall").to_string())
                        .collect();
                    assert_eq!(&got, walls, "{at}: fillWalls");
                }
            }
            None => {
                assert!(
                    item["seeds"].is_null() && item["gap"].is_null(),
                    "{at}: no tags applied"
                );
            }
        }
    }
}

#[test]
fn near_identical_rows_match_the_baseline() {
    for row in rows("near") {
        let facts = |v: &Value| AreaFacts {
            area: f(&v["area"]),
            bounds: bounds(&v["b"]),
        };
        let got = is_near_identical_area(&facts(&row["in"]["a"]), &facts(&row["in"]["b"]));
        assert_eq!(Some(got), row["o"]["v"].as_bool(), "{}", name(&row));
    }
}

#[test]
fn the_probe_is_asked_in_the_baseline_order_and_never_twice() {
    // Replays one absorbing row and checks the question sequence per candidate.
    let rows = rows("merge");
    let row = rows
        .iter()
        .find(|r| name(r) == "merge-near-miss-within-3px-makes-two-islands")
        .expect("row");
    let infos = row["o"]["info"].as_array().expect("info");
    let mut replay = Replay {
        probes: infos.iter().map(|i| i["probe"].clone()).collect(),
        asked: Vec::new(),
    };
    let absorbed = select_absorbed(&[0], &mut replay).expect("probe");
    assert_eq!(absorbed, vec![0]);
    assert_eq!(
        replay.asked,
        vec![("bounds", 0), ("divider", 0), ("strict", 0), ("near", 0)]
    );
}
