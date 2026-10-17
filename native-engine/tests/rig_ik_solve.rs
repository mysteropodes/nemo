//! Tests for the two-bone IK kernel (L03-C, I12): the baseline oracle rows through production,
//! plus hand-pinned literals computed by the verbatim baseline functions (`tools/literals.cjs`)
//! and the typed contract (missing vertices, non-finite poses, atomic output).

use super::oracle_rows::{
    apply, bone, chain, hex, parse_rig, r, rows, run, same, Tally, ORACLE, PRODUCTION,
};
use super::{
    anchor_point, chain_key, drag_ik_end, drag_writes, find_ik_chain, hypot2, js_max, js_min,
    set_ik, solve_raw, IkBone, IkChain, IkError, IkSlot, Point2,
};

/// Row counts of the oracle (`tools/gen.cjs`); a vacuous or truncated oracle fails here.
pub(crate) const ROW_COUNTS: [(&str, usize); 5] = [
    ("anc", 27),
    ("set", 49),
    ("drg", 308),
    ("fnd", 47),
    ("flw", 20),
];

/// Bones A = [S, E] and B = [E, W] with the 100/100 chain of the contract fixtures.
fn limb() -> Vec<IkBone> {
    vec![
        bone("boneA", &[[0.0, 0.0], [50.0, 86.60254037844386]]),
        bone("boneB", &[[50.0, 86.60254037844386], [100.0, 0.0]]),
    ]
}

fn chain_of(l1: f64, l2: f64, flip: bool) -> IkChain {
    IkChain {
        root: r("boneA", 0),
        joint: r("boneA", 1),
        end: r("boneB", 1),
        l1,
        l2,
        flip,
    }
}

fn close(a: Point2, b: Point2, tol: f64) -> bool {
    (a[0] - b[0]).abs() <= tol && (a[1] - b[1]).abs() <= tol
}

fn dist(a: Point2, b: Point2) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn drag(l1: f64, l2: f64, flip: bool, target: Point2) -> super::IkPose {
    drag_ik_end(&limb(), &chain_of(l1, l2, flip), target).expect("finite pose")
}

#[test]
fn oracle_row_counts_are_pinned() {
    for (op, count) in ROW_COUNTS {
        assert_eq!(rows(op).len(), count, "rows for {op}");
    }
    assert_eq!(ORACLE.lines().count(), 451);
}

#[test]
fn every_oracle_row_matches_production() {
    let t = run(&PRODUCTION);
    assert_eq!(t.wrong(), 0, "{t:?}");
}

#[test]
fn baseline_facts_the_rows_pin() {
    // 6 drg rows threw in the baseline (2 of them after writing the joint) and 8 returned a
    // NaN or infinite pose (NaN target, NaN length, 0/0 once the 1e-6 guard is absorbed).
    let t = run(&PRODUCTION);
    assert_eq!((t.threw, t.threw_partial, t.non_finite), (6, 2, 8));
    assert_eq!(Tally::default().wrong(), 0);
}

#[test]
fn anchor_prefers_live_falls_back_to_rest_and_needs_the_rest_vertex() {
    let mut b = bone("boneS", &[[1.0, 1.0]]);
    b.rest_segments = vec![[1.0, 1.0], [2.0, 2.0], [3.0, 3.0]];
    let rig = vec![b, bone("boneL", &[[7.0, 7.0], [8.0, 8.0]])];
    assert_eq!(anchor_point(&rig, &r("boneS", 0)), Some([1.0, 1.0]));
    assert_eq!(anchor_point(&rig, &r("boneS", 2)), Some([3.0, 3.0]));
    assert_eq!(anchor_point(&rig, &r("boneS", 3)), None);
    assert_eq!(anchor_point(&rig, &r("nope", 0)), None);
    let mut live_only = bone("boneN", &[[1.0, 1.0], [2.0, 2.0]]);
    live_only.rest_segments = vec![[1.0, 1.0]];
    assert_eq!(anchor_point(&[live_only], &r("boneN", 1)), None);
}

#[test]
fn set_ik_stores_v8_hypot_lengths_flip_and_the_end_key() {
    let c = set_ik(
        &limb(),
        &r("boneA", 0),
        &r("boneA", 1),
        &r("boneB", 1),
        false,
    )
    .expect("chain");
    assert_eq!((c.l1, c.l2), (99.99999999999999, 99.99999999999999));
    assert_eq!(c.key(), "boneB:1");
    assert_eq!(chain_key(&r("x", 12)), "x:12");
    let rig = vec![
        bone("boneA", &[[0.0, 0.0], [30.0, 40.0]]),
        bone("boneB", &[[30.0, 40.0], [30.0, 90.0]]),
    ];
    let c = set_ik(&rig, &r("boneA", 0), &r("boneA", 1), &r("boneB", 1), true).expect("chain");
    assert_eq!((c.l1, c.l2, c.flip), (50.0, 50.0, true));
}

