//! Controls for the rig-widget geometry kernel (L03-B): single-flaw mutants the oracle rows
//! must reject, guards against a vacuous oracle, and the "silent zero" the kernel refuses.
//!
//! A mutant here is a deliberate one-flaw copy of one baseline rule, run through the same row
//! harness as production. With `Flaw::None` each copy must equal production on every row;
//! with a flaw set it must not. The generator (`tools/gen.cjs`) proves that every one of
//! these flaws that exists in JavaScript changes at least one row of the baseline output
//! before the oracle is written (the Rust-only traps `f64::max`/`f64::round`/`clamp` are
//! covered by the rows that carry NaN, ties and negative ties).

use super::oracle_rows::{
    axis_run, c01_run, geo_run, map_run, puck_run, rest_run, rows, span_run, value_run, write_run,
    Run,
};
use super::tests::{assert_run, joystick, slider};
use super::{
    axis_value, clamp01, geom_for, js_round, span, to_local, to_world, value_from_local,
    write_from_local, AxisRest, AxisSample, AxisSlot, AxisSpec, AxisUnavailable, AxisValueSource,
    ControlWrite, Placement, WidgetGeom, WidgetGeometryError, WidgetKind, WidgetSpec, WidgetWrites,
};

#[derive(Clone, Copy, PartialEq)]
enum Flaw {
    None,
    /// `t.max(0.0).min(1.0)`: NaN becomes 0 instead of passing through.
    ClampNanToZero,
    /// No `1e-9` guard on the span.
    SpanGuardOff,
    /// Guard only an exact zero span.
    SpanGuardZeroOnly,
    /// `valueFromLocal` without the `clamp01`.
    Unclamped,
    /// `f64::round`: ties away from zero.
    HalfAway,
    /// Rounds to `1e-2`.
    Grid100,
    /// Pad edge floor 0 instead of 8.
    SizeFloorZero,
    /// `f64::max(8.0, v)`: a NaN edge becomes 8.
    NanEdgeEight,
    /// Joystick and slider default sizes exchanged.
    SwappedDefaults,
    /// `toWorld`/`toLocal` with the sine terms flipped.
    SignFlip,
    /// Y axis not inverted (`ty - 0.5`).
    YNotInverted,
    /// A slider reads and maps its Y axis.
    SliderUsesY,
    /// Absent rest reads as 0 instead of the minimum.
    RestDefaultZero,
    /// Null rest treated as absent (the minimum).
    NullAsAbsent,
    /// Writes a Y value for a slider too.
    WriteIgnoresKind,
    /// The Y write uses the un-inverted fraction.
    WriteYNotInverted,
    /// An empty key is sampled instead of being a typed no-key.
    IgnoresEmptyKey,
    /// An unavailable axis reads as 0 (the baseline), not a typed refusal.
    SilentZero,
}

fn m_clamp(t: f64, flaw: Flaw) -> f64 {
    if flaw == Flaw::ClampNanToZero {
        t.max(0.0).min(1.0)
    } else {
        clamp01(t)
    }
}

fn m_span(min: f64, max: f64, flaw: Flaw) -> f64 {
    let s = max - min;
    match flaw {
        Flaw::SpanGuardOff => s,
        Flaw::SpanGuardZeroOnly => {
            if s == 0.0 {
                1.0
            } else {
                s
            }
        }
        _ => span(min, max),
    }
}

fn m_value(axis: &AxisSpec, t: f64, flaw: Flaw) -> f64 {
    let t = if flaw == Flaw::Unclamped {
        t
    } else {
        m_clamp(t, flaw)
    };
    let v = axis.min + t * m_span(axis.min, axis.max, flaw);
    match flaw {
        Flaw::HalfAway => (v * 1000.0).round() / 1000.0,
        Flaw::Grid100 => js_round(v * 100.0) / 100.0,
        _ => js_round(v * 1000.0) / 1000.0,
    }
}

