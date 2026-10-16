//! Oracle replay for the group hierarchy kernel (G24, L02-A).
//!
//! `fixtures/object-group-hierarchy/oracle.txt` (`ORACLE`, 1945 JSON lines,
//! 624794 bytes, sha256 1166fd8b8877b8011148a0ac29cd37724e71e895cb0318192abb8486faf1da23)
//! was produced by running the verbatim `src/js/group-bridge.js` of fbe4b8a3 in a
//! Node `vm` context (generator `tools/gen_oracle.js`; Date.now and Math.random
//! pinned, the unexported `detachLeaf` exported by one inserted line, every other
//! global stubbed) against fabricated `ld.groups` / `layer.children`.
//!
//! Rows: `{"k":"layer","l":id,"spec":{items,groups}}` declares a layer (items in
//! z-order `{id,dup?,gid?}`; groups in table order `{gid,name?,mode?,parent?,order}`);
//! every other row is `{"k":op,"l":layer id,"p":params,"o":baseline result}`.
//! Mutations carry the baseline's resulting layer (`state`) and `applied` (the
//! baseline did not return early); queries carry `v`. The corpus holds 14 seeded
//! random layers with corrupt shapes (cycles, dangling parents, tag mismatches).

use std::collections::HashMap;

use super::selection::{CombineOutcome, GroupOutcome};
use super::{CombineMode, GroupEntry, GroupError, GroupLayer, LayerItem};
use serde_json::{json, Map, Value};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-group-hierarchy/oracle.txt");
/// The id the baseline generated with Date.now / Math.random pinned.
pub(crate) const BASELINE_GID: &str = "grp_fr5hugnf_500000";

fn text(v: &Value) -> String {
    v.as_str().expect("string field").to_string()
}

fn indices(v: &Value) -> Vec<usize> {
    v.as_array()
        .expect("array")
        .iter()
        .map(|n| n.as_u64().expect("index") as usize)
        .collect()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array().expect("array").iter().map(text).collect()
}

pub(crate) fn layer_from(spec: &Value) -> GroupLayer {
    let entries = spec["groups"]
        .as_array()
        .expect("groups")
        .iter()
        .map(|g| GroupEntry {
            gid: text(&g["gid"]),
            name: g.get("name").map(text),
            combine_mode: g
                .get("mode")
                .map(|m| CombineMode::parse(m.as_str().unwrap()).expect("mode")),
            parent: g.get("parent").map(text),
            order: strings(&g["order"]),
        })
        .collect();
    let items = spec["items"]
        .as_array()
        .expect("items")
        .iter()
        .map(|i| LayerItem {
            stroke_id: text(&i["id"]),
            dup_index: i.get("dup").map(|d| d.as_u64().expect("dup") as u32),
            group_id: i.get("gid").map(text),
        })
        .collect();
    GroupLayer::new(entries, items).expect("fixture layers are well formed")
}

/// Same shape the generator's `snap()` emits (absent keys stay absent).
pub(crate) fn snap(layer: &GroupLayer) -> Value {
    let items: Vec<Value> = layer
        .items()
        .iter()
        .map(|i| {
            let mut m = Map::new();
            m.insert("id".into(), json!(i.stroke_id));
            if let Some(d) = i.dup_index {
                m.insert("dup".into(), json!(d));
            }
            if let Some(g) = &i.group_id {
                m.insert("gid".into(), json!(g));
            }
            Value::Object(m)
        })
        .collect();
    let groups: Vec<Value> = layer
        .entries()
        .iter()
        .map(|g| {
            let mut m = Map::new();
            m.insert("gid".into(), json!(g.gid));
            if let Some(n) = &g.name {
                m.insert("name".into(), json!(n));
            }
            if let Some(mode) = g.combine_mode {
                m.insert("mode".into(), json!(mode.as_str()));
            }
            if let Some(p) = &g.parent {
                m.insert("parent".into(), json!(p));
            }
            m.insert("order".into(), json!(g.order));
            Value::Object(m)
        })
        .collect();
    json!({ "items": items, "groups": groups })
}

struct Row {
    kind: String,
    layer: String,
    p: Value,
    o: Value,
}

fn corpus() -> (HashMap<String, Value>, Vec<Row>) {
    let mut layers = HashMap::new();
    let mut rows = Vec::new();
    for line in ORACLE.lines() {
        let v: Value = serde_json::from_str(line).expect("oracle line is JSON");
        if v["k"] == "layer" {
            layers.insert(text(&v["l"]), v["spec"].clone());
        } else {
            rows.push(Row {
                kind: text(&v["k"]),
                layer: text(&v["l"]),
                p: v["p"].clone(),
                o: v["o"].clone(),
            });
        }
    }
    (layers, rows)
}

