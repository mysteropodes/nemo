//! Mutation controls for the group hierarchy kernel (G24, L02-A).
//!
//! Each rule is guarded by a checker. The real kernel result must satisfy it,
//! and a hand-built mutant (the same scenario with that rule removed) must
//! FAIL it. A rule dropped from the production code therefore fails the first
//! assertion, and a checker that stopped discriminating fails the second.

use super::controls::{copy, grp, item, make, tags, tower};
use super::selection::GroupUnit;
use super::{CombineMode as M, GroupLayer};

type Check = Result<(), String>;

/// Rule: a group left with fewer than two entries dissolves.
fn no_single_entry_group(layer: &GroupLayer) -> Check {
    match layer.entries().iter().find(|e| e.order.len() < 2) {
        Some(e) => Err(format!("group {} kept {} entries", e.gid, e.order.len())),
        None => Ok(()),
    }
}

/// Rule: a group's leaves follow layer z-order, never selection order.
fn leaves_in_z_order(layer: &GroupLayer, gid: &str) -> Check {
    let at: Vec<usize> = layer
        .entry(gid)
        .ok_or("missing group")?
        .order
        .iter()
        .filter_map(|id| layer.items().iter().position(|i| i.stroke_id == *id))
        .collect();
    if at.windows(2).all(|w| w[0] < w[1]) {
        Ok(())
    } else {
        Err(format!("order {at:?}"))
    }
}

/// Rule: the new group sits at the smallest order index its units had.
fn sits_at(layer: &GroupLayer, owner: &str, gid: &str, want: usize) -> Check {
    let got = layer
        .entry(owner)
        .ok_or("owner dissolved")?
        .order
        .iter()
        .position(|e| e == gid);
    if got == Some(want) {
        Ok(())
    } else {
        Err(format!("at {got:?}, wanted {want}"))
    }
}

/// Rule: ungroup dissolves exactly one level.
fn one_group_fewer(before: &GroupLayer, after: &GroupLayer) -> Check {
    let (b, a) = (before.entries().len(), after.entries().len());
    if a + 1 == b {
        Ok(())
    } else {
        Err(format!("{b} groups became {a}"))
    }
}

/// Rule: surviving groups keep their combine mode and name.
fn survivors_keep_metadata(before: &GroupLayer, after: &GroupLayer) -> Check {
    for e in after.entries() {
        let old = before
            .entry(&e.gid)
            .ok_or("survivor was not in the table")?;
        if (old.combine_mode, &old.name) != (e.combine_mode, &e.name) {
            return Err(format!("{} lost its metadata", e.gid));
        }
    }
    Ok(())
}

/// Rule: every tag names a group that lists the item (no dangling tag).
fn tags_resolve(layer: &GroupLayer) -> Check {
    for i in layer.items() {
        if let Some(t) = i.group_id.as_deref() {
            let listed = layer
                .entry(t)
                .is_some_and(|e| e.order.contains(&i.stroke_id));
            if !listed {
                return Err(format!("{} tagged {t} but not listed", i.stroke_id));
            }
        }
    }
    Ok(())
}

/// Rule: dissolve only touches the dup-0 bucket of a leaf id.
fn copies_keep_tags(layer: &GroupLayer) -> Check {
    if layer
        .items()
        .iter()
        .filter(|i| i.dup_index.is_some())
        .all(|i| i.group_id.is_some())
    {
        Ok(())
    } else {
        Err("a duplicator copy lost its tag".into())
    }
}

fn unchanged(before: &GroupLayer, after: &GroupLayer) -> Check {
    if before == after {
        Ok(())
    } else {
        Err("refusal mutated the layer".into())
    }
}

fn assert_guard(real: Check, mutant: Check, rule: &str) {
    assert_eq!(real, Ok(()), "{rule}: the kernel must satisfy its own rule");
    assert!(
        mutant.is_err(),
        "{rule}: the checker must reject the mutant"
    );
}

fn pair() -> GroupLayer {
    let mut named = grp("OUTER", None, Some(M::Unite), &["IN", "z"]);
    named.name = Some("keep".into());
    let items = vec![
        item("y1", Some("IN")),
        item("y2", Some("IN")),
        item("z", Some("OUTER")),
    ];
    make(
        items,
        vec![
            named,
            grp("IN", Some("OUTER"), Some(M::Subtract), &["y1", "y2"]),
        ],
    )
}