fn m_geom(spec: &WidgetSpec, p: &Placement, frame: f64, flaw: Flaw) -> WidgetGeom {
    let mut g = geom_for(spec, p, frame);
    let swapped = match spec.kind {
        WidgetKind::Joystick => WidgetKind::Slider,
        WidgetKind::Slider => WidgetKind::Joystick,
    };
    let size = match (flaw, spec.size) {
        (Flaw::SwappedDefaults, None) => swapped.default_size(),
        (_, Some(size)) => size,
        (_, None) => spec.kind.default_size(),
    };
    let edge = |v: f64| match flaw {
        Flaw::SizeFloorZero => {
            if v.is_nan() || v > 0.0 {
                v
            } else {
                0.0
            }
        }
        Flaw::NanEdgeEight => f64::max(8.0, v),
        _ => {
            if v.is_nan() || v > 8.0 {
                v
            } else {
                8.0
            }
        }
    };
    g.w = edge(size[0]);
    g.h = edge(size[1]);
    g
}

fn m_world(g: &WidgetGeom, lx: f64, ly: f64, flaw: Flaw) -> [f64; 2] {
    if flaw == Flaw::SignFlip {
        [
            g.cx + lx * g.cos + ly * g.sin,
            g.cy + lx * g.sin + ly * g.cos,
        ]
    } else {
        to_world(g, lx, ly)
    }
}

fn m_local_of_world(g: &WidgetGeom, wx: f64, wy: f64, flaw: Flaw) -> [f64; 2] {
    if flaw == Flaw::SignFlip {
        let (dx, dy) = (wx - g.cx, wy - g.cy);
        [dx * g.cos - dy * g.sin, dx * g.sin + dy * g.cos]
    } else {
        to_local(g, wx, wy)
    }
}

fn reads_y(spec: &WidgetSpec, flaw: Flaw) -> bool {
    spec.kind == WidgetKind::Joystick || flaw == Flaw::SliderUsesY
}

fn m_local(spec: &WidgetSpec, g: &WidgetGeom, vx: f64, vy: f64, flaw: Flaw) -> [f64; 2] {
    let frac = |a: &AxisSpec, v: f64| m_clamp((v - a.min) / span(a.min, a.max), flaw);
    let lx = (frac(&spec.x, vx) - 0.5) * g.w;
    let mut ly = 0.0;
    if let (true, Some(y)) = (reads_y(spec, flaw), &spec.y) {
        let ty = frac(y, vy);
        ly = if flaw == Flaw::YNotInverted {
            (ty - 0.5) * g.h
        } else {
            (0.5 - ty) * g.h
        };
    }
    [lx, ly]
}

fn m_rest_value(axis: &AxisSpec, flaw: Flaw) -> f64 {
    match axis.rest {
        AxisRest::Absent if flaw == Flaw::RestDefaultZero => 0.0,
        AxisRest::Absent => axis.min,
        AxisRest::Null if flaw == Flaw::NullAsAbsent => axis.min,
        AxisRest::Null => 0.0,
        AxisRest::Value(rest) => rest,
    }
}

fn m_rest(spec: &WidgetSpec, g: &WidgetGeom, flaw: Flaw) -> [f64; 2] {
    let vy = spec.y.as_ref().map_or(0.0, |y| m_rest_value(y, flaw));
    m_local(spec, g, m_rest_value(&spec.x, flaw), vy, flaw)
}

fn m_axis(
    axis: Option<&AxisSpec>,
    frame: f64,
    src: &dyn AxisValueSource,
    flaw: Flaw,
) -> Result<f64, AxisUnavailable> {
    if flaw == Flaw::IgnoresEmptyKey && axis.and_then(|a| a.key.as_deref()) == Some("") {
        return match src.sample("", frame) {
            AxisSample::Values(v) => v.first().copied().ok_or(AxisUnavailable::EmptySample),
            AxisSample::Absent => Err(AxisUnavailable::ControlAbsent),
            AxisSample::ProviderMissing => Err(AxisUnavailable::ProviderMissing),
        };
    }
    let got = axis_value(axis, frame, src);
    if flaw == Flaw::SilentZero {
        return Ok(got.unwrap_or(0.0));
    }
    got
}

