//! Literal pins for the path-effect kernel. Every expected value below was produced by the baseline
//! `src/js/path-fx.js` (fbe4b8a3) and printed by Node (`tools/gen.cjs` rows cover the same functions); they
//! are independent of the oracle parser, so a parser mistake cannot make the oracle test and these agree.

use super::super::{
    active_stack, apply, apply_roughen, apply_zigzag, js_round, param_value, roughen_phase,
    type_def, PathFxEntry, PathFxStroke, PathSegment, P1, P2, P3, PATH_FX_TYPES,
};
use super::{Rest, Stroke};
use std::borrow::Cow;

fn anchor(x: f64, y: f64) -> PathSegment {
    PathSegment {
        point: [x, y],
        handle_in: None,
        handle_out: None,
    }
}

fn stroke(points: &[[f64; 2]], closed: bool) -> Stroke {
    PathFxStroke {
        is_raster: false,
        closed,
        segments: Some(points.iter().map(|p| anchor(p[0], p[1])).collect()),
        rest: Rest {
            width: Some(2.5f64.to_bits()),
            tag: "t".into(),
        },
    }
}

fn rect() -> Stroke {
    stroke(
        &[[0.0, 0.0], [100.0, 0.0], [100.0, 60.0], [0.0, 60.0]],
        true,
    )
}

fn line() -> Stroke {
    stroke(&[[0.0, 0.0], [100.0, 40.0]], false)
}

fn zigzag(p: [Option<f64>; 3]) -> PathFxEntry {
    PathFxEntry {
        fx_type: "zigzag".into(),
        enabled: None,
        params: p,
    }
}

fn roughen(p: [Option<f64>; 3]) -> PathFxEntry {
    PathFxEntry {
        fx_type: "roughen".into(),
        enabled: None,
        params: p,
    }
}

fn pts(s: &Stroke) -> Vec<[f64; 2]> {
    s.segments
        .as_ref()
        .expect("segments")
        .iter()
        .map(|g| g.point)
        .collect()
}

fn zig(sd: &Stroke, p: [Option<f64>; 3]) -> Stroke {
    apply_zigzag(sd, &zigzag(p))
        .expect("budget")
        .expect("changed")
}

#[test]
fn zigzag_on_a_closed_rect_alternates_sides_with_the_length_guarded_normal() {
    let z = zig(&rect(), [Some(12.0), Some(4.0), Some(0.0)]);
    let p = pts(&z);
    assert_eq!(p.len(), 32, "4 curves * ridges 4 * 2, closed: no tail");
    // t = 0 has a zero derivative: the normal falls back to length 1 and the point stays put.
    assert_eq!(p[0], [0.0, 0.0]);
    assert_eq!(p[1], [4.296875, -12.0], "odd index: side -1");
    assert_eq!(p[2], [15.625, 12.0], "even index: side +1");
    assert_eq!(p[31], [-12.0, 2.578125]);
    let all_zero = z
        .segments
        .as_ref()
        .unwrap()
        .iter()
        .all(|g| g.handle_in == Some([0.0; 2]) && g.handle_out == Some([0.0; 2]));
    assert!(all_zero, "hard points carry explicit zero handles");
    assert_eq!(
        (z.closed, z.is_raster, &z.rest),
        (true, false, &rect().rest),
        "other fields survive"
    );
}

#[test]
fn an_open_path_appends_its_last_anchor_displaced_by_the_end_normal() {
    let p = pts(&zig(&line(), [Some(12.0), Some(4.0), None]));
    assert_eq!(p.len(), 9, "1 curve * 8 + the tail at index 8");
    assert_eq!(p[0], [0.0, 0.0]);
    assert_eq!(p[1], [8.753563116249245, -9.42297029062311]);
    assert_eq!(
        p[8],
        [95.54331188375076, 51.14172029062311],
        "tail: normal of (last - previous), side +1"
    );
}

#[test]
fn smooth_zigzag_gives_catmull_rom_handles_and_open_ends_keep_zero_handles() {
    let s = zig(&rect(), [Some(12.0), Some(4.0), Some(1.0)]);
    let g = s.segments.as_ref().unwrap();
    assert_eq!(
        g[0].handle_out,
        Some([2.7161458333333335, -2.4296875]),
        "(next - prev) / 6"
    );
    assert_eq!(g[0].handle_in, Some([-2.7161458333333335, 2.4296875]));
    assert_eq!(g[1].handle_out, Some([2.6041666666666665, 2.0]));
    let o = zig(&line(), [Some(12.0), Some(1.0), Some(1.0)]);
    let g = o.segments.as_ref().unwrap();
    assert_eq!(g.len(), 3);
    assert_eq!(
        (g[0].handle_in, g[0].handle_out),
        (Some([0.0; 2]), Some([0.0; 2]))
    );
    assert_eq!(
        (g[2].handle_in, g[2].handle_out),
        (Some([0.0; 2]), Some([0.0; 2]))
    );
    assert_eq!(g[1].point, [54.456688116249246, 8.858279709376887]);
    assert_eq!(
        g[1].handle_out,
        Some([15.92388531395846, 8.523620048437186])
    );
    // p3 must exceed 0.5 strictly.
    let hard = zig(&rect(), [Some(12.0), Some(4.0), Some(0.5)]);
    assert_eq!(
        hard.segments.as_ref().unwrap()[0].handle_out,
        Some([0.0; 2])
    );
    let soft = zig(&rect(), [Some(12.0), Some(4.0), Some(0.5000000000000001)]);
    assert_ne!(
        soft.segments.as_ref().unwrap()[0].handle_out,
        Some([0.0; 2])
    );
}

