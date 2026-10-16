//! Oracle tests for the camera track kernel (K-CAM).
//!
//! `fixtures/property-camera-eval/oracle.txt` (`ORACLE`, sha256
//! 2d3bdbc4aa7b60d8fcf81a10ddfd8b3345f5f32aeab546b106f6e80bee214e8f) was produced by running `bezierEase`,
//! `snap`, `cameraAtFrameFor` and `cameraMatrixAtFrame`, cut verbatim from
//! `src/js/camera.js` at 1f663ac3 against the real Paper.js 0.12.18 `Matrix`
//! (generator `gen.js`, kept outside the repo).
//!
//! Rows (space separated, f64 as 16 hex digits, `!` absent, `-` null):
//! `kcam1 cam KEYS FRAME CW CH POSE MAT` and `kcam1 bez T EASE(4 joined by .) Y`.
//! KEYS is `^` null, `!` undefined, `.` empty, else `frame,x,y,w,rot,ease,hOut,hIn`
//! keys joined by `;`. Numbers compare on bits; two NaNs match (payload ignored).
//! One exception: when the pose has a roll, the matrix goes through `cos`/`sin`,
//! whose last ulp is libm-dependent (macOS differs from the Linux/V8 oracle),
//! so each matrix group may differ by `TRIG_ULPS` units of roundoff of its
//! largest entry. Poses, rows without roll and `bez` rows stay bit-exact.

use crate::evaluation::property_camera_eval::{
    bezier_ease, camera_at_frame, camera_matrix_at_frame, CameraKey, CameraPose, DEFAULT_EASE,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/property-camera-eval/oracle.txt");

fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

fn opt<T>(tok: &str, f: impl Fn(&str) -> T) -> Option<T> {
    (tok != "!").then(|| f(tok))
}

fn arr<const N: usize>(tok: &str) -> [f64; N] {
    let v: Vec<f64> = tok.split('.').map(hex).collect();
    v.try_into().expect("array length")
}

fn key(tok: &str) -> CameraKey {
    let f: Vec<&str> = tok.split(',').collect();
    let num = |s: &str| if s == "!" { f64::NAN } else { hex(s) };
    CameraKey {
        frame: num(f[0]),
        x: num(f[1]),
        y: num(f[2]),
        w: num(f[3]),
        rot: opt(f[4], hex),
        ease: opt(f[5], arr::<4>),
        h_out: opt(f[6], arr::<2>),
        h_in: opt(f[7], arr::<2>),
    }
}

fn keys(tok: &str) -> Option<Vec<CameraKey>> {
    match tok {
        "^" | "!" => None,
        "." => Some(vec![]),
        _ => Some(tok.split(';').map(key).collect()),
    }
}

struct CamRow {
    keys: Option<Vec<CameraKey>>,
    frame: f64,
    cw: f64,
    ch: f64,
    pose: Option<[f64; 4]>,
    mat: Option<[f64; 6]>,
}

fn cam_rows() -> Vec<CamRow> {
    ORACLE
        .lines()
        .filter_map(|l| {
            let t: Vec<&str> = l.split(' ').collect();
            (t[1] == "cam").then(|| CamRow {
                keys: keys(t[2]),
                frame: hex(t[3]),
                cw: hex(t[4]),
                ch: hex(t[5]),
                pose: (t[6] != "-").then(|| arr_c::<4>(t[6])),
                mat: (t[7] != "-").then(|| arr_c::<6>(t[7])),
            })
        })
        .collect()
}

fn arr_c<const N: usize>(tok: &str) -> [f64; N] {
    let v: Vec<f64> = tok.split(',').map(hex).collect();
    v.try_into().expect("array length")
}

fn pose_arr(p: CameraPose) -> [f64; 4] {
    [p.x, p.y, p.w, p.rot]
}

fn all_same(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(*x, *y))
}

const TRIG_ULPS: f64 = 4.0;

