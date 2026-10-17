//! Hand-written literal controls for the Flatten plan (G19, L02-B).
//!
//! Expectations come from the rules in the task brief, not from the oracle;
//! each rule is paired with the input that flips it.

use super::super::tests::{grp, lay, lay_dup, Lay, Rec};
use super::super::{CombineFold, CombineMode as M, FoldError};
use super::{execute_flatten, plan_flatten, FlattenError, OperandRef};

fn four() -> Lay {
    lay(
        &["a", "b", "c", "d"],
        vec![grp("g", Some(M::Subtract), None, &["c", "a", "d"])],
    )
}

#[test]
fn fewer_than_two_members_unknown_group_and_missing_mode_refuse() {
    let one = lay(
        &["a"],
        vec![grp("g", Some(M::Unite), None, &["a", "ghost"])],
    );
    assert_eq!(
        plan_flatten(&one.view(), "g"),
        Err(FlattenError::FewerThanTwoMembers(1))
    );
    assert_eq!(
        plan_flatten(&one.view(), "zz"),
        Err(FlattenError::UnknownGroup("zz".to_string()))
    );
    let legacy = lay(&["a", "b"], vec![grp("g", None, None, &["a", "b"])]);
    assert_eq!(
        plan_flatten(&legacy.view(), "g"),
        Err(FlattenError::MissingCombineMode("g".to_string()))
    );
    // the member count is checked before the mode, like the baseline
    let tiny = lay(&["a"], vec![grp("g", None, None, &["a"])]);
    assert_eq!(
        plan_flatten(&tiny.view(), "g"),
        Err(FlattenError::FewerThanTwoMembers(1))
    );
}

#[test]
fn mode_none_becomes_unite_and_other_modes_are_kept() {
    let none = lay(
        &["a", "b"],
        vec![grp("g", Some(M::None), None, &["a", "b"])],
    );
    assert_eq!(plan_flatten(&none.view(), "g").unwrap().mode, M::Unite);
    for mode in [M::Unite, M::Subtract, M::Intersect, M::Exclude] {
        let layer = lay(&["a", "b"], vec![grp("g", Some(mode), None, &["a", "b"])]);
        assert_eq!(plan_flatten(&layer.view(), "g").unwrap().mode, mode);
    }
}

#[test]
fn the_result_goes_where_the_first_member_in_group_order_was() {
    // members resolve as c, a, d: the FIRST member is c (index 2), not the lowest (a)
    let plan = plan_flatten(&four().view(), "g").unwrap();
    assert_eq!(plan.insert_at, 2);
    assert_eq!(plan.remove, vec![0, 2, 3]);
    // inserting at 2 and removing index 0 (< 2) leaves the first island at 1
    assert_eq!(plan.final_index, 1);
    assert_eq!(
        plan.operands,
        vec![
            OperandRef {
                item: 2,
                companion: None
            },
            OperandRef {
                item: 0,
                companion: None
            },
            OperandRef {
                item: 3,
                companion: None
            },
        ]
    );
    // flipped: group order a, c, d puts the first member at the lowest index
    let sorted = lay(
        &["a", "b", "c", "d"],
        vec![grp("g", Some(M::Subtract), None, &["a", "c", "d"])],
    );
    let plan = plan_flatten(&sorted.view(), "g").unwrap();
    assert_eq!((plan.insert_at, plan.final_index), (0, 0));
}

#[test]
fn hidden_members_flatten_and_nested_children_are_not_collapsed() {
    let layer = lay(
        &["a", "b", "c", "d"],
        vec![
            grp("P", Some(M::Unite), None, &["a", "C", "d"]),
            grp("C", Some(M::Subtract), Some("P"), &["b", "c"]),
        ],
    )
    .with("b", |f| f.visible = false);
    let plan = plan_flatten(&layer.view(), "P").unwrap();
    assert_eq!(plan.mode, M::Unite);
    assert_eq!(plan.remove, vec![0, 1, 2, 3]);
    let mut port = layer.port();
    let islands = execute_flatten(&plan, &mut port).unwrap();
    // ONE fold over the four leaves (hidden b included), not a nested fold
    assert_eq!(
        port.calls,
        vec!["fold:unite(a,b,c,d)", "islands:unite(a,b,c,d)"]
    );
    assert_eq!(islands.len(), 1);
    assert_eq!(plan.removed_groups, vec!["P", "C"]);
}

