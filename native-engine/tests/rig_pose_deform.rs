//! Hand fixtures of the rig deform kernel (L03-E, I12). Every expected number is the output of the
//! VERBATIM baseline `applyRigDeform` (fbe4b8a3, real Paper.js) on the same scenario, printed by
//! `tools/hand.cjs`. Exact asserts where no trig is involved, `1e-9` where `cos`/`sin`/`atan2` are.

use super::{apply_rig_deform, BindInput, BindPose, PathBind, PathPose, RestHandle};
use crate::evaluation::object_image_mesh_kernels::rig_pose_deform_anchor::{
    AnchorResolver, DeformBone, DeformSegment,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::{
    ControlWeight, OffsetWeight, Point2, WeightEntry,
};

pub(super) fn dseg(p: Point2) -> DeformSegment {
    DeformSegment {
        point: Some(p),
        handle_in: None,
        handle_out: None,
    }
}
pub(super) fn bone(id: &str, closed: bool, rest: &[Point2], live: &[Point2]) -> DeformBone {
    DeformBone {
        id: id.to_string(),
        closed,
        segments: Some(live.iter().map(|p| dseg(*p)).collect()),
        rest_segments: Some(rest.iter().map(|p| dseg(*p)).collect()),
    }
}
/// The straight bone of the contract fixture, turned 90 degrees about its first control point.
pub(super) fn arm() -> DeformBone {
    bone(
        "b1",
        false,
        &[[0.0, 0.0], [100.0, 0.0], [200.0, 0.0]],
        &[[0.0, 0.0], [0.0, 100.0], [0.0, 200.0]],
    )
}
pub(super) fn ctl(b: &str, pt: usize, w: f64) -> WeightEntry {
    WeightEntry::Control(ControlWeight {
        bone_id: b.to_string(),
        pt_index: pt,
        w,
        stray_offset: None,
    })
}
pub(super) fn off(b: &str, offset: Option<f64>, w: f64) -> WeightEntry {
    WeightEntry::Offset(OffsetWeight {
        bone_id: b.to_string(),
        offset,
        w,
    })
}
pub(super) fn path<'a>(
    rest: &'a [Point2],
    weights: &'a [Vec<WeightEntry>],
    rotate: bool,
    live: usize,
) -> PathBind<'a> {
    PathBind {
        rest,
        rest_handles: None,
        weights,
        rotate,
        live_len: Some(live),
        center: None,
    }
}
pub(super) fn run_path(bones: &[DeformBone], bind: PathBind) -> PathPose {
    match apply_rig_deform(bones, &[BindInput::Path(bind)]) {
        Ok(mut v) => match v.pop() {
            Some(BindPose::Path(p)) => p,
            other => panic!("not a path pose: {other:?}"),
        },
        Err(e) => panic!("deform failed: {e:?}"),
    }
}
pub(super) fn points(p: &PathPose) -> Vec<Point2> {
    p.segments.iter().map(|s| s.point).collect()
}
pub(super) fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9
}
pub(super) fn near2(a: Point2, b: Point2) -> bool {
    near(a[0], b[0]) && near(a[1], b[1])
}

pub(super) const V3: [Point2; 3] = [[100.0, 0.0], [100.0, 40.0], [50.0, 0.0]];
pub(super) fn w3() -> Vec<Vec<WeightEntry>> {
    vec![
        vec![ctl("b1", 1, 1.0)],
        vec![ctl("b1", 1, 1.0)],
        vec![ctl("b1", 0, 0.5), ctl("b1", 1, 0.5)],
    ]
}

#[test]
fn non_rotate_moves_each_point_by_the_weighted_control_point_translation() {
    let w = w3();
    let p = run_path(&[arm()], path(&V3, &w, false, 3));
    assert_eq!(points(&p), vec![[0.0, 100.0], [0.0, 140.0], [0.0, 50.0]]);
    assert!(p.center.is_none());
    assert!(p
        .segments
        .iter()
        .all(|s| s.handle_in.is_none() && s.handle_out.is_none()));
}