#[test]
fn ridge_counts_use_javascript_rounding_then_at_least_one() {
    let n = |r: f64| pts(&zig(&rect(), [Some(5.0), Some(r), None])).len();
    // closed rect: 4 curves * (ridges * 2)
    assert_eq!(
        [
            n(4.0),
            n(2.5),
            n(1.5),
            n(0.5),
            n(-0.5),
            n(0.0),
            n(0.49999999999999994)
        ],
        [32, 24, 16, 8, 8, 8, 8]
    );
    for (x, want) in [
        (0.5, 1.0),
        (1.5, 2.0),
        (2.5, 3.0),
        (0.49999999999999994, 0.0),
        (2.4999999999999996, 2.0),
    ] {
        assert_eq!(js_round(x), want, "{x}");
    }
    assert_eq!([js_round(-1.5), js_round(-2.5)], [-1.0, -2.0]);
    assert_eq!(
        js_round(-0.5).to_bits(),
        (-0.0f64).to_bits(),
        "-0.5 -> -0, not -1"
    );
    assert_eq!(js_round(-0.3).to_bits(), (-0.0f64).to_bits());
    assert!(js_round(f64::NAN).is_nan() && js_round(f64::NEG_INFINITY) == f64::NEG_INFINITY);
}

#[test]
fn zero_size_and_zero_amount_return_the_input_itself() {
    let sd = rect();
    for fx in [
        zigzag([Some(0.0), None, None]),
        zigzag([Some(-0.0), None, None]),
        zigzag([Some(f64::NAN), None, None]),
    ] {
        assert_eq!(apply_zigzag(&sd, &fx), Ok(None));
    }
    for fx in [
        roughen([Some(0.0), None, None]),
        roughen([Some(f64::NAN), None, None]),
    ] {
        assert_eq!(apply_roughen(&sd, &fx, Some(3.0)), Ok(None));
    }
    let strokes = vec![sd];
    let out = apply(
        &strokes,
        Some(&[Some(zigzag([Some(0.0), None, None]))][..]),
        None,
    )
    .expect("budget");
    assert!(
        matches!(&out[0], Cow::Borrowed(p) if std::ptr::eq(*p, &strokes[0])),
        "same value, not a copy"
    );
}

#[test]
fn absent_and_null_params_read_the_type_default_but_an_explicit_zero_stays_zero() {
    assert_eq!(
        PATH_FX_TYPES.map(|t| (t.id, t.defaults)),
        [("zigzag", [12.0, 4.0, 0.0]), ("roughen", [8.0, 6.0, 0.0])]
    );
    assert!(type_def("wobble").is_none());
    let none = zigzag([None; 3]);
    assert_eq!(
        [
            param_value(&none, P1),
            param_value(&none, P2),
            param_value(&none, P3)
        ],
        [12.0, 4.0, 0.0]
    );
    assert_eq!(param_value(&zigzag([Some(0.0), None, None]), P1), 0.0);
    let unknown = PathFxEntry {
        fx_type: "wobble".into(),
        enabled: None,
        params: [None; 3],
    };
    assert_eq!(
        param_value(&unknown, P1),
        0.0,
        "an unknown type has no default"
    );
    let sd = rect();
    assert_eq!(
        apply_zigzag(&sd, &none),
        apply_zigzag(&sd, &zigzag([Some(12.0), Some(4.0), Some(0.0)]))
    );
    assert_eq!(
        apply_zigzag(&sd, &zigzag([Some(0.0), None, None])),
        Ok(None),
        "explicit 0 is not the default 12"
    );
    let r = |p| apply_roughen(&sd, &roughen(p), Some(5.0));
    assert_eq!(r([None; 3]), r([Some(8.0), Some(6.0), Some(0.0)]));
}

