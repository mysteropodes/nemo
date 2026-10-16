//! Tests for the rig-widget geometry kernel (L03-B, F19): the baseline oracle rows through
//! production, plus hand-pinned literals computed by the verbatim baseline functions
//! (`tools/literals.cjs`) and the typed missing-control contract.

use super::oracle_rows::{
    axis_run, c01_run, geo_run, map_run, puck_run, rest_run, rows, span_run, value_run, write_run,
    Run, TableSource,
};
use super::{
    axis_value, clamp01, geom_for, js_round, local_from_values, puck_local, rest_local, span,
    to_local, to_world, value_from_local, write_from_local, AxisRest, AxisSample, AxisSlot,
    AxisSpec, AxisUnavailable, Placement, WidgetGeometryError, WidgetKind, WidgetSpec,
};

/// Row counts of the oracle (`tools/gen.cjs`); a vacuous or truncated oracle fails here.
pub(crate) const ROW_COUNTS: [(&str, usize); 10] = [
    ("c01", 14),
    ("spn", 11),
    ("vfl", 126),
    ("geo", 106),
    ("tw", 108),
    ("tl", 144),
    ("rl", 32),
    ("pk", 145),
    ("wfl", 96),
    ("axv", 32),
];

pub(crate) fn axis_spec(key: &str, min: f64, max: f64, rest: AxisRest) -> AxisSpec {
    AxisSpec {
        key: Some(key.to_string()),
        min,
        max,
        rest,
    }
}

pub(crate) fn joystick(size: Option<[f64; 2]>) -> WidgetSpec {
    WidgetSpec {
        kind: WidgetKind::Joystick,
        size,
        x: axis_spec("xc_x", -100.0, 100.0, AxisRest::Value(0.0)),
        y: Some(axis_spec("xc_y", -100.0, 100.0, AxisRest::Value(0.0))),
    }
}

pub(crate) fn slider(size: Option<[f64; 2]>) -> WidgetSpec {
    WidgetSpec {
        kind: WidgetKind::Slider,
        size,
        x: axis_spec("xc_v", 0.0, 100.0, AxisRest::Value(0.0)),
        y: Some(axis_spec("xc_y", -100.0, 100.0, AxisRest::Absent)),
    }
}

pub(crate) fn placed(x: f64, y: f64, angle: f64) -> Placement {
    Placement {
        anchor: [x, y],
        angle_degrees: angle,
    }
}

pub(crate) fn table(pairs: &[(&str, AxisSample)]) -> TableSource {
    TableSource::new(
        pairs
            .iter()
            .map(|(k, s)| (k.to_string(), s.clone()))
            .collect(),
        true,
    )
}

fn values(v: &[f64]) -> AxisSample {
    AxisSample::Values(v.to_vec())
}

