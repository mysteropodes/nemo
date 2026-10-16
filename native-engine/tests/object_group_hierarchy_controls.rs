//! Independent literal controls for the group hierarchy kernel (G24, L02-A).
//!
//! Every layer and every expected value below is written by hand from the
//! baseline rules (3-level nest, partial-group selection, duplicate leaf ids
//! across dupIndex copies, corrupt parent cycle, empty order, depth caps); none
//! is read from the oracle. Compact notation: a table entry prints as
//! `gid^parent#mode[order]` (`^` and `#` appear only when the key is present),
//! tags as `strokeId:gid` with `-` for untagged.

use super::selection::{CombineOutcome, GroupUnit};
use super::{CombineMode as M, GroupEntry, GroupError, GroupLayer, LayerItem};

pub(crate) fn item(id: &str, gid: Option<&str>) -> LayerItem {
    LayerItem {
        stroke_id: id.into(),
        dup_index: None,
        group_id: gid.map(str::to_string),
    }
}

pub(crate) fn copy(id: &str, dup: u32, gid: Option<&str>) -> LayerItem {
    LayerItem {
        dup_index: Some(dup),
        ..item(id, gid)
    }
}

pub(crate) fn grp(gid: &str, parent: Option<&str>, mode: Option<M>, order: &[&str]) -> GroupEntry {
    GroupEntry {
        gid: gid.into(),
        name: None,
        combine_mode: mode,
        parent: parent.map(str::to_string),
        order: order.iter().map(|s| s.to_string()).collect(),
    }
}

pub(crate) fn show(layer: &GroupLayer) -> Vec<String> {
    layer
        .entries()
        .iter()
        .map(|e| {
            let parent = e
                .parent
                .as_ref()
                .map(|p| format!("^{p}"))
                .unwrap_or_default();
            let mode = e
                .combine_mode
                .map(|m| format!("#{}", m.as_str()))
                .unwrap_or_default();
            format!("{}{}{}[{}]", e.gid, parent, mode, e.order.join(","))
        })
        .collect()
}

pub(crate) fn tags(layer: &GroupLayer) -> String {
    let parts: Vec<String> = layer
        .items()
        .iter()
        .map(|i| format!("{}:{}", i.stroke_id, i.group_id.as_deref().unwrap_or("-")))
        .collect();
    parts.join(" ")
}

pub(crate) fn make(items: Vec<LayerItem>, groups: Vec<GroupEntry>) -> GroupLayer {
    GroupLayer::new(groups, items).expect("well formed literal layer")
}

/// OUT{ MID{ INN{p1,p2,p3}, q1 }, r1 } plus a loose x.
pub(crate) fn tower() -> GroupLayer {
    let mut out = grp("OUT", None, Some(M::Unite), &["MID", "r1"]);
    out.name = Some("outer".into());
    make(
        vec![
            item("p1", Some("INN")),
            item("p2", Some("INN")),
            item("p3", Some("INN")),
            item("q1", Some("MID")),
            item("r1", Some("OUT")),
            item("x", None),
        ],
        vec![
            out,
            grp("MID", Some("OUT"), Some(M::Subtract), &["INN", "q1"]),
            grp("INN", Some("MID"), Some(M::None), &["p1", "p2", "p3"]),
        ],
    )
}

#[test]
fn three_level_nest_queries() {
    let t = tower();
    assert_eq!(t.root_group_of("INN"), "OUT");
    assert_eq!(t.root_group_of("MID"), "OUT");
    assert_eq!(t.root_group_of("OUT"), "OUT");
    assert_eq!(t.root_group_of("unknown"), "unknown");
    assert_eq!(t.parent_group_of("INN").as_deref(), Some("MID"));
    assert_eq!(t.parent_group_of("OUT"), None);
    assert_eq!(t.root_group_of_item(0).as_deref(), Some("OUT"));
    assert_eq!(t.root_group_of_item(5), None);
    assert_eq!(
        t.collect_group_stroke_ids("OUT"),
        ["p1", "p2", "p3", "q1", "r1"]
    );
    assert_eq!(t.resolve_group_members("MID", None), [0, 1, 2, 3]);
    assert_eq!(t.members_of(0), [0, 1, 2, 3, 4]);
    assert_eq!(t.members_of(5), [5]);
}

