//! Controls for the combine provider and the engine (G19, L02-I).
//!
//! The provider is exercised through the REAL combine plan of L02-B
//! (`render_combines`, `plan_flatten`, `execute_flatten`) over literal member
//! geometry: the only thing supplied from outside is each member's polygons,
//! which in production come from the record owner. The negative controls
//! re-create the failures the oracle rows alone could not name: a fold that
//! keeps only the largest accumulated piece (the bug `boolean_op_multi` was
//! added to fix), a wrong operation and a wrong operand order.

use std::collections::HashMap;

use super::tests::{facts_of, one, square};
use super::*;
use crate::object_document::object_group_kernels::object_boolean_topology::Polygon;
use crate::object_document::object_group_kernels::object_group_combine_plan::{
    execute_flatten, plan_flatten, render_combines, CombineError, CombineFold, CombineLayer,
    FoldError, MemberFacts,
};
use crate::object_document::object_group_kernels::object_group_hierarchy::{
    CombineMode as M, GroupEntry, GroupLayer, LayerItem,
};

/// Member geometry by item index: the record owner's job in production.
struct Table(HashMap<usize, MultiPolygon>);

impl MemberSource for Table {
    fn member(
        &mut self,
        item: usize,
        _companion: Option<usize>,
    ) -> Result<MultiPolygon, FoldError> {
        self.0
            .get(&item)
            .cloned()
            .ok_or_else(|| FoldError::Failed(format!("no geometry for item {item}")))
    }
}

fn table(squares: &[(f64, f64, f64)]) -> Table {
    Table(
        squares
            .iter()
            .enumerate()
            .map(|(item, &(x, y, s))| (item, one(square(x, y, s))))
            .collect(),
    )
}

fn entry(gid: &str, mode: M, parent: Option<&str>, order: &[&str]) -> GroupEntry {
    GroupEntry {
        gid: gid.to_string(),
        name: None,
        combine_mode: Some(mode),
        parent: parent.map(str::to_string),
        order: order.iter().map(|id| id.to_string()).collect(),
    }
}

fn layer(ids: &[&str], entries: Vec<GroupEntry>) -> (GroupLayer, Vec<MemberFacts>) {
    let items = ids
        .iter()
        .map(|id| LayerItem {
            stroke_id: id.to_string(),
            dup_index: None,
            group_id: None,
        })
        .collect();
    let facts = ids
        .iter()
        .map(|_| MemberFacts {
            visible: true,
            linked_fill_id: None,
            brush_group_id: None,
            is_linked_fill_companion: false,
            is_vector_brush: false,
            fill_color: Some("#ff0000ff".to_string()),
            stroke_color: None,
            stroke_width: 1.0,
            opacity: 1.0,
        })
        .collect();
    (
        GroupLayer::new(entries, items).expect("well formed layer"),
        facts,
    )
}

fn areas(shapes: &[MultiPolygon]) -> Vec<f64> {
    shapes.iter().map(|s| multi_area(s)).collect()
}

#[test]
fn a_nested_combined_child_is_one_operand_with_all_its_pieces() {
    let (groups, facts) = layer(
        &["a", "b", "c", "d"],
        vec![
            entry("P", M::Unite, None, &["a", "C", "d"]),
            entry("C", M::Unite, Some("P"), &["b", "c"]),
        ],
    );
    let combine = CombineLayer::new(&groups, &facts).unwrap();
    // b and c overlap (one 15 x 10 piece); a and d stand apart
    let mut port = OverlayFold::new(table(&[
        (0.0, 0.0, 10.0),
        (100.0, 0.0, 10.0),
        (105.0, 0.0, 10.0),
        (300.0, 0.0, 10.0),
    ]));
    let out = render_combines(&combine, &mut port).unwrap();
    // first-seen order, inner group first: the same nested layer is pinned as [1, 2, 0, 3] by L02-B
    // (`a_combining_child_is_exactly_one_operand`), the baseline `suppressBlock` walk
    assert_eq!(out.suppress, vec![1, 2, 0, 3]);
    let shapes: Vec<MultiPolygon> = out.paths.iter().map(|p| p.shape.clone()).collect();
    let got = areas(&shapes);
    assert_eq!(got.len(), 3);
    for (got, want) in got.iter().zip([100.0, 150.0, 100.0]) {
        assert!((got - want).abs() < 1e-6, "{shapes:?}");
    }
    assert!(out.paths.iter().all(|p| p.group == "P"));
}

