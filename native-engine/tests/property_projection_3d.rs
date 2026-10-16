//! Oracle tests for the 3D projection kernel (K-3D).
//!
//! `fixtures/property-projection-3d/oracle.txt` (`ORACLE`, sha256
//! be58af8ad32899fabec9c8227eae8fcc5a71f8539e69aaf5ab466fe8bcdb4c7b) was produced by running `rotate3D`,
//! `project3DToScreen`, `make3DProjector`, `project3DSegments` and
//! `project3DImageRect`, cut verbatim from `src/js/motion.js` at 1f663ac3 under
//! Node (generator `gen.js`, kept outside the repo, `valueAtFrame` stubbed).
//!
//! Rows (space separated, f64 as 16 hex digits, `!` absent, `-` none, lists
//! comma-joined): `P wx,wy,wz,cw,ch x,y`, `R x,y,z,rx,ry,rz x,y,z`,
//! `M ld bounds cw,ch extra px,py x,y`, `S ld bounds cw,ch extra segs segs'`
//! (segs `;`-fields, `|`-joined), `I ld bounds cw,ch extra x,y,w,h,rot out`.
//! `ld` = posX,posY,ancX,ancY,rot,sclX,sclY,posZ,rotX,rotY. Numbers compare on
//! bits; two NaNs match (payload ignored). One exception: values that go
//! through `cos`/`sin`/`atan2` (an R row with an angle, an M/S/I row whose pose
//! or delta rotates, every I-row rotation) are libm-dependent in the last ulp
//! (macOS differs from the Linux/V8 oracle). They may differ by `TRIG_ULPS`
//! units of roundoff of their group's largest entry; an angle's floor is a
//! half turn, as atan2 error is absolute. P rows and unrotated rows stay exact.

use std::f64::consts::PI;

use crate::evaluation::property_projection_3d::{
    make_projector, project_image_rect, project_segments, project_to_screen, rotate_3d, Bounds,
    ExtraDelta, Pose3D, Projector3D, Rect, Seg,
};

pub(crate) const ORACLE: &str = include_str!("fixtures/property-projection-3d/oracle.txt");

fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

fn all_same(a: &[f64], b: &[f64]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| same(*x, *y))
}

const TRIG_ULPS: f64 = 4.0;
const ANGLE_FLOOR: f64 = 180.0;

/// Bit-equal, or a finite rounding difference within `TRIG_ULPS` of the
/// group's magnitude (at least `floor`); a signed-zero mismatch never passes.
fn trig_near(a: &[f64], b: &[f64], floor: f64) -> bool {
    let mag = a
        .iter()
        .chain(b)
        .filter(|x| x.is_finite())
        .fold(floor, |m, x| m.max(x.abs()));
    a.len() == b.len()
        && a.iter().zip(b).all(|(&x, &y)| {
            same(x, y)
                || (x.is_finite()
                    && y.is_finite()
                    && x != y
                    && (x - y).abs() <= TRIG_ULPS * mag * f64::EPSILON)
        })
}

fn matches(a: &[f64], b: &[f64], trig: bool) -> bool {
    if trig {
        trig_near(a, b, 0.0)
    } else {
        all_same(a, b)
    }
}

fn truthy(v: f64) -> bool {
    v != 0.0 && !v.is_nan()
}

/// Whether the row's projector rotates (pose or delta rot, rotX or rotY).
fn rotates(t: &[&str]) -> bool {
    let p = pose(t[1]);
    let e = extra(t[4]).unwrap_or_default();
    [p.rotation, p.rotation_x, p.rotation_y]
        .into_iter()
        .any(truthy)
        || [e.drot, e.drx, e.dry].into_iter().flatten().any(truthy)
}

fn list(tok: &str) -> Vec<f64> {
    tok.split(',').filter(|s| !s.is_empty()).map(hex).collect()
}

fn pose(tok: &str) -> Pose3D {
    let v = list(tok);
    Pose3D {
        position: [v[0], v[1]],
        anchor: [v[2], v[3]],
        rotation: v[4],
        scale: [v[5], v[6]],
        position_z: v[7],
        rotation_x: v[8],
        rotation_y: v[9],
    }
}

fn bounds(tok: &str) -> Bounds {
    let v = list(tok);
    Bounds {
        x: v[0],
        y: v[1],
        width: v[2],
        height: v[3],
    }
}