#[test]
fn ungroup_peels_exactly_one_level_per_application() {
    let mut t = tower();
    let first = t.ungroup_selection(&[0]).unwrap();
    assert_eq!(
        (first.dissolved, first.untagged),
        (vec!["OUT".to_string()], vec![])
    );
    assert_eq!(show(&t), ["MID#subtract[INN,q1]", "INN^MID#none[p1,p2,p3]"]);
    assert_eq!(tags(&t), "p1:INN p2:INN p3:INN q1:MID r1:- x:-");
    assert_eq!(t.ungroup_selection(&[0]).unwrap().dissolved, ["MID"]);
    assert_eq!(show(&t), ["INN#none[p1,p2,p3]"]);
    assert_eq!(tags(&t), "p1:INN p2:INN p3:INN q1:- r1:- x:-");
    assert_eq!(t.ungroup_selection(&[0]).unwrap().dissolved, ["INN"]);
    assert!(show(&t).is_empty());
    assert_eq!(tags(&t), "p1:- p2:- p3:- q1:- r1:- x:-");
    assert_eq!(
        t.ungroup_selection(&[0]),
        Err(GroupError::NoGroupInSelection)
    );
}

#[test]
fn dissolving_an_inner_group_promotes_into_the_grandparent_order_slot() {
    let mut t = tower();
    t.dissolve_group("MID").unwrap();
    assert_eq!(show(&t), ["OUT#unite[INN,q1,r1]", "INN^OUT#none[p1,p2,p3]"]);
    assert_eq!(tags(&t), "p1:INN p2:INN p3:INN q1:OUT r1:OUT x:-");
}

#[test]
fn a_fully_selected_group_nests_as_one_unit_and_partial_leaves_leave_a_survivor() {
    let mut t = tower();
    let out = t.group_selection(&[5, 2, 1, 0], "G", "N").unwrap();
    assert_eq!(
        out.units,
        [GroupUnit::Group("INN".into()), GroupUnit::Leaf(5)]
    );
    assert_eq!((out.common_owner, out.owner_dissolved), (None, false));
    // MID keeps its single remaining entry: the baseline never dissolves it here.
    assert_eq!(
        show(&t),
        [
            "OUT#unite[MID,r1]",
            "MID^OUT#subtract[q1]",
            "INN^G#none[p1,p2,p3]",
            "G#none[INN,x]"
        ]
    );
    assert_eq!(t.entry("G").unwrap().name.as_deref(), Some("N"));
    assert_eq!(tags(&t), "p1:INN p2:INN p3:INN q1:MID r1:OUT x:G");
}

#[test]
fn partial_selection_inside_a_group_inserts_at_the_minimum_order_index() {
    let mut t = tower();
    t.group_selection(&[2, 1], "G", "N").unwrap();
    assert_eq!(show(&t)[2..], ["INN^MID#none[p1,G]", "G^INN#none[p2,p3]"]);
    assert_eq!(tags(&t), "p1:INN p2:G p3:G q1:MID r1:OUT x:-");
}

#[test]
fn common_owner_left_with_one_entry_dissolves_afterwards() {
    let mut p = grp("P", None, Some(M::Exclude), &["a", "b"]);
    p.name = Some("pp".into());
    // The tag names no entry, so both shapes are loose leaves of P.
    let mut layer = make(
        vec![
            item("a", Some("ZZ")),
            item("b", Some("ZZ")),
            item("c", None),
        ],
        vec![p],
    );
    let out = layer.group_selection(&[0, 1], "G", "N").unwrap();
    assert_eq!(
        (out.common_owner.as_deref(), out.owner_dissolved),
        (Some("P"), true)
    );
    assert_eq!(show(&layer), ["G#none[a,b]"]);
    assert_eq!(tags(&layer), "a:G b:G c:-");
}