#[test]
fn rotate_turns_the_rest_offset_by_the_chord_angle_delta() {
    let w = w3();
    let p = run_path(&[arm()], path(&V3, &w, true, 3));
    let got = points(&p);
    // (100,40) sits 40 above control point 1: a quarter turn puts it 40 to the left of the moved point
    assert!(
        near2(got[0], [0.0, 100.0]) && near2(got[1], [-40.0, 100.0]) && near2(got[2], [0.0, 50.0])
    );
}

#[test]
fn sum_w_above_one_normalizes_and_below_one_does_not() {
    let rest = [[100.0, 0.0]];
    let over = vec![vec![ctl("b1", 1, 1.5), ctl("b1", 1, 1.5)]];
    let under = vec![vec![ctl("b1", 1, 0.5)]];
    let p = run_path(&[arm()], path(&rest, &over, false, 1));
    assert_eq!(points(&p), vec![[0.0, 100.0]]); // 3 * (-100, 100) / 3
    let p = run_path(&[arm()], path(&rest, &under, false, 1));
    assert_eq!(points(&p), vec![[50.0, 50.0]]); // half the pull, divided by 1
}

#[test]
fn unresolved_entries_are_skipped_and_not_folded_into_the_weight_sum() {
    let rest = [[100.0, 0.0]];
    // a bone the rig lacks and a point index past the bone: both null, only the 0.5 pull remains
    let w = vec![vec![
        ctl("zz", 0, 0.9),
        ctl("b1", 1, 0.5),
        ctl("b1", 9, 0.7),
    ]];
    let p = run_path(&[arm()], path(&rest, &w, false, 1));
    assert_eq!(points(&p), vec![[50.0, 50.0]]);
}

fn handles() -> Vec<RestHandle> {
    vec![
        RestHandle {
            handle_in: None,
            handle_out: Some([30.0, 0.0]),
        },
        RestHandle {
            handle_in: Some([-30.0, 0.0]),
            handle_out: Some([30.0, 0.0]),
        },
        RestHandle {
            handle_in: None,
            handle_out: None,
        },
    ]
}

#[test]
fn rest_handles_rotate_by_the_average_angle_only_with_rotation_weight() {
    let (w, rh) = (w3(), handles());
    let mut bind = path(&V3, &w, true, 3);
    bind.rest_handles = Some(&rh);
    let p = run_path(&[arm()], bind);
    let s = &p.segments;
    // avgDa = pi/2: (30,0) -> (0,30), (-30,0) -> (0,-30)
    assert!(s[0].handle_in.is_none() && near2(s[0].handle_out.expect("out"), [0.0, 30.0]));
    assert!(near2(s[1].handle_in.expect("in"), [0.0, -30.0]));
    assert!(near2(s[1].handle_out.expect("out"), [0.0, 30.0]));
    assert!(s[2].handle_in.is_none() && s[2].handle_out.is_none());
}

#[test]
fn rest_handles_are_copied_verbatim_without_rotate_or_without_weights() {
    let (w, rh) = (w3(), handles());
    let mut plain = path(&V3, &w, false, 3);
    plain.rest_handles = Some(&rh);
    let p = run_path(&[arm()], plain);
    assert_eq!(p.segments[0].handle_out, Some([30.0, 0.0]));
    assert_eq!(p.segments[1].handle_in, Some([-30.0, 0.0]));
    // rotate on, but no entry contributed rotation (sumWRot = 0): the same verbatim copy
    let none: Vec<Vec<WeightEntry>> = vec![vec![], vec![], vec![]];
    let mut idle = path(&V3, &none, true, 3);
    idle.rest_handles = Some(&rh);
    let p = run_path(&[arm()], idle);
    assert_eq!(points(&p), V3.to_vec());
    assert_eq!(p.segments[1].handle_in, Some([-30.0, 0.0]));
    assert_eq!(p.segments[1].handle_out, Some([30.0, 0.0]));
}

#[test]
fn absent_or_short_rest_handles_leave_handles_untouched() {
    let (w, rh) = (w3(), handles());
    let p = run_path(&[arm()], path(&V3, &w, true, 3));
    assert!(p
        .segments
        .iter()
        .all(|s| s.handle_in.is_none() && s.handle_out.is_none()));
    let mut short = path(&V3, &w, true, 3);
    short.rest_handles = Some(&rh[..1]);
    let p = run_path(&[arm()], short);
    assert!(p.segments[0].handle_out.is_some());
    assert!(p.segments[1].handle_in.is_none() && p.segments[1].handle_out.is_none());
}

