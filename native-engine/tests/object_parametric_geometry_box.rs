//! Oracle replay of the box, rotation and animation rows of
//! `fixtures/object-parametric-geometry/oracle.txt` (see `object_parametric_geometry.rs` for the row format).
//!
//! - `stamp x,y,w,h > box`: `stampParamShapeBox` from Paper's `path.bounds`.
//! - `bnd segs > x y w h`: Paper's `path.bounds`; the kernel answers only for all-zero handles.
//! - `trans box dx,dy > box`, `scale spec box ang sx,sy,ax,ay > box rebuilt`: `syncParamShapeBoxOnTranslate/OnScale`
//!   (`rebuilt` is `=` when the baseline left the anchors alone, else the rebuilt rect).
//! - `apply spec box ang bx,by,bw,bh > box segs`: `applyParamShapeEllipse/Star/Rect` (including the self-heal).
//! - `rot ang cx cy segs > segs`: `Path.rotate(ang, centre)`.
//! - `anim spec segs a1..a9 > segs`, `hasm kind a1..a9 > 0|1`: motion.js `applyParamShapeFor` / `hasParamShapeMotionFor`.

use super::super::boxed::{
    has_param_shape_motion, rotate_segments, straight_bounds, sync_box_on_translate,
    AnimatedParams, ParamBox, ParamShape, ParamShapeKind, PathBounds,
};
use super::controls::Mutation;
use super::{geom, hex, nums, opt, params_of, segs_of, Check, Report};

pub(crate) fn spec_of(t: &str) -> ParamShape {
    let f: Vec<&str> = t.split(':').collect();
    let v = |i: usize| opt(f[i]);
    match f[0] {
        "R" => ParamShape::Rect {
            tl: v(1),
            tr: v(2),
            br: v(3),
            bl: v(4),
        },
        "E" => ParamShape::Ellipse {
            start_angle: v(1),
            sweep: v(2),
            inner_radius: v(3),
        },
        _ => ParamShape::Star {
            point_count: v(1),
            inner_ratio: v(2),
            corner_radius: v(3),
        },
    }
}

pub(crate) fn box_of(t: &str) -> Option<ParamBox> {
    (t != "-").then(|| {
        let v = nums(t);
        ParamBox {
            x1: v[0],
            y1: v[1],
            x2: v[2],
            y2: v[3],
        }
    })
}

pub(crate) fn anim_of(t: &str) -> AnimatedParams {
    let a: Vec<Option<f64>> = t.split(',').map(opt).collect();
    AnimatedParams {
        corner_tl: a[0],
        corner_tr: a[1],
        corner_br: a[2],
        corner_bl: a[3],
        arc_start: a[4],
        arc_sweep: a[5],
        arc_inner: a[6],
        star_inner: a[7],
        star_corner: a[8],
    }
}

fn kind_of(t: &str) -> Option<ParamShapeKind> {
    match t {
        "rect" => Some(ParamShapeKind::Rect),
        "ellipse" => Some(ParamShapeKind::Ellipse),
        "star" => Some(ParamShapeKind::Star),
        "!" => None,
        other => panic!("unknown kind {other}"),
    }
}

fn check_box(c: &mut Check, what: &str, got: Option<ParamBox>, want: Option<ParamBox>) {
    match (got, want) {
        (None, None) => {}
        (Some(g), Some(w)) => {
            c.num(&format!("{what}.x1"), g.x1, w.x1, 0.0);
            c.num(&format!("{what}.y1"), g.y1, w.y1, 0.0);
            c.num(&format!("{what}.x2"), g.x2, w.x2, 0.0);
            c.num(&format!("{what}.y2"), g.y2, w.y2, 0.0);
        }
        (g, w) => c.fail(format!("{what}: {g:?} vs {w:?}")),
    }
}

fn row_stamp(r: &mut Report, t: &[&str], out: &str) {
    let b = nums(t[1]);
    let mut c = r.check(&format!("stamp {}", t[1]));
    let got = ParamBox::stamp(PathBounds {
        x: b[0],
        y: b[1],
        width: b[2],
        height: b[3],
    });
    check_box(&mut c, "box", Some(got), box_of(out));
    r.done("stamp", c);
}

fn row_bnd(r: &mut Report, t: &[&str], out: &str) {
    let segs = params_of(t[1]);
    let want: Vec<f64> = out.split(' ').map(hex).collect();
    let mut c = r.check(&format!("bnd {}", t[1]));
    match straight_bounds(&segs) {
        Some(b) => {
            c.num("x", b.x, want[0], 0.0);
            c.num("y", b.y, want[1], 0.0);
            c.num("width", b.width, want[2], 0.0);
            c.num("height", b.height, want[3], 0.0);
            r.tick("bnd:straight");
        }
        None => {
            let curved = segs
                .iter()
                .any(|s| s.handle_in != [0.0, 0.0] || s.handle_out != [0.0, 0.0]);
            if !curved {
                c.fail("refused a straight path".to_string());
            }
            r.tick("bnd:curved");
        }
    }
    r.done("bnd", c);
}

fn row_trans(r: &mut Report, t: &[&str], out: &str) {
    let d = nums(t[2]);
    let mut c = r.check(&format!("trans {} {}", t[1], t[2]));
    match sync_box_on_translate(box_of(t[1]), d[0], d[1]) {
        Ok(got) => check_box(&mut c, "box", got, box_of(out)),
        Err(e) => c.fail(format!("refused: {e:?}")),
    }
    r.done("trans", c);
}

