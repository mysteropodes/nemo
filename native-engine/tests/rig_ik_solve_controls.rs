//! Negative and mutation controls for the two-bone IK kernel (L03-C). Each `Flaw` is one plausible
//! mistake (a JavaScript mutant the generator proved the oracle catches, or a Rust-only trap such
//! as `f64::min/max` dropping NaN). The flawed copies run through the SAME row harness as
//! production; with `Flaw::None` the copies must equal production, so a rejection is caused by the
//! flaw alone.

use std::cell::Cell;

use super::oracle_rows::{rows, run, Ops, Tally, PRODUCTION};
use super::{
    anchor_point, distance, drag_writes, hypot2, js_max, js_min, other_end, set_ik, IkBone,
    IkChain, IkChainSpec, IkError, IkPose, IkRef, IkSlot, Point2, PointWrite,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Flaw {
    None,
    // solve_raw / drag
    DistUnclamped,
    EndAtTarget,
    FlipInverted,
    CosUnclamped,
    CosNoUpperClamp,
    CosNoLowerClamp,
    MinDSigned,
    EpsLarge,
    EpsZero,
    EpsNearDropped,
    EpsFarDropped,
    Atan2Swapped,
    JointUsesL2,
    StdMinMax,
    RootFromRest,
    NoFiniteGuard,
    NoVertexGuard,
    // anchor / set
    AnchorRestOnly,
    AnchorLiveOnly,
    AnchorNoRestGuard,
    SwapLengths,
    DropFlip,
    NaiveHypot,
    // find
    FindTol1,
    FindTol3,
    FindLe,
    FindClosedEndOk,
    FindShortEndOk,
    FindInteriorOk,
    FindSelfOk,
    FindLastWins,
    FindCandClosedOk,
    FindCandShortOk,
    FindOtherEndIdentity,
    FindRootViIsJoint,
    FindNearIsEnd,
    FindEmptyIdTruthy,
    FindFirstIndexOnly,
    FindLastIndexOnly,
    // drag step writes
    NoMirror,
    MirrorEnd,
    JointDropped,
}

thread_local! {
    static FLAW: Cell<Flaw> = Cell::new(Flaw::None);
}

fn flaw() -> Flaw {
    FLAW.with(|c| c.get())
}

fn f_anchor(bones: &[IkBone], r: &IkRef) -> Option<Point2> {
    let bone = bones.iter().find(|b| b.id == r.bone_id)?;
    match flaw() {
        Flaw::AnchorRestOnly => bone.rest_segments.get(r.vi).copied(),
        Flaw::AnchorLiveOnly => bone.segments.get(r.vi).copied(),
        Flaw::AnchorNoRestGuard => bone
            .segments
            .get(r.vi)
            .or_else(|| bone.rest_segments.get(r.vi))
            .copied(),
        _ => anchor_point(bones, r),
    }
}

fn f_set(
    bones: &[IkBone],
    root: &IkRef,
    joint: &IkRef,
    end: &IkRef,
    flip: bool,
) -> Result<IkChain, IkError> {
    let f = flaw();
    let mut c = set_ik(bones, root, joint, end, flip && f != Flaw::DropFlip)?;
    match f {
        Flaw::SwapLengths => {
            let (a, b) = (c.l1, c.l2);
            c.l1 = b;
            c.l2 = a;
        }
        Flaw::NaiveHypot => {
            let p = |r: &IkRef| anchor_point(bones, r).expect("anchor");
            let (rp, jp, ep) = (p(root), p(joint), p(end));
            let naive = |a: Point2, b: Point2| {
                ((b[0] - a[0]) * (b[0] - a[0]) + (b[1] - a[1]) * (b[1] - a[1])).sqrt()
            };
            c.l1 = naive(rp, jp);
            c.l2 = naive(jp, ep);
        }
        _ => {}
    }
    Ok(c)
}

fn f_raw(root: Point2, l1: f64, l2: f64, flip: bool, target: Point2) -> IkPose {
    let f = flaw();
    let (dx, dy) = (target[0] - root[0], target[1] - root[1]);
    let dist = hypot2(dx, dy);
    let max_d = l1 + l2;
    let min_d = if f == Flaw::MinDSigned {
        l1 - l2
    } else {
        (l1 - l2).abs()
    };
    let (e_near, e_far) = match f {
        Flaw::EpsLarge => (1e-5, 1e-5),
        Flaw::EpsZero => (0.0, 0.0),
        Flaw::EpsNearDropped => (0.0, 1e-6),
        Flaw::EpsFarDropped => (1e-6, 0.0),
        _ => (1e-6, 1e-6),
    };
    let clamped = match f {
        Flaw::DistUnclamped => dist,
        Flaw::StdMinMax => (min_d + e_near).max(dist.min(max_d - e_far)),
        _ => js_max(min_d + e_near, js_min(max_d - e_far, dist)),
    };
    let base = if f == Flaw::Atan2Swapped {
        dx.atan2(dy)
    } else {
        dy.atan2(dx)
    };
    let q = (l1 * l1 + clamped * clamped - l2 * l2) / (2.0 * l1 * clamped);
    let cos_a = match f {
        Flaw::CosUnclamped => q,
        Flaw::CosNoUpperClamp => js_max(-1.0, q),
        Flaw::CosNoLowerClamp => js_min(1.0, q),
        Flaw::StdMinMax => (-1.0f64).max(1.0f64.min(q)),
        _ => js_max(-1.0, js_min(1.0, q)),
    };
    let a = cos_a.acos();
    let minus = if f == Flaw::FlipInverted { !flip } else { flip };
    let angle = base + if minus { -a } else { a };
    let reach = if f == Flaw::JointUsesL2 { l2 } else { l1 };
    IkPose {
        joint: [root[0] + reach * angle.cos(), root[1] + reach * angle.sin()],
        end: if f == Flaw::EndAtTarget {
            target
        } else {
            [
                root[0] + clamped * base.cos(),
                root[1] + clamped * base.sin(),
            ]
        },
    }
}

fn point_of(bones: &[IkBone], r: &IkRef, rest: bool) -> Option<Point2> {
    let b = bones.iter().find(|b| b.id == r.bone_id)?;
    let list = if rest { &b.rest_segments } else { &b.segments };
    list.get(r.vi).copied()
}

fn f_drag(bones: &[IkBone], chain: &IkChain, target: Point2) -> Result<IkPose, IkError> {
    let f = flaw();
    let root = point_of(bones, &chain.root, f == Flaw::RootFromRest)
        .ok_or(IkError::SegmentMissing(IkSlot::Root))?;
    if f != Flaw::NoVertexGuard {
        point_of(bones, &chain.joint, false).ok_or(IkError::SegmentMissing(IkSlot::Joint))?;
        point_of(bones, &chain.end, false).ok_or(IkError::SegmentMissing(IkSlot::End))?;
    }
    let pose = f_raw(root, chain.l1, chain.l2, chain.flip, target);
    let finite = pose
        .joint
        .iter()
        .chain(pose.end.iter())
        .all(|v| v.is_finite());
    if f != Flaw::NoFiniteGuard && !finite {
        return Err(IkError::NonFinitePose);
    }
    Ok(pose)
}

fn f_find(bones: &[IkBone], bone_id: &str, vi: usize) -> Option<IkChainSpec> {
    let f = flaw();
    let end_bone = bones.iter().find(|b| b.id == bone_id)?;
    let min_len = if f == Flaw::FindShortEndOk { 1 } else { 2 };
    let len = end_bone.segments.len();
    if (end_bone.closed && f != Flaw::FindClosedEndOk) || len < min_len {
        return None;
    }
    if f != Flaw::FindInteriorOk && vi != 0 && vi != len - 1 {
        return None;
    }
    let near_idx = if f == Flaw::FindOtherEndIdentity {
        vi.min(len - 1)
    } else {
        other_end(len, vi)
    };
    let near = end_bone.segments[near_idx];
    let tol = match f {
        Flaw::FindTol1 => 1.0,
        Flaw::FindTol3 => 3.0,
        _ => 2.0,
    };
    let truthy = f == Flaw::FindEmptyIdTruthy;
    let is_set =
        |r: &Option<(&IkBone, usize)>| r.map_or(false, |(b, _)| truthy || !b.id.is_empty());
    let mut root: Option<(&IkBone, usize)> = None;
    for bone in bones {
        let cand_min = if f == Flaw::FindCandShortOk { 1 } else { 2 };
        if (bone.id == bone_id && f != Flaw::FindSelfOk)
            || (f != Flaw::FindLastWins && is_set(&root))
        {
            continue;
        }
        if (bone.closed && f != Flaw::FindCandClosedOk) || bone.segments.len() < cand_min {
            continue;
        }
        let last = bone.segments.len() - 1;
        let ends: Vec<usize> = match f {
            Flaw::FindFirstIndexOnly => vec![0],
            Flaw::FindLastIndexOnly => vec![last],
            _ => vec![0, last],
        };
        for i in ends {
            if f != Flaw::FindLastWins && is_set(&root) {
                continue;
            }
            let d = distance(bone.segments[i], near);
            if d < tol || (f == Flaw::FindLe && d <= tol) {
                root = Some((bone, i));
            }
        }
    }
    let (root_bone, joint_idx) = root.filter(|(b, _)| truthy || !b.id.is_empty())?;
    let make = |id: &str, vi: usize| IkRef {
        bone_id: id.to_string(),
        vi,
    };
    let root_vi = if f == Flaw::FindRootViIsJoint {
        joint_idx
    } else {
        other_end(root_bone.segments.len(), joint_idx)
    };
    Some(IkChainSpec {
        root: make(&root_bone.id, root_vi),
        joint: make(&root_bone.id, joint_idx),
        end_near: make(
            bone_id,
            if f == Flaw::FindNearIsEnd {
                vi
            } else {
                near_idx
            },
        ),
        end: make(bone_id, vi),
    })
}

fn f_writes(
    bones: &[IkBone],
    spec: &IkChainSpec,
    chain: &IkChain,
    target: Point2,
) -> Result<Vec<PointWrite>, IkError> {
    let mut w = drag_writes(bones, spec, chain, target)?;
    match flaw() {
        Flaw::NoMirror => {
            w.pop();
        }
        Flaw::MirrorEnd => {
            let end = w[1].point;
            w[2].point = end;
        }
        Flaw::JointDropped => {
            w.remove(0);
        }
        _ => {}
    }
    Ok(w)
}

const FLAWED: Ops = Ops {
    anchor: f_anchor,
    set: f_set,
    raw: f_raw,
    drag: f_drag,
    find: f_find,
    writes: f_writes,
};

/// Which tally field a flaw must show up in.
#[derive(Debug, Clone, Copy)]
enum Field {
    Anc,
    Set,
    Raw,
    Drag,
    Fnd,
    Flw,
}

fn count(t: &Tally, field: Field) -> usize {
    match field {
        Field::Anc => t.anc,
        Field::Set => t.set,
        Field::Raw => t.raw,
        Field::Drag => t.drag,
        Field::Fnd => t.fnd,
        Field::Flw => t.flw,
    }
}

fn run_with(f: Flaw) -> Tally {
    FLAW.with(|c| c.set(f));
    let t = run(&FLAWED);
    FLAW.with(|c| c.set(Flaw::None));
    t
}

const FLAWS: [(Flaw, Field); 42] = [
    (Flaw::DistUnclamped, Field::Raw),
    (Flaw::EndAtTarget, Field::Raw),
    (Flaw::FlipInverted, Field::Raw),
    (Flaw::CosUnclamped, Field::Raw),
    (Flaw::CosNoUpperClamp, Field::Raw),
    (Flaw::CosNoLowerClamp, Field::Raw),
    (Flaw::MinDSigned, Field::Raw),
    (Flaw::EpsLarge, Field::Raw),
    (Flaw::EpsZero, Field::Raw),
    (Flaw::EpsNearDropped, Field::Raw),
    (Flaw::EpsFarDropped, Field::Raw),
    (Flaw::Atan2Swapped, Field::Raw),
    (Flaw::JointUsesL2, Field::Raw),
    (Flaw::StdMinMax, Field::Raw),
    (Flaw::RootFromRest, Field::Drag),
    (Flaw::NoFiniteGuard, Field::Drag),
    (Flaw::NoVertexGuard, Field::Drag),
    (Flaw::AnchorRestOnly, Field::Anc),
    (Flaw::AnchorLiveOnly, Field::Anc),
    (Flaw::AnchorNoRestGuard, Field::Anc),
    (Flaw::SwapLengths, Field::Set),
    (Flaw::DropFlip, Field::Set),
    (Flaw::NaiveHypot, Field::Set),
    (Flaw::FindTol1, Field::Fnd),
    (Flaw::FindTol3, Field::Fnd),
    (Flaw::FindLe, Field::Fnd),
    (Flaw::FindClosedEndOk, Field::Fnd),
    (Flaw::FindShortEndOk, Field::Fnd),
    (Flaw::FindInteriorOk, Field::Fnd),
    (Flaw::FindSelfOk, Field::Fnd),
    (Flaw::FindLastWins, Field::Fnd),
    (Flaw::FindCandClosedOk, Field::Fnd),
    (Flaw::FindCandShortOk, Field::Fnd),
    (Flaw::FindOtherEndIdentity, Field::Fnd),
    (Flaw::FindRootViIsJoint, Field::Fnd),
    (Flaw::FindNearIsEnd, Field::Fnd),
    (Flaw::FindEmptyIdTruthy, Field::Fnd),
    (Flaw::FindFirstIndexOnly, Field::Fnd),
    (Flaw::FindLastIndexOnly, Field::Fnd),
    (Flaw::NoMirror, Field::Flw),
    (Flaw::MirrorEnd, Field::Flw),
    (Flaw::JointDropped, Field::Flw),
];

#[test]
fn the_flaw_free_copies_equal_production_on_every_row() {
    assert_eq!(run_with(Flaw::None), run(&PRODUCTION));
    assert_eq!(run_with(Flaw::None).wrong(), 0);
}

#[test]
fn every_flaw_is_rejected_by_the_op_it_breaks() {
    for (flaw, field) in FLAWS {
        let t = run_with(flaw);
        assert!(
            count(&t, field) > 0,
            "{flaw:?} survived in {field:?}: {t:?}"
        );
    }
}

#[test]
fn rust_only_traps_are_caught_by_non_finite_rows() {
    // f64::min/max drop a NaN operand where Math.min/max keep it, so a NaN target "works" in Rust.
    let t = run_with(Flaw::StdMinMax);
    assert!(t.raw > 0 && t.drag > 0, "{t:?}");
    // a pose with NaN is refused only because of the finiteness guard
    let t = run_with(Flaw::NoFiniteGuard);
    assert!(t.drag >= t.non_finite, "{t:?}");
}

#[test]
fn constant_functions_cannot_pass_the_rows() {
    let none = Ops {
        anchor: |_, _| None,
        ..PRODUCTION
    };
    assert_eq!(run(&none).anc, 13);
    let never = Ops {
        find: |_, _, _| None,
        ..PRODUCTION
    };
    assert_eq!(run(&never).fnd, 23);
    assert_eq!(run(&never).flw, 19);
    let refuse = Ops {
        set: |_, _, _, _, _| Err(IkError::AnchorMissing(IkSlot::Root)),
        ..PRODUCTION
    };
    assert_eq!(run(&refuse).set, 46);
}

#[test]
fn the_oracle_is_not_vacuous() {
    let count_where = |op: &str, i: usize, pred: &dyn Fn(&str) -> bool| {
        rows(op).iter().filter(|r| pred(r[i])).count()
    };
    assert_eq!(
        (
            count_where("anc", 3, &|s| s == "-"),
            count_where("anc", 3, &|s| s != "-")
        ),
        (14, 13)
    );
    assert_eq!(
        (
            count_where("set", 6, &|s| s == "-"),
            count_where("set", 6, &|s| s != "-")
        ),
        (3, 46)
    );
    assert_eq!(
        (
            count_where("fnd", 4, &|s| s == "-"),
            count_where("fnd", 4, &|s| s != "-")
        ),
        (24, 23)
    );
    assert_eq!(
        (
            count_where("flw", 5, &|s| s == "-"),
            count_where("flw", 5, &|s| s == "x")
        ),
        (1, 1)
    );
    assert_eq!(count_where("drg", 4, &|s| s.starts_with("throw:")), 6);
    assert_eq!(count_where("drg", 4, &|s| s.starts_with("ok:")), 302);
}