/// Bit-equal, or a finite rounding difference within `TRIG_ULPS` of the
/// group's magnitude (a signed-zero mismatch never passes).
fn trig_near(a: &[f64], b: &[f64]) -> bool {
    let mag = a
        .iter()
        .chain(b)
        .filter(|x| x.is_finite())
        .fold(0.0_f64, |m, x| m.max(x.abs()));
    a.len() == b.len()
        && a.iter().zip(b).all(|(&x, &y)| {
            same(x, y)
                || (x.is_finite()
                    && y.is_finite()
                    && x != y
                    && (x - y).abs() <= TRIG_ULPS * mag * f64::EPSILON)
        })
}

/// Linear part and translation are separate magnitude groups.
fn mat_matches(g: &[f64; 6], w: &[f64; 6], rolled: bool) -> bool {
    if rolled {
        trig_near(&g[..4], &w[..4]) && trig_near(&g[4..], &w[4..])
    } else {
        all_same(g, w)
    }
}

fn k(frame: f64, x: f64, y: f64, w: f64) -> CameraKey {
    CameraKey {
        frame,
        x,
        y,
        w,
        rot: None,
        h_out: None,
        h_in: None,
        ease: None,
    }
}

fn linear(mut key: CameraKey) -> CameraKey {
    key.ease = Some([0.0, 0.0, 1.0, 1.0]);
    key
}

fn pose(ks: &[CameraKey], f: f64) -> CameraPose {
    camera_at_frame(Some(ks), f).expect("pose")
}

#[test]
fn oracle_has_expected_shape() {
    assert_eq!(ORACLE.lines().count(), 229);
    assert_eq!(cam_rows().len(), 133);
}

#[test]
fn every_cam_row_matches_the_oracle() {
    let mut n = 0;
    for (i, r) in cam_rows().iter().enumerate() {
        let ks = r.keys.as_deref();
        let got = camera_at_frame(ks, r.frame).map(pose_arr);
        match (&got, &r.pose) {
            (None, None) => {}
            (Some(g), Some(w)) => assert!(all_same(g, w), "row {i} pose {g:?} != {w:?}"),
            _ => panic!("row {i} pose presence {got:?} vs {:?}", r.pose),
        }
        let m = camera_matrix_at_frame(ks, r.frame, r.cw, r.ch);
        match (&m, &r.mat) {
            (None, None) => {}
            (Some(g), Some(w)) => {
                let rolled = got.is_some_and(|p| p[3] != 0.0 && !p[3].is_nan());
                assert!(mat_matches(g, w, rolled), "row {i} matrix {g:?} != {w:?}")
            }
            _ => panic!("row {i} matrix presence {m:?} vs {:?}", r.mat),
        }
        n += 1;
    }
    assert_eq!(n, 133);
}

#[test]
fn every_bez_row_is_bit_identical() {
    let mut n = 0;
    for (i, l) in ORACLE.lines().enumerate() {
        let t: Vec<&str> = l.split(' ').collect();
        if t[1] != "bez" {
            continue;
        }
        let e = arr::<4>(t[3]);
        let got = bezier_ease(hex(t[2]), e[0], e[1], e[2], e[3]);
        assert!(same(got, hex(t[4])), "row {i}: {got:?} != {:?}", hex(t[4]));
        n += 1;
    }
    assert_eq!(n, 96);
}

#[test]
fn oracle_reaches_each_distinction() {
    let rows = cam_rows();
    let has = |f: &dyn Fn(&CamRow) -> bool| rows.iter().any(f);
    assert!(has(
        &|r| r.keys.as_ref().is_none_or(|k| k.is_empty()) && r.pose.is_none()
    ));
    assert!(has(&|r| r.keys.as_ref().is_some_and(|k| !k.is_empty()
        && r.frame < k[0].frame
        && r.pose.is_some())));
    assert!(has(&|r| r.keys.as_ref().is_some_and(
        |k| !k.is_empty() && r.frame > k[k.len() - 1].frame
    )));
    assert!(has(&|r| r.keys.as_ref().is_some_and(|k| k
        .iter()
        .any(|c| c.h_out.is_some() || c.h_in.is_some()))));
    assert!(has(&|r| r
        .keys
        .as_ref()
        .is_some_and(|k| k.windows(2).any(|w| w[0].frame > w[1].frame))));
    assert!(has(&|r| r.keys.as_ref().is_some_and(|k| k
        .windows(2)
        .any(|w| w[0].frame == w[1].frame))));
    assert!(has(
        &|r| r.pose.is_some_and(|p| p[3] == 0.0) && r.mat.is_some_and(|m| m[1] == 0.0)
    ));
    assert!(has(
        &|r| r.pose.is_some_and(|p| p[3] != 0.0) && r.mat.is_some_and(|m| m[1] != 0.0)
    ));
    assert!(has(&|r| r
        .mat
        .is_some_and(|m| m.iter().any(|v| !v.is_finite()))));
    assert!(has(&|r| r.pose.is_some_and(|p| p[2] < 0.0)));
    // easing outcomes: overshoot above 1, and exact 0/1 at the t<=0 / t>=1 guards
    let ys: Vec<f64> = ORACLE
        .lines()
        .filter(|l| l.contains(" bez "))
        .map(|l| hex(l.rsplit(' ').next().unwrap()))
        .collect();
    assert!(ys.iter().any(|y| *y > 1.0) && ys.contains(&0.0) && ys.contains(&1.0));
    assert!(ys.iter().any(|y| *y > 0.0 && *y < 1.0));
}