/// The pairwise bug `boolean_op_multi` fixed: the accumulator collapses to its largest piece.
struct LargestOnly(OverlayFold<Table>);

impl CombineFold for LargestOnly {
    type Shape = MultiPolygon;
    fn member_shape(
        &mut self,
        item: usize,
        companion: Option<usize>,
    ) -> Result<MultiPolygon, FoldError> {
        self.0.member_shape(item, companion)
    }
    fn fold(&mut self, mode: M, operands: &[MultiPolygon]) -> Result<MultiPolygon, FoldError> {
        let op = BoolOp::from_mode(mode).unwrap();
        let mut acc = operands[0].clone();
        for next in &operands[1..] {
            let biggest = acc
                .iter()
                .cloned()
                .max_by(|a, b| {
                    ring_area(&a.exterior)
                        .partial_cmp(&ring_area(&b.exterior))
                        .unwrap()
                })
                .into_iter()
                .collect::<Vec<Polygon>>();
            acc = fold_multi(op, &[biggest, next.clone()]).unwrap();
        }
        Ok(acc)
    }
    fn islands(&mut self, shape: &MultiPolygon) -> Result<Vec<MultiPolygon>, FoldError> {
        self.0.islands(shape)
    }
}

#[test]
fn a_fold_that_keeps_only_the_largest_piece_is_detected() {
    let (groups, facts) = layer(
        &["a", "b", "c"],
        vec![entry("g", M::Unite, None, &["a", "b", "c"])],
    );
    let combine = CombineLayer::new(&groups, &facts).unwrap();
    let squares = [(0.0, 0.0, 50.0), (100.0, 0.0, 50.0), (200.0, 0.0, 50.0)];
    let real = render_combines(&combine, &mut OverlayFold::new(table(&squares))).unwrap();
    let broken = render_combines(
        &combine,
        &mut LargestOnly(OverlayFold::new(table(&squares))),
    )
    .unwrap();
    assert_eq!(real.paths.len(), 3);
    assert_eq!(
        broken.paths.len(),
        2,
        "the middle piece is lost by the pairwise bug"
    );
}

#[test]
fn a_group_that_does_not_combine_is_never_folded() {
    let (groups, facts) = layer(&["a", "b"], vec![entry("g", M::None, None, &["a", "b"])]);
    let combine = CombineLayer::new(&groups, &facts).unwrap();
    let out = render_combines(&combine, &mut OverlayFold::new(table(&[]))).unwrap();
    assert!(out.suppress.is_empty() && out.paths.is_empty());
}

#[test]
fn a_member_without_geometry_degrades_to_an_uncombined_group() {
    let (groups, facts) = layer(&["a", "b"], vec![entry("g", M::Unite, None, &["a", "b"])]);
    let combine = CombineLayer::new(&groups, &facts).unwrap();
    let out = render_combines(&combine, &mut OverlayFold::new(table(&[(0.0, 0.0, 10.0)]))).unwrap();
    assert!(out.suppress.is_empty() && out.paths.is_empty());
}

#[test]
fn mode_none_is_a_failed_fold_not_a_panic() {
    let mut port = OverlayFold::new(table(&[]));
    let result = port.fold(
        M::None,
        &[one(square(0.0, 0.0, 1.0)), one(square(2.0, 0.0, 1.0))],
    );
    assert_eq!(
        result.err(),
        Some(FoldError::Failed("ModeNone".to_string()))
    );
    let bad = port.fold(
        M::Unite,
        &[
            one(vec![[0.0, 0.0], [f64::NAN, 0.0], [1.0, 1.0]]),
            one(square(0.0, 0.0, 1.0)),
        ],
    );
    assert_eq!(
        bad.err(),
        Some(FoldError::Failed("NonFinite { operand: 0 }".to_string()))
    );
}