fn expect_refusal(
    layer: &GroupLayer,
    before: &GroupLayer,
    row: &Row,
    err: GroupError,
    want: GroupError,
) {
    assert_eq!(err, want, "{} on {}: {}", row.kind, row.layer, row.p);
    assert_eq!(
        layer, before,
        "{} refusal must not mutate {}",
        row.kind, row.layer
    );
    assert_eq!(
        snap(layer),
        row.o["state"],
        "{} refusal state on {}",
        row.kind,
        row.layer
    );
}

/// Replays one row against the kernel; returns whether `group` dissolved its owner.
fn replay(row: &Row, spec: &Value) -> bool {
    let mut layer = layer_from(spec);
    let before = layer.clone();
    let applied = row.o.get("applied").and_then(Value::as_bool);
    let who = format!("{} on {} with {}", row.kind, row.layer, row.p);
    let gid = row.p.get("gid").map(text).unwrap_or_default();
    let sel = row.p.get("sel").map(indices).unwrap_or_default();
    let item = row
        .p
        .get("item")
        .map(|n| n.as_u64().unwrap() as usize)
        .unwrap_or(0);
    let mut owner_dissolved = false;
    match row.kind.as_str() {
        "root" => assert_eq!(json!(layer.root_group_of(&gid)), row.o["v"], "{who}"),
        "parent" => assert_eq!(json!(layer.parent_group_of(&gid)), row.o["v"], "{who}"),
        "rootItem" => assert_eq!(json!(layer.root_group_of_item(item)), row.o["v"], "{who}"),
        "collect" => assert_eq!(
            json!(layer.collect_group_stroke_ids(&gid)),
            row.o["v"],
            "{who}"
        ),
        "members" => assert_eq!(json!(layer.members_of(item)), row.o["v"], "{who}"),
        "resolve" => {
            let dup = row.p.get("dup").map(|d| d.as_u64().unwrap() as u32);
            assert_eq!(
                json!(layer.resolve_group_members(&gid, dup)),
                row.o["v"],
                "{who}"
            );
        }
        "group" => {
            let name = text(&row.p["name"]);
            match layer.group_selection(&sel, &gid, &name) {
                Ok(out) => {
                    assert_eq!(applied, Some(true), "{who}");
                    assert_eq!(out.gid, gid);
                    assert_eq!(json!(out.units.len()), row.o["units"], "{who}");
                    owner_dissolved = out.owner_dissolved;
                }
                Err(err) => {
                    assert_eq!(applied, Some(false), "{who}");
                    expect_refusal(
                        &layer,
                        &before,
                        row,
                        err,
                        GroupError::FewerThanTwoUnits(if sel.len() < 2 { sel.len() } else { 1 }),
                    );
                }
            }
        }
        "ungroup" => match layer.ungroup_selection(&sel) {
            Ok(_) => assert_eq!(applied, Some(true), "{who}"),
            Err(err) => {
                assert_eq!(applied, Some(false), "{who}");
                let want = if sel.is_empty() {
                    GroupError::EmptySelection
                } else {
                    GroupError::NoGroupInSelection
                };
                expect_refusal(&layer, &before, row, err, want);
            }
        },
        "combine" => {
            let mode = CombineMode::parse(row.p["mode"].as_str().unwrap()).expect("mode");
            match layer.combine_selection(&sel, mode, &gid) {
                Ok(out) => {
                    assert_eq!(applied, Some(true), "{who}");
                    let created = matches!(out, CombineOutcome::Created { .. });
                    assert_eq!(json!(created), row.o["created"], "{who}");
                }
                Err(err) => {
                    assert_eq!(applied, Some(false), "{who}");
                    expect_refusal(
                        &layer,
                        &before,
                        row,
                        err,
                        GroupError::FewerThanTwoUnits(sel.len()),
                    );
                }
            }
        }
        "setmode" => {
            let mode = CombineMode::parse(row.p["mode"].as_str().unwrap()).expect("mode");
            match layer.set_combine_mode(&gid, mode) {
                Ok(_) => assert_eq!(applied, Some(true), "{who}"),
                Err(err) => {
                    assert_eq!(applied, Some(false), "{who}");
                    expect_refusal(
                        &layer,
                        &before,
                        row,
                        err,
                        GroupError::UnknownGroup(gid.clone()),
                    );
                }
            }
        }
        "rename" => {
            match layer.rename_group(&gid, &text(&row.p["name"]), &strings(&row.p["members"])) {
                Ok(_) => assert_eq!(applied, Some(true), "{who}"),
                Err(err) => {
                    assert_eq!(applied, Some(false), "{who}");
                    expect_refusal(&layer, &before, row, err, GroupError::EmptyGroupId);
                }
            }
        }
        "remove" => match layer.remove_member(item) {
            Ok(_) => assert_eq!(applied, Some(true), "{who}"),
            Err(err) => {
                assert_eq!(applied, Some(false), "{who}");
                expect_refusal(&layer, &before, row, err, GroupError::NotInGroup(item));
            }
        },
        "dissolve" => match layer.dissolve_group(&gid) {
            Ok(()) => assert_eq!(applied, Some(true), "{who}"),
            Err(err) => {
                assert_eq!(applied, Some(false), "{who}");
                expect_refusal(
                    &layer,
                    &before,
                    row,
                    err,
                    GroupError::UnknownGroup(gid.clone()),
                );
            }
        },
        "detach" => layer.detach_leaf(&text(&row.p["sid"])),
        "subtree" => layer.remove_subtree(&gid),
        other => panic!("unknown oracle kind {other}"),
    }
    if row.o.get("state").is_some() {
        assert_eq!(snap(&layer), row.o["state"], "{who}");
    }
    owner_dissolved
}