#[test]
fn clamps_use_array_ends_as_given() {
    let ks = [k(20.0, 900.0, 100.0, 250.0), k(0.0, 0.0, 0.0, 1000.0)];
    // unsorted: frame <= ks[0].frame (20) snaps to ks[0], never ks[1]
    assert_eq!(pose(&ks, 10.0).x, 900.0);
    assert_eq!(pose(&ks, 30.0).x, 0.0); // ... and frame >= ks[last].frame (0) to ks[last]
    let dup = [
        k(10.0, 1.0, 0.0, 1.0),
        k(10.0, 2.0, 0.0, 1.0),
        k(10.0, 3.0, 0.0, 1.0),
    ];
    assert_eq!(pose(&dup, 10.0).x, 1.0); // leading duplicate wins on `<=`
    let dup_end = [
        k(0.0, 0.0, 0.0, 1.0),
        k(10.0, 2.0, 0.0, 1.0),
        k(10.0, 3.0, 0.0, 1.0),
    ];
    assert_eq!(pose(&dup_end, 10.0).x, 3.0);
    assert!(pose(&dup_end, 9.999).x < 2.0);
    assert!(camera_at_frame(Some(&[]), 0.0).is_none() && camera_at_frame(None, 0.0).is_none());
}

#[test]
fn interpolation_reads_departing_ease_and_both_handles() {
    // `a` linear, `b` has a strong ease: only a's ease may matter.
    let a = linear(k(0.0, 0.0, 0.0, 1000.0));
    let mut b = k(10.0, 1000.0, 500.0, 500.0);
    b.ease = Some([0.9, 0.0, 0.1, 1.0]);
    let p = pose(&[a, b], 2.5);
    // y = t = 0.25 (a's linear ease; b's steep ease would not give 875)
    assert_eq!(p.w, 875.0);
    assert!((p.x - 156.25).abs() < 1e-9 && (p.y - 78.125).abs() < 1e-9);
    // default ease when absent, and equal to explicit DEFAULT_EASE
    let (c, mut d) = (k(0.0, 0.0, 0.0, 1000.0), k(10.0, 1000.0, 500.0, 500.0));
    d.ease = Some([0.0, 0.0, 1.0, 1.0]);
    let mut c2 = c;
    c2.ease = Some(DEFAULT_EASE);
    assert!(all_same(
        &pose_arr(pose(&[c, d], 3.0)),
        &pose_arr(pose(&[c2, d], 3.0))
    ));
    assert_ne!(pose(&[c, d], 3.0).x, pose(&[linear(c), d], 3.0).x);
    // hOut from the departing key, hIn from the arriving one
    let mut a = linear(k(0.0, 0.0, 0.0, 1.0));
    a.h_out = Some([300.0, 0.0]);
    let mut b = k(10.0, 300.0, 0.0, 1.0);
    b.h_in = Some([-300.0, 0.0]);
    let straight = pose(
        &[linear(k(0.0, 0.0, 0.0, 1.0)), k(10.0, 300.0, 0.0, 1.0)],
        5.0,
    )
    .x;
    assert_eq!(straight, 150.0);
    assert_eq!(pose(&[a, k(10.0, 300.0, 0.0, 1.0)], 5.0).x, 262.5);
    assert_eq!(pose(&[linear(k(0.0, 0.0, 0.0, 1.0)), b], 5.0).x, 37.5);
}