#[test]
fn set_ik_names_the_first_missing_anchor() {
    let rig = limb();
    let (a0, a1, b1) = (r("boneA", 0), r("boneA", 1), r("boneB", 1));
    let miss = |id: &str| r(id, 0);
    assert_eq!(
        set_ik(&rig, &miss("no"), &a1, &b1, false),
        Err(IkError::AnchorMissing(IkSlot::Root))
    );
    assert_eq!(
        set_ik(&rig, &a0, &miss("no"), &b1, false),
        Err(IkError::AnchorMissing(IkSlot::Joint))
    );
    assert_eq!(
        set_ik(&rig, &a0, &a1, &r("boneB", 2), false),
        Err(IkError::AnchorMissing(IkSlot::End))
    );
    assert_eq!(
        set_ik(&rig, &miss("no"), &miss("no"), &miss("no"), false),
        Err(IkError::AnchorMissing(IkSlot::Root))
    );
}

#[test]
fn flip_mirrors_the_joint_and_both_poses_keep_the_lengths() {
    let (a, b) = (
        drag(100.0, 100.0, false, [100.0, 0.0]),
        drag(100.0, 100.0, true, [100.0, 0.0]),
    );
    assert!(close(
        a.joint,
        [49.999999999999986, 86.60254037844388],
        1e-9
    ));
    assert!(close(
        b.joint,
        [49.999999999999986, -86.60254037844388],
        1e-9
    ));
    assert!((a.joint[0] - b.joint[0]).abs() < 1e-12 && (a.joint[1] + b.joint[1]).abs() < 1e-12);
    // the target is reachable, so the end is exactly (100, 0): atan2(0, 100) = 0, cos 0 = 1
    assert_eq!((a.end, b.end), ([100.0, 0.0], [100.0, 0.0]));
    for pose in [a, b] {
        assert!(
            (dist([0.0, 0.0], pose.joint) - 100.0).abs() < 1e-9,
            "joint is l1 from the root"
        );
        assert!(
            (dist(pose.joint, pose.end) - 100.0).abs() < 1e-9,
            "end is l2 from the joint"
        );
    }
}

#[test]
fn rest_target_returns_the_rest_joint_and_touches_nothing_else() {
    let rig = limb();
    let c = set_ik(&rig, &r("boneA", 0), &r("boneA", 1), &r("boneB", 1), false).expect("chain");
    let pose = drag_ik_end(&rig, &c, [100.0, 0.0]).expect("pose");
    assert!(close(pose.joint, [50.0, 86.60254037844386], 1e-9));
    assert!(close(pose.end, [100.0, 0.0], 1e-9));
    // applying only the joint and end leaves every other vertex bit for bit as supplied
    let mut after = rig.clone();
    let writes = [
        super::PointWrite {
            target: c.joint.clone(),
            point: pose.joint,
        },
        super::PointWrite {
            target: c.end.clone(),
            point: pose.end,
        },
    ];
    apply(&mut after, &writes);
    assert_eq!(after[0].segments[0], rig[0].segments[0]);
    assert_eq!(after[1].segments[0], rig[1].segments[0]);
    assert_eq!(after[0].rest_segments, rig[0].rest_segments);
    assert_eq!(after[1].rest_segments, rig[1].rest_segments);
}

#[test]
fn far_target_clamps_to_reach_minus_epsilon_on_the_base_angle_ray() {
    let t = [500.0, 0.0];
    let p = drag(100.0, 100.0, false, t);
    assert_eq!(p.end, [199.999999, 0.0]);
    assert!(p.end != t, "the end is NOT the raw target");
    assert!(close(
        p.joint,
        [99.99999950000002, 0.009999999846090343],
        1e-9
    ));
    let q = drag(100.0, 100.0, true, [0.0, -300.0]);
    assert!(close(q.end, [0.0, -199.999999], 1e-9));
    assert!(close(
        q.joint,
        [-0.009999999846080338, -99.99999950000002],
        1e-9
    ));
    // an infinite target is the same clamped pose along its ray
    assert_eq!(drag(100.0, 100.0, false, [f64::INFINITY, 0.0]), p);
}

