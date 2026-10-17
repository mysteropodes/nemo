//! Commit half of the paint-bucket join (G18, L02-I): recolour in place, stacking and stamp, and the
//! same-colour merge through the touch probe and the curved-union ports. The probes below are the ports of
//! the join (their real providers are outside this crate); the join itself is the real code.
use super::tests::hit;
use super::*;
use crate::object_document::object_group_kernels::object_boolean_topology::{
    Bounds, FillFacts, MergeDecision, MergeError, ProbeError, TouchProbe, UnavailableTouch,
    UniteDecision,
};

fn fill(hex: &str) -> FillFacts {
    FillFacts {
        is_path: true,
        closed: true,
        fill_hex8: Some(hex.to_string()),
        ..FillFacts::default()
    }
}
fn child(x: f64, y: f64, w: f64, h: f64, covers: bool) -> ChildFacts {
    ChildFacts {
        filled_shape: true,
        excluded: false,
        bounds: Bounds {
            x,
            y,
            width: w,
            height: h,
        },
        area: w * h,
        covers_click: covers,
    }
}

/// Every candidate touches; the union provider answers from a script.
struct Touching;
impl TouchProbe for Touching {
    fn bounds_intersect(&mut self, _c: usize) -> Result<bool, ProbeError> {
        Ok(true)
    }
    fn stroke_between(&mut self, _c: usize) -> Result<bool, ProbeError> {
        Ok(false)
    }
    fn strict_touch(&mut self, _c: usize) -> Result<bool, ProbeError> {
        Ok(true)
    }
    fn near_miss(&mut self, _c: usize) -> Result<bool, ProbeError> {
        Ok(true)
    }
}
struct Unions(Vec<bool>, Vec<usize>);
impl UnionProvider for Unions {
    fn unite(&mut self, step: usize) -> Result<bool, MergeError> {
        self.1.push(step);
        Ok(self.0.remove(0))
    }
}

fn input<'a>(
    hit: Option<&'a TraceHit>,
    children: &'a [ChildFacts],
    facts: &'a [FillFacts],
    bounds: Bounds,
) -> CommitInput<'a> {
    CommitInput {
        click: [5.0, 6.0],
        hit,
        fill_hex8: "#ff0000ff",
        bounds,
        children,
        facts,
        allow_fill_shape_absorb: false,
    }
}
const BOX: Bounds = Bounds {
    x: 0.0,
    y: 0.0,
    width: 100.0,
    height: 100.0,
};

#[test]
fn no_region_recolours_in_place_or_places_with_a_stamp() {
    let region = hit(10_000.0, false, &["w1", "w2"]);
    let none = plan_fill_trace_commit(
        &input(None, &[], &[], BOX),
        &mut Touching,
        &mut UnavailableUnion,
    );
    assert_eq!(none, Ok(FillTracePlan::NoRegion));
    // the traced region IS an existing shape: recolour it
    let same = [child(0.0, 0.0, 100.0, 100.0, true)];
    let facts = [fill("#00ff00ff")];
    let plan = plan_fill_trace_commit(
        &input(Some(&region), &same, &facts, BOX),
        &mut Touching,
        &mut UnavailableUnion,
    );
    assert_eq!(plan, Ok(FillTracePlan::Recolor { child: 0 }));
    // a different region: stacked above the topmost cover, stamped with the click, the gap and the walls
    let mut found = region.clone();
    found.gap_px = 24;
    let smaller = Bounds {
        x: 10.0,
        y: 10.0,
        width: 30.0,
        height: 30.0,
    };
    let plan = plan_fill_trace_commit(
        &input(Some(&found), &same, &facts, smaller),
        &mut Touching,
        &mut Unions(vec![], vec![]),
    );
    // colours differ from the neighbour: nothing to merge
    assert_eq!(
        plan,
        Ok(FillTracePlan::Place {
            insert_at: 1,
            stamp: FillStamp {
                seed: [5.0, 6.0],
                gap_px: 24,
                walls: Some(vec!["w1".to_string(), "w2".to_string()]),
            },
            merge: None,
        })
    );
    // no walls: no fillWalls member at all
    let bare = hit(10_000.0, false, &[]);
    assert_eq!(stamp_for([1.0, 2.0], &bare).walls, None);
}