fn row_scale(r: &mut Report, t: &[&str], out: &str, m: Mutation) {
    let shape = spec_of(t[1]);
    let stored = box_of(t[2]);
    let ang = hex(t[3]);
    let n = nums(t[4]);
    let (box_tok, rebuilt_tok) = out.split_once(' ').expect("rebuilt token");
    let mut c = r.check(&format!("scale {} {} {} {}", t[1], t[2], t[3], t[4]));
    match m.sync_on_scale(&shape, stored, ang, n[0], n[1], [n[2], n[3]]) {
        Ok(None) => {
            if stored.is_some() || box_tok != "-" || rebuilt_tok != "=" {
                c.fail("no result for a boxed shape".to_string());
            }
        }
        Ok(Some(s)) => {
            check_box(&mut c, "box", Some(s.box_), box_of(box_tok));
            match (&s.rebuilt, rebuilt_tok) {
                (None, "=") => {}
                (Some(segs), tok) if tok != "=" => {
                    c.segments(&geom(segs), &segs_of(tok), ang != 0.0)
                }
                (g, w) => c.fail(format!("rebuilt {} vs {w}", g.is_some())),
            }
        }
        Err(e) => c.fail(format!("refused: {e:?}")),
    }
    r.tick(if n[0] < 0.0 || n[1] < 0.0 {
        "scale:negative"
    } else {
        "scale:positive"
    });
    r.done("scale", c);
}

fn row_apply(r: &mut Report, t: &[&str], out: &str, m: Mutation) {
    let shape = spec_of(t[1]);
    let stored = box_of(t[2]);
    let ang = opt(t[3]).unwrap_or(0.0);
    let bd = nums(t[4]);
    let bounds = PathBounds {
        x: bd[0],
        y: bd[1],
        width: bd[2],
        height: bd[3],
    };
    let (box_tok, segs_tok) = out.split_once(' ').expect("segments token");
    let mut c = r.check(&format!("apply {} {} {} {}", t[1], t[2], t[3], t[4]));
    match m.apply(&shape, stored, Some(bounds), ang) {
        Ok(a) => {
            check_box(&mut c, "box", Some(a.box_), box_of(box_tok));
            let trig = shape.kind() != ParamShapeKind::Rect || ang != 0.0;
            c.segments(&geom(&a.segments), &segs_of(segs_tok), trig);
        }
        Err(e) => c.fail(format!("refused: {e:?}")),
    }
    r.tick(if stored.is_some() {
        "apply:boxed"
    } else {
        "apply:healed"
    });
    if ang != 0.0 {
        r.tick("apply:rotated");
    }
    r.done("apply", c);
}

fn row_rot(r: &mut Report, t: &[&str], out: &str) {
    let ang = hex(t[1]);
    let centre = [hex(t[2]), hex(t[3])];
    let segs = params_of(t[4]);
    let mut c = r.check(&format!("rot {} {} {} {}", t[1], t[2], t[3], t[4]));
    match rotate_segments(&segs, ang, centre) {
        Ok(got) => c.segments(&geom(&got), &segs_of(out), true),
        Err(e) => c.fail(format!("refused: {e:?}")),
    }
    let tiny = |h: [f64; 2]| h.iter().any(|v| *v != 0.0 && v.abs() <= 1e-12);
    if segs.iter().any(|s| tiny(s.handle_in) || tiny(s.handle_out)) {
        r.tick("rot:tiny-handle");
    }
    r.done("rot", c);
}

fn row_anim(r: &mut Report, t: &[&str], out: &str, m: Mutation) {
    let shape = spec_of(t[1]);
    let stored = params_of(t[2]);
    let a = anim_of(t[3]);
    let mut c = r.check(&format!("anim {} {}", t[1], t[3]));
    match m.anim(&shape, &stored, &a) {
        Ok(got) => c.segments(
            &geom(&got),
            &segs_of(out),
            shape.kind() != ParamShapeKind::Rect,
        ),
        Err(e) => c.fail(format!("refused: {e:?}")),
    }
    r.done("anim", c);
}

fn row_hasm(r: &mut Report, t: &[&str], out: &str) {
    let mut c = r.check(&format!("hasm {} {}", t[1], t[2]));
    let got = has_param_shape_motion(kind_of(t[1]), &anim_of(t[2]));
    if got != (out == "1") {
        c.fail(format!("{got} vs {out}"));
    }
    r.done("hasm", c);
}

/// Replay one non-builder row.
pub(crate) fn row(r: &mut Report, t: &[&str], out: &str, m: Mutation) {
    match t[0] {
        "stamp" => row_stamp(r, t, out),
        "bnd" => row_bnd(r, t, out),
        "trans" => row_trans(r, t, out),
        "scale" => row_scale(r, t, out, m),
        "apply" => row_apply(r, t, out, m),
        "rot" => row_rot(r, t, out),
        "anim" => row_anim(r, t, out, m),
        "hasm" => row_hasm(r, t, out),
        other => panic!("unknown op {other}"),
    }
}

/// Literal intermediates from the independently observed unmodified shipped
/// Paper Path.rotate call at row 1070 (#2130), not native-libm expectations.
#[test]
fn rotation_trigonometry_matches_observed_original_paper_and_preserves_zero() {
    let radians = f64::from_bits(0x401261be25f391e8);
    assert_eq!(libm::cos(radians).to_bits(), 0xbfbdde22283de9e0);
    assert_eq!(libm::sin(radians).to_bits(), 0xbfefc80dd063ad0e);
    assert_eq!(libm::sin(0.0).to_bits(), 0.0_f64.to_bits());
    assert_eq!(libm::sin(-0.0).to_bits(), (-0.0_f64).to_bits());
    assert_eq!(libm::cos(-0.0).to_bits(), 1.0_f64.to_bits());
}