#[test]
fn z_order_not_selection_order_decides_the_new_order() {
    let mut layer = make(
        (1..=4).map(|n| item(&format!("e{n}"), None)).collect(),
        vec![],
    );
    let out = layer.group_selection(&[3, 0], "G", "N").unwrap();
    assert_eq!(out.units, [GroupUnit::Leaf(0), GroupUnit::Leaf(3)]);
    assert_eq!(show(&layer), ["G#none[e1,e4]"]);
    assert_eq!(tags(&layer), "e1:G e2:- e3:- e4:G");
}

#[test]
fn fewer_than_two_units_refuses_and_changes_nothing() {
    let layer = make(
        vec![
            item("f1", Some("F")),
            item("f2", Some("F")),
            item("f3", None),
        ],
        vec![grp("F", None, Some(M::Unite), &["f1", "f2"])],
    );
    for (sel, units) in [(vec![0, 1], 1), (vec![0], 1), (vec![], 0)] {
        let mut copy = layer.clone();
        assert_eq!(
            copy.group_selection(&sel, "G", "N"),
            Err(GroupError::FewerThanTwoUnits(units))
        );
        assert_eq!(copy, layer, "refusal for {sel:?} must not mutate");
    }
    let mut copy = layer.clone();
    assert_eq!(
        copy.combine_selection(&[0], M::Unite, "G"),
        Err(GroupError::FewerThanTwoUnits(1))
    );
    assert_eq!(copy, layer);
}

#[test]
fn duplicate_leaf_ids_across_dup_index_copies_stay_in_their_bucket() {
    let items = vec![
        item("s", Some("G")),
        item("t", Some("G")),
        copy("s", 1, Some("G")),
        copy("t", 1, Some("G")),
        copy("s", 2, Some("G")),
        copy("t", 2, Some("G")),
    ];
    let mut layer = make(items, vec![grp("G", None, Some(M::Unite), &["s", "t"])]);
    assert_eq!(layer.resolve_group_members("G", None), [0, 1]);
    assert_eq!(layer.resolve_group_members("G", Some(0)), [0, 1]);
    assert_eq!(layer.resolve_group_members("G", Some(1)), [2, 3]);
    assert_eq!(layer.resolve_group_members("G", Some(2)), [4, 5]);
    assert!(layer.resolve_group_members("G", Some(3)).is_empty());
    // A copy expands to the dup-0 leaves of its outermost group, as in the baseline.
    assert_eq!(layer.members_of(2), [0, 1]);
    layer.dissolve_group("G").unwrap();
    assert_eq!(tags(&layer), "s:- t:- s:G t:G s:G t:G");
}

#[test]
fn a_tag_without_a_table_entry_expands_by_tag_and_ungroup_clears_only_the_selection() {
    let mut layer = make(
        vec![
            item("a", Some("X")),
            item("b", Some("X")),
            copy("c", 1, Some("X")),
        ],
        vec![],
    );
    assert_eq!(layer.root_group_of_item(0).as_deref(), Some("X"));
    assert_eq!(layer.members_of(0), [0, 1, 2]);
    let out = layer.ungroup_selection(&[0]).unwrap();
    assert_eq!((out.dissolved, out.untagged), (vec![], vec![0]));
    assert_eq!(tags(&layer), "a:- b:X c:X");
}

#[test]
fn a_corrupt_parent_cycle_terminates_everywhere() {
    let items = vec![item("a1", Some("A")), item("b1", Some("B"))];
    let groups = vec![
        grp("A", Some("B"), None, &["a1", "B"]),
        grp("B", Some("A"), None, &["b1", "A"]),
    ];
    let mut layer = make(items, groups);
    assert_eq!(layer.root_group_of("A"), "A");
    assert_eq!(layer.root_group_of("B"), "B");
    assert_eq!(layer.collect_group_stroke_ids("A"), ["a1", "b1"]);
    assert_eq!(layer.resolve_group_members("A", None), [0, 1]);
    let mut gone = layer.clone();
    gone.remove_subtree("A");
    assert!(show(&gone).is_empty());
    layer.dissolve_group("A").unwrap();
    assert_eq!(show(&layer), ["B^B[b1,a1,B]"]);
    assert_eq!(tags(&layer), "a1:B b1:B");
}