#[test]
fn near_target_clamps_to_the_length_difference_plus_epsilon() {
    let p = drag(150.0, 50.0, false, [10.0, 0.0]);
    assert_eq!(p.end, [100.000001, 0.0]);
    assert!(close(
        p.joint,
        [149.99999950000003, 0.01224744839454493],
        1e-9
    ));
    let q = drag(50.0, 150.0, true, [-100.0, 0.0]);
    assert!(close(q.end, [-100.000001, 0.0], 1e-9));
    assert!(close(
        q.joint,
        [49.99999850000002, 0.012247448562169073],
        1e-9
    ));
}

#[test]
fn cos_a_is_clamped_into_unit_range_before_acos() {
    // l2 = 0, l1 = 100, target on the ray: the unclamped cosA is 1.0000000000000002.
    let c = 100.000001_f64;
    let raw_cos = (100.0 * 100.0 + c * c - 0.0) / (2.0 * 100.0 * c);
    assert!(raw_cos > 1.0, "{raw_cos}: acos of it would be NaN");
    assert!(raw_cos.acos().is_nan());
    let p = drag(100.0, 0.0, false, [100.0, 0.0]);
    assert_eq!((p.joint, p.end), ([100.0, 0.0], [100.000001, 0.0]));
    assert_eq!(js_max(-1.0, js_min(1.0, raw_cos)), 1.0);
}

#[test]
fn degenerate_poses_are_the_baselines_or_typed_refusals() {
    // l1 = 0: the joint sits on the root (cosA = +inf clamps to 1)
    let p = drag(0.0, 100.0, false, [50.0, 50.0]);
    assert_eq!(p.joint, [0.0, 0.0]);
    assert!(close(p.end, [70.71067882576153, 70.71067882576152], 1e-9));
    // target equal to the root: clamped to the 1e-6 guard, the elbow bends to 90 degrees
    let q = drag(100.0, 100.0, false, [0.0, 0.0]);
    assert!(close(q.joint, [9.094947078961621e-7, 100.0], 1e-9) && close(q.end, [1e-6, 0.0], 1e-9));
    // 0 / 0 (l1 = 0 and the guard absorbed by l2 = 1e12) and NaN inputs are refused, not written
    for (l1, l2, t) in [
        (0.0, 1e12, [5.0, 5.0]),
        (100.0, 100.0, [f64::NAN, 0.0]),
        (f64::NAN, 100.0, [30.0, 40.0]),
    ] {
        let rig = limb();
        assert_eq!(
            drag_ik_end(&rig, &chain_of(l1, l2, false), t),
            Err(IkError::NonFinitePose)
        );
        let raw = solve_raw([0.0, 0.0], l1, l2, false, t);
        assert!(
            raw.joint[0].is_nan() && raw.joint[1].is_nan(),
            "baseline NaN is kept by solve_raw"
        );
        assert_eq!(rig, limb(), "a refusal changes nothing");
    }
}

#[test]
fn missing_vertices_are_typed_and_nothing_is_returned() {
    let rig = limb();
    let mut c = chain_of(100.0, 100.0, false);
    c.root = r("boneA", 9);
    assert_eq!(
        drag_ik_end(&rig, &c, [50.0, 50.0]),
        Err(IkError::SegmentMissing(IkSlot::Root))
    );
    let mut c = chain_of(100.0, 100.0, false);
    c.joint = r("nope", 1);
    assert_eq!(
        drag_ik_end(&rig, &c, [50.0, 50.0]),
        Err(IkError::SegmentMissing(IkSlot::Joint))
    );
    let mut c = chain_of(100.0, 100.0, true);
    c.end = r("boneB", 9);
    // the baseline wrote the joint first and then threw at the end: the partial mutation is gone
    assert_eq!(
        drag_ik_end(&rig, &c, [50.0, 50.0]),
        Err(IkError::SegmentMissing(IkSlot::End))
    );
}

#[test]
fn stored_lengths_and_the_live_root_decide_the_pose() {
    let mut posed = limb();
    posed[0].segments[0] = [10.0, 10.0];
    posed[0].rest_segments[0] = [0.0, 0.0];
    let live = drag_ik_end(&posed, &chain_of(100.0, 100.0, false), [100.0, 0.0]).expect("pose");
    let rest = drag_ik_end(&limb(), &chain_of(100.0, 100.0, false), [100.0, 0.0]).expect("pose");
    assert!(
        !close(live.joint, rest.joint, 1.0),
        "root read from the live vertex"
    );
    // stale lengths win over the bone geometry (30/40 against the 100/100 limb)
    let stale = drag(30.0, 40.0, false, [20.0, 20.0]);
    assert!((dist([0.0, 0.0], stale.joint) - 30.0).abs() < 1e-9);
}

