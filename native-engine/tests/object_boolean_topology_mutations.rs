//! Mutation guards for the boolean result topology kernel (L02-D).
//!
//! Each rule is a checker over a finished [`InsertPlan`]. The real kernel output
//! must satisfy every rule on several shapes, and a hand-built mutant (the plan
//! broken in exactly one place) must fail exactly the rule that guards that
//! place; a checker that accepted every plan would fail the mutant assertions.

use super::controls::{data, ext, hole};
use super::{
    plan_insert, DataValue, InsertPlan, InsertRequest, PaintArg, ResultShape, Ring, StrokeIdAction,
};

fn mixed() -> ResultShape {
    ResultShape::Compound(vec![
        ext(0.0, 0.0, 100.0, 100.0),
        hole(10.0, 10.0, 20.0, 20.0),
        hole(50.0, 50.0, 20.0, 20.0),
        ext(200.0, 0.0, 50.0, 50.0),
        ext(300.0, 0.0, 50.0, 50.0),
        hole(310.0, 10.0, 10.0, 10.0),
    ])
}

fn donut() -> ResultShape {
    ResultShape::Compound(vec![
        ext(0.0, 0.0, 100.0, 100.0),
        hole(10.0, 10.0, 20.0, 20.0),
    ])
}

fn flat() -> ResultShape {
    ResultShape::Compound(vec![
        ext(0.0, 0.0, 10.0, 10.0),
        ext(20.0, 0.0, 10.0, 10.0),
        ext(40.0, 0.0, 10.0, 10.0),
    ])
}

fn rings(shape: &ResultShape) -> &[Ring] {
    match shape {
        ResultShape::Compound(rings) => rings,
        other => panic!("{other:?}"),
    }
}

const BASE: usize = 2;

fn plan_of(shape: &ResultShape) -> InsertPlan {
    let src = data(&[("strokeId", DataValue::Text("sSRC".to_string()))]);
    plan_insert(
        shape,
        &InsertRequest {
            insert_at: BASE,
            layer_len: 5,
            fill: PaintArg::Set("#ff0000".to_string()),
            opacity: Some(0.5),
            stroke: None,
            src: Some(&src),
            preserve_geometry: false,
        },
    )
    .unwrap()
}

/// One island per exterior, none invented.
fn guard_one_island_per_exterior(plan: &InsertPlan, shape: &ResultShape) -> bool {
    let exteriors = rings(shape).iter().filter(|r| r.clockwise).count();
    plan.islands.len() == exteriors && plan.topology.exterior_count == exteriors
}

/// Every hole is merged into at least one island and no island names a non-hole.
fn guard_every_hole_is_merged(plan: &InsertPlan, shape: &ResultShape) -> bool {
    let all = rings(shape);
    let merged: Vec<usize> = plan
        .islands
        .iter()
        .flat_map(|i| i.island.holes.clone())
        .collect();
    let named_a_hole = merged.iter().all(|&h| !all[h].clockwise);
    let none_missing = (0..all.len())
        .filter(|&i| !all[i].clockwise)
        .all(|h| merged.contains(&h));
    named_a_hole && none_missing
}

/// Islands keep the exterior order of the result.
fn guard_exterior_order(plan: &InsertPlan) -> bool {
    plan.islands
        .windows(2)
        .all(|pair| pair[0].island.ring < pair[1].island.ring)
}

/// Slots are `BASE + k`: z-order of the source is preserved.
fn guard_slots_are_consecutive(plan: &InsertPlan) -> bool {
    plan.islands
        .iter()
        .enumerate()
        .all(|(k, i)| i.index == BASE + k)
}

/// The identity belongs to the first island only; every other island asks for a fresh one.
fn guard_single_identity_owner(plan: &InsertPlan) -> bool {
    plan.islands
        .iter()
        .enumerate()
        .all(|(k, i)| match (&i.carry.stroke_id, k) {
            (StrokeIdAction::Carry(DataValue::Text(id)), 0) => id == "sSRC",
            (StrokeIdAction::Fresh, k) => k > 0,
            _ => false,
        })
}

/// One style for the whole result.
fn guard_uniform_style(plan: &InsertPlan) -> bool {
    plan.islands.iter().all(|i| {
        i.fill == plan.islands[0].fill
            && i.opacity == plan.islands[0].opacity
            && i.stroke == plan.islands[0].stroke
    })
}

fn all_guards(plan: &InsertPlan, shape: &ResultShape) -> [bool; 6] {
    [
        guard_one_island_per_exterior(plan, shape),
        guard_every_hole_is_merged(plan, shape),
        guard_exterior_order(plan),
        guard_slots_are_consecutive(plan),
        guard_single_identity_owner(plan),
        guard_uniform_style(plan),
    ]
}

#[test]
fn the_real_plans_satisfy_every_guard() {
    for shape in [mixed(), donut(), flat()] {
        let plan = plan_of(&shape);
        assert_eq!(all_guards(&plan, &shape), [true; 6], "{shape:?}");
    }
}

#[test]
fn each_mutant_fails_exactly_its_own_guard() {
    let shape = mixed();
    let good = plan_of(&shape);

    let mut dropped_island = good.clone();
    dropped_island.islands.pop();
    dropped_island.topology.islands.pop();
    dropped_island.topology.exterior_count -= 1;
    assert_eq!(
        all_guards(&dropped_island, &shape),
        [false, false, true, true, true, true]
    );

    let mut lost_hole = good.clone();
    lost_hole.islands[0].island.holes.pop();
    assert_eq!(
        all_guards(&lost_hole, &shape),
        [true, false, true, true, true, true]
    );

    let mut swapped = good.clone();
    swapped.islands.swap(0, 2);
    assert!(!guard_exterior_order(&swapped));

    let mut shifted = good.clone();
    shifted.islands[2].index += 1;
    assert_eq!(
        all_guards(&shifted, &shape),
        [true, true, true, false, true, true]
    );

    let mut two_owners = good.clone();
    two_owners.islands[1].carry.stroke_id = good.islands[0].carry.stroke_id.clone();
    assert_eq!(
        all_guards(&two_owners, &shape),
        [true, true, true, true, false, true]
    );

    let mut no_owner = good.clone();
    no_owner.islands[0].carry.stroke_id = StrokeIdAction::Fresh;
    assert!(!guard_single_identity_owner(&no_owner));

    let mut restyled = good.clone();
    restyled.islands[1].fill = PaintArg::Set("#00ff00".to_string());
    assert_eq!(
        all_guards(&restyled, &shape),
        [true, true, true, true, true, false]
    );

    let mut faded = good.clone();
    faded.islands[2].opacity = Some(1.0);
    assert!(!guard_uniform_style(&faded));

    let mut phantom_hole = good.clone();
    phantom_hole.islands[1].island.holes.push(0);
    assert!(!guard_every_hole_is_merged(&phantom_hole, &shape));
}