#[test]
fn a_dangling_parent_is_ignored_and_empty_orders_are_harmless() {
    let items = vec![
        item("d1", Some("D")),
        item("d2", Some("D")),
        item("p", Some("PP")),
    ];
    let groups = vec![
        grp("D", Some("nope"), None, &["d1", "d2"]),
        grp("E", None, Some(M::Unite), &[]),
        grp("PP", None, None, &["E", "p"]),
    ];
    let mut layer = make(items, groups);
    assert_eq!(layer.parent_group_of("D"), None);
    assert_eq!(layer.root_group_of("D"), "D");
    assert!(layer.collect_group_stroke_ids("E").is_empty());
    assert!(layer.resolve_group_members("E", None).is_empty());
    assert_eq!(layer.collect_group_stroke_ids("PP"), ["p"]);
    layer.dissolve_group("D").unwrap();
    layer.dissolve_group("E").unwrap();
    assert_eq!(show(&layer), ["PP[E,p]"]);
    assert_eq!(tags(&layer), "d1:- d2:- p:PP");
}

#[test]
fn removing_a_member_dissolves_below_two_but_keeps_mode_and_name_of_survivors() {
    let mut p = grp("P", None, Some(M::Unite), &["Q", "m3"]);
    p.name = Some("keep".into());
    let items = vec![
        item("m1", Some("Q")),
        item("m2", Some("Q")),
        item("m3", Some("P")),
    ];
    let mut layer = make(
        items,
        vec![p, grp("Q", Some("P"), Some(M::Subtract), &["m1", "m2"])],
    );
    assert_eq!(layer.remove_member(0), Ok(true));
    assert_eq!(show(&layer), ["P#unite[m2,m3]"]);
    assert_eq!(layer.entry("P").unwrap().name.as_deref(), Some("keep"));
    assert_eq!(tags(&layer), "m1:- m2:P m3:P");
    assert_eq!(layer.remove_member(0), Err(GroupError::NotInGroup(0)));
    assert_eq!(layer.remove_member(1), Ok(true));
    assert!(show(&layer).is_empty());
    assert_eq!(tags(&layer), "m1:- m2:- m3:-");
}

#[test]
fn detach_leaf_dissolves_below_two_and_leaves_the_item_tag_alone() {
    let items = vec![
        item("a", Some("G1")),
        item("b", Some("G1")),
        item("c", Some("G1")),
    ];
    let mut layer = make(
        items,
        vec![grp("G1", None, Some(M::Unite), &["a", "b", "c"])],
    );
    layer.detach_leaf("b");
    assert_eq!(show(&layer), ["G1#unite[a,c]"]);
    assert_eq!(tags(&layer), "a:G1 b:G1 c:G1");
    layer.detach_leaf("a");
    assert!(show(&layer).is_empty());
    assert_eq!(tags(&layer), "a:G1 b:G1 c:-");
}

#[test]
fn combine_mode_rename_and_combine_selection_keep_absent_and_explicit_distinct() {
    let items = vec![
        item("c1", Some("C")),
        item("c2", Some("C")),
        item("c3", None),
    ];
    let mut layer = make(items, vec![grp("C", None, None, &["c1", "c2"])]);
    assert_eq!(layer.set_combine_mode("C", M::None), Ok(true));
    assert_eq!(layer.set_combine_mode("C", M::None), Ok(false));
    assert_eq!(
        layer.set_combine_mode("nope", M::Unite),
        Err(GroupError::UnknownGroup("nope".into()))
    );
    let renamed = layer.rename_group("C", "cc", &[]).unwrap();
    assert_eq!((renamed.created, renamed.changed), (false, true));
    assert!(!layer.rename_group("C", "cc", &[]).unwrap().changed);
    let made = layer.rename_group("N", "nn", &["c3".to_string()]).unwrap();
    assert_eq!((made.created, made.changed), (true, true));
    assert_eq!(show(&layer), ["C#none[c1,c2]", "N#none[c3]"]);
    assert_eq!(
        layer.rename_group("", "x", &[]),
        Err(GroupError::EmptyGroupId)
    );
    let same = layer.combine_selection(&[1, 0], M::Unite, "NEW").unwrap();
    assert_eq!(
        same,
        CombineOutcome::Updated {
            gid: "C".into(),
            changed: true
        }
    );
    let wider = layer
        .combine_selection(&[0, 1, 2], M::Subtract, "NEW")
        .unwrap();
    assert_eq!(wider, CombineOutcome::Created { gid: "NEW".into() });
    // Not detached from C: the baseline leaves c1/c2 listed twice.
    assert_eq!(
        show(&layer),
        ["C#unite[c1,c2]", "N#none[c3]", "NEW#subtract[c1,c2,c3]"]
    );
    assert!(
        layer.entry("NEW").unwrap().name.is_none() && layer.entry("NEW").unwrap().parent.is_none()
    );
    assert_eq!(tags(&layer), "c1:NEW c2:NEW c3:NEW");
}

