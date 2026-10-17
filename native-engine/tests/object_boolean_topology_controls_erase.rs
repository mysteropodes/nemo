//! Independent literal controls for the eraser admission (L02-D): the replace
//! slot, delete-on-empty, unusable rings, polygon drops, the outline rule and the
//! plausibility boundaries. Hand-written expectations; nothing is read from the oracle.

use super::controls::{b, data, ext, text};
use super::{
    erase_stroke_info, plan_erase_polygons, plan_erase_shape, wasm_result_plausible, Bounds,
    EraseOutcome, EraseTarget, PaintArg, PathFacts, Polygon, ResultShape, Ring, SourceData,
    StrokeIdAction, StrokeInfo, TopologyError,
};

fn square(x: f64, y: f64, s: f64) -> Vec<[f64; 2]> {
    vec![[x, y], [x + s, y], [x + s, y + s], [x, y + s]]
}

fn poly(exterior: Vec<[f64; 2]>, holes: Vec<Vec<[f64; 2]>>) -> Polygon {
    Polygon { exterior, holes }
}

fn target<'a>(index: usize, len: usize, data: Option<&'a SourceData>) -> EraseTarget<'a> {
    EraseTarget {
        index,
        layer_len: len,
        fill: Some("#00ff00".to_string()),
        opacity: 0.8,
        stroke: None,
        data,
    }
}

fn two_islands() -> ResultShape {
    ResultShape::Compound(vec![ext(0.0, 0.0, 10.0, 10.0), ext(20.0, 0.0, 10.0, 10.0)])
}

#[test]
fn an_erase_replaces_its_target_in_its_own_slot() {
    let src = data(&[("strokeId", text("sT"))]);
    let plan = plan_erase_shape(&target(2, 5, Some(&src)), &two_islands()).unwrap();
    match plan.outcome {
        EraseOutcome::Replace(insert) => {
            let slots: Vec<usize> = insert.islands.iter().map(|i| i.index).collect();
            assert_eq!(slots, vec![2, 3], "the layer shrinks to 4 then grows to 6");
            assert_eq!(insert.islands[0].fill, PaintArg::Set("#00ff00".to_string()));
            assert_eq!(insert.islands[1].opacity, Some(0.8));
            assert_eq!(
                insert.islands[0].carry.stroke_id,
                StrokeIdAction::Carry(text("sT"))
            );
            assert_eq!(insert.islands[1].carry.stroke_id, StrokeIdAction::Fresh);
            assert!(
                !insert.skip_refit,
                "eraseAtPoint does not pass preserveGeometry"
            );
        }
        other => panic!("{other:?}"),
    }
    let last = plan_erase_shape(&target(4, 5, None), &two_islands()).unwrap();
    match last.outcome {
        EraseOutcome::Replace(insert) => assert_eq!(insert.islands[0].index, 4),
        other => panic!("{other:?}"),
    }
}

#[test]
fn zero_islands_delete_the_target_and_no_fill_changes_nothing() {
    for empty in [
        ResultShape::Empty,
        ResultShape::Compound(vec![]),
        ResultShape::Single(PathFacts {
            bounds: Bounds::EMPTY,
            segments: 0,
        }),
    ] {
        assert_eq!(
            plan_erase_shape(&target(1, 3, None), &empty)
                .unwrap()
                .outcome,
            EraseOutcome::DeleteTarget
        );
    }
    let mut bare = target(1, 3, None);
    bare.fill = None;
    assert_eq!(
        plan_erase_shape(&bare, &two_islands()).unwrap().outcome,
        EraseOutcome::Unchanged
    );
    bare.fill = Some(String::new());
    assert_eq!(
        plan_erase_polygons(&bare, &[]).unwrap().outcome,
        EraseOutcome::Unchanged
    );
}

#[test]
fn rings_under_three_segments_and_bad_targets_are_refused() {
    let short = ResultShape::Compound(vec![
        ext(0.0, 0.0, 9.0, 9.0),
        Ring {
            segments: 2,
            ..ext(20.0, 0.0, 9.0, 9.0)
        },
    ]);
    assert_eq!(
        plan_erase_shape(&target(0, 1, None), &short).unwrap_err(),
        TopologyError::UnusableRing {
            ring: 1,
            segments: 2
        }
    );
    let single = ResultShape::Single(PathFacts {
        bounds: b(0.0, 0.0, 1.0, 1.0),
        segments: 2,
    });
    assert_eq!(
        plan_erase_shape(&target(0, 1, None), &single).unwrap_err(),
        TopologyError::UnusableRing {
            ring: 0,
            segments: 2
        }
    );
    assert_eq!(
        plan_erase_shape(&target(3, 3, None), &two_islands()).unwrap_err(),
        TopologyError::TargetOutOfRange {
            index: 3,
            layer_len: 3
        }
    );
    assert!(plan_erase_shape(&target(0, 0, None), &two_islands()).is_err());
}

#[test]
fn polygon_erase_reports_what_it_dropped_and_deletes_when_nothing_is_left() {
    let polys = vec![
        poly(vec![[0.0, 0.0], [5.0, 5.0]], vec![]),
        poly(square(0.0, 0.0, 60.0), vec![square(10.0, 10.0, 10.0)]),
    ];
    let plan = plan_erase_polygons(&target(0, 2, None), &polys).unwrap();
    assert_eq!(plan.dropped, vec![0]);
    assert!(matches!(plan.outcome, EraseOutcome::Replace(_)));
    let nothing = plan_erase_polygons(&target(0, 2, None), &polys[..1]).unwrap();
    assert_eq!(
        (nothing.outcome, nothing.dropped),
        (EraseOutcome::DeleteTarget, vec![0])
    );
}

#[test]
fn vector_brush_ink_never_redraws_its_outline() {
    let info = StrokeInfo {
        color: "#003366".to_string(),
        width: 4.0,
        cap: "butt".to_string(),
        join: "miter".to_string(),
    };
    assert_eq!(
        erase_stroke_info(false, Some(info.clone())),
        Some(info.clone())
    );
    assert_eq!(erase_stroke_info(true, Some(info)), None);
    assert_eq!(erase_stroke_info(false, None), None);
    let blank = StrokeInfo {
        color: String::new(),
        width: 1.0,
        cap: String::new(),
        join: String::new(),
    };
    assert_eq!(erase_stroke_info(false, Some(blank)), None);
}

#[test]
fn plausibility_boundaries() {
    // eraser footprint 100: a result may not lose more than six footprints (600)
    // when the target is larger than four (400).
    assert!(
        wasm_result_plausible(600.0, 0.0, 100.0),
        "removing exactly 600 is allowed"
    );
    assert!(
        !wasm_result_plausible(601.0, 0.0, 100.0),
        "removing 601 is not"
    );
    assert!(wasm_result_plausible(1000.0, 400.0, 100.0));
    assert!(!wasm_result_plausible(1000.0, 399.0, 100.0));
    assert!(
        wasm_result_plausible(400.0, 0.0, 100.0),
        "a target of four footprints is exempt"
    );
    assert!(
        !wasm_result_plausible(-1000.0, -399.0, 100.0),
        "areas are absolute values"
    );
    assert!(
        !wasm_result_plausible(1000.0, f64::NAN, 100.0),
        "a NaN result area counts as zero"
    );
}