#[test]
fn style_comes_from_the_last_member_and_prefers_its_companion() {
    let layer = lay(
        &["r", "cmp", "a"],
        vec![grp("g", Some(M::Unite), None, &["a", "r"])],
    )
    .with("cmp", |f| {
        f.is_linked_fill_companion = true;
        f.linked_fill_id = Some("L".to_string());
        f.fill_color = Some("vis".to_string());
    })
    .with("r", |f| {
        f.is_vector_brush = true;
        f.linked_fill_id = Some("L".to_string());
        f.fill_color = Some("ink".to_string());
        f.stroke_color = Some("ss".to_string());
        f.opacity = 0.4;
    });
    let plan = plan_flatten(&layer.view(), "g").unwrap();
    assert_eq!((plan.style_item, plan.data_source), (0, 1));
    assert_eq!(plan.fill.as_deref(), Some("vis"));
    assert_eq!(plan.opacity, 0.4);
    // the companion goes with its member even though it is not a member
    assert_eq!(plan.remove, vec![0, 1, 2]);
    assert_eq!(plan.final_index, 0);
    // flipped: no companion, so the ribbon's own fill is used ...
    let alone = layer.with("r", |f| f.linked_fill_id = None);
    let plan = plan_flatten(&alone.view(), "g").unwrap();
    assert_eq!((plan.data_source, plan.fill.as_deref()), (0, Some("ink")));
    assert_eq!(plan.remove, vec![0, 2]);
    // ... and a fill-less member falls back to its stroke colour
    let stroke_only = alone.with("r", |f| f.fill_color = None);
    assert_eq!(
        plan_flatten(&stroke_only.view(), "g")
            .unwrap()
            .fill
            .as_deref(),
        Some("ss")
    );
}

#[test]
fn only_the_group_and_its_descendants_leave_the_table() {
    let layer = lay(
        &["a", "b", "c", "d"],
        vec![
            grp("P", Some(M::None), None, &["a", "C", "d"]),
            grp("C", Some(M::Exclude), Some("P"), &["b", "c"]),
            grp("K", Some(M::Unite), None, &["a", "d"]),
        ],
    );
    assert_eq!(
        plan_flatten(&layer.view(), "P").unwrap().removed_groups,
        vec!["P", "C"]
    );
    // flattening the child keeps its parent, whose order still names it (baseline)
    assert_eq!(
        plan_flatten(&layer.view(), "C").unwrap().removed_groups,
        vec!["C"]
    );
    assert_eq!(
        plan_flatten(&layer.view(), "K").unwrap().removed_groups,
        vec!["K"]
    );
}

struct Spy {
    inner: Rec,
    asked: Vec<usize>,
}

impl CombineFold for Spy {
    type Shape = <Rec as CombineFold>::Shape;
    fn member_shape(
        &mut self,
        item: usize,
        companion: Option<usize>,
    ) -> Result<Self::Shape, FoldError> {
        self.asked.push(item);
        self.inner.member_shape(item, companion)
    }
    fn fold(&mut self, mode: M, operands: &[Self::Shape]) -> Result<Self::Shape, FoldError> {
        self.inner.fold(mode, operands)
    }
    fn islands(&mut self, shape: &Self::Shape) -> Result<Vec<Self::Shape>, FoldError> {
        self.inner.islands(shape)
    }
}

#[test]
fn every_member_shape_is_requested_from_the_port_for_the_current_frame() {
    let layer = four();
    let plan = plan_flatten(&layer.view(), "g").unwrap();
    let mut inner = layer.port();
    inner.mo = vec![true, false, true, false];
    let mut spy = Spy {
        inner,
        asked: Vec::new(),
    };
    let islands = execute_flatten(&plan, &mut spy).unwrap();
    // group order c, a, d; the port bakes Motion for c and a (marked @ here)
    assert_eq!(spy.asked, vec![2, 0, 3]);
    assert_eq!(spy.inner.calls[0], "fold:subtract(c@,a@,d)");
    // subtract yields two islands, and they are the new selection in order
    let sel: Vec<&str> = islands.iter().map(|s| s.desc.as_str()).collect();
    assert_eq!(sel, vec!["subtract(c@,a@,d)#0", "subtract(c@,a@,d)#1"]);
    // flipped: without Motion the operands are the rest shapes
    let mut rest = layer.port();
    execute_flatten(&plan, &mut rest).unwrap();
    assert_eq!(rest.calls[0], "fold:subtract(c,a,d)");
}

#[test]
fn duplicator_copies_flatten_only_the_unindexed_bucket() {
    let layer = lay_dup(
        &[("a", Some(1)), ("b", Some(1)), ("a", None), ("b", Some(0))],
        vec![grp("g", Some(M::Unite), None, &["a", "b"])],
    );
    let plan = plan_flatten(&layer.view(), "g").unwrap();
    assert_eq!(plan.remove, vec![2, 3]);
}