#[test]
fn roughen_evolution_zero_freezes_the_phase_and_fifty_at_frame_ten_is_five() {
    assert_eq!(roughen_phase(0.0, Some(10.0)), 0.0);
    assert_eq!(roughen_phase(50.0, Some(10.0)), 5.0);
    assert_eq!(roughen_phase(25.0, Some(8.0)), 2.0);
    for ignored in [None, Some(f64::NAN), Some(0.0)] {
        assert_eq!(roughen_phase(50.0, ignored), 0.0, "{ignored:?}");
    }
    assert_eq!(roughen_phase(f64::NAN, Some(10.0)), 0.0);
    let sd = rect();
    let at = |ev: f64, frame: f64| {
        apply_roughen(&sd, &roughen([Some(8.0), Some(6.0), Some(ev)]), Some(frame))
            .unwrap()
            .unwrap()
    };
    assert_eq!(
        at(50.0, 10.0),
        at(100.0, 5.0),
        "both use z = 5, bit for bit"
    );
    assert_eq!(at(0.0, 99.0), at(0.0, 0.0), "evolution 0 ignores the frame");
    assert_ne!(
        at(50.0, 10.0),
        at(50.0, 0.0),
        "a non-zero evolution advances with the frame"
    );
}

#[test]
fn roughen_matches_the_baseline_within_the_sin_hash_bound() {
    let r = apply_roughen(
        &line(),
        &roughen([Some(8.0), Some(2.0), Some(50.0)]),
        Some(10.0),
    )
    .unwrap()
    .unwrap();
    let g = r.segments.as_ref().unwrap();
    let near =
        |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() <= 1e-9 && (a[1] - b[1]).abs() <= 1e-9;
    assert_eq!(g.len(), 3);
    assert!(
        near(g[0].point, [0.0, 0.0]) && near(g[1].point, [49.87560631126031, 20.310984221849225]),
        "{:?}",
        g[1].point
    );
    assert!(near(g[2].point, [98.62699303947002, 43.43251740132495]));
    assert!(near(
        g[1].handle_out.unwrap(),
        [16.437832173245003, 7.238752900220825]
    ));
    assert_eq!(
        g[0].handle_out,
        Some([0.0; 2]),
        "open endpoints keep zero handles after roundCorners"
    );
}

#[test]
fn rasters_short_strokes_and_missing_geometry_pass_through_every_effect() {
    let mut raster = rect();
    raster.is_raster = true;
    let mut none = rect();
    none.segments = None;
    let strokes = vec![
        raster,
        none,
        stroke(&[[1.0, 2.0]], false),
        stroke(&[], true),
        rect(),
    ];
    let fx = vec![Some(zigzag([None; 3])), Some(roughen([None; 3]))];
    let out = apply(&strokes, Some(fx.as_slice()), Some(4.0)).expect("budget");
    for k in 0..4 {
        assert!(
            matches!(&out[k], Cow::Borrowed(p) if std::ptr::eq(*p, &strokes[k])),
            "stroke {k}"
        );
    }
    assert!(matches!(&out[4], Cow::Owned(_)));
}

#[test]
fn stack_semantics_enabled_unknown_null_and_order() {
    let off = PathFxEntry {
        enabled: Some(false),
        ..zigzag([None; 3])
    };
    let on = PathFxEntry {
        enabled: Some(true),
        ..zigzag([None; 3])
    };
    let unknown = PathFxEntry {
        fx_type: "wobble".into(),
        enabled: None,
        params: [None; 3],
    };
    let list = vec![
        Some(off.clone()),
        Some(roughen([None; 3])),
        Some(unknown),
        None,
        Some(on),
    ];
    let kept = active_stack(Some(list.as_slice())).expect("two active effects");
    assert_eq!(kept.len(), 2);
    assert!(
        std::ptr::eq(kept[0], list[1].as_ref().unwrap())
            && std::ptr::eq(kept[1], list[4].as_ref().unwrap())
    );
    let empty: Vec<Option<PathFxEntry>> = Vec::new();
    assert!(active_stack(None).is_none() && active_stack(Some(empty.as_slice())).is_none());
    assert!(
        active_stack(Some(&[Some(off.clone()), None][..])).is_none(),
        "nothing left after filtering"
    );
    let sd = vec![rect()];
    let run = |l: Vec<Option<PathFxEntry>>| {
        apply(&sd, Some(l.as_slice()), Some(7.0))
            .unwrap()
            .remove(0)
            .into_owned()
    };
    let (z, r) = (zigzag([None; 3]), roughen([None; 3]));
    let zr = run(vec![Some(z.clone()), Some(r.clone())]);
    let rz = run(vec![Some(r.clone()), Some(z.clone())]);
    assert_ne!(zr, rz, "order changes the result");
    assert_eq!(
        run(vec![Some(off), Some(r.clone())]),
        run(vec![Some(r)]),
        "a disabled effect is skipped"
    );
}

#[test]
fn the_source_strokes_are_never_modified_and_outputs_are_new_vectors() {
    let strokes = vec![rect(), line()];
    let before = strokes.clone();
    let fx = vec![Some(zigzag([None; 3])), Some(roughen([None; 3]))];
    let out = apply(&strokes, Some(fx.as_slice()), Some(3.0)).expect("budget");
    assert_eq!(strokes, before);
    for (k, o) in out.iter().enumerate() {
        assert!(matches!(o, Cow::Owned(_)));
        assert_eq!((o.closed, &o.rest), (strokes[k].closed, &strokes[k].rest));
        assert_ne!(o.segments, strokes[k].segments);
    }
}