fn extra(tok: &str) -> Option<ExtraDelta> {
    if tok == "-" {
        return None;
    }
    let f: Vec<Option<f64>> = tok
        .split(',')
        .filter(|s| !s.is_empty())
        .map(|s| (s != "!").then(|| hex(s)))
        .collect();
    let g = |i: usize| f.get(i).copied().flatten();
    Some(ExtraDelta {
        dx: g(0),
        dy: g(1),
        drot: g(2),
        dsx_pct: g(3),
        dsy_pct: g(4),
        dz: g(5),
        drx: g(6),
        dry: g(7),
    })
}

fn seg_list(tok: &str) -> Vec<Seg> {
    let h = |s: &str| (s != "-").then(|| <[f64; 2]>::try_from(list(s)).expect("pair"));
    tok.split('|')
        .filter(|s| !s.is_empty())
        .map(|s| {
            let f: Vec<&str> = s.split(';').collect();
            Seg {
                point: h(f[0]).unwrap(),
                handle_in: h(f[1]),
                handle_out: h(f[2]),
            }
        })
        .collect()
}

fn seg_flat(v: &[Seg]) -> Vec<f64> {
    let mut o = vec![];
    for s in v {
        o.extend(s.point);
        for h in [s.handle_in, s.handle_out] {
            o.extend(h.unwrap_or([f64::INFINITY, -f64::INFINITY]));
        }
    }
    o
}

fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .map(|l| l.split(' ').collect::<Vec<_>>())
        .filter(|t| t[0] == kind)
        .collect()
}

fn proj(t: &[&str]) -> Projector3D {
    let cs = list(t[3]);
    make_projector(
        &pose(t[1]),
        &bounds(t[2]),
        cs[0],
        cs[1],
        extra(t[4]).as_ref(),
    )
}

/// The fixture's image rect (x 100, y 50, 400 x 200) with a given rotation.
fn img(rotation: Option<f64>) -> Rect {
    Rect {
        x: 100.0,
        y: 50.0,
        width: 400.0,
        height: 200.0,
        rotation,
    }
}

/// An I-row rect token `x,y,w,h,rot` (`!` = absent rotation).
fn rect_tok(tok: &str) -> Rect {
    let r: Vec<Option<f64>> = tok.split(',').map(|s| (s != "!").then(|| hex(s))).collect();
    let v = |i: usize| r[i].expect("rect field");
    Rect {
        x: v(0),
        y: v(1),
        width: v(2),
        height: v(3),
        rotation: r[4],
    }
}

fn flat_seg_tok(tok: &str) -> Vec<f64> {
    seg_flat(&seg_list(tok))
}

fn base() -> Pose3D {
    pose_of([960.0, 540.0, 0.0, 0.0, 0.0, 100.0, 100.0, 0.0, 0.0, 0.0])
}

fn pose_of(v: [f64; 10]) -> Pose3D {
    Pose3D {
        position: [v[0], v[1]],
        anchor: [v[2], v[3]],
        rotation: v[4],
        scale: [v[5], v[6]],
        position_z: v[7],
        rotation_x: v[8],
        rotation_y: v[9],
    }
}

const B: Bounds = Bounds {
    x: 100.0,
    y: 50.0,
    width: 400.0,
    height: 200.0,
};

#[test]
fn oracle_has_expected_shape() {
    assert_eq!(ORACLE.lines().count(), 168);
    let n: Vec<usize> = ["P", "R", "M", "S", "I"]
        .iter()
        .map(|k| rows(k).len())
        .collect();
    assert_eq!(n, vec![32, 33, 65, 15, 23]);
}

#[test]
fn every_p_row_is_bit_identical() {
    for (i, t) in rows("P").iter().enumerate() {
        let a = list(t[1]);
        let got = project_to_screen(a[0], a[1], a[2], a[3], a[4]);
        assert!(
            all_same(&got, &list(t[2])),
            "P {i}: {got:?} != {:?}",
            list(t[2])
        );
    }
}

#[test]
fn every_r_row_matches_the_oracle() {
    for (i, t) in rows("R").iter().enumerate() {
        let a = list(t[1]);
        let got = rotate_3d(a[0], a[1], a[2], a[3], a[4], a[5]);
        let trig = a[3..].iter().any(|v| truthy(*v));
        assert!(
            matches(&got, &list(t[2]), trig),
            "R {i}: {got:?} != {:?}",
            list(t[2])
        );
    }
}