/// K0 > K1 > ... > K{len-1}, whose last group lists `leaf`.
fn chain(len: usize, leaf: &str) -> GroupLayer {
    let groups = (0..len)
        .map(|n| {
            let parent = (n > 0).then(|| format!("K{}", n - 1));
            let next = if n + 1 == len {
                leaf.to_string()
            } else {
                format!("K{}", n + 1)
            };
            grp(&format!("K{n}"), parent.as_deref(), None, &[next.as_str()])
        })
        .collect();
    make(vec![item(leaf, None)], groups)
}

#[test]
fn nesting_caps_are_depth_16_for_members_and_32_for_the_root_walk() {
    // The leaf of K16 sits at depth 16 (kept); the leaf of K17 at depth 17 (dropped).
    assert_eq!(chain(17, "leaf").collect_group_stroke_ids("K0"), ["leaf"]);
    assert!(chain(18, "leaf").collect_group_stroke_ids("K0").is_empty());
    assert!(chain(18, "leaf")
        .resolve_group_members("K0", None)
        .is_empty());
    let mut deep = chain(20, "leaf");
    deep.remove_subtree("K0");
    assert_eq!(
        show(&deep),
        ["K17^K16[K18]", "K18^K17[K19]", "K19^K18[leaf]"]
    );
    // Parents run the other way: 40 deep, the walk stops after 32 steps.
    let up: Vec<GroupEntry> = (0..40)
        .map(|n| {
            grp(
                &format!("U{n}"),
                (n < 39).then(|| format!("U{}", n + 1)).as_deref(),
                None,
                &[],
            )
        })
        .collect();
    assert_eq!(make(vec![], up).root_group_of("U0"), "U32");
}

#[test]
fn malformed_input_is_refused_before_any_write() {
    assert_eq!(
        GroupLayer::new(
            vec![grp("G", None, None, &[]), grp("G", None, None, &[])],
            vec![]
        ),
        Err(GroupError::DuplicateGroupId("G".into()))
    );
    assert_eq!(
        GroupLayer::new(vec![grp("", None, None, &[])], vec![]),
        Err(GroupError::EmptyGroupId)
    );
    assert_eq!(
        GroupLayer::new(vec![], vec![item("", None)]),
        Err(GroupError::EmptyStrokeId(0))
    );
    let layer = make(
        vec![item("a", None), item("b", None)],
        vec![grp("T", None, None, &[])],
    );
    for (sel, gid, want) in [
        (vec![0, 1], "T", GroupError::DuplicateGroupId("T".into())),
        (vec![0, 1], "", GroupError::EmptyGroupId),
        (vec![0, 5], "G", GroupError::UnknownItem(5)),
        (vec![1, 1], "G", GroupError::DuplicateSelection(1)),
    ] {
        let mut copy = layer.clone();
        assert_eq!(copy.group_selection(&sel, gid, "N"), Err(want));
        assert_eq!(copy, layer);
    }
    let mut copy = layer.clone();
    assert_eq!(copy.ungroup_selection(&[]), Err(GroupError::EmptySelection));
    assert_eq!(copy.remove_member(9), Err(GroupError::UnknownItem(9)));
    assert_eq!(
        copy.dissolve_group("none"),
        Err(GroupError::UnknownGroup("none".into()))
    );
    assert_eq!(copy, layer);
}