#[test]
fn guard_group_below_two_entries_dissolves() {
    let mut real = pair();
    real.remove_member(0).unwrap();
    // Mutant: the member leaves but the group is not dissolved.
    let mut kept = pair();
    kept.entries[1].order.retain(|id| id != "y1");
    kept.items[0].group_id = None;
    assert_guard(
        no_single_entry_group(&real),
        no_single_entry_group(&kept),
        "dissolve below two",
    );
    // Mutant: dissolved and promoted, but the surviving leaf keeps its stale tag.
    let mut stale = kept.clone();
    stale.entries[0].order = vec!["y2".into(), "z".into()];
    stale.entries.remove(1);
    assert_guard(
        tags_resolve(&real),
        tags_resolve(&stale),
        "tags follow the dissolve",
    );
}

#[test]
fn guard_common_owner_left_with_one_entry_dissolves() {
    let layer = make(
        vec![item("a", Some("ZZ")), item("b", Some("ZZ"))],
        vec![grp("P", None, Some(M::Exclude), &["a", "b"])],
    );
    let mut real = layer.clone();
    real.group_selection(&[0, 1], "G", "N").unwrap();
    // Mutant: the owner keeps the new group as its only entry.
    let mut mutant = layer.clone();
    mutant.group_selection(&[0, 1], "G", "N").unwrap();
    mutant
        .entries
        .insert(0, grp("P", None, Some(M::Exclude), &["G"]));
    mutant.entries[1].parent = Some("P".into());
    assert_guard(
        no_single_entry_group(&real),
        no_single_entry_group(&mutant),
        "owner dissolve",
    );
}

#[test]
fn guard_units_are_not_dissolved_while_detaching() {
    let items = vec![
        item("a", Some("W")),
        item("b", Some("W")),
        item("c", Some("W")),
    ];
    let layer = make(
        items,
        vec![grp("W", None, Some(M::Unite), &["a", "b", "c"])],
    );
    let mut real = layer.clone();
    real.group_selection(&[1, 0], "G", "N").unwrap();
    // Mutant: W dissolved as soon as the first unit left (a mid-way dissolve).
    let mut mutant = layer.clone();
    mutant.group_selection(&[1, 0], "G", "N").unwrap();
    mutant.dissolve_group("W").unwrap();
    assert_guard(
        sits_at(&real, "W", "G", 0),
        sits_at(&mutant, "W", "G", 0),
        "no mid-way dissolve",
    );
    assert_eq!(real.entry("W").unwrap().order, ["G", "c"]);
}

#[test]
fn guard_new_order_follows_z_order_not_selection_order() {
    let layer = make(
        vec![item("e1", None), item("e2", None), item("e3", None)],
        vec![],
    );
    let mut real = layer.clone();
    real.group_selection(&[2, 0], "G", "N").unwrap();
    let mut mutant = real.clone();
    mutant.entries[0].order = vec!["e3".into(), "e1".into()];
    assert_guard(
        leaves_in_z_order(&real, "G"),
        leaves_in_z_order(&mutant, "G"),
        "z-order",
    );
}

#[test]
fn guard_new_group_takes_the_minimum_order_index_of_its_owner() {
    let mut real = tower();
    real.group_selection(&[1, 0], "G", "N").unwrap();
    assert_eq!(real.entry("INN").unwrap().order, ["G", "p3"]);
    // Mutant: appended after the owner's remaining entries instead.
    let mut mutant = real.clone();
    let at = mutant.slot("INN").unwrap();
    mutant.entries[at].order = vec!["p3".into(), "G".into()];
    assert_guard(
        sits_at(&real, "INN", "G", 0),
        sits_at(&mutant, "INN", "G", 0),
        "minimum index",
    );
}

#[test]
fn guard_ungroup_dissolves_one_level_only() {
    let before = tower();
    let mut real = before.clone();
    real.ungroup_selection(&[0]).unwrap();
    // Mutant: also dissolves the child group it just promoted.
    let mut mutant = real.clone();
    mutant.dissolve_group("MID").unwrap();
    assert_guard(
        one_group_fewer(&before, &real),
        one_group_fewer(&before, &mutant),
        "one level",
    );
}