#[test]
fn every_m_row_matches_the_oracle() {
    for (i, t) in rows("M").iter().enumerate() {
        let p = list(t[5]);
        let got = proj(t).project(p[0], p[1]);
        assert!(
            matches(&got, &list(t[6]), rotates(t)),
            "M {i}: {got:?} != {:?}",
            list(t[6])
        );
    }
}

#[test]
fn every_s_row_matches_the_oracle() {
    for (i, t) in rows("S").iter().enumerate() {
        let got = seg_flat(&project_segments(&seg_list(t[5]), &proj(t)));
        let want = flat_seg_tok(t[6]);
        // one magnitude group per segment: handles are differences of projected points
        let ok = got.len() == want.len()
            && got
                .chunks(6)
                .zip(want.chunks(6))
                .all(|(g, w)| matches(g, w, rotates(t)));
        assert!(ok, "S {i}: {got:?} != {want:?}");
    }
}

#[test]
fn every_i_row_matches_the_oracle() {
    for (i, t) in rows("I").iter().enumerate() {
        let rect = rect_tok(t[5]);
        let o = project_image_rect(&rect, &proj(t));
        let got = [o.x, o.y, o.width, o.height, o.rotation];
        let w = list(t[6]);
        let trig = rotates(t) || rect.rotation.is_some_and(truthy);
        assert!(matches(&got[..4], &w[..4], trig), "I {i}: {got:?} != {w:?}");
        // the rotation always goes through atan2
        assert!(
            trig_near(&got[4..], &w[4..], ANGLE_FLOOR),
            "I {i} rot {:?} != {:?}",
            got[4],
            w[4]
        );
    }
}

#[test]
fn oracle_reaches_each_distinction() {
    let p = rows("P");
    assert!(p
        .iter()
        .any(|t| list(t[1])[2] <= -1900.0 && list(t[2])[0].is_finite()));
    assert!(p.iter().any(|t| list(t[2])[0].is_nan()));
    assert!(p.iter().any(|t| list(t[1])[2] == f64::INFINITY));
    let r = rows("R");
    assert!(r.iter().any(|t| list(t[1])[3..].iter().all(|a| *a == 0.0)));
    assert!(r.iter().any(|t| list(t[1])[3].is_nan()));
    assert!(r.iter().any(|t| list(t[1])[5].abs() > 360.0));
    let m = rows("M");
    assert!(m.iter().any(|t| t[4] == "-") && m.iter().any(|t| t[4] != "-"));
    assert!(m.iter().any(|t| t[4].is_empty()) && m.iter().any(|t| t[4].contains('!')));
    assert!(m.iter().any(|t| list(t[1])[5] == 0.0) && m.iter().any(|t| list(t[1])[5] < 0.0));
    let s = rows("S");
    assert!(s.iter().any(|t| t[5].is_empty()) && s.iter().any(|t| t[5].contains(";-;-")));
    assert!(s.iter().any(|t| seg_list(t[5])
        .iter()
        .any(|g| g.handle_in.is_some() && g.handle_out.is_none())));
    assert!(s.iter().any(|t| seg_list(t[5])
        .iter()
        .any(|g| g.handle_in.is_none() && g.handle_out.is_some())));
    let i = rows("I");
    assert!(i.iter().any(|t| t[5].ends_with(",!")) && i.iter().any(|t| !t[5].ends_with(",!")));
    assert!(i.iter().any(|t| list(t[6])[2].abs() < 1e-9));
    assert!(i.iter().any(|t| list(t[6])[4].abs() > 90.0));
    assert!(i
        .iter()
        .any(|t| t[5].split(',').nth(2) == Some("0000000000000000")));
}