fn m_puck(
    spec: &WidgetSpec,
    g: &WidgetGeom,
    src: &dyn AxisValueSource,
    flaw: Flaw,
) -> Result<[f64; 2], WidgetGeometryError> {
    let vx = m_axis(Some(&spec.x), g.frame, src, flaw);
    let vy = match (&spec.y, reads_y(spec, flaw)) {
        (Some(y), true) => Some(m_axis(Some(y), g.frame, src, flaw)),
        _ => None,
    };
    let at = |axis, reason| WidgetGeometryError::AxisUnavailable { axis, reason };
    let vx = vx.map_err(|reason| at(AxisSlot::X, reason))?;
    let vy = match vy {
        Some(result) => result.map_err(|reason| at(AxisSlot::Y, reason))?,
        None => 0.0,
    };
    Ok(m_local(spec, g, vx, vy, flaw))
}

fn m_write(
    spec: &WidgetSpec,
    g: &WidgetGeom,
    lx: f64,
    ly: f64,
    flaw: Flaw,
) -> Result<WidgetWrites, WidgetGeometryError> {
    let mut w = write_from_local(spec, g, lx, ly)?;
    let fraction = if flaw == Flaw::WriteYNotInverted {
        ly / g.h + 0.5
    } else {
        0.5 - ly / g.h
    };
    if matches!(flaw, Flaw::WriteIgnoresKind | Flaw::WriteYNotInverted) {
        if let Some(y) = &spec.y {
            w.y = Some(ControlWrite {
                key: y.key.clone().unwrap_or_default(),
                value: m_value(y, fraction, Flaw::None),
            });
        }
    }
    Ok(w)
}

/// Every runner with the given flaw; production must equal `Flaw::None`.
fn all_runs(flaw: Flaw) -> Vec<(&'static str, Run)> {
    vec![
        ("c01", c01_run(&|t| m_clamp(t, flaw))),
        ("spn", span_run(&|a, b| m_span(a, b, flaw))),
        ("vfl", value_run(&|ax, t| m_value(ax, t, flaw))),
        ("geo", geo_run(&|s, p, f| m_geom(s, p, f, flaw))),
        ("tw", map_run("tw", &|g, x, y| m_world(g, x, y, flaw))),
        (
            "tl",
            map_run("tl", &|g, x, y| m_local_of_world(g, x, y, flaw)),
        ),
        ("rl", rest_run(&|s, g| m_rest(s, g, flaw))),
        (
            "pk",
            puck_run(&|s, g, src| m_puck(s, g, src, flaw), &|s, g, vx, vy| {
                m_local(s, g, vx, vy, flaw)
            }),
        ),
        ("wfl", write_run(&|s, g, x, y| m_write(s, g, x, y, flaw))),
        ("axv", axis_run(&|a, f, src| m_axis(a, f, src, flaw))),
    ]
}

/// Ops whose rows the flaw changes (the rest must stay clean).
fn caught(flaw: Flaw, ops: &[&str]) {
    for (op, run) in all_runs(flaw) {
        if ops.contains(&op) {
            assert!(!run.1.is_empty(), "{op} rows must reject the flaw");
        }
    }
}

#[test]
fn the_flaw_free_copies_equal_production_on_every_row() {
    for (op, run) in all_runs(Flaw::None) {
        assert_run("flaw-free copy", op, run);
    }
}

#[test]
fn nan_and_span_flaws_are_rejected() {
    caught(Flaw::ClampNanToZero, &["c01", "vfl", "pk"]);
    caught(Flaw::SpanGuardOff, &["spn", "vfl"]);
    caught(Flaw::SpanGuardZeroOnly, &["spn", "vfl"]);
}

#[test]
fn value_mapping_flaws_are_rejected() {
    caught(Flaw::Unclamped, &["vfl"]);
    caught(Flaw::HalfAway, &["vfl"]);
    caught(Flaw::Grid100, &["vfl"]);
}