#[test]
fn a_same_colour_neighbour_is_absorbed_through_the_curved_union_port() {
    let region = hit(900.0, false, &[]);
    let near = [child(20.0, 0.0, 200.0, 200.0, false)];
    let facts = [fill("#ff0000ff")];
    let small = Bounds {
        x: 0.0,
        y: 0.0,
        width: 30.0,
        height: 30.0,
    };
    let mut unions = Unions(vec![true], vec![]);
    let plan = plan_fill_trace_commit(
        &input(Some(&region), &near, &facts, small),
        &mut Touching,
        &mut unions,
    )
    .unwrap();
    // the new fill sits at layer slot 0, so the neighbour is slot 1
    assert_eq!(unions.1, vec![1]);
    match plan {
        FillTracePlan::Place {
            insert_at: 0,
            merge: Some(step),
            ..
        } => {
            assert_eq!(step.absorbed, vec![1]);
            assert_eq!(
                step.decision,
                MergeDecision::Unite(UniteDecision {
                    united: vec![1],
                    failed: vec![]
                })
            );
        }
        other => panic!("{other:?}"),
    }
    // a failed union leaves the new fill as drawn
    let mut failing = Unions(vec![false], vec![]);
    let plan = plan_fill_trace_commit(
        &input(Some(&region), &near, &facts, small),
        &mut Touching,
        &mut failing,
    )
    .unwrap();
    match plan {
        FillTracePlan::Place {
            merge: Some(step), ..
        } => assert_eq!(step.decision, MergeDecision::NoChange),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_near_copy_is_kept_and_never_fed_to_the_union() {
    // 4.5 px of drift: not an existing match (4 px), but a near-identical copy (8 px tolerance on 200 wide)
    let region = hit(40_000.0, false, &[]);
    let copy = [child(4.5, 0.0, 200.0, 200.0, false)];
    let facts = [fill("#ff0000ff")];
    let bounds = Bounds {
        x: 0.0,
        y: 0.0,
        width: 200.0,
        height: 200.0,
    };
    let mut unions = Unions(vec![], vec![]);
    let plan = plan_fill_trace_commit(
        &input(Some(&region), &copy, &facts, bounds),
        &mut Touching,
        &mut unions,
    )
    .unwrap();
    assert!(unions.1.is_empty());
    match plan {
        FillTracePlan::Place {
            merge: Some(step), ..
        } => assert_eq!(step.decision, MergeDecision::KeepExisting(1)),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_fill_brush_shape_is_absorbed_only_when_the_trace_used_its_stroke() {
    let mut brush = fill("#ff0000ff");
    brush.is_fill_shape = true;
    brush.stroke_id = Some("w1".to_string());
    let facts = [brush];
    let near = [child(20.0, 0.0, 200.0, 200.0, false)];
    let small = Bounds {
        x: 0.0,
        y: 0.0,
        width: 30.0,
        height: 30.0,
    };
    let with_wall = hit(900.0, false, &["w1"]);
    let mut yes = Unions(vec![true], vec![]);
    let plan = plan_fill_trace_commit(
        &input(Some(&with_wall), &near, &facts, small),
        &mut Touching,
        &mut yes,
    )
    .unwrap();
    assert!(matches!(plan, FillTracePlan::Place { merge: Some(_), .. }));
    let other_wall = hit(900.0, false, &["w9"]);
    let mut no = Unions(vec![], vec![]);
    let plan = plan_fill_trace_commit(
        &input(Some(&other_wall), &near, &facts, small),
        &mut Touching,
        &mut no,
    )
    .unwrap();
    assert!(matches!(plan, FillTracePlan::Place { merge: None, .. }));
}

#[test]
fn unavailable_geometry_is_typed_and_decides_nothing() {
    let region = hit(900.0, false, &[]);
    let near = [child(20.0, 0.0, 200.0, 200.0, false)];
    let facts = [fill("#ff0000ff")];
    let small = Bounds {
        x: 0.0,
        y: 0.0,
        width: 30.0,
        height: 30.0,
    };
    assert_eq!(
        plan_fill_trace_commit(
            &input(Some(&region), &near, &facts, small),
            &mut UnavailableTouch,
            &mut UnavailableUnion
        ),
        Err(FillTraceJoinError::Merge(MergeError::GeometryUnavailable))
    );
    assert_eq!(
        plan_fill_trace_commit(
            &input(Some(&region), &near, &facts, small),
            &mut Touching,
            &mut UnavailableUnion
        ),
        Err(FillTraceJoinError::Merge(MergeError::GeometryUnavailable))
    );
}