#[test]
fn point_and_rotation_controls() {
    // centre of a 1920x1080 canvas offsets; wz = 0 leaves depth at 1
    assert_eq!(
        project_to_screen(300.0, -200.0, 0.0, 1920.0, 1080.0),
        [1260.0, 340.0]
    );
    // wz = 1000 -> depth 1.5, wz <= -1900 -> floor 0.05 (finite, huge)
    assert_eq!(
        project_to_screen(300.0, 0.0, 1000.0, 1920.0, 1080.0)[0],
        960.0 + 300.0 / 1.5
    );
    assert_eq!(
        project_to_screen(300.0, 0.0, -1950.0, 1920.0, 1080.0)[0],
        960.0 + 300.0 / 0.05
    );
    assert!(project_to_screen(1.0, 1.0, f64::NAN, 10.0, 10.0)[0].is_nan());
    // Z rotation by 90 deg turns (1,0) into ~(0,1); X-only 90 sends y into z
    let z = rotate_3d(1.0, 0.0, 0.0, 0.0, 0.0, 90.0);
    assert!(z[0].abs() < 1e-15 && (z[1] - 1.0).abs() < 1e-15 && z[2] == 0.0);
    let x = rotate_3d(0.0, 1.0, 0.0, 90.0, 0.0, 0.0);
    assert!(x[1].abs() < 1e-15 && (x[2] - 1.0).abs() < 1e-15);
    // order is X then Y then Z: Y-then-X would differ for this input
    let xy = rotate_3d(1.0, 0.0, 0.0, 90.0, 90.0, 0.0);
    assert!(xy[0].abs() < 1e-15 && xy[1].abs() < 1e-15 && (xy[2] + 1.0).abs() < 1e-15);
    // NaN/zero angle skips the axis; angle sign flips the result
    assert_eq!(
        rotate_3d(1.0, 2.0, 3.0, f64::NAN, -0.0, 0.0),
        [1.0, 2.0, 3.0]
    );
    assert!(
        rotate_3d(1.0, 0.0, 0.0, 0.0, 0.0, 30.0)[1] > 0.0
            && rotate_3d(1.0, 0.0, 0.0, 0.0, 0.0, -30.0)[1] < 0.0
    );
}

#[test]
fn projector_extra_and_segment_controls() {
    // at the pivot, identity pose: screen = canvas centre + (pivot + pos - canvas/2)
    let id = make_projector(&base(), &B, 1920.0, 1080.0, None);
    assert_eq!(id.project(300.0, 150.0), [1260.0, 690.0]);
    // an empty ExtraDelta behaves like none, a present one adds
    let z = ExtraDelta::default();
    assert_eq!(make_projector(&base(), &B, 1920.0, 1080.0, Some(&z)), id);
    let dx = ExtraDelta {
        dx: Some(10.0),
        dy: Some(-20.0),
        ..z
    };
    assert_eq!(
        make_projector(&base(), &B, 1920.0, 1080.0, Some(&dx)).project(300.0, 150.0),
        [1270.0, 670.0]
    );
    // scale percent is additive: 100 + 100 doubles distance from the pivot
    let ds = ExtraDelta {
        dsx_pct: Some(100.0),
        ..z
    };
    assert_eq!(
        make_projector(&base(), &B, 1920.0, 1080.0, Some(&ds)).project(310.0, 150.0)[0],
        1280.0
    );
    // dz pushes away: depth 2 halves the offset from the canvas centre
    let dz = ExtraDelta {
        dz: Some(2000.0),
        ..z
    };
    assert_eq!(
        make_projector(&base(), &B, 1920.0, 1080.0, Some(&dz)).project(300.0, 150.0),
        [960.0 + 150.0, 540.0 + 75.0]
    );
    // handles are projected absolute then differenced (relative stays relative at depth 1)
    let seg = [Seg {
        point: [300.0, 150.0],
        handle_in: Some([-10.0, 5.0]),
        handle_out: None,
    }];
    assert_eq!(project_segments(&seg, &id)[0].handle_in, Some([-10.0, 5.0]));
    assert_eq!(project_segments(&seg, &id)[0].handle_out, None);
    let far = make_projector(
        &pose_of([960.0, 540.0, 0.0, 0.0, 0.0, 100.0, 100.0, 2000.0, 0.0, 0.0]),
        &B,
        1920.0,
        1080.0,
        None,
    );
    assert_eq!(project_segments(&seg, &far)[0].handle_in, Some([-5.0, 2.5]));
    // image rect: identity pose keeps size, centre lands at the projected centre
    let r = project_image_rect(&img(None), &id);
    assert_eq!(
        (r.x, r.y, r.width, r.height, r.rotation),
        (1060.0, 590.0, 400.0, 200.0, 0.0)
    );
    // rect rotation adds to the projected rotation under identity
    let q = project_image_rect(&img(Some(30.0)), &id);
    assert!((q.rotation - 30.0).abs() < 1e-12 && (q.width - 400.0).abs() < 1e-9);
    // layer rotation adds the same way
    let lr = make_projector(
        &pose_of([960.0, 540.0, 0.0, 0.0, 20.0, 100.0, 100.0, 0.0, 0.0, 0.0]),
        &B,
        1920.0,
        1080.0,
        None,
    );
    let rr = project_image_rect(&img(Some(30.0)), &lr);
    assert!((rr.rotation - 50.0).abs() < 1e-12);
}

