//! Hand-written literal controls for the combine operand plan (G24/G19, L02-B).
//!
//! Every expectation is derived by hand from the rules in the task brief, not
//! from the oracle. Rules come in twins: the rule's input and the same layer
//! with the rule's trigger flipped, so a kernel that ignored the rule fails
//! exactly one of the two assertions.

use super::tests::{grp, lay, lay_dup, Lay, Rec, Sh};
use super::{
    render_combines, CombineError, CombineLayer, CombineMode as M, CombineOutcome, CombinedPath,
    MemberFacts, ResultStyle,
};

fn run(layer: &Lay) -> (Rec, CombineOutcome<Sh>) {
    let mut port = layer.port();
    let out = render_combines(&layer.view(), &mut port).expect("render");
    (port, out)
}

fn strs(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

fn groups_of(out: &CombineOutcome<Sh>) -> Vec<&str> {
    out.paths
        .iter()
        .map(|p: &CombinedPath<Sh>| p.group.as_str())
        .collect()
}

fn nested(child_mode: Option<M>) -> Lay {
    lay(
        &["a", "b", "c", "d"],
        vec![
            grp("P", Some(M::Unite), None, &["a", "C", "d"]),
            grp("C", child_mode, Some("P"), &["b", "c"]),
        ],
    )
}

#[test]
fn a_combining_child_is_exactly_one_operand() {
    let (port, out) = run(&nested(Some(M::Unite)));
    assert_eq!(
        port.calls,
        strs(&[
            "fold:unite(b,c)",
            "fold:unite(a,unite(b,c),d)",
            "islands:unite(a,unite(b,c),d)"
        ])
    );
    // computed once, handed up: only the parent emits
    assert_eq!(groups_of(&out), vec!["P"]);
    assert_eq!(out.paths[0].shape.desc, "unite(a,unite(b,c),d)#0");
    assert_eq!(out.suppress, vec![1, 2, 0, 3]);
}

#[test]
fn a_non_combining_child_contributes_its_leaves_as_if_absent() {
    for mode in [Some(M::None), None] {
        let (port, out) = run(&nested(mode));
        assert_eq!(
            port.calls,
            strs(&["fold:unite(a,b,c,d)", "islands:unite(a,b,c,d)"]),
            "{mode:?}"
        );
        assert_eq!(out.suppress, vec![0, 1, 2, 3]);
    }
}

#[test]
fn a_combining_child_under_a_plain_parent_emits_itself() {
    let layer = lay(
        &["a", "b", "c"],
        vec![
            grp("P", Some(M::None), None, &["a", "C"]),
            grp("C", Some(M::Subtract), Some("P"), &["b", "c"]),
        ],
    );
    let (port, out) = run(&layer);
    assert_eq!(
        port.calls,
        strs(&["fold:subtract(b,c)", "islands:subtract(b,c)"])
    );
    assert_eq!(groups_of(&out), vec!["C", "C"]);
    assert_eq!(out.paths[1].shape.desc, "subtract(b,c)#1");
    assert_eq!(out.suppress, vec![1, 2]);
}

#[test]
fn a_hidden_member_leaves_operands_leaves_and_suppression() {
    let base = lay(
        &["a", "b", "c"],
        vec![grp("g", Some(M::Unite), None, &["a", "b", "c"])],
    );
    let shown = run(&base);
    assert_eq!(shown.0.calls[0], "fold:unite(a,b,c)");
    assert_eq!(shown.1.suppress, vec![0, 1, 2]);
    let hidden = base.with("b", |f| f.visible = false);
    let (port, out) = run(&hidden);
    assert_eq!(port.calls[0], "fold:unite(a,c)");
    assert_eq!(out.suppress, vec![0, 2]);
}

#[test]
fn fewer_than_two_operands_return_unchanged_and_fold_nothing() {
    let layer = lay(
        &["a", "b"],
        vec![grp("g", Some(M::Unite), None, &["a", "b"])],
    )
    .with("b", |f| f.visible = false);
    let (port, out) = run(&layer);
    assert!(port.calls.is_empty());
    assert!(out.suppress.is_empty() && out.paths.is_empty());
    // a missing item is skipped the same way
    let ghost = lay(
        &["a"],
        vec![grp("g", Some(M::Unite), None, &["a", "ghost"])],
    );
    assert!(run(&ghost).0.calls.is_empty());
}

#[test]
fn depth_cap_is_sixteen_and_an_ancestor_cycle_returns_nothing() {
    let ids: Vec<String> = (0..18).map(|i| format!("x{i}")).collect();
    let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    let names: Vec<String> = (0..18).map(|i| format!("g{i}")).collect();
    let mut groups = Vec::new();
    for i in 0..17 {
        let parent = if i == 0 {
            None
        } else {
            Some(names[i - 1].as_str())
        };
        groups.push(grp(
            &names[i],
            Some(M::Unite),
            parent,
            &[ids[i].as_str(), names[i + 1].as_str()],
        ));
    }
    groups.push(grp("g17", Some(M::Unite), Some("g16"), &["x17", "x16"]));
    let (port, _) = run(&lay(&id_refs, groups));
    // g17 sits at depth 17 and contributes nothing, so g16 keeps one operand
    // and the first fold is g15's.
    assert_eq!(port.calls[0], "fold:unite(x15,x16)");
    assert_eq!(port.calls.len(), 17);
    let cyc = lay(
        &["a", "b"],
        vec![
            grp("g1", Some(M::Unite), None, &["a", "g2"]),
            grp("g2", Some(M::Unite), Some("g1"), &["b", "g1"]),
        ],
    );
    assert_eq!(run(&cyc).0.calls[0], "fold:unite(a,b)");
}

#[test]
fn a_group_whose_parent_exists_is_not_a_root() {
    let layer = lay(
        &["a", "b"],
        vec![grp("g", Some(M::Unite), Some("g"), &["a", "b"])],
    );
    assert!(run(&layer).0.calls.is_empty());
    let dangling = lay(
        &["a", "b"],
        vec![grp("g", Some(M::Unite), Some("nowhere"), &["a", "b"])],
    );
    assert_eq!(run(&dangling).0.calls.len(), 2);
}

fn style_of(layer: &Lay) -> ResultStyle {
    run(layer).1.paths[0].style.clone()
}

fn style(fill: Option<&str>, stroke: Option<&str>, width: f64, opacity: f64) -> ResultStyle {
    ResultStyle {
        fill: fill.map(str::to_string),
        stroke: stroke.map(str::to_string),
        stroke_width: width,
        opacity,
    }
}

fn paint(f: &mut MemberFacts, fill: Option<&str>, stroke: Option<&str>, w: f64, o: f64) {
    f.fill_color = fill.map(str::to_string);
    f.stroke_color = stroke.map(str::to_string);
    f.stroke_width = w;
    f.opacity = o;
}

#[test]
fn style_comes_from_the_last_visible_leaf() {
    let three = lay(
        &["a", "b", "c"],
        vec![grp("g", Some(M::Unite), None, &["a", "b", "c"])],
    )
    .with("a", |f| paint(f, Some("red"), Some("k"), 3.0, 0.5))
    .with("b", |f| paint(f, Some("blue"), Some("w"), 7.0, 0.25))
    .with("c", |f| paint(f, Some("green"), None, 9.0, 0.75));
    assert_eq!(style_of(&three), style(Some("green"), None, 9.0, 0.75));
    let hidden = three.with("c", |f| f.visible = false);
    assert_eq!(style_of(&hidden), style(Some("blue"), Some("w"), 7.0, 0.25));
}

#[test]
fn a_stroke_only_style_source_falls_back_to_its_stroke_colour() {
    let base = lay(
        &["a", "b"],
        vec![grp("g", Some(M::Unite), None, &["a", "b"])],
    );
    let only = base.with("b", |f| paint(f, None, Some("ink"), 5.0, 1.0));
    assert_eq!(style_of(&only), style(Some("ink"), None, 5.0, 1.0));
    let empty = only.with("b", |f| f.fill_color = Some(String::new()));
    assert_eq!(style_of(&empty), style(Some("ink"), None, 5.0, 1.0));
    let both = empty.with("b", |f| f.fill_color = Some("fb".to_string()));
    assert_eq!(style_of(&both), style(Some("fb"), Some("ink"), 5.0, 1.0));
    let neither = both.with("b", |f| paint(f, None, None, 5.0, 1.0));
    assert_eq!(style_of(&neither), style(None, None, 5.0, 1.0));
}

#[test]
fn a_vector_brush_ribbon_never_uses_its_ink_as_the_fill() {
    let layer = lay(
        &["a", "cmp", "r"],
        vec![grp("g", Some(M::Unite), None, &["a", "r"])],
    )
    .with("cmp", |f| {
        f.is_linked_fill_companion = true;
        f.linked_fill_id = Some("L".to_string());
        paint(f, Some("vis"), None, 1.0, 1.0);
    })
    .with("r", |f| {
        f.is_vector_brush = true;
        f.linked_fill_id = Some("L".to_string());
        paint(f, Some("ink"), Some("outline"), 9.0, 0.8);
    });
    assert_eq!(style_of(&layer), style(Some("vis"), Some("ink"), 9.0, 0.8));
    // no companion: the ribbon has no fill at all, only its ink as the stroke
    let alone = layer.with("r", |f| f.linked_fill_id = None);
    assert_eq!(style_of(&alone), style(None, Some("ink"), 9.0, 0.8));
    // not a brush: its own fill wins and the companion is ignored
    let plain = alone.with("r", |f| {
        f.is_vector_brush = false;
        f.linked_fill_id = Some("L".to_string());
    });
    assert_eq!(
        style_of(&plain),
        style(Some("ink"), Some("outline"), 9.0, 0.8)
    );
}

#[test]
fn suppression_is_the_leaves_plus_linked_and_brush_siblings_in_layer_order() {
    let layer = lay(
        &["d1", "r", "cmp", "x", "d2", "y"],
        vec![grp("g", Some(M::Unite), None, &["x", "r"])],
    )
    .with("d1", |f| f.brush_group_id = Some("B".to_string()))
    .with("d2", |f| f.brush_group_id = Some("B".to_string()))
    .with("y", |f| f.brush_group_id = Some("Z".to_string()))
    .with("cmp", |f| {
        f.is_linked_fill_companion = true;
        f.linked_fill_id = Some("L".to_string());
    })
    .with("r", |f| {
        f.brush_group_id = Some("B".to_string());
        f.linked_fill_id = Some("L".to_string());
    });
    let (port, out) = run(&layer);
    // the companion is unioned into r's operand, not folded as its own operand
    assert_eq!(port.calls[0], "fold:unite(x,r~cmp)");
    // x, then r followed by d1, cmp, d2; y (another brush group) is untouched
    assert_eq!(out.suppress, vec![3, 1, 0, 2, 4]);
    let untagged = layer.with("r", |f| {
        f.brush_group_id = None;
        f.linked_fill_id = None;
    });
    assert_eq!(run(&untagged).1.suppress, vec![3, 1]);
}

#[test]
fn each_duplicator_bucket_folds_once_with_its_own_members() {
    // absent and 0 share a bucket; the first match per bucket is the member
    let layer = lay_dup(
        &[
            ("a", None),
            ("b", Some(0)),
            ("a", Some(2)),
            ("b", Some(2)),
            ("a", Some(0)),
        ],
        vec![grp("g", Some(M::Unite), None, &["a", "b"])],
    );
    let (port, out) = run(&layer);
    assert_eq!(
        port.calls.iter().filter(|c| c.starts_with("fold:")).count(),
        2
    );
    // the second `a` of bucket 0 is never a member, so it keeps painting
    assert_eq!(out.suppress, vec![0, 1, 2, 3]);
    assert_eq!(out.paths.len(), 2);
}

#[test]
fn a_failed_fold_degrades_to_un_combined_operands_without_suppressing() {
    let layer = nested(Some(M::Unite));
    let mut port = layer.port();
    port.fail = strs(&["unite(b,c)"]);
    let out = render_combines(&layer.view(), &mut port).unwrap();
    assert_eq!(
        port.calls,
        strs(&[
            "fold:unite(b,c)",
            "fold:unite(a,b,c,d)",
            "islands:unite(a,b,c,d)"
        ])
    );
    assert_eq!(out.suppress, vec![0, 1, 2, 3]);
    assert_eq!(groups_of(&out), vec!["P"]);
    let flat = lay(
        &["a", "b"],
        vec![grp("g", Some(M::Unite), None, &["a", "b"])],
    );
    let mut port = flat.port();
    port.fail = strs(&["unite(a,b)"]);
    let none = render_combines(&flat.view(), &mut port).unwrap();
    assert_eq!(port.calls, strs(&["fold:unite(a,b)"]));
    assert!(none.suppress.is_empty() && none.paths.is_empty());
}

#[test]
fn baseline_quirk_a_combining_parent_left_with_one_result_loses_the_members() {
    // P folds only when it has two operands; here C's result is the only one
    // (h is hidden), so C's members stop painting and nothing is drawn.
    let layer = lay(
        &["b", "c", "h"],
        vec![
            grp("P", Some(M::Unite), None, &["C", "h"]),
            grp("C", Some(M::Unite), Some("P"), &["b", "c"]),
        ],
    )
    .with("h", |f| f.visible = false);
    let (port, out) = run(&layer);
    assert_eq!(port.calls, strs(&["fold:unite(b,c)"]));
    assert_eq!(out.suppress, vec![0, 1]);
    assert!(out.paths.is_empty());
}

#[test]
fn baseline_quirk_a_plain_group_between_two_combining_ones_draws_twice() {
    let layer = lay(
        &["a", "b", "c", "d"],
        vec![
            grp("G", Some(M::Unite), None, &["a", "P"]),
            grp("P", Some(M::None), Some("G"), &["b", "C"]),
            grp("C", Some(M::Unite), Some("P"), &["c", "d"]),
        ],
    );
    let (port, out) = run(&layer);
    assert_eq!(
        port.calls,
        strs(&[
            "fold:unite(c,d)",
            "islands:unite(c,d)",
            "fold:unite(a,b,unite(c,d)#0)",
            "islands:unite(a,b,unite(c,d)#0)"
        ])
    );
    assert_eq!(groups_of(&out), vec!["C", "G"]);
}

#[test]
fn facts_must_be_parallel_to_the_items() {
    let layer = lay(&["a", "b"], vec![]);
    assert_eq!(
        CombineLayer::new(&layer.groups, &layer.facts[..1]).err(),
        Some(CombineError::FactsMismatch { items: 2, facts: 1 })
    );
}