pub(crate) fn assert_run(name: &str, op: &str, run: Run) {
    let want = ROW_COUNTS.iter().find(|(o, _)| *o == op).expect("op").1;
    assert_eq!(run.0, want, "{name}: oracle rows for {op}");
    assert!(run.1.is_empty(), "{name}: wrong rows {:?}", run.1);
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

#[test]
fn oracle_row_counts_are_pinned() {
    for (op, count) in ROW_COUNTS {
        assert_eq!(rows(op).count(), count, "rows for {op}");
    }
    assert_eq!(super::oracle_rows::ORACLE.lines().count(), 814);
}

#[test]
fn clamp_span_and_value_rows_match_baseline() {
    assert_run("clamp01", "c01", c01_run(&clamp01));
    assert_run("span", "spn", span_run(&span));
    assert_run("value", "vfl", value_run(&value_from_local));
}

#[test]
fn geometry_and_transform_rows_match_baseline() {
    assert_run("geom", "geo", geo_run(&|s, p, f| geom_for(s, p, f)));
    assert_run("world", "tw", map_run("tw", &to_world));
    assert_run("local", "tl", map_run("tl", &to_local));
}

#[test]
fn puck_rest_write_and_axis_rows_match_baseline() {
    assert_run("rest", "rl", rest_run(&rest_local));
    assert_run("puck", "pk", puck_run(&puck_local, &local_from_values));
    assert_run("write", "wfl", write_run(&write_from_local));
    assert_run("axis", "axv", axis_run(&axis_value));
}

#[test]
fn rotated_sized_joystick_maps_values_to_the_baseline_world_point() {
    let spec = joystick(Some([200.0, 200.0]));
    let g = geom_for(&spec, &placed(960.0, 540.0, 30.0), 5.0);
    assert!(close(g.cos, 0.8660254037844387, 1e-15));
    assert!(close(g.sin, 0.49999999999999994, 1e-15));
    assert_eq!(
        (g.w, g.h, g.cx, g.cy, g.angle),
        (200.0, 200.0, 960.0, 540.0, 30.0)
    );
    let src = table(&[("xc_x", values(&[50.0])), ("xc_y", values(&[-50.0]))]);
    let local = puck_local(&spec, &g, &src).expect("both axes sampled");
    assert_eq!(local, [50.0, 50.0]);
    let world = to_world(&g, local[0], local[1]);
    assert!(close(world[0], 978.3012701892219, 1e-12), "{world:?}");
    assert!(close(world[1], 608.3012701892219, 1e-12), "{world:?}");
    let back = to_local(&g, world[0], world[1]);
    assert!(
        close(back[0], 50.0, 1e-12) && close(back[1], 50.0, 1e-12),
        "{back:?}"
    );
}

#[test]
fn local_world_round_trip_holds_across_angles_and_anchors() {
    let spec = joystick(None);
    for angle in [0.0, 30.0, 90.0, -45.0, 180.0, 270.5, 720.0] {
        for anchor in [[960.0, 540.0], [-37.25, 812.125], [1e5, -1e5]] {
            let g = geom_for(&spec, &placed(anchor[0], anchor[1], angle), 0.0);
            for p in [[0.0, 0.0], [50.0, 50.0], [-80.0, 30.5]] {
                let w = to_world(&g, p[0], p[1]);
                let l = to_local(&g, w[0], w[1]);
                assert!(
                    close(l[0], p[0], 1e-9) && close(l[1], p[1], 1e-9),
                    "{angle} {anchor:?} {p:?} {l:?}"
                );
            }
        }
    }
}

#[test]
fn y_is_inverted_so_the_axis_maximum_sits_at_the_top() {
    let spec = joystick(Some([200.0, 200.0]));
    let g = geom_for(&spec, &placed(0.0, 0.0, 0.0), 0.0);
    let at = |x: f64, y: f64| {
        let src = table(&[("xc_x", values(&[x])), ("xc_y", values(&[y]))]);
        puck_local(&spec, &g, &src).expect("available")
    };
    assert_eq!(at(0.0, 100.0), [0.0, -100.0]);
    assert_eq!(at(0.0, -100.0), [0.0, 100.0]);
    assert_eq!(at(100.0, 0.0), [100.0, 0.0]);
    assert_eq!(at(-100.0, 0.0), [-100.0, 0.0]);
    assert_eq!(
        at(1e6, -1e6),
        [100.0, 100.0],
        "values beyond the range clamp to the pad edge"
    );
}

#[test]
fn slider_ignores_y_both_when_reading_and_when_writing() {
    let spec = slider(None);
    let g = geom_for(&spec, &placed(100.0, 100.0, 0.0), 3.0);
    assert_eq!((g.w, g.h), (200.0, 26.0));
    let src = table(&[("xc_v", values(&[25.0])), ("xc_y", values(&[100.0]))]);
    assert_eq!(puck_local(&spec, &g, &src), Ok([-50.0, 0.0]));
    assert_eq!(src.log_token(), format!("xc_v@{:016x}", 3.0f64.to_bits()));
    let w = write_from_local(&spec, &g, 100.0, 999.0).expect("x is addressable");
    assert_eq!((w.x.key.as_str(), w.x.value), ("xc_v", 100.0));
    assert!(w.y.is_none());
    assert_eq!(local_from_values(&spec, &g, 0.0, 100.0)[1], 0.0);
}

#[test]
fn kind_outside_joystick_slider_is_a_typed_invalid() {
    for bad in ["dial", "JOYSTICK", "Slider", "", "joystick ", "xy"] {
        assert_eq!(
            WidgetKind::parse(bad),
            Err(WidgetGeometryError::InvalidKind),
            "{bad:?}"
        );
    }
    assert_eq!(WidgetKind::parse("joystick"), Ok(WidgetKind::Joystick));
    assert_eq!(WidgetKind::parse("slider"), Ok(WidgetKind::Slider));
}

#[test]
fn pad_edges_clamp_to_eight_and_default_per_kind() {
    let edges = |spec: &WidgetSpec| {
        let g = geom_for(spec, &placed(0.0, 0.0, 0.0), 0.0);
        (g.w, g.h)
    };
    assert_eq!(edges(&joystick(Some([4.0, 4.0]))), (8.0, 8.0));
    assert_eq!(edges(&joystick(Some([8.0, 7.999]))), (8.0, 8.0));
    assert_eq!(edges(&joystick(Some([123.456, 78.9]))), (123.456, 78.9));
    assert_eq!(edges(&joystick(Some([-50.0, 300.0]))), (8.0, 300.0));
    assert_eq!(edges(&joystick(Some([-0.0, 1e6]))), (8.0, 1e6));
    assert_eq!(edges(&joystick(None)), (160.0, 160.0));
    assert_eq!(edges(&slider(None)), (200.0, 26.0));
    let nan = edges(&joystick(Some([f64::NAN, 100.0])));
    assert!(
        nan.0.is_nan() && nan.1 == 100.0,
        "Math.max(8, NaN) is NaN, not 8"
    );
    assert_eq!(WidgetKind::Joystick.default_size(), [160.0, 160.0]);
    assert_eq!(WidgetKind::Slider.default_size(), [200.0, 26.0]);
}

#[test]
fn value_from_local_clamps_then_rounds_half_toward_positive_infinity() {
    let unit = axis_spec("k", 0.0, 1.0, AxisRest::Absent);
    assert_eq!(value_from_local(&unit, 0.0005), 0.001);
    assert_eq!(value_from_local(&unit, 0.0015), 0.002);
    assert_eq!(value_from_local(&unit, 0.0025), 0.003);
    assert_eq!(value_from_local(&unit, 0.0014999), 0.001);
    assert_eq!(value_from_local(&unit, 0.5), 0.5);
    assert_eq!(value_from_local(&unit, 1.0 / 3.0), 0.333);
    assert_eq!(
        value_from_local(&unit, -3.0),
        0.0,
        "t below 0 clamps to the minimum"
    );
    assert_eq!(
        value_from_local(&unit, 3.0),
        1.0,
        "t above 1 clamps to the maximum"
    );
    assert!(
        value_from_local(&unit, f64::NAN).is_nan(),
        "NaN passes through clamp01"
    );
    let negative =
        |lo: f64, hi: f64| value_from_local(&axis_spec("k", lo, hi, AxisRest::Absent), 0.0);
    let tie = negative(-0.0005, 0.0005);
    assert!(
        tie == 0.0 && tie.is_sign_negative(),
        "-0.5 rounds to -0, not -1: {tie:?}"
    );
    assert_eq!(negative(-0.0025, 0.0025), -0.002);
    assert_eq!(negative(-0.0015, 0.0015), -0.001);
    assert_eq!(negative(0.0025, 0.0035), 0.003);
    let reversed = axis_spec("k", 100.0, -100.0, AxisRest::Absent);
    assert_eq!(value_from_local(&reversed, 0.25), 50.0);
}

#[test]
fn narrow_span_collapses_to_one() {
    assert_eq!(span(5.0, 5.0), 1.0);
    assert_eq!(span(0.0, 1e-10), 1.0);
    assert_eq!(span(0.0, -1e-10), 1.0);
    assert_eq!(span(0.0, 2e-9), 2e-9);
    assert_eq!(span(100.0, -100.0), -200.0);
    assert!(span(f64::NAN, 1.0).is_nan());
    let flat = axis_spec("k", 5.0, 5.0, AxisRest::Absent);
    assert_eq!(value_from_local(&flat, 0.75), 5.75);
}

#[test]
fn rest_is_absent_null_or_a_number_and_the_three_stay_distinct() {
    let rest_of = |rest: AxisRest| {
        let mut spec = slider(None);
        spec.x = axis_spec("a", -100.0, 100.0, rest);
        let g = geom_for(&spec, &placed(0.0, 0.0, 0.0), 0.0);
        rest_local(&spec, &g)
    };
    assert_eq!(
        rest_of(AxisRest::Absent),
        [-100.0, 0.0],
        "absent rest is the minimum"
    );
    assert_eq!(
        rest_of(AxisRest::Value(0.0)),
        [0.0, 0.0],
        "an explicit 0 differs from absent"
    );
    assert_eq!(rest_of(AxisRest::Null), [0.0, 0.0], "null reads as 0");
    assert_eq!(
        rest_of(AxisRest::Value(500.0)),
        [100.0, 0.0],
        "rest beyond the range clamps"
    );
    let g = geom_for(&joystick(None), &placed(0.0, 0.0, 0.0), 0.0);
    assert_eq!(rest_local(&joystick(None), &g), [0.0, 0.0]);
}

#[test]
fn missing_controls_are_typed_unavailable_never_a_defaulted_zero() {
    let spec = joystick(Some([200.0, 200.0]));
    let g = geom_for(&spec, &placed(0.0, 0.0, 0.0), 9.0);
    let un = |axis: AxisSlot, reason: AxisUnavailable| -> Result<[f64; 2], WidgetGeometryError> {
        Err(WidgetGeometryError::AxisUnavailable { axis, reason })
    };
    let empty = table(&[]);
    assert_eq!(
        puck_local(&spec, &g, &empty),
        un(AxisSlot::X, AxisUnavailable::ControlAbsent)
    );
    let no_y = table(&[("xc_x", values(&[10.0]))]);
    assert_eq!(
        puck_local(&spec, &g, &no_y),
        un(AxisSlot::Y, AxisUnavailable::ControlAbsent)
    );
    let blank = table(&[("xc_x", values(&[])), ("xc_y", values(&[1.0]))]);
    assert_eq!(
        puck_local(&spec, &g, &blank),
        un(AxisSlot::X, AxisUnavailable::EmptySample)
    );
    let none = TableSource::new(Vec::new(), false);
    assert_eq!(
        puck_local(&spec, &g, &none),
        un(AxisSlot::X, AxisUnavailable::ProviderMissing)
    );
    let mut keyless = spec.clone();
    keyless.y = Some(AxisSpec {
        key: None,
        ..axis_spec("", -100.0, 100.0, AxisRest::Absent)
    });
    let ok_x = table(&[("xc_x", values(&[10.0]))]);
    assert_eq!(
        puck_local(&keyless, &g, &ok_x),
        un(AxisSlot::Y, AxisUnavailable::NoKey)
    );
    keyless.y = Some(axis_spec("", -100.0, 100.0, AxisRest::Absent));
    assert_eq!(
        puck_local(&keyless, &g, &ok_x),
        un(AxisSlot::Y, AxisUnavailable::NoKey)
    );
    // The baseline's silent 0 is only reachable by saying so through the pure core.
    assert_eq!(local_from_values(&spec, &g, 0.0, 0.0), [0.0, 0.0]);
    let zero_slider = slider(Some([200.0, 26.0]));
    let gs = geom_for(&zero_slider, &placed(0.0, 0.0, 0.0), 0.0);
    assert_eq!(
        local_from_values(&zero_slider, &gs, 0.0, 0.0),
        [-100.0, 0.0]
    );
    assert_eq!(
        puck_local(&zero_slider, &gs, &empty),
        un(AxisSlot::X, AxisUnavailable::ControlAbsent)
    );
}

#[test]
fn axis_value_reports_each_reason_and_reads_only_the_first_element() {
    let ax = axis_spec("xc_x", -100.0, 100.0, AxisRest::Absent);
    let src = table(&[("xc_x", values(&[5.0, 6.0])), ("nan", values(&[f64::NAN]))]);
    assert_eq!(axis_value(Some(&ax), 2.0, &src), Ok(5.0));
    let nan = axis_spec("nan", 0.0, 1.0, AxisRest::Absent);
    assert!(axis_value(Some(&nan), 2.0, &src)
        .expect("a NaN sample is a value")
        .is_nan());
    assert_eq!(axis_value(None, 2.0, &src), Err(AxisUnavailable::NoAxis));
    let null_key = AxisSpec {
        key: None,
        ..ax.clone()
    };
    assert_eq!(
        axis_value(Some(&null_key), 2.0, &src),
        Err(AxisUnavailable::NoKey)
    );
    let empty_key = axis_spec("", 0.0, 1.0, AxisRest::Absent);
    assert_eq!(
        axis_value(Some(&empty_key), 2.0, &src),
        Err(AxisUnavailable::NoKey)
    );
    let ghost = axis_spec("ghost", 0.0, 1.0, AxisRest::Absent);
    assert_eq!(
        axis_value(Some(&ghost), 2.0, &src),
        Err(AxisUnavailable::ControlAbsent)
    );
    let blank = table(&[("xc_x", values(&[]))]);
    assert_eq!(
        axis_value(Some(&ax), 2.0, &blank),
        Err(AxisUnavailable::EmptySample)
    );
    let none = TableSource::new(Vec::new(), false);
    assert_eq!(
        axis_value(Some(&ax), 2.0, &none),
        Err(AxisUnavailable::ProviderMissing)
    );
    assert_eq!(
        none.log_token(),
        "_",
        "no sampler call is made without a provider"
    );
    let f = format!("{:016x}", 2.0f64.to_bits());
    assert_eq!(
        src.log_token(),
        format!("xc_x@{f};nan@{f};ghost@{f}"),
        "only axes with a usable key reach the sampler"
    );
}

#[test]
fn both_axes_are_sampled_in_order_even_when_the_first_is_unavailable() {
    let spec = joystick(None);
    let g = geom_for(&spec, &placed(0.0, 0.0, 0.0), 7.5);
    let src = table(&[("xc_y", values(&[1.0]))]);
    assert!(puck_local(&spec, &g, &src).is_err());
    let f = format!("{:016x}", 7.5f64.to_bits());
    assert_eq!(src.log_token(), format!("xc_x@{f};xc_y@{f}"));
}

#[test]
fn write_from_local_returns_baseline_values_and_is_atomic() {
    let spec = joystick(Some([200.0, 100.0]));
    let g = geom_for(&spec, &placed(0.0, 0.0, 0.0), 0.0);
    let w = write_from_local(&spec, &g, 50.0, 25.0).expect("both addressable");
    assert_eq!((w.x.key.as_str(), w.x.value), ("xc_x", 50.0));
    let y = w.y.expect("joystick writes y");
    assert_eq!((y.key.as_str(), y.value), ("xc_y", -50.0));
    let far = write_from_local(&spec, &g, -1e3, 1e3).expect("clamped");
    assert_eq!((far.x.value, far.y.expect("y").value), (-100.0, -100.0));
    let top = write_from_local(&spec, &g, 0.0, -50.0).expect("top edge");
    assert_eq!(
        top.y.expect("y").value,
        100.0,
        "pad top is the axis maximum"
    );
    let mut blind = spec.clone();
    blind.y = Some(AxisSpec {
        key: None,
        ..axis_spec("", -100.0, 100.0, AxisRest::Absent)
    });
    assert_eq!(
        write_from_local(&blind, &g, 50.0, 25.0),
        Err(WidgetGeometryError::AxisUnavailable {
            axis: AxisSlot::Y,
            reason: AxisUnavailable::NoKey
        }),
        "an unaddressable Y axis publishes no write at all, not even X"
    );
    let mut keyless_x = spec;
    keyless_x.x.key = Some(String::new());
    assert_eq!(
        write_from_local(&keyless_x, &g, 50.0, 25.0),
        Err(WidgetGeometryError::AxisUnavailable {
            axis: AxisSlot::X,
            reason: AxisUnavailable::NoKey
        })
    );
}

#[test]
fn writing_the_puck_position_reads_back_as_the_same_value() {
    let spec = joystick(Some([200.0, 200.0]));
    let g = geom_for(&spec, &placed(0.0, 0.0, 0.0), 0.0);
    for (vx, vy) in [
        (-100.0, 100.0),
        (-33.333, 12.5),
        (0.0, 0.0),
        (12.5, -33.333),
        (100.0, -100.0),
    ] {
        let src = table(&[("xc_x", values(&[vx])), ("xc_y", values(&[vy]))]);
        let p = puck_local(&spec, &g, &src).expect("available");
        let w = write_from_local(&spec, &g, p[0], p[1]).expect("addressable");
        assert_eq!((w.x.value, w.y.expect("y").value), (vx, vy));
    }
}

#[test]
fn js_round_is_math_round() {
    assert_eq!(js_round(2.5), 3.0);
    assert_eq!(js_round(-2.5), -2.0);
    assert_eq!(js_round(0.49999999999999994), 0.0);
    assert!(js_round(-0.4).is_sign_negative() && js_round(-0.4) == 0.0);
    assert!(js_round(f64::NAN).is_nan());
    assert_eq!(js_round(f64::INFINITY), f64::INFINITY);
}
