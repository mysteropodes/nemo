//! L02-F: chains of lifecycle operations replayed against the baseline oracle
//! (`fixtures/object-layer-lifecycle/oracle.txt`, generated from the verbatim
//! `reorderLayersAtGap` / `deleteLayer` / `duplicateLayer` / `renameLayer` of
//! fbe4b8a3 by `tools/gen_oracle.js`). Each row is one scenario run in ONE
//! baseline context, so a step sees the layers and selection the previous step
//! left behind. A layer's color names its original layer through reorders,
//! deletes and copies, which lets the replay check content as well as order.
use super::object_layer_lifecycle_scenario::{check_content, scenario_doc};
use super::object_layer_lifecycle_tests::{
    bytes, duplicate, owner_of, rename, reorder, selection, value,
};
use super::*;
use serde_json::{json, Value};

const ORACLE: &str = include_str!("fixtures/object-layer-lifecycle/oracle.txt");

fn rows() -> Vec<Value> {
    ORACLE
        .trim()
        .split('\n')
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

fn column(document: &Value, key: &str) -> Value {
    Value::Array(
        document["layers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|l| l[key].clone())
            .collect(),
    )
}
fn slot(index: &Value, uids: &[String]) -> String {
    uids[index.as_u64().unwrap() as usize].clone()
}
fn slots(indices: &Value, uids: &[String]) -> Vec<String> {
    indices
        .as_array()
        .unwrap()
        .iter()
        .map(|i| slot(i, uids))
        .collect()
}
fn borrowed(list: &[String]) -> Vec<&str> {
    list.iter().map(String::as_str).collect()
}
/// `(changed, selection slots)` of one prepared-and-committed operation.
type Done = (bool, Option<(usize, Vec<usize>, Option<usize>)>);
fn commit(owner: &mut NativeObjectHistory, op: &str, payload: &Value) -> Result<Done, String> {
    let raw = payload.to_string();
    let candidate = match op {
        "delete" => owner.prepare_layer_delete_json(raw.as_bytes()),
        "duplicate" => owner.prepare_layer_duplicate_json(raw.as_bytes()),
        "rename" => owner.prepare_layer_rename_json(raw.as_bytes()),
        _ => owner.prepare_layer_reorder_json(raw.as_bytes()),
    }
    .map_err(|code| format!("{op} refused with {code:?}: {payload}"))?;
    let changed = candidate.changed();
    let outcome = candidate
        .selection()
        .map(|s| (s.active, s.selected.clone(), s.anchor));
    owner
        .commit_layer_lifecycle(candidate)
        .map_err(|code| format!("{op} commit refused with {code:?}"))?;
    Ok((changed, outcome))
}
fn payload_for(input: &Value, uids: &[String]) -> (String, Value) {
    let pick = |i: &Value| -> Value {
        let anchor = input["anchor"].as_i64().filter(|a| *a >= 0);
        let anchor: Option<String> = anchor.map(|a| uids[a as usize].clone());
        let active = slot(&i["active"], uids);
        let chosen = slots(&i["sel"], uids);
        let refs = borrowed(&chosen);
        selection(&active, &refs, anchor.as_deref())
    };
    let op = input["op"].as_str().unwrap().to_string();
    let payload = match op.as_str() {
        "reorder" => {
            let moving = slots(&input["moving"], uids);
            reorder(
                &borrowed(&moving),
                input["gap"].as_f64().unwrap(),
                pick(input),
            )
        }
        "delete" => {
            let chosen = pick(input);
            json!({"command":"object.layer.delete","selection":chosen})
        }
        "duplicate" => duplicate(&slot(&input["src"], uids)),
        _ => rename(&slot(&input["idx"], uids), input["name"].as_str().unwrap()),
    };
    (op, payload)
}
fn run(row: &Value, version: u32) -> Result<(), String> {
    let mut owner = owner_of(&scenario_doc(&row["layers"], version));
    let mut trail: Vec<(Vec<u8>, Vec<u8>)> = vec![];
    for (n, step) in row["steps"].as_array().unwrap().iter().enumerate() {
        let (input, want) = (&step["in"], &step["o"]);
        let uids: Vec<String> = column(&value(&owner), "layerUid")
            .as_array()
            .unwrap()
            .iter()
            .map(|u| u.as_str().unwrap().to_string())
            .collect();
        let (op, payload) = payload_for(input, &uids);
        let (before, depths) = (bytes(&owner), owner.history_depths());
        let (changed, outcome) = commit(&mut owner, &op, &payload)?;
        let after = value(&owner);
        let at = format!("step {n} {op} v{version}");
        for (key, field) in [
            ("name", "names"),
            ("color", "colors"),
            ("visible", "visible"),
        ] {
            let wanted = &want[field];
            if &column(&after, key) != wanted {
                return Err(format!(
                    "{at}: {key}s {} != baseline {wanted}",
                    column(&after, key)
                ));
            }
        }
        if op != "rename" {
            let anchor = want["anchor"]
                .as_i64()
                .filter(|a| *a >= 0)
                .map(|a| a as usize);
            let sel: Vec<usize> = want["sel"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i.as_u64().unwrap() as usize)
                .collect();
            let expect = (want["active"].as_u64().unwrap() as usize, sel, anchor);
            if outcome != Some(expect.clone()) {
                return Err(format!(
                    "{at}: selection {outcome:?} != baseline {expect:?}"
                ));
            }
        }
        let wants_change = if op == "reorder" {
            want["changed"] == true
        } else {
            true
        };
        if changed != wants_change {
            return Err(format!(
                "{at}: changed {changed} != baseline {wants_change}"
            ));
        }
        let (undo, redo) = owner.history_depths();
        if (undo, redo) != (depths.0 + usize::from(changed), 0) {
            return Err(format!("{at}: depths {:?}", owner.history_depths()));
        }
        if !changed && bytes(&owner) != before {
            return Err(format!("{at}: a no-op changed the bytes"));
        }
        check_content(&after, version).map_err(|e| format!("{at}: {e}"))?;
        if changed {
            trail.push((before, bytes(&owner)));
        }
    }
    for (n, (before, _)) in trail.iter().enumerate().rev() {
        owner
            .undo(owner.content_revision())
            .map_err(|c| format!("undo {n}: {c:?}"))?;
        if &bytes(&owner) != before {
            return Err(format!(
                "undo {n} v{version} did not restore the exact bytes"
            ));
        }
    }
    for (n, (_, after)) in trail.iter().enumerate() {
        owner
            .redo(owner.content_revision())
            .map_err(|c| format!("redo {n}: {c:?}"))?;
        if &bytes(&owner) != after {
            return Err(format!(
                "redo {n} v{version} did not restore the exact bytes"
            ));
        }
    }
    Ok(())
}

#[test]
fn every_baseline_chain_replays_exactly_on_v1_and_v2_documents() {
    let all = rows();
    assert_eq!(all.len(), 35);
    for (n, row) in all.iter().enumerate() {
        for version in [1, 2] {
            if let Err(why) = run(row, version) {
                panic!("scenario {n}: {why}");
            }
        }
    }
}

#[test]
fn the_oracle_exercises_every_operation_and_the_boundary_cases() {
    let all = rows();
    let steps: Vec<&Value> = all
        .iter()
        .flat_map(|row| row["steps"].as_array().unwrap().iter())
        .collect();
    let count = |op: &str| steps.iter().filter(|s| s["in"]["op"] == op).count();
    assert_eq!(steps.len(), 153);
    assert_eq!(
        (
            count("reorder"),
            count("duplicate"),
            count("delete"),
            count("rename")
        ),
        (51, 43, 35, 24)
    );
    let noops = steps
        .iter()
        .filter(|s| s["in"]["op"] == "reorder" && s["o"]["changed"] == false)
        .count();
    assert_eq!(noops, 13, "in-place drops");
    let fractional = steps
        .iter()
        .filter(|s| s["in"]["op"] == "reorder" && s["in"]["gap"].as_f64().unwrap().fract() != 0.0)
        .count();
    assert_eq!(fractional, 10, "rounded gaps");
    let widest = steps
        .iter()
        .map(|s| s["o"]["names"].as_array().unwrap().len())
        .max();
    assert_eq!(widest, Some(7));
}

#[test]
fn a_corrupted_expectation_is_caught_in_every_scenario() {
    // The comparator must be able to fail: wrong names, wrong selection and a
    // wrong changed flag in the first step each disagree with the real result.
    let all = rows();
    for how in 0..3 {
        let mut caught = 0;
        for row in &all {
            let mut bad = row.clone();
            let first = &mut bad["steps"][0]["o"];
            match how {
                0 => {
                    let names = first["names"].as_array_mut().unwrap();
                    if names.len() < 2 {
                        continue;
                    }
                    names.swap(0, 1);
                }
                1 => {
                    let len = first["names"].as_array().unwrap().len() as u64;
                    if first.get("active").is_none() || len < 2 {
                        continue;
                    }
                    first["active"] = json!((first["active"].as_u64().unwrap() + 1) % len);
                }
                _ => {
                    if bad["steps"][0]["in"]["op"] != "reorder" {
                        continue;
                    }
                    let flag = bad["steps"][0]["o"]["changed"] == true;
                    bad["steps"][0]["o"]["changed"] = json!(!flag);
                }
            }
            assert!(
                run(&bad, 2).is_err(),
                "mutation {how} went unnoticed: {row}"
            );
            caught += 1;
        }
        assert!(caught >= 8, "mutation {how} was exercised {caught} times");
    }
    // The fixture is honest on the same documents.
    assert!(run(&all[0], 2).is_ok());
}