#[test]
fn pad_geometry_flaws_are_rejected() {
    caught(Flaw::SizeFloorZero, &["geo"]);
    caught(Flaw::NanEdgeEight, &["geo"]);
    caught(Flaw::SwappedDefaults, &["geo"]);
    caught(Flaw::SignFlip, &["tw", "tl"]);
}

#[test]
fn puck_rest_and_write_flaws_are_rejected() {
    caught(Flaw::YNotInverted, &["rl", "pk"]);
    caught(Flaw::SliderUsesY, &["pk"]);
    caught(Flaw::RestDefaultZero, &["rl"]);
    caught(Flaw::NullAsAbsent, &["rl"]);
    caught(Flaw::WriteIgnoresKind, &["wfl"]);
    caught(Flaw::WriteYNotInverted, &["wfl"]);
}

#[test]
fn the_baseline_silent_zero_is_rejected_for_unavailable_axes() {
    caught(Flaw::SilentZero, &["pk", "axv"]);
    caught(Flaw::IgnoresEmptyKey, &["axv"]);
}

#[test]
fn a_constant_function_cannot_pass_the_rows() {
    // Rows whose expected output is not a positive zero: a constant 0 gets exactly these wrong.
    assert_eq!(c01_run(&|_| 0.0).1.len(), 11);
    assert_eq!(value_run(&|_, _| 0.0).1.len(), 102);
    assert_eq!(map_run("tw", &|_, _, _| [0.0, 0.0]).1.len(), 99);
    assert_eq!(rest_run(&|_, _| [0.0, 0.0]).1.len(), 30);
}

#[test]
fn the_oracle_is_not_vacuous() {
    let missing = rows("pk")
        .filter(|(_, t)| {
            matches!(t[2].as_bytes()[0], b'u' | b'n' | b'e')
                || matches!(t[3].as_bytes()[0], b'u' | b'n' | b'e')
        })
        .count();
    assert_eq!(missing, 12, "pk rows with a missing control");
    let rotated = rows("tw")
        .chain(rows("tl"))
        .filter(|(_, t)| {
            t[1].split('/').nth(3).map_or(false, |a| {
                a != "0000000000000000" && a != "8000000000000000"
            })
        })
        .count();
    assert_eq!(rotated, 196, "rotated transform rows");
    assert_eq!(rows("geo").filter(|(_, t)| t[2] == "bad").count(), 6);
    let nan_rows = rows("vfl")
        .filter(|(_, t)| t[4] == "7ff8000000000000")
        .count();
    assert_eq!(nan_rows, 9, "vfl rows with a NaN result");
    let silent_zero = rows("axv")
        .filter(|(_, t)| t[4] == "0000000000000000" && !matches!(t[2].as_bytes()[0], b'v' | b'm'))
        .count();
    assert_eq!(silent_zero, 16, "axv rows where the baseline answered 0");
    let slider_with_y = rows("pk")
        .filter(|(_, t)| t[1].starts_with("slider/") && t[3] != "-")
        .count();
    assert_eq!(slider_with_y, 56, "slider rows that carry a Y sample");
}

#[test]
fn a_write_set_is_all_or_nothing() {
    let g = geom_for(
        &joystick(None),
        &Placement {
            anchor: [0.0, 0.0],
            angle_degrees: 0.0,
        },
        0.0,
    );
    let mut spec = joystick(None);
    spec.y = spec.y.map(|y| AxisSpec { key: None, ..y });
    assert!(
        write_from_local(&spec, &g, 1.0, 1.0).is_err(),
        "no X write without a Y write"
    );
    let gs = geom_for(
        &slider(None),
        &Placement {
            anchor: [0.0, 0.0],
            angle_degrees: 0.0,
        },
        0.0,
    );
    let mut keyless_y = slider(None);
    keyless_y.y = keyless_y.y.map(|y| AxisSpec { key: None, ..y });
    let w = write_from_local(&keyless_y, &gs, 10.0, 10.0).expect("a slider never addresses Y");
    assert!(w.y.is_none());
}