#[test]
fn loops_stop_at_the_shorter_of_the_live_segments_and_the_rest_points() {
    let w = w3();
    assert_eq!(
        run_path(&[arm()], path(&V3, &w, false, 2)).segments.len(),
        2
    );
    assert_eq!(
        run_path(&[arm()], path(&V3, &w, false, 9)).segments.len(),
        3
    );
    assert_eq!(
        run_path(&[arm()], path(&V3, &w[..1], false, 3)).segments[2].point,
        [50.0, 0.0]
    );
    assert!(run_path(&[arm()], path(&[], &[], false, 4))
        .segments
        .is_empty());
}

fn flip() -> DeformBone {
    bone(
        "f1",
        false,
        &[[0.0, 0.0], [-100.0, -25.0]],
        &[[0.0, 0.0], [-100.0, 25.0]],
    )
}

#[test]
fn offset_model_anchors_on_the_bone_curve_and_its_angle_is_not_wrapped() {
    let bones = [flip()];
    let mut resolver = AnchorResolver::new(&bones);
    let a = resolver
        .resolve(&off("f1", Some(20.0), 1.0))
        .expect("ok")
        .expect("located");
    let raw = 25f64.atan2(-100.0) - (-25f64).atan2(-100.0);
    assert!(near(a.da, 5.793227980925858) && near(a.da, raw) && a.da > std::f64::consts::PI);
    let c = resolver
        .resolve(&ctl("f1", 0, 1.0))
        .expect("ok")
        .expect("located");
    assert!(near(c.da, raw - 2.0 * std::f64::consts::PI)); // the control-point model wraps
                                                           // the placed point: 20 along each bone
    let rest = [-50.0, -10.0];
    let w = vec![vec![off("f1", Some(20.0), 1.0)]];
    let p = run_path(&bones, path(std::slice::from_ref(&rest), &w, true, 1));
    assert!(near2(
        p.segments[0].point,
        [-48.8235294117647, 14.705882352941188]
    ));
    let p = run_path(&bones, path(std::slice::from_ref(&rest), &w, false, 1));
    assert!(near2(p.segments[0].point, [-50.0, -0.29857499854668035]));
}

#[test]
fn offset_model_matches_the_baseline_on_the_arm() {
    let rest = [[100.0, 40.0]];
    let w = vec![vec![off("b1", Some(100.0), 1.0)]];
    let p = run_path(&[arm()], path(&rest, &w, true, 1));
    assert!(near2(p.segments[0].point, [-40.0, 100.0]));
    // an offset past the rest bone's end is null and skipped; the 0.5 control pull stands alone
    let w = vec![vec![off("b1", Some(250.0), 1.0), ctl("b1", 1, 0.5)]];
    let p = run_path(&[arm()], path(&rest, &w, false, 1));
    assert_eq!(points(&p), vec![[50.0, 90.0]]);
    // NaN and an absent offset are null too
    let w = vec![vec![
        off("b1", Some(f64::NAN), 1.0),
        off("b1", None, 1.0),
        ctl("b1", 1, 0.5),
    ]];
    let p = run_path(&[arm()], path(&rest, &w, false, 1));
    assert_eq!(points(&p), vec![[50.0, 90.0]]);
}

#[test]
fn closed_bones_run_back_to_their_first_segment() {
    let sq = bone(
        "s1",
        true,
        &[[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0]],
        &[[10.0, 10.0], [130.0, 10.0], [130.0, 130.0], [10.0, 130.0]],
    );
    let rest = [[50.0, 5.0]];
    let at = |o: f64, extra: Option<WeightEntry>| {
        let mut l = vec![off("s1", Some(o), 1.0)];
        l.extend(extra);
        points(&run_path(
            std::slice::from_ref(&sq),
            path(&rest, &[l], false, 1),
        ))
    };
    assert_eq!(at(150.0, None), vec![[80.0, -5.0]]);
    assert_eq!(at(400.0, None), vec![[60.0, 95.0]]); // the rest path's full length reads curve 0 at time 0
    assert_eq!(at(401.0, Some(ctl("s1", 2, 0.5))), vec![[65.0, 20.0]]); // past the rest end: skipped
}