#[test]
fn guard_survivors_keep_mode_and_name() {
    let before = pair();
    let mut real = before.clone();
    real.remove_member(0).unwrap();
    let mut mutant = real.clone();
    mutant.entries[0].combine_mode = Some(M::None);
    assert_guard(
        survivors_keep_metadata(&before, &real),
        survivors_keep_metadata(&before, &mutant),
        "mode survives",
    );
    let mut nameless = real.clone();
    nameless.entries[0].name = None;
    assert!(
        survivors_keep_metadata(&before, &nameless).is_err(),
        "name survives"
    );
}

#[test]
fn guard_dissolve_only_touches_the_dup_zero_bucket() {
    let items = vec![
        item("s", Some("G")),
        item("t", Some("G")),
        copy("s", 1, Some("G")),
    ];
    let layer = make(items, vec![grp("G", None, None, &["s", "t"])]);
    let mut real = layer.clone();
    real.dissolve_group("G").unwrap();
    // Mutant: every item with the stroke id loses its tag.
    let mut mutant = real.clone();
    mutant.items[2].group_id = None;
    assert_guard(
        copies_keep_tags(&real),
        copies_keep_tags(&mutant),
        "dup buckets",
    );
    assert_eq!(tags(&real), "s:- t:- s:G");
}

/// The naive walk the ancestor guard exists to prevent: depth cap only.
fn naive_collect(layer: &GroupLayer, gid: &str, depth: usize, out: &mut Vec<String>) {
    let Some(g) = layer.entry(gid) else { return };
    if depth > 16 {
        return;
    }
    for m in &g.order {
        if layer.entry(m).is_some() {
            naive_collect(layer, m, depth + 1, out);
        } else {
            out.push(m.clone());
        }
    }
}

fn no_repeats(ids: &[String]) -> Check {
    match ids
        .iter()
        .enumerate()
        .find(|(n, id)| ids[..*n].contains(id))
    {
        Some((_, id)) => Err(format!("{id} repeated")),
        None => Ok(()),
    }
}

#[test]
fn guard_a_true_cycle_contributes_nothing_and_a_shared_sub_group_is_legal() {
    let items = vec![item("a1", Some("A")), item("b1", Some("B"))];
    let cyc = make(
        items,
        vec![
            grp("A", Some("B"), None, &["a1", "B"]),
            grp("B", Some("A"), None, &["b1", "A"]),
        ],
    );
    let mut naive = Vec::new();
    naive_collect(&cyc, "A", 0, &mut naive);
    assert_guard(
        no_repeats(&cyc.collect_group_stroke_ids("A")),
        no_repeats(&naive),
        "cycle guard",
    );
    // The same sub-group in two branches is NOT a cycle: both occurrences count.
    let items = vec![item("s1", Some("SUB")), item("s2", Some("SUB"))];
    let groups = vec![
        grp("R", None, None, &["L", "M"]),
        grp("L", Some("R"), None, &["SUB"]),
        grp("M", Some("R"), None, &["SUB"]),
        grp("SUB", Some("L"), None, &["s1", "s2"]),
    ];
    let shared = make(items, groups);
    assert_eq!(
        shared.collect_group_stroke_ids("R"),
        ["s1", "s2", "s1", "s2"]
    );
    assert_eq!(shared.resolve_group_members("R", None), [0, 1, 0, 1]);
}

#[test]
fn guard_refusals_leave_the_layer_untouched() {
    let layer = tower();
    let mut real = layer.clone();
    assert!(real.group_selection(&[0, 1, 2], "G", "N").is_err());
    // Mutant: a refusal that already created the (empty) group entry.
    let mut mutant = layer.clone();
    mutant.entries.push(grp("G", None, Some(M::None), &[]));
    assert_guard(
        unchanged(&layer, &real),
        unchanged(&layer, &mutant),
        "refusal is a no-op",
    );
    let units = layer
        .clone()
        .group_selection(&[0, 1, 2, 5], "G", "N")
        .unwrap()
        .units;
    assert_eq!(units, [GroupUnit::Group("INN".into()), GroupUnit::Leaf(5)]);
}