#[test]
fn islands_are_one_polygon_each_and_keep_their_holes() {
    let mut port = OverlayFold::new(table(&[]));
    let shape = vec![
        Polygon {
            exterior: square(0.0, 0.0, 100.0),
            holes: vec![square(10.0, 10.0, 20.0)],
        },
        Polygon {
            exterior: square(200.0, 0.0, 50.0),
            holes: vec![],
        },
    ];
    let islands = port.islands(&shape).unwrap();
    assert_eq!(islands.len(), 2);
    assert_eq!(islands[0], vec![shape[0].clone()]);
    assert_eq!(islands[0][0].holes.len(), 1);
    assert_eq!(islands[1], vec![shape[1].clone()]);
}

#[test]
fn flatten_through_the_real_provider_folds_members_in_group_order() {
    let (groups, facts) = layer(
        &["a", "b", "c"],
        vec![entry("g", M::Subtract, None, &["c", "a", "b"])],
    );
    let combine = CombineLayer::new(&groups, &facts).unwrap();
    let plan = plan_flatten(&combine, "g").unwrap();
    // c (item 2) is the subject; a (inside it) and b (over its right edge) are cut out of it
    let mut port = OverlayFold::new(table(&[
        (10.0, 10.0, 10.0),
        (35.0, 0.0, 10.0),
        (0.0, 0.0, 40.0),
    ]));
    let islands = execute_flatten(&plan, &mut port).unwrap();
    assert_eq!(islands.len(), 1);
    assert!((multi_area(&islands[0]) - (1_600.0 - 100.0 - 50.0)).abs() < 1e-6);
    assert_eq!(
        islands[0][0].holes.len(),
        1,
        "a is a hole; b's notch is part of the outline"
    );
    assert_eq!(
        plan.operands.iter().map(|o| o.item).collect::<Vec<_>>(),
        vec![2, 0, 1]
    );
}

#[test]
fn an_unavailable_provider_stays_typed() {
    use crate::object_document::object_group_kernels::object_group_combine_plan::UnavailableFold;
    let (groups, facts) = layer(&["a", "b"], vec![entry("g", M::Unite, None, &["a", "b"])]);
    let combine = CombineLayer::new(&groups, &facts).unwrap();
    assert_eq!(
        render_combines(&combine, &mut UnavailableFold).err(),
        Some(CombineError::FoldUnavailable)
    );
}

#[test]
fn a_wrong_operation_is_visible_in_the_area() {
    let a = one(square(0.0, 0.0, 100.0));
    let b = one(square(50.0, 50.0, 100.0));
    let area = |op| multi_area(&fold_multi(op, &[a.clone(), b.clone()]).unwrap());
    assert!((area(BoolOp::Unite) - 17_500.0).abs() < 1e-6);
    assert!((area(BoolOp::Intersect) - 2_500.0).abs() < 1e-6);
    assert!((area(BoolOp::Subtract) - 7_500.0).abs() < 1e-6);
    assert!((area(BoolOp::Exclude) - 15_000.0).abs() < 1e-6);
}

#[test]
fn operand_order_decides_a_subtraction() {
    let big = one(square(0.0, 0.0, 100.0));
    let small = one(square(40.0, 40.0, 20.0));
    let cut = fold_multi(BoolOp::Subtract, &[big.clone(), small.clone()]).unwrap();
    assert_eq!(cut.len(), 1);
    assert_eq!(cut[0].holes.len(), 1);
    assert!((multi_area(&cut) - 9_600.0).abs() < 1e-6);
    assert!(fold_multi(BoolOp::Subtract, &[small, big])
        .unwrap()
        .is_empty());
}

#[test]
fn a_hole_survives_a_later_unite_that_does_not_cover_it() {
    let donut = vec![Polygon {
        exterior: square(0.0, 0.0, 100.0),
        holes: vec![square(30.0, 30.0, 40.0)],
    }];
    let apart = one(square(300.0, 0.0, 10.0));
    let result = fold_multi(BoolOp::Unite, &[donut, apart]).unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].holes.len(), 1);
    let hole = facts_of(&result[0].holes[0]);
    for (got, want) in hole.bounds.iter().zip([30.0, 30.0, 40.0, 40.0]) {
        assert!((got - want).abs() < 1e-6, "{:?}", hole.bounds);
    }
    assert!((multi_area(&result) - (10_000.0 - 1_600.0 + 100.0)).abs() < 1e-6);
}
