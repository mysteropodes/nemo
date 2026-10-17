//! The rig callers of the nearest-location kernel (L03-I, I12): `rigWeighOnePoint`, the radius scan of
//! `rigAutoAssignLayer` and the add-bone row of the weight popover. Oracle rows `wgh`, `rad` and `abo`
//! (grammar in `rig_pose_nearest_rows.rs`: the baseline functions run VERBATIM over Paper) plus hand pins
//! whose literals come from `tools/literals.cjs`. Straight bones go through `Curve.getTimeOf`, so the
//! pins compare with the oracle's `u`-row tolerance.

use super::{add_bone_offset, auto_assign_radius, weigh_one_point, BoneNearestError, NearestBone};
use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest::oracle_rows::{
    bones, failure, fmt, opt, point, rows_of, same_row, seglist, with_out, TOL_REL,
};
use crate::evaluation::object_image_mesh_kernels::rig_pose_nearest::NearestError;
use crate::evaluation::object_image_mesh_kernels::rig_pose_weights::OffsetWeight;
use crate::evaluation::property_follow_path_sample::SegmentIn;

pub(crate) const KINDS: [&str; 3] = ["wgh", "rad", "abo"];

/// The caller operations the rows exercise; `controls` implements it with one flaw at a time.
pub(crate) trait Ops {
    fn weigh(
        &self,
        bones: &[NearestBone],
        ids: &[String],
        p: [f64; 2],
        radius: f64,
        softness: f64,
    ) -> Result<Vec<OffsetWeight>, BoneNearestError>;
    fn radius(
        &self,
        default: Option<f64>,
        bones: &[NearestBone],
        points: &[[f64; 2]],
    ) -> Result<Option<f64>, BoneNearestError>;
}

pub(crate) struct Production;

impl Ops for Production {
    fn weigh(
        &self,
        bones: &[NearestBone],
        ids: &[String],
        p: [f64; 2],
        radius: f64,
        softness: f64,
    ) -> Result<Vec<OffsetWeight>, BoneNearestError> {
        weigh_one_point(bones, ids, p, radius, softness)
    }
    fn radius(
        &self,
        default: Option<f64>,
        bones: &[NearestBone],
        points: &[[f64; 2]],
    ) -> Result<Option<f64>, BoneNearestError> {
        auto_assign_radius(default, bones, points)
    }
}

fn weigh_out(r: Result<Vec<OffsetWeight>, BoneNearestError>) -> String {
    match r {
        Err(_) => "throw".to_string(),
        Ok(w) if w.is_empty() => "_".to_string(),
        Ok(w) => w
            .iter()
            .map(|e| {
                format!(
                    "{}:{}:{}",
                    e.bone_id,
                    fmt(e.offset.expect("offset")),
                    fmt(e.w)
                )
            })
            .collect::<Vec<_>>()
            .join(";"),
    }
}

fn radius_out(r: Result<Option<f64>, BoneNearestError>) -> String {
    match r {
        Err(_) => "throw".to_string(),
        Ok(None) => "none".to_string(),
        Ok(Some(x)) => fmt(x),
    }
}

fn points(tok: &str) -> Vec<[f64; 2]> {
    if tok == "_" {
        return Vec::new();
    }
    tok.split(';').map(point).collect()
}

/// Every `wgh` / `rad` / `abo` row that `ops` does not reproduce.
pub(crate) fn failures(ops: &dyn Ops) -> Vec<String> {
    let mut bad = Vec::new();
    for kind in KINDS {
        for (n, row) in rows_of(kind) {
            let t: Vec<&str> = row.split(' ').collect();
            let out = match kind {
                "wgh" => {
                    let ids: Vec<String> = if t[3] == "-" {
                        Vec::new()
                    } else {
                        t[3].split(',').map(str::to_string).collect()
                    };
                    let radius = opt(t[5]).unwrap_or(f64::NAN);
                    let soft = opt(t[6]).unwrap_or(0.0);
                    weigh_out(ops.weigh(&bones(t[2]), &ids, point(t[4]), radius, soft))
                }
                "rad" => radius_out(ops.radius(opt(t[3]), &bones(t[2]), &points(t[4]))),
                _ => match add_bone_offset(&bones(t[2])[0], point(t[3])) {
                    Ok(x) => fmt(x),
                    Err(BoneNearestError::Unresolved { .. }) => "unres".to_string(),
                    Err(_) => "throw".to_string(),
                },
            };
            let got = with_out(row, &out);
            if !same_row(row, &got) {
                bad.push(failure(n, row, &got));
            }
        }
    }
    bad
}