#[test]
fn nan_extra_fields_and_rect_rotation_read_as_zero() {
    // JS `v || 0`: NaN is falsy, so each NaN field contributes nothing
    let id = make_projector(&base(), &B, 1920.0, 1080.0, None);
    let n = Some(f64::NAN);
    let all_nan = ExtraDelta {
        dx: n,
        dy: n,
        drot: n,
        dsx_pct: n,
        dsy_pct: n,
        dz: n,
        drx: n,
        dry: n,
    };
    assert_eq!(
        make_projector(&base(), &B, 1920.0, 1080.0, Some(&all_nan)),
        id
    );
    assert_eq!(
        project_image_rect(&img(n), &id),
        project_image_rect(&img(None), &id)
    );
}

#[test]
fn trig_tolerance_still_rejects_real_mistakes() {
    let got = rotate_3d(120.0, -80.0, 15.0, 20.0, 35.0, 50.0);
    let flipped = rotate_3d(120.0, -80.0, 15.0, -20.0, 35.0, 50.0);
    assert!(matches(&got, &got, true) && !matches(&flipped, &got, true));
    let mut nudged = got;
    nudged[2] *= 1.0 + 1e-12;
    assert!(!matches(&nudged, &got, true));
    let mut one_ulp = got;
    one_ulp[0] = f64::from_bits(got[0].to_bits() + 1);
    assert!(matches(&one_ulp, &got, true) && !matches(&one_ulp, &got, false));
    assert!(!trig_near(&[0.0], &[-0.0], ANGLE_FLOOR));
    assert!(!trig_near(&[10.0], &[10.0 + 1e-11], ANGLE_FLOOR));
}

#[test]
fn hypot_is_v8_math_hypot_bit_for_bit() {
    // V8 Math.hypot results (Node 22) where libm `f64::hypot` rounds differently.
    // hypot2 is IEEE arithmetic plus sqrt only, so it is exact on every host,
    // and this pins the port even where trig rows carry TRIG_ULPS.
    for row in [
        "406364edf70c0000 c06865f0f0000000 406f2addc1c80788",
        "40666ec470000000 4065a3f290000000 406f2b5f83753a01",
        "c07230d356c00000 4071f92d1f800000 407992839d618cc0",
        "407734e718000000 40757fcca8000000 407fa286cff995a0",
        "c05ec15926000000 4011fd4920000000 405ec69b9003dd6a",
        "4041e613b8000000 4075a9261b000000 4075c6a68df8fa23",
    ] {
        let v: Vec<f64> = row.split(' ').map(hex).collect();
        assert_eq!(super::hypot2(v[0], v[1]).to_bits(), v[2].to_bits(), "{row}");
    }
}

#[test]
fn image_rect_size_uses_the_v8_hypot() {
    // recompute the half axes with the kernel's own projector; size must be 2 * hypot2
    for t in rows("I") {
        let rect = rect_tok(t[5]);
        let r = [rect.x, rect.y, rect.width, rect.height];
        let (p, o) = (proj(&t), project_image_rect(&rect, &proj(&t)));
        let rot = rect.rotation.filter(|v| truthy(*v)).unwrap_or(0.0) * PI / 180.0;
        let (c, s) = (rot.cos(), rot.sin());
        let (cx, cy, hw, hh) = (r[0] + r[2] / 2.0, r[1] + r[3] / 2.0, r[2] / 2.0, r[3] / 2.0);
        let half = |dx: f64, dy: f64| {
            let (a, b) = (p.project(cx + dx, cy + dy), p.project(cx - dx, cy - dy));
            super::hypot2((a[0] - b[0]) / 2.0, (a[1] - b[1]) / 2.0) * 2.0
        };
        assert!(same(o.width, half(hw * c, hw * s)), "{t:?}");
        assert!(same(o.height, half(hh * -s, hh * c)), "{t:?}");
    }
}
