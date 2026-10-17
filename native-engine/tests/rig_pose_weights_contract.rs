//! Typed-contract tests of the rig weights and anchors kernel (L03-D, I12): the angle pins, the
//! two weight shapes and their round trip, and the anchor resolution and its typed refusals.

use std::f64::consts::PI;

use super::oracle_rows::near;
use super::tests::{bone, hand, ids};
use super::{
    anchor_for, chord_angle, normalize_angle, weigh_one_point_by_control, Anchor, AnchorError,
    ChordError, Point2, PointSource, WeightEntry, WeightParts,
};

#[test]
fn normalize_angle_pins() {
    assert_eq!(normalize_angle(-PI), -PI);
    assert_eq!(normalize_angle(PI), -PI);
    assert_eq!(normalize_angle(3.0 * PI), -PI);
    assert_eq!(normalize_angle(-3.0 * PI), -PI);
    assert_eq!(normalize_angle(2.0 * PI), 0.0);
    assert_eq!(normalize_angle(-2.0 * PI), 0.0);
    assert_eq!(normalize_angle(0.0), 0.0);
    // Tiny negatives are absorbed by `a + pi` and come back as exactly 0.
    for tiny in [-1e-17, -1e-15, 1e-15] {
        assert_eq!(normalize_angle(tiny), 0.0);
    }
    assert_eq!(normalize_angle(PI / 2.0), PI / 2.0);
    assert_eq!(normalize_angle(-PI / 2.0), -PI / 2.0);
    assert_eq!(normalize_angle(7.0), 0.7168146928204138);
    assert_eq!(normalize_angle(-7.0), -0.7168146928204138);
    assert!(normalize_angle(f64::NAN).is_nan());
    assert!(normalize_angle(f64::INFINITY).is_nan());
}

#[test]
fn chord_angle_clamps_both_ends_and_is_zero_for_short_lists() {
    let l3: Vec<Option<Point2>> = vec![Some([0.0, 0.0]), Some([10.0, 0.0]), Some([10.0, 10.0])];
    let at = |k: usize| chord_angle(&l3, k).unwrap();
    assert_eq!(at(0), 0.0);
    assert!(near(at(1), 0.7853981633974483, PI));
    assert_eq!(at(2), PI / 2.0);
    // k = len: prev = last, next = last (clamped), a zero chord.
    assert_eq!(at(3), 0.0);
    let l2: Vec<Option<Point2>> = vec![Some([0.0, 0.0]), Some([10.0, 0.0])];
    assert_eq!(
        [0, 1, 2].map(|k| chord_angle(&l2, k).unwrap()),
        [0.0, 0.0, 0.0]
    );
    let vertical: Vec<Option<Point2>> =
        vec![Some([0.0, 0.0]), Some([0.0, 10.0]), Some([0.0, 20.0])];
    for k in 0..3 {
        assert_eq!(chord_angle(&vertical, k).unwrap(), PI / 2.0);
    }
    let back: Vec<Option<Point2>> = vec![Some([0.0, 0.0]), Some([-10.0, 0.0])];
    assert_eq!(chord_angle(&back, 0).unwrap(), PI);
    assert_eq!(chord_angle(&[Some([1.0, 1.0])], 5).unwrap(), 0.0);
    assert_eq!(chord_angle(&[], 0).unwrap(), 0.0);
    // A short list is 0 even when the point is missing or k is far past the end.
    assert_eq!(chord_angle(&[None], 9).unwrap(), 0.0);
}