#[test]
fn rot_and_matrix_controls() {
    // missing rot reads 0 and skips rotate; the roll is rotate(-rot, centre)
    let mut a = linear(k(0.0, 100.0, 100.0, 960.0));
    a.rot = Some(90.0);
    let m = camera_matrix_at_frame(Some(&[a]), 0.0, 1920.0, 1080.0).unwrap();
    // scale 2 about (100,100) then roll -90deg: a column vector maps (1,0) -> (0, -2) rotated back
    assert!((m[0]).abs() < 1e-12 && (m[1] + 2.0).abs() < 1e-12 && (m[2] - 2.0).abs() < 1e-12);
    let plain =
        camera_matrix_at_frame(Some(&[k(0.0, 100.0, 100.0, 960.0)]), 0.0, 1920.0, 1080.0).unwrap();
    assert_eq!(
        (plain[0], plain[1], plain[2], plain[3]),
        (2.0, 0.0, 0.0, 2.0)
    );
    // scale centre is the camera centre: it lands on the canvas centre
    assert!(
        (2.0f64 * 100.0 + plain[4] - 960.0).abs() < 1e-12
            && (2.0f64 * 100.0 + plain[5] - 540.0).abs() < 1e-12
    );
    // zero width: infinite scale is returned, not rejected
    let z = camera_matrix_at_frame(Some(&[k(0.0, 0.0, 0.0, 0.0)]), 0.0, 1920.0, 1080.0).unwrap();
    assert!(z[0].is_infinite());
    // t at/under 0 and at/over 1 are exact, solvers never NaN on degenerate eases
    assert_eq!(
        (
            bezier_ease(0.0, 0.1, 0.2, 0.3, 0.4),
            bezier_ease(1.0, 0.1, 0.2, 0.3, 0.4)
        ),
        (0.0, 1.0)
    );
    assert!(bezier_ease(0.5, 0.0, 0.0, 0.0, 0.0).is_finite());
}

#[test]
fn nan_rot_reads_as_zero_and_skips_the_roll() {
    // JS `k.rot || 0`: a NaN rot is +0, and the matrix is the no-rot one
    let mut a = k(0.0, 100.0, 100.0, 960.0);
    a.rot = Some(f64::NAN);
    assert_eq!(pose(&[a], 0.0).rot.to_bits(), 0.0f64.to_bits());
    let plain = k(0.0, 100.0, 100.0, 960.0);
    assert_eq!(
        camera_matrix_at_frame(Some(&[a]), 0.0, 1920.0, 1080.0),
        camera_matrix_at_frame(Some(&[plain]), 0.0, 1920.0, 1080.0)
    );
    // interpolating from a NaN rot starts from 0
    let mut b = linear(k(10.0, 100.0, 100.0, 960.0));
    b.rot = Some(90.0);
    let a = linear(a);
    assert_eq!(pose(&[a, b], 5.0).rot, 45.0);
}

#[test]
fn trig_tolerance_still_rejects_real_mistakes() {
    let mut a = k(0.0, 100.0, 100.0, 960.0);
    a.rot = Some(30.0);
    let mut b = a;
    b.rot = Some(-30.0);
    let m = camera_matrix_at_frame(Some(&[a]), 0.0, 1920.0, 1080.0).unwrap();
    let flipped = camera_matrix_at_frame(Some(&[b]), 0.0, 1920.0, 1080.0).unwrap();
    assert!(mat_matches(&m, &m, true));
    assert!(!mat_matches(&flipped, &m, true));
    let mut nudged = m;
    nudged[4] *= 1.0 + 1e-12;
    assert!(!mat_matches(&nudged, &m, true));
    let mut one_ulp = m;
    one_ulp[0] = f64::from_bits(m[0].to_bits() + 1);
    assert!(mat_matches(&one_ulp, &m, true) && !mat_matches(&one_ulp, &m, false));
}