fn near(a: f64, b: f64) -> bool {
    a == b || (a - b).abs() <= TOL_REL * a.abs().max(b.abs()).max(100.0)
}

pub(crate) fn bone(id: &str, radius: Option<f64>, rest: Option<Vec<SegmentIn>>) -> NearestBone {
    NearestBone {
        id: id.to_string(),
        radius,
        closed: false,
        rest_segments: rest,
    }
}

fn seg(x: f64, y: f64) -> SegmentIn {
    SegmentIn {
        point: [x, y],
        handle_in: None,
        handle_out: None,
    }
}

pub(crate) fn line(x0: f64, y0: f64, x1: f64, y1: f64) -> Option<Vec<SegmentIn>> {
    Some(vec![seg(x0, y0), seg(x1, y1)])
}

fn arm() -> Option<Vec<SegmentIn>> {
    Some(vec![seg(0.0, 0.0), seg(120.0, 40.0), seg(240.0, 0.0)])
}

fn ids(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn weights(
    bs: &[NearestBone],
    list: &[&str],
    p: [f64; 2],
    radius: f64,
    soft: f64,
) -> Vec<(String, f64, f64)> {
    weigh_one_point(bs, &ids(list), p, radius, soft)
        .expect("resolved")
        .into_iter()
        .map(|e| (e.bone_id, e.offset.expect("offset"), e.w))
        .collect()
}

fn check(got: &[(String, f64, f64)], want: &[(&str, f64, f64)]) {
    assert_eq!(got.len(), want.len(), "{got:?}");
    for ((id, off, w), (wid, woff, ww)) in got.iter().zip(want) {
        assert_eq!(id.as_str(), *wid);
        assert!(
            near(*off, *woff) && near(*w, *ww),
            "{id}: {off} {w} vs {woff} {ww}"
        );
    }
}

#[test]
fn every_caller_oracle_row_matches_production() {
    let bad = failures(&Production);
    assert!(
        bad.is_empty(),
        "{} failed, first: {:?}",
        bad.len(),
        bad.first()
    );
}

#[test]
fn the_taper_is_measured_from_the_nearest_location_and_stores_its_offset() {
    let a = [bone("a", None, line(0.0, 0.0, 100.0, 0.0))];
    check(
        &weights(&a, &["a"], [30.0, 40.0], 200.0, 0.0),
        &[("a", 30.000000000000004, 0.8)],
    );
    // Softness blends the taper t with the smoothstep t*t*(3-2t): 0.8 -> 0.896 at softness 1.
    check(
        &weights(&a, &["a"], [30.0, 40.0], 200.0, 0.5),
        &[("a", 30.000000000000004, 0.8480000000000001)],
    );
    check(
        &weights(&a, &["a"], [30.0, 40.0], 200.0, 1.0),
        &[("a", 30.000000000000004, 0.8960000000000001)],
    );
    // Exactly on the reach, or outside it: no entry, at any softness (the taper clamps to 0 before the blend).
    assert!(weights(&a, &["a"], [30.0, 200.0], 200.0, 0.0).is_empty());
    assert!(weights(&a, &["a"], [30.0, 400.0], 200.0, 1.0).is_empty());
}

#[test]
fn bones_follow_the_id_list_and_a_curve_offset_counts_the_curves_before() {
    let two = [
        bone("a", None, line(0.0, 0.0, 100.0, 0.0)),
        bone("b", None, arm()),
    ];
    let both = [
        ("a", 59.999999999999986, 0.85),
        ("b", 66.40783086353595, 0.9525658350974743),
    ];
    check(&weights(&two, &["a", "b"], [60.0, 30.0], 200.0, 0.0), &both);
    check(
        &weights(&two, &["b", "a"], [60.0, 30.0], 200.0, 0.0),
        &[both[1], both[0]],
    );
    check(
        &weights(&two, &["b"], [200.0, 30.0], 300.0, 0.0),
        &[("b", 205.54804791094466, 0.947295372330527)],
    );
    // An unknown id is skipped, a repeated id repeats its entry, an empty list gives nothing.
    check(
        &weights(&two, &["zz", "a"], [60.0, 30.0], 200.0, 0.0),
        &[both[0]],
    );
    assert_eq!(
        weights(&two, &["a", "a"], [60.0, 30.0], 200.0, 0.0).len(),
        2
    );
    assert!(weights(&two, &[], [60.0, 30.0], 200.0, 0.0).is_empty());
}

#[test]
fn the_bone_radius_wins_unless_it_is_falsy_and_the_bind_radius_has_no_default() {
    let r50 = [bone("a", Some(50.0), line(0.0, 0.0, 100.0, 0.0))];
    check(
        &weights(&r50, &["a"], [30.0, 40.0], 200.0, 0.0),
        &[("a", 30.000000000000004, 0.19999999999999996)],
    );
    for falsy in [0.0, -0.0, f64::NAN] {
        let b = [bone("a", Some(falsy), line(0.0, 0.0, 100.0, 0.0))];
        check(
            &weights(&b, &["a"], [30.0, 40.0], 200.0, 0.0),
            &[("a", 30.000000000000004, 0.8)],
        );
    }
    // No `radius || 200` here: an undefined radius (NaN) gives NaN, which is not above zero.
    let a = [bone("a", None, line(0.0, 0.0, 100.0, 0.0))];
    assert!(weights(&a, &["a"], [30.0, 40.0], f64::NAN, 0.0).is_empty());
    let own = [bone("a", Some(80.0), line(0.0, 0.0, 100.0, 0.0))];
    assert_eq!(weights(&own, &["a"], [30.0, 40.0], f64::NAN, 0.0).len(), 1);
}

#[test]
fn a_bone_without_a_nearest_location_is_skipped_and_a_missing_list_is_a_typed_error() {
    let empty = bone("e", None, Some(Vec::new()));
    let single = bone("s", None, Some(vec![seg(5.0, 5.0)]));
    let a = bone("a", None, line(0.0, 0.0, 100.0, 0.0));
    let bs = [empty, single, a];
    check(
        &weights(&bs, &["e", "s", "a"], [30.0, 40.0], 200.0, 0.0),
        &[("a", 30.000000000000004, 0.8)],
    );
    let missing = [
        bone("m", None, None),
        bone("a", None, line(0.0, 0.0, 100.0, 0.0)),
    ];
    assert_eq!(
        weigh_one_point(&missing, &ids(&["m"]), [30.0, 40.0], 200.0, 0.0),
        Err(BoneNearestError::MissingRestSegments {
            bone_id: "m".to_string()
        })
    );
    // Bones the id list never names are never built, so a missing list there is harmless; order matters
    // for the first failure only.
    assert_eq!(weights(&missing, &["a"], [30.0, 40.0], 200.0, 0.0).len(), 1);
    assert!(weigh_one_point(&missing, &ids(&["a", "m"]), [30.0, 40.0], 200.0, 0.0).is_err());
}

#[test]
fn an_unresolved_winning_location_is_a_typed_error() {
    let huge = [bone(
        "h",
        None,
        Some(vec![
            seg(360198498172777.94, -482254505149207.3),
            seg(359787303874402.94, -481901452771109.25),
        ]),
    )];
    let q = [360520900182013.94, -481821864446099.06];
    let want = BoneNearestError::Unresolved {
        bone_id: "h".to_string(),
        error: NearestError::TimeUnresolved { curve_index: 0 },
    };
    assert_eq!(
        weigh_one_point(&huge, &ids(&["h"]), q, 200.0, 0.0),
        Err(want.clone())
    );
    assert_eq!(auto_assign_radius(None, &huge, &[q]), Err(want.clone()));
    assert_eq!(add_bone_offset(&huge[0], q), Err(want));
}

fn scan(default: Option<f64>, bs: &[NearestBone], pts: &[[f64; 2]]) -> Option<f64> {
    auto_assign_radius(default, bs, pts).expect("resolved")
}

#[test]
fn the_radius_scan_starts_at_the_default_and_grows_with_the_farthest_vertex() {
    let a = [bone("a", None, line(0.0, 0.0, 100.0, 0.0))];
    // 200 (undefined, zero, NaN, null all fall back), 300 as given, negative kept; times 1.05, ceiled.
    assert_eq!(scan(None, &a, &[]), Some(210.0));
    assert_eq!(scan(Some(0.0), &a, &[]), Some(210.0));
    assert_eq!(scan(Some(f64::NAN), &a, &[]), Some(210.0));
    assert_eq!(scan(Some(300.0), &a, &[]), Some(315.0));
    assert_eq!(scan(Some(-50.0), &a, &[]), Some(-52.0));
    // (50, 900) is 900 from the bone: 900 * 1.05 = 945.
    assert_eq!(
        scan(Some(200.0), &a, &[[10.0, 10.0], [50.0, 900.0]]),
        Some(945.0)
    );
    // A vertex nearer than the default changes nothing; (0, 100) is exactly 100 from the bone start.
    assert_eq!(
        scan(Some(200.0), &a, &[[10.0, 10.0], [90.0, -20.0]]),
        Some(210.0)
    );
    assert_eq!(scan(Some(100.0), &a, &[[0.0, 100.0]]), Some(105.0));
}

#[test]
fn only_bones_without_a_radius_of_their_own_are_scanned() {
    let own = [bone("a", Some(40.0), line(0.0, 0.0, 100.0, 0.0))];
    assert_eq!(
        scan(Some(200.0), &own, &[[50.0, 900.0]]),
        Some(210.0),
        "a bone radius skips the bone"
    );
    for falsy in [0.0, -0.0, f64::NAN] {
        let b = [bone("a", Some(falsy), line(0.0, 0.0, 100.0, 0.0))];
        assert_eq!(
            scan(Some(200.0), &b, &[[50.0, 900.0]]),
            Some(945.0),
            "{falsy}"
        );
    }
    let negative = [bone("a", Some(-3.0), line(0.0, 0.0, 100.0, 0.0))];
    assert_eq!(
        scan(Some(200.0), &negative, &[[50.0, 900.0]]),
        Some(210.0),
        "a negative radius is truthy"
    );
    // Each point takes the largest distance over the unskipped bones (the NEAREST point of each bone).
    let two = [
        bone("a", None, line(0.0, 0.0, 100.0, 0.0)),
        bone("b", None, line(0.0, 500.0, 100.0, 500.0)),
    ];
    assert_eq!(scan(Some(50.0), &two, &[[50.0, 250.0]]), Some(263.0));
    // No bones: the baseline returns before the scan.
    assert_eq!(scan(Some(200.0), &[], &[[50.0, 250.0]]), None);
}

#[test]
fn the_scan_builds_a_bone_only_when_it_is_used() {
    let missing = [bone("m", None, None)];
    assert_eq!(
        scan(Some(200.0), &missing, &[]),
        Some(210.0),
        "no point, no path built"
    );
    assert_eq!(
        auto_assign_radius(Some(200.0), &missing, &[[1.0, 1.0]]),
        Err(BoneNearestError::MissingRestSegments {
            bone_id: "m".to_string()
        })
    );
    let skipped = [bone("m", Some(10.0), None)];
    assert_eq!(
        scan(Some(200.0), &skipped, &[[1.0, 1.0]]),
        Some(210.0),
        "a radius skips before the path"
    );
}

#[test]
fn the_add_bone_row_stores_the_offset_or_zero() {
    let b = bone("b", None, arm());
    let off = add_bone_offset(&b, [200.0, 30.0]).expect("resolved");
    assert!(near(off, 205.54804791094466), "{off}");
    assert_eq!(
        add_bone_offset(&bone("e", None, Some(Vec::new())), [30.0, 40.0]),
        Ok(0.0)
    );
    assert_eq!(
        add_bone_offset(&bone("m", None, None), [30.0, 40.0]),
        Err(BoneNearestError::MissingRestSegments {
            bone_id: "m".to_string()
        })
    );
    assert!(
        seglist("_").is_some_and(|s| s.is_empty()),
        "the harness reads an empty list"
    );
}