#[test]
fn inputs_are_never_mutated_by_any_oracle_drag() {
    for row in rows("drg") {
        let bones = parse_rig(row[1]);
        let before = bones.clone();
        let ch = chain(row[2]);
        let t = super::oracle_rows::point(row[3]);
        let _ = drag_ik_end(&bones, &ch, t);
        assert_eq!(bones, before, "{}", row[3]);
    }
}

#[test]
fn drag_writes_solve_then_mirror_the_joint_onto_the_end_bones_near_vertex() {
    let rig = limb();
    let spec = find_ik_chain(&rig, "boneB", 1).expect("chain");
    let c = set_ik(&rig, &spec.root, &spec.joint, &spec.end, false).expect("chain");
    let w = drag_writes(&rig, &spec, &c, [100.0, 0.0]).expect("writes");
    let targets: Vec<(&str, usize)> = w
        .iter()
        .map(|x| (x.target.bone_id.as_str(), x.target.vi))
        .collect();
    assert_eq!(targets, [("boneA", 1), ("boneB", 1), ("boneB", 0)]);
    assert!(
        same(w[0].point[0], w[2].point[0]) && same(w[0].point[1], w[2].point[1]),
        "joint copied bit for bit"
    );
    assert_eq!(w[1].point, [100.0, 0.0]);
    let mut short = rig.clone();
    short[1].segments.truncate(1);
    assert_eq!(
        drag_writes(&short, &spec, &c, [1.0, 1.0]),
        Err(IkError::SegmentMissing(IkSlot::End))
    );
    let mut no_near = spec.clone();
    no_near.end_near = r("boneB", 5);
    assert_eq!(
        drag_writes(&rig, &no_near, &c, [1.0, 1.0]),
        Err(IkError::SegmentMissing(IkSlot::EndNear))
    );
}

#[test]
fn hypot2_is_v8_math_hypot_bit_for_bit() {
    // (a, b, V8 Math.hypot, sqrt(a*a + b*b)): the last differs, so a naive port fails here.
    for row in [
        "c03f63924c800000 c06adca4e1cd0000 406b259d65a048f2 406b259d65a048f3",
        "406c9bb96b500000 bfdb092f70000000 406c9bbc9ce88136 406c9bbc9ce88137",
        "406643fe25190000 4051320a6d360000 4067de3059d45414 4067de3059d45415",
        "c06df51ef5600000 402cbbcdca800000 406e02e37043f7e1 406e02e37043f7e3",
        "c06dc0b8abdb0000 c07020824c5b0000 4075f0c738f7640c 4075f0c738f7640b",
        "406b097251630000 c030d138fe500000 406b1e560c49383c 406b1e560c49383b",
    ] {
        let v: Vec<f64> = row.split(' ').map(hex).collect();
        assert_eq!(hypot2(v[0], v[1]).to_bits(), v[2].to_bits(), "{row}");
        assert_ne!(
            (v[0] * v[0] + v[1] * v[1]).sqrt().to_bits(),
            v[2].to_bits(),
            "{row}"
        );
    }
    assert_eq!(hypot2(3.0, -4.0), 5.0);
    assert!(hypot2(f64::NAN, 1.0).is_nan() && hypot2(f64::NAN, f64::INFINITY) == f64::INFINITY);
    assert_eq!(hypot2(0.0, -0.0).to_bits(), 0.0f64.to_bits());
}

#[test]
fn js_min_max_follow_math_min_max_not_f64_min_max() {
    assert!(js_max(f64::NAN, 1.0).is_nan() && js_min(1.0, f64::NAN).is_nan());
    assert_eq!(
        1.0f64.max(f64::NAN),
        1.0,
        "f64::max drops the NaN, which is the trap"
    );
    assert_eq!(js_max(-0.0, 0.0).to_bits(), 0.0f64.to_bits());
    assert_eq!(js_max(0.0, -0.0).to_bits(), 0.0f64.to_bits());
    assert_eq!(js_min(0.0, -0.0).to_bits(), (-0.0f64).to_bits());
    assert_eq!(js_min(-0.0, 0.0).to_bits(), (-0.0f64).to_bits());
    assert_eq!(
        (
            js_max(2.0, 3.0),
            js_min(2.0, 3.0),
            js_max(-1.0, -2.0),
            js_min(-1.0, -2.0)
        ),
        (3.0, 2.0, -1.0, -2.0)
    );
}
