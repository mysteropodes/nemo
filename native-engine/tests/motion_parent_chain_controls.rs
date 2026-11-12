//! nemo-test-harness: test-evaluation => motion_parent_chain_controls_tests
//! Hand-written controls for the parent-chain kernel: walk guards and the
//! JavaScript coercions the generated oracle reaches only incidentally.

use crate::evaluation::motion_parent_chain::{
    ancestor_entry, blend_weighted, compose_static_chain, invert_vector_through_chain, js_hypot2,
    legacy_chain, motion_mat_from_values, parent_uid_at, rot_pt, rot_vec, walk_chain, AncestorBase,
    ChainEntry, MotionMat, Pose,
};

const CANVAS: [f64; 2] = [1920.0, 1080.0];

fn bits(x: f64) -> u64 {
    if x.is_nan() {
        f64::NAN.to_bits()
    } else {
        x.to_bits()
    }
}

const UNIT: MotionMat = MotionMat {
    dx: 1.0,
    dy: 0.0,
    rot: 0.0,
    sx: 1.0,
    sy: 1.0,
    op: 1.0,
    ax: 0.0,
    ay: 0.0,
};

/// `n` unit-translation layers, each parented to the next (the last to the
/// first when `cyclic`).
fn next(n: usize, cyclic: bool) -> impl Fn(usize) -> Option<usize> {
    move |i| (i + 1 < n || cyclic).then_some((i + 1) % n)
}

fn legacy_len(n: usize, cyclic: bool, start: usize) -> usize {
    let base = |i: usize| AncestorBase::Content(Some([i as f64, -(i as f64)]));
    legacy_chain(start, next(n, cyclic), |i| (Some(UNIT), base(i)), CANVAS).len()
}

#[test]
fn walk_guard_and_cycles() {
    // A 65-ancestor chain stops after 64; the walking layer never re-enters.
    assert_eq!(legacy_len(66, false, 0), 64);
    assert_eq!(legacy_len(65, false, 0), 64);
    assert_eq!(legacy_len(64, false, 0), 63);
    assert_eq!(legacy_len(5, true, 2), 4);
    assert_eq!(legacy_len(1, true, 0), 0, "self parent");
    // composeChainTransform counts its start node inside the same limit.
    let centre = |i: usize| Some([i as f64, -(i as f64)]);
    let p = compose_static_chain(
        Some(0),
        next(66, false),
        |i| (Some(UNIT), centre(i)),
        CANVAS,
    );
    assert_eq!(p.map(|p| p.dx), Some(64.0));
    assert_eq!(
        compose_static_chain(None, next(3, false), |_| (Some(UNIT), None), CANVAS),
        None
    );
    assert_eq!(
        walk_chain(Some(3), None, |i| Some((i + 1) % 5)),
        vec![3, 4, 0, 1, 2]
    );
}

#[test]
fn javascript_coercions_are_kept() {
    // A falsy angle skips the rotation entirely, so NaN does not poison.
    assert_eq!(rot_pt(1.0, 2.0, 5.0, 5.0, f64::NAN), [1.0, 2.0]);
    assert_eq!(bits(rot_vec(-0.0, 1.0, -0.0)[0]), bits(-0.0));
    // A zero or NaN scale reads as 1 when inverting.
    let flat = ChainEntry {
        mat: motion_mat_from_values([0.0, 0.0], [0.0, 0.0], 0.0, [0.0, f64::NAN], 100.0)
            .expect("zero scale is not identity"),
        pivot: [0.0, 0.0],
    };
    assert_eq!(invert_vector_through_chain(&[flat], 3.0, 4.0), [3.0, 4.0]);
    // Exact identity values return no mat; -0 opacity is clamped to +0.
    assert_eq!(
        motion_mat_from_values(
            [-0.0, f64::NAN],
            [0.0, -0.0],
            f64::NAN,
            [100.0, 100.0],
            100.0
        ),
        None
    );
    let m = motion_mat_from_values([0.0; 2], [0.0; 2], 0.0, [100.0; 2], -0.0).expect("mat");
    assert_eq!(bits(m.op), bits(0.0));
    // Keyed parents: the first key's uid before it, the last reached after.
    let keys = [(5.0, Some("A")), (10.0, None)];
    assert_eq!(parent_uid_at(&keys, Some(&"S"), 0.0), Some("A"));
    assert_eq!(parent_uid_at(&keys, Some(&"S"), 10.0), None);
    assert_eq!(parent_uid_at::<&str>(&[], Some(&"S"), 0.0), Some("S"));
    // Null pivots fall back to the canvas centre; content without bounds
    // contributes nothing.
    let e = ancestor_entry(Some(m), AncestorBase::Null(None), CANVAS).expect("null entry");
    assert_eq!(e.pivot, [960.0, 540.0]);
    assert_eq!(
        ancestor_entry(Some(m), AncestorBase::Content(None), CANVAS),
        None
    );
    // Math.hypot: Infinity beats NaN, all-zero is +0.
    assert_eq!(js_hypot2(f64::NAN, f64::NEG_INFINITY), f64::INFINITY);
    assert!(js_hypot2(f64::NAN, 1.0).is_nan());
    assert_eq!(bits(js_hypot2(-0.0, -0.0)), bits(0.0));
    // All weights zero (or NaN) fall back to the first parent alone.
    let a = Pose {
        dx: 7.0,
        dy: 0.0,
        rot: 0.0,
        sx: 1.0,
        sy: 1.0,
    };
    let m = blend_weighted(&[(0.0, Some(a)), (f64::NAN, None)]).expect("fallback");
    assert_eq!((m.dx, m.rot), (7.0, 0.0));
    assert_eq!(blend_weighted(&[]), None);
}
