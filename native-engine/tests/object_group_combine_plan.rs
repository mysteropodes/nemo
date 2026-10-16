//! Oracle replay for the combine operand-plan kernel (G24/G19, L02-B).
//!
//! `fixtures/object-group-combine-plan/oracle.txt` (`ORACLE`, 125 JSON lines,
//! 152082 bytes) was produced by running the verbatim `src/js/group-bridge.js`
//! of fbe4b8a3 (with the verbatim `findLinkedFillCompanion` of tools.js) in a
//! Node `vm` context against fabricated Paper-like items and symbolic boolean
//! stubs (generator `tools/gen_oracle.js`). Line 1 is `meta`; every other line
//! is one `case`: `items` in z-order, `groups` in table order, the injected
//! `fail` folds, the baseline `render` result (call log, suppress indices,
//! emitted islands with paint) and one `flatten` row per group.
//!
//! Symbolic shapes: an item is `<id>[@ when its Motion is baked][~<companion id>]`,
//! a fold is `<mode>(<operands>)`, its islands are `<fold>#<k>`; `meta.isl` gives
//! the island count per mode. [`Rec`] is the Rust port that speaks the same words.

use std::collections::HashMap;

use serde_json::Value;

use super::super::object_group_hierarchy::{GroupEntry, LayerItem};
use super::{
    execute_flatten, plan_flatten, render_combines, CombineFold, CombineLayer, CombineMode,
    FlattenError, FoldError, GroupLayer, MemberFacts,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-group-combine-plan/oracle.txt");

pub(crate) fn text(v: &Value) -> String {
    v.as_str().expect("string field").to_string()
}

pub(crate) fn opt_text(v: Option<&Value>) -> Option<String> {
    match v {
        None | Some(Value::Null) => None,
        Some(other) => Some(text(other)),
    }
}

pub(crate) fn strings(v: &Value) -> Vec<String> {
    v.as_array().expect("array").iter().map(text).collect()
}

pub(crate) fn indices(v: &Value) -> Vec<usize> {
    v.as_array()
        .expect("array")
        .iter()
        .map(|n| n.as_u64().expect("index") as usize)
        .collect()
}

pub(crate) struct Case {
    pub(crate) name: String,
    pub(crate) ids: Vec<String>,
    pub(crate) mo: Vec<bool>,
    pub(crate) fail: Vec<String>,
    pub(crate) groups: GroupLayer,
    pub(crate) facts: Vec<MemberFacts>,
    pub(crate) row: Value,
}

pub(crate) fn island_counts() -> HashMap<String, usize> {
    let meta: Value = serde_json::from_str(ORACLE.lines().next().unwrap()).unwrap();
    meta["isl"]
        .as_object()
        .expect("isl map")
        .iter()
        .map(|(mode, n)| (mode.clone(), n.as_u64().unwrap() as usize))
        .collect()
}

pub(crate) fn cases() -> Vec<Case> {
    ORACLE
        .lines()
        .skip(1)
        .map(|line| serde_json::from_str::<Value>(line).expect("oracle line"))
        .map(|row| {
            assert_eq!(row["k"], "case");
            let items = row["items"].as_array().expect("items");
            let entries = row["groups"]
                .as_array()
                .expect("groups")
                .iter()
                .map(|g| GroupEntry {
                    gid: text(&g["gid"]),
                    name: opt_text(g.get("name")),
                    combine_mode: g
                        .get("mode")
                        .map(|m| CombineMode::parse(m.as_str().unwrap()).expect("mode")),
                    parent: opt_text(g.get("parent")),
                    order: strings(&g["order"]),
                })
                .collect();
            let layer_items = items
                .iter()
                .map(|i| LayerItem {
                    stroke_id: text(&i["id"]),
                    dup_index: i.get("dup").map(|d| d.as_u64().unwrap() as u32),
                    group_id: None,
                })
                .collect();
            let facts = items
                .iter()
                .map(|i| MemberFacts {
                    visible: i.get("vis").map_or(true, |v| v.as_bool().unwrap()),
                    linked_fill_id: opt_text(i.get("lf")),
                    brush_group_id: opt_text(i.get("bg")),
                    is_linked_fill_companion: i.get("comp").is_some(),
                    is_vector_brush: i.get("vb").is_some(),
                    fill_color: opt_text(i.get("fill")),
                    stroke_color: opt_text(i.get("stroke")),
                    stroke_width: i["sw"].as_f64().unwrap(),
                    opacity: i["op"].as_f64().unwrap(),
                })
                .collect();
            Case {
                name: text(&row["n"]),
                ids: items.iter().map(|i| text(&i["id"])).collect(),
                mo: items.iter().map(|i| i.get("mo").is_some()).collect(),
                fail: strings(&row["fail"]),
                groups: GroupLayer::new(entries, layer_items).expect("well formed fixture"),
                facts,
                row,
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sh {
    pub(crate) desc: String,
}

/// Recording port speaking the oracle's symbolic words; `fail` lists fold
/// descriptions that raise `FoldError::Failed` (the baseline's thrown fold).
pub(crate) struct Rec {
    pub(crate) ids: Vec<String>,
    pub(crate) mo: Vec<bool>,
    pub(crate) fail: Vec<String>,
    pub(crate) isl: HashMap<String, usize>,
    pub(crate) calls: Vec<String>,
}

impl Rec {
    pub(crate) fn for_case(case: &Case) -> Rec {
        Rec {
            ids: case.ids.clone(),
            mo: case.mo.clone(),
            fail: case.fail.clone(),
            isl: island_counts(),
            calls: Vec::new(),
        }
    }
}

impl CombineFold for Rec {
    type Shape = Sh;

    fn member_shape(&mut self, item: usize, companion: Option<usize>) -> Result<Sh, FoldError> {
        let baked = if self.mo[item] { "@" } else { "" };
        let tail = companion.map_or(String::new(), |c| format!("~{}", self.ids[c]));
        Ok(Sh {
            desc: format!("{}{}{}", self.ids[item], baked, tail),
        })
    }

    fn fold(&mut self, mode: CombineMode, operands: &[Sh]) -> Result<Sh, FoldError> {
        let parts: Vec<&str> = operands.iter().map(|o| o.desc.as_str()).collect();
        let desc = format!("{}({})", mode.as_str(), parts.join(","));
        self.calls.push(format!("fold:{desc}"));
        if self.fail.contains(&desc) {
            return Err(FoldError::Failed(desc));
        }
        Ok(Sh { desc })
    }

    fn islands(&mut self, shape: &Sh) -> Result<Vec<Sh>, FoldError> {
        self.calls.push(format!("islands:{}", shape.desc));
        let mode = shape.desc.split('(').next().unwrap();
        Ok((0..self.isl[mode])
            .map(|k| Sh {
                desc: format!("{}#{}", shape.desc, k),
            })
            .collect())
    }
}

/// Hand-built layer for the literal controls (never derived from the oracle).
pub(crate) struct Lay {
    pub(crate) groups: GroupLayer,
    pub(crate) facts: Vec<MemberFacts>,
    pub(crate) ids: Vec<String>,
}

pub(crate) fn fact() -> MemberFacts {
    MemberFacts {
        visible: true,
        linked_fill_id: None,
        brush_group_id: None,
        is_linked_fill_companion: false,
        is_vector_brush: false,
        fill_color: Some("F".to_string()),
        stroke_color: None,
        stroke_width: 1.0,
        opacity: 1.0,
    }
}

pub(crate) fn grp(
    gid: &str,
    mode: Option<CombineMode>,
    parent: Option<&str>,
    order: &[&str],
) -> GroupEntry {
    GroupEntry {
        gid: gid.to_string(),
        name: None,
        combine_mode: mode,
        parent: parent.map(str::to_string),
        order: order.iter().map(|id| id.to_string()).collect(),
    }
}

/// `items` are `(stroke id, duplicator index)` in z-order.
pub(crate) fn lay_dup(items: &[(&str, Option<u32>)], groups: Vec<GroupEntry>) -> Lay {
    let layer_items = items
        .iter()
        .map(|(id, dup)| LayerItem {
            stroke_id: id.to_string(),
            dup_index: *dup,
            group_id: None,
        })
        .collect();
    Lay {
        groups: GroupLayer::new(groups, layer_items).expect("well formed layer"),
        facts: items.iter().map(|_| fact()).collect(),
        ids: items.iter().map(|(id, _)| id.to_string()).collect(),
    }
}

pub(crate) fn lay(ids: &[&str], groups: Vec<GroupEntry>) -> Lay {
    let items: Vec<(&str, Option<u32>)> = ids.iter().map(|id| (*id, None)).collect();
    lay_dup(&items, groups)
}

impl Lay {
    /// Edits the facts of the FIRST item with this stroke id.
    pub(crate) fn with(mut self, id: &str, edit: impl FnOnce(&mut MemberFacts)) -> Lay {
        let at = self.ids.iter().position(|have| have == id).expect("item");
        edit(&mut self.facts[at]);
        self
    }

    pub(crate) fn view(&self) -> CombineLayer<'_> {
        CombineLayer::new(&self.groups, &self.facts).expect("facts parallel items")
    }

    pub(crate) fn port(&self) -> Rec {
        Rec {
            ids: self.ids.clone(),
            mo: vec![false; self.ids.len()],
            fail: Vec::new(),
            isl: island_counts(),
            calls: Vec::new(),
        }
    }
}

#[test]
fn oracle_corpus_shape_is_pinned() {
    assert_eq!(ORACLE.len(), 152082);
    let meta: Value = serde_json::from_str(ORACLE.lines().next().unwrap()).unwrap();
    assert_eq!(meta["commit"], "fbe4b8a33e6d5a229f014154df2ba21988a9fabc");
    let mut counts: Vec<(String, usize)> = island_counts().into_iter().collect();
    counts.sort();
    let want: Vec<(String, usize)> = [
        ("exclude", 3),
        ("intersect", 0),
        ("subtract", 2),
        ("unite", 1),
    ]
    .iter()
    .map(|(m, n)| (m.to_string(), *n))
    .collect();
    assert_eq!(counts, want);
    let all = cases();
    assert_eq!(all.len(), 124);
    let folding = all
        .iter()
        .filter(|c| !c.row["render"]["calls"].as_array().unwrap().is_empty())
        .count();
    assert_eq!(folding, 96);
    assert_eq!(all.iter().filter(|c| !c.fail.is_empty()).count(), 23);
    let (mut paths, mut suppress, mut calls) = (0, 0, 0);
    for c in &all {
        paths += c.row["render"]["extra"].as_array().unwrap().len();
        suppress += c.row["render"]["suppress"].as_array().unwrap().len();
        calls += c.row["render"]["calls"].as_array().unwrap().len();
    }
    assert_eq!((paths, suppress, calls), (142, 310, 304));
    let mut outcomes: HashMap<String, usize> = HashMap::new();
    for c in &all {
        for f in c.row["flatten"].as_array().unwrap() {
            *outcomes.entry(text(&f["out"])).or_default() += 1;
        }
    }
    assert_eq!(outcomes["ok"], 229);
    assert_eq!(outcomes["fewer"], 31);
    assert_eq!(outcomes["throws"], 24);
    assert_eq!(outcomes.len(), 3);
}

#[test]
fn render_combines_match_the_baseline() {
    for case in cases() {
        let layer = CombineLayer::new(&case.groups, &case.facts).unwrap();
        let mut port = Rec::for_case(&case);
        let out = render_combines(&layer, &mut port).expect(&case.name);
        let want = &case.row["render"];
        assert_eq!(port.calls, strings(&want["calls"]), "{} calls", case.name);
        assert_eq!(
            out.suppress,
            indices(&want["suppress"]),
            "{} suppress",
            case.name
        );
        let extra = want["extra"].as_array().unwrap();
        assert_eq!(out.paths.len(), extra.len(), "{} paths", case.name);
        for (got, exp) in out.paths.iter().zip(extra) {
            let at = format!("{} {}", case.name, got.shape.desc);
            assert_eq!(got.group, text(&exp["g"]), "{at}");
            assert_eq!(got.shape.desc, text(&exp["d"]), "{at}");
            assert_eq!(got.style.fill, opt_text(exp.get("fill")), "{at} fill");
            assert_eq!(got.style.stroke, opt_text(exp.get("stroke")), "{at} stroke");
            assert_eq!(
                got.style.stroke_width,
                exp["sw"].as_f64().unwrap(),
                "{at} width"
            );
            assert_eq!(
                got.style.opacity,
                exp["op"].as_f64().unwrap(),
                "{at} opacity"
            );
        }
    }
}

#[test]
fn flatten_matches_the_baseline() {
    let (mut ok, mut fewer, mut throws) = (0, 0, 0);
    for case in cases() {
        let layer = CombineLayer::new(&case.groups, &case.facts).unwrap();
        for f in case.row["flatten"].as_array().unwrap() {
            let gid = text(&f["gid"]);
            let at = format!("{} flatten {gid}", case.name);
            let planned = plan_flatten(&layer, &gid);
            match f["out"].as_str().unwrap() {
                "fewer" => {
                    fewer += 1;
                    let n = f["members"].as_u64().unwrap() as usize;
                    assert_eq!(planned, Err(FlattenError::FewerThanTwoMembers(n)), "{at}");
                }
                "throws" => {
                    throws += 1;
                    assert_eq!(
                        planned,
                        Err(FlattenError::MissingCombineMode(gid.clone())),
                        "{at}"
                    );
                    assert_eq!(case.groups.entry(&gid).unwrap().combine_mode, None, "{at}");
                }
                "ok" => {
                    ok += 1;
                    let plan = planned.expect(&at);
                    assert_eq!(plan.remove, indices(&f["removed"]), "{at} remove");
                    assert_eq!(
                        plan.insert_at,
                        f["at"].as_u64().unwrap() as usize,
                        "{at} insert"
                    );
                    assert_eq!(plan.fill, opt_text(f.get("fill")), "{at} fill");
                    assert_eq!(plan.opacity, f["op"].as_f64().unwrap(), "{at} opacity");
                    assert_eq!(
                        plan.data_source,
                        f["src"].as_u64().unwrap() as usize,
                        "{at} src"
                    );
                    assert_eq!(plan.removed_groups, strings(&f["gone"]), "{at} gone");
                    let kept: Vec<String> = case
                        .groups
                        .entries()
                        .iter()
                        .map(|e| e.gid.clone())
                        .filter(|g| !plan.removed_groups.contains(g))
                        .collect();
                    assert_eq!(kept, strings(&f["groups"]), "{at} table");
                    // The flatten rows were recorded without injected failures
                    // (gen_oracle.js runFlatten passes null); the case's render
                    // `fail` list may name a fold this flatten also produces.
                    let mut port = Rec::for_case(&case);
                    port.fail.clear();
                    let islands = execute_flatten(&plan, &mut port).expect(&at);
                    // The oracle logs fold entries only (the baseline stub never logs
                    // islands); the Rec port also records its islands() call.
                    let folds: Vec<String> = port
                        .calls
                        .iter()
                        .filter(|c| c.starts_with("fold:"))
                        .cloned()
                        .collect();
                    assert_eq!(folds, strings(&f["calls"]), "{at} calls");
                    assert!(port.calls[0].starts_with(&format!("fold:{}(", plan.mode.as_str())));
                    assert_eq!(port.calls.len(), folds.len() + 1, "{at} call count");
                    assert_eq!(
                        port.calls[1],
                        format!("islands:{}", &folds[0]["fold:".len()..]),
                        "{at} islands call"
                    );
                    let sel: Vec<String> = islands.iter().map(|s| s.desc.clone()).collect();
                    assert_eq!(sel, strings(&f["sel"]), "{at} selection");
                    assert_eq!(
                        islands.len(),
                        f["islands"].as_u64().unwrap() as usize,
                        "{at}"
                    );
                    if !islands.is_empty() {
                        let want = f["final"].as_u64().unwrap() as usize;
                        assert_eq!(plan.final_index, want, "{at} final index");
                    }
                }
                other => panic!("unknown outcome {other}"),
            }
        }
    }
    assert_eq!((ok, fewer, throws), (229, 31, 24));
}