#[test]
fn chord_angle_errors_name_the_first_unreadable_index() {
    let l3: Vec<Option<Point2>> = vec![Some([0.0, 0.0]), Some([10.0, 0.0]), Some([10.0, 10.0])];
    assert_eq!(chord_angle(&l3, 4), Err(ChordError { index: 3 }));
    let gap: Vec<Option<Point2>> = vec![Some([0.0, 0.0]), None, Some([10.0, 0.0])];
    // k = 0: prev = 0 (fine), next = 1 (missing). k = 2: prev = 1 (missing).
    assert_eq!(chord_angle(&gap, 0), Err(ChordError { index: 1 }));
    assert_eq!(chord_angle(&gap, 2), Err(ChordError { index: 1 }));
    // k = 1: next (2) is readable, prev (0) is readable too, so the missing centre is not read.
    assert!(chord_angle(&gap, 1).is_ok());
    // Both neighbours missing: the baseline reads `next` first.
    let both: Vec<Option<Point2>> = vec![None, Some([1.0, 1.0]), None];
    assert_eq!(chord_angle(&both, 1), Err(ChordError { index: 2 }));
}

fn parts(pt: Option<usize>, off: Option<f64>) -> WeightParts {
    WeightParts {
        bone_id: "b".to_string(),
        pt_index: pt,
        offset: off,
        w: 0.5,
    }
}

#[test]
fn weight_shapes_are_told_apart_by_the_presence_of_pt_index_only() {
    // Index 0 is a control point (a truthiness test would route it to the offset model).
    assert!(
        matches!(WeightEntry::classify(parts(Some(0), None)), WeightEntry::Control(c) if c.pt_index == 0)
    );
    assert!(
        matches!(WeightEntry::classify(parts(None, Some(12.0))), WeightEntry::Offset(o) if o.offset == Some(12.0))
    );
    // Neither field: still the offset model, with its offset left absent (not defaulted).
    assert!(
        matches!(WeightEntry::classify(parts(None, None)), WeightEntry::Offset(o) if o.offset.is_none())
    );
    // Both: ptIndex wins and the stray offset is kept.
    match WeightEntry::classify(parts(Some(2), Some(7.0))) {
        WeightEntry::Control(c) => assert_eq!((c.pt_index, c.stray_offset), (2, Some(7.0))),
        other => panic!("expected control, got {other:?}"),
    }
}

#[test]
fn both_weight_shapes_round_trip_through_the_typed_enum() {
    for p in [
        parts(Some(0), None),
        parts(Some(3), Some(9.5)),
        parts(None, Some(0.0)),
        parts(None, None),
    ] {
        assert_eq!(WeightEntry::classify(p.clone()).parts(), p);
    }
    let emitted = weigh_one_point_by_control(&hand(), &ids(&["b1"]), [50.0, 50.0], 200.0, 0.0);
    for c in emitted {
        let entry = WeightEntry::from(c.clone());
        assert_eq!(entry, WeightEntry::Control(c.clone()));
        assert_eq!(WeightEntry::classify(entry.parts()), entry);
        assert_eq!(entry.parts().offset, None);
    }
}

#[test]
fn anchors_resolve_control_points_with_live_and_rest_and_the_local_turn() {
    let mut r1 = bone("r1", &[[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]]);
    r1.segments = Some(vec![Some([0.0, 0.0]), Some([0.0, 10.0]), Some([0.0, 20.0])]);
    let entry = WeightEntry::classify(WeightParts {
        bone_id: "r1".into(),
        pt_index: Some(1),
        offset: None,
        w: 1.0,
    });
    match anchor_for(&[r1], &entry).unwrap() {
        Anchor::Control(c) => {
            assert_eq!(
                (c.rest_x, c.rest_y, c.cur_x, c.cur_y),
                (10.0, 0.0, 0.0, 10.0)
            );
            assert!(near(c.da, 1.5707963267948966, PI));
        }
        other => panic!("expected control anchor, got {other:?}"),
    }
    // Fully reversed chord: the turn of pi wraps to -pi, exactly.
    let mut r2 = bone("r2", &[[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]]);
    r2.segments = Some(vec![Some([20.0, 0.0]), Some([10.0, 0.0]), Some([0.0, 0.0])]);
    let entry = WeightEntry::classify(WeightParts {
        bone_id: "r2".into(),
        pt_index: Some(1),
        offset: None,
        w: 1.0,
    });
    match anchor_for(&[r2], &entry).unwrap() {
        Anchor::Control(c) => assert_eq!(c.da, -PI),
        other => panic!("expected control anchor, got {other:?}"),
    }
    // A pure translation turns nothing.
    let mut r3 = bone("r3", &[[0.0, 0.0], [10.0, 0.0], [20.0, 0.0]]);
    r3.segments = Some(vec![Some([5.0, 5.0]), Some([15.0, 5.0]), Some([25.0, 5.0])]);
    let entry = WeightEntry::classify(WeightParts {
        bone_id: "r3".into(),
        pt_index: Some(0),
        offset: None,
        w: 1.0,
    });
    match anchor_for(&[r3], &entry).unwrap() {
        Anchor::Control(c) => assert_eq!((c.rest_x, c.cur_x, c.cur_y, c.da), (0.0, 5.0, 5.0, 0.0)),
        other => panic!("expected control anchor, got {other:?}"),
    }
}

