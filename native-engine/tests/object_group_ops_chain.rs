//! L02-G baseline chains: every scenario of `fixtures/object-group-ops/oracle.txt`
//! (generated from the verbatim `src/js/group-bridge.js` of fbe4b8a3 by chaining
//! groupSelection / ungroupSelection / combineSelection / setGroupCombineMode /
//! renameGroup in one Node vm context, then normalising with the baseline's own
//! dissolveGroup) is replayed through the real owner on v1 and v2 documents. The
//! expected tables are literals in the fixture; the replay also walks undo and
//! redo back and forth over the exact bytes of every step.
use super::object_group_ops_tests::{
    apply, bytes, combine, document, exit_group, group, owner_of, rename, set_mode, table,
    table_of, ungroup,
};
use super::*;
use serde_json::{json, Value};

const ORACLE: &str = include_str!("fixtures/object-group-ops/oracle.txt");

fn rows() -> Vec<Value> {
    ORACLE
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .map(|list| list.iter().map(|item| item.as_str().unwrap()).collect())
        .unwrap_or_default()
}
fn start(version: u32, row: &Value) -> Value {
    let groups = row["table"]
        .as_bool()
        .unwrap()
        .then(|| table(row["entries"].clone(), row["members"].clone()));
    document(version, &strings(&row["items"]), groups)
}
/// The short op name and payload of one recorded step.
fn payload_of(step: &Value) -> (&'static str, Value) {
    let strokes = strings(&step["strokes"]);
    let text = |key: &str| step[key].as_str().unwrap_or_default().to_string();
    match step["op"].as_str().unwrap() {
        "group" => ("group", group(&strokes, &text("gid"), &text("name"))),
        "ungroup" => ("ungroup", ungroup(&strokes)),
        "combine" => ("combine", combine(&strokes, &text("gid"), &text("mode"))),
        "mode" => ("mode", set_mode(&text("gid"), &text("mode"))),
        "rename" => ("rename", rename(&text("gid"), &text("name"))),
        "exit" => ("exit", exit_group(&text("stroke"))),
        other => panic!("unknown oracle op {other}"),
    }
}

/// Replays every step; `corrupt` flips one expectation so a comparator that
/// ignores its input would pass. Returns the number of steps replayed.
fn replay(version: u32, corrupt: bool) -> usize {
    let mut replayed = 0;
    for row in rows() {
        let mut owner = owner_of(&start(version, &row));
        let mut states = vec![bytes(&owner)];
        for step in row["steps"].as_array().unwrap() {
            let (op, payload) = payload_of(step);
            let (before, revision, depths) = (
                bytes(&owner),
                owner.content_revision(),
                owner.history_depths(),
            );
            let outcome = apply(&mut owner, op, &payload);
            let mut want_changed = step["changed"].as_bool().unwrap();
            if corrupt && replayed == 0 {
                want_changed = !want_changed;
            }
            let label = format!("{} v{version} step {replayed}: {step}", row["id"]);
            if want_changed {
                assert_eq!(outcome, Ok(true), "{label}");
                assert_eq!(owner.content_revision(), revision + 1, "{label}");
                assert_eq!(owner.history_depths(), (depths.0 + 1, 0), "{label}");
                states.push(bytes(&owner));
            } else {
                // Refused by the kernel or a semantic no-op: nothing is published.
                assert!(matches!(outcome, Err(_) | Ok(false)), "{label}");
                assert_eq!(
                    (
                        bytes(&owner),
                        owner.content_revision(),
                        owner.history_depths()
                    ),
                    (before, revision, depths),
                    "{label}"
                );
            }
            let (entries, members) = table_of(&owner);
            assert_eq!(entries, step["entries"], "{label}");
            assert_eq!(members, step["members"], "{label}");
            replayed += 1;
        }
        // Undo to the first state and redo to the last, byte for byte.
        for earlier in states.iter().rev().skip(1) {
            owner.undo(owner.content_revision()).unwrap();
            assert_eq!(&bytes(&owner), earlier, "{} v{version} undo", row["id"]);
        }
        for later in states.iter().skip(1) {
            owner.redo(owner.content_revision()).unwrap();
            assert_eq!(&bytes(&owner), later, "{} v{version} redo", row["id"]);
        }
        assert_eq!(owner.history_depths(), (states.len() - 1, 0));
    }
    replayed
}

#[test]
fn the_oracle_corpus_shape_is_pinned() {
    let rows = rows();
    assert_eq!(rows.len(), 21);
    let steps: usize = rows
        .iter()
        .map(|r| r["steps"].as_array().unwrap().len())
        .sum();
    assert_eq!(steps, 378);
    let mut ops = std::collections::BTreeMap::new();
    let (mut changed, mut noops) = (0, 0);
    for step in rows
        .iter()
        .flat_map(|r| r["steps"].as_array().unwrap().iter())
    {
        *ops.entry(step["op"].as_str().unwrap().to_string())
            .or_insert(0) += 1;
        if step["changed"].as_bool().unwrap() {
            changed += 1;
        } else {
            noops += 1;
        }
    }
    assert_eq!(
        ops.keys().map(String::as_str).collect::<Vec<_>>(),
        ["combine", "exit", "group", "mode", "rename", "ungroup"]
    );
    assert_eq!((changed, noops), (305, 73));
    assert_eq!(
        ops.values().copied().collect::<Vec<_>>(),
        [64, 40, 134, 56, 34, 50]
    );
}

#[test]
fn every_baseline_chain_replays_exactly_on_v1_and_v2_documents() {
    assert_eq!(replay(1, false), 378);
    assert_eq!(replay(2, false), 378);
}

#[test]
fn a_corrupted_expectation_is_caught() {
    for version in [1, 2] {
        let caught = std::panic::catch_unwind(|| replay(version, true));
        assert!(
            caught.is_err(),
            "a flipped `changed` flag must fail the replay (v{version})"
        );
    }
}

#[test]
fn a_changing_first_step_really_changes_the_table() {
    let all = rows();
    let row = all
        .iter()
        .find(|row| row["steps"][0]["changed"] == true)
        .unwrap();
    let mut owner = owner_of(&start(1, row));
    let before = table_of(&owner);
    let (op, payload) = payload_of(&row["steps"][0]);
    assert_eq!(apply(&mut owner, op, &payload), Ok(true));
    assert_ne!(table_of(&owner), before);
    assert_ne!(
        table_of(&owner).0,
        json!([]),
        "the recorded step leaves a non-empty table"
    );
}