fn replay_kinds(kinds: &[&str]) -> usize {
    let (layers, rows) = corpus();
    let mut count = 0;
    for row in rows.iter().filter(|r| kinds.contains(&r.kind.as_str())) {
        replay(row, &layers[&row.layer]);
        count += 1;
    }
    count
}

#[test]
fn oracle_corpus_shape_is_pinned() {
    assert_eq!(ORACLE.len(), 624794);
    let (layers, rows) = corpus();
    assert_eq!(layers.len(), 31);
    assert_eq!(rows.len(), 1914);
    let mut by_kind: HashMap<&str, usize> = HashMap::new();
    for row in &rows {
        *by_kind.entry(row.kind.as_str()).or_default() += 1;
    }
    let want = [
        ("root", 96),
        ("parent", 96),
        ("collect", 96),
        ("resolve", 138),
        ("dissolve", 96),
        ("subtree", 42),
        ("setmode", 63),
        ("rename", 125),
        ("rootItem", 155),
        ("members", 114),
        ("remove", 155),
        ("detach", 122),
        ("ungroup", 209),
        ("group", 252),
        ("combine", 155),
    ];
    for (kind, n) in want {
        assert_eq!(by_kind.get(kind), Some(&n), "{kind} rows");
    }
    let applied = |kind: &str, flag: bool| {
        rows.iter()
            .filter(|r| r.kind == kind && r.o["applied"] == json!(flag))
            .count()
    };
    assert_eq!((applied("group", true), applied("group", false)), (175, 77));
    assert_eq!(
        (applied("ungroup", true), applied("ungroup", false)),
        (168, 41)
    );
    assert_eq!(
        (applied("combine", true), applied("combine", false)),
        (131, 24)
    );
    assert_eq!(
        (applied("remove", true), applied("remove", false)),
        (120, 35)
    );
    assert_eq!(
        (applied("dissolve", true), applied("dissolve", false)),
        (79, 17)
    );
}

#[test]
fn queries_match_the_baseline_including_corrupt_tables() {
    assert_eq!(
        replay_kinds(&["root", "parent", "rootItem", "collect", "resolve", "members"]),
        695
    );
}

#[test]
fn group_selection_matches_the_baseline() {
    let (layers, rows) = corpus();
    let mut dissolved = 0;
    let mut owners = 0;
    for row in rows.iter().filter(|r| r.kind == "group") {
        if replay(row, &layers[&row.layer]) {
            dissolved += 1;
        }
        let before = layers[&row.layer]["groups"].as_array().unwrap().len();
        if row.o["applied"] == json!(true)
            && row.o["state"]["groups"].as_array().unwrap().len() <= before
        {
            owners += 1;
        }
    }
    // Rust reports exactly the rows where the baseline's table did not grow
    // (one group added, the common owner dissolved).
    assert_eq!((dissolved, owners), (13, 13));
}

#[test]
fn ungroup_and_combine_match_the_baseline() {
    assert_eq!(replay_kinds(&["ungroup", "combine"]), 364);
}

#[test]
fn member_and_table_edits_match_the_baseline() {
    let n = replay_kinds(&[
        "setmode", "rename", "remove", "detach", "dissolve", "subtree",
    ]);
    assert_eq!(n, 63 + 125 + 155 + 122 + 96 + 42);
}

#[test]
fn outcome_units_count_matches_loose_shapes_and_nested_groups() {
    let (layers, rows) = corpus();
    let mut nested = 0;
    for row in rows
        .iter()
        .filter(|r| r.kind == "group" && r.o["applied"] == json!(true))
    {
        let mut layer = layer_from(&layers[&row.layer]);
        let out: GroupOutcome = layer
            .group_selection(&indices(&row.p["sel"]), BASELINE_GID, "autoNameGroup")
            .expect("applied row");
        if out.units.len() < indices(&row.p["sel"]).len() {
            nested += 1;
        }
    }
    assert_eq!(
        nested, 29,
        "rows where a fully selected group was nested as ONE unit"
    );
}