#[test]
fn anchors_are_unresolved_or_typed_errors_never_invented() {
    let b = hand();
    let ctl = |id: &str, k: usize| {
        WeightEntry::classify(WeightParts {
            bone_id: id.into(),
            pt_index: Some(k),
            offset: None,
            w: 1.0,
        })
    };
    assert_eq!(anchor_for(&b, &ctl("zz", 0)), Ok(Anchor::Unresolved));
    assert_eq!(anchor_for(&b, &ctl("b1", 3)), Ok(Anchor::Unresolved));
    let mut no_live = bone("s", &[[0.0, 0.0]]);
    no_live.segments = None;
    assert_eq!(anchor_for(&[no_live], &ctl("s", 0)), Ok(Anchor::Unresolved));
    let mut no_rest = bone("s", &[[0.0, 0.0]]);
    no_rest.rest_segments = None;
    assert_eq!(anchor_for(&[no_rest], &ctl("s", 0)), Ok(Anchor::Unresolved));
    // Rest point missing: typed, before any value exists.
    let mut gap = bone("s", &[[0.0, 0.0], [1.0, 1.0], [2.0, 2.0]]);
    gap.rest_segments = Some(vec![Some([0.0, 0.0]), None, Some([2.0, 2.0])]);
    assert_eq!(
        anchor_for(&[gap.clone()], &ctl("s", 1)),
        Err(AnchorError::MissingPoint {
            source: PointSource::Rest,
            index: 1
        })
    );
    gap.segments = Some(vec![Some([0.0, 0.0]), None, Some([2.0, 2.0])]);
    gap.rest_segments = Some(vec![Some([0.0, 0.0]), Some([1.0, 1.0]), Some([2.0, 2.0])]);
    assert_eq!(
        anchor_for(&[gap.clone()], &ctl("s", 1)),
        Err(AnchorError::MissingPoint {
            source: PointSource::Live,
            index: 1
        })
    );
    // The live chord at k = 0 needs the missing live neighbour.
    assert_eq!(
        anchor_for(&[gap], &ctl("s", 0)),
        Err(AnchorError::MissingPoint {
            source: PointSource::Live,
            index: 1
        })
    );
}

#[test]
fn offset_entries_are_reported_for_the_curve_kernel_not_resolved_here() {
    let b = hand();
    let off = |id: &str, o: Option<f64>| {
        WeightEntry::classify(WeightParts {
            bone_id: id.into(),
            pt_index: None,
            offset: o,
            w: 1.0,
        })
    };
    assert_eq!(
        anchor_for(&b, &off("b1", Some(12.0))),
        Ok(Anchor::OffsetModel { offset: Some(12.0) })
    );
    assert_eq!(
        anchor_for(&b, &off("b1", None)),
        Ok(Anchor::OffsetModel { offset: None })
    );
    assert_eq!(
        anchor_for(&b, &off("zz", Some(12.0))),
        Err(AnchorError::OffsetBoneMissing)
    );
}
