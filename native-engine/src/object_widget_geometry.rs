//! Rig-widget geometry and value-mapping kernel (F19), pure and I/O free.
//!
//! Mirrors the numeric core of `src/js/rig-widget.js` at fbe4b8a3: `widgetOf`
//! (kind check), `span`, `clamp01`, `axisValue`, the size clamp and rotation
//! trig of `geomFor`, `toWorld`, `toLocal`, `puckLocal`, `restLocal`,
//! `valueFromLocal` and `writeFromLocal`. A widget owns no value of its own: each axis
//! points at an ordinary expression control by key, and this kernel only maps
//! between that control's number and a puck offset on the pad.
//!
//! Boundaries kept on purpose:
//! - the anchor and accumulated angle arrive ALREADY COMPOSED (the layer's own
//!   motion plus the parent chain, a different provider); no transform
//!   composition happens here, only the pad's own rotation about the anchor;
//! - control values arrive through [`AxisValueSource`], the port the raw
//!   control-sampling owner implements; this kernel keeps no control catalog
//!   and evaluates no expression;
//! - the baseline reads a missing control as a silent `0` (`axisValue`). This
//!   kernel does NOT default: a null axis, a null or empty key, a missing
//!   sampling provider, an absent control and an empty sample are each a
//!   typed [`AxisUnavailable`]. [`local_from_values`] is the explicit pure
//!   core, so a caller that wants the baseline's `0` has to say so.
//!
//! Semantics kept from the baseline:
//! - kind is exactly `"joystick"` or `"slider"` (case sensitive), anything
//!   else is [`WidgetGeometryError::InvalidKind`]; a slider never reads or
//!   writes its `y` axis;
//! - the pad edge is `max(8, size)` with JavaScript `Math.max` (a NaN edge
//!   stays NaN); an absent size falls back to the per-kind default;
//! - X grows right, Y is INVERTED: the axis maximum sits at the top
//!   (`local y = -h / 2`); `puckLocal` clamps each axis fraction into `[0,1]`,
//!   NaN passes through `clamp01` unchanged;
//! - `span` is `max - min`, or `1` when its magnitude is below `1e-9`;
//! - `rest` is absent (use `min`), `null` (reads as `0`) or a number, three
//!   distinct states;
//! - `valueFromLocal` clamps the fraction, then rounds to `1e-3` with
//!   `Math.round` (ties toward +infinity, `-0.5` gives `-0`).
//!
//! One deliberate difference: [`write_from_local`] refuses a write whose axis
//! has no control key (the baseline passes the null key on to the writer);
//! nothing is emitted unless every axis it would write is addressable.
//!
//! Not provided: the widget layer record, creation, drag/hit-testing, overlay
//! drawing, label lookup, the puck motion path, or the actual control write.

use std::f64::consts::PI;

/// Smallest pad edge (`Math.max(8, size)` in `geomFor`).
pub(crate) const MIN_EDGE: f64 = 8.0;

/// `Math.abs(span) < 1e-9` collapses to a span of one.
const SPAN_EPSILON: f64 = 1e-9;

/// The two widget kinds the baseline recognises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WidgetKind {
    Joystick,
    Slider,
}

impl WidgetKind {
    /// `widgetOf`'s kind test: exact, case-sensitive.
    pub(crate) fn parse(text: &str) -> Result<WidgetKind, WidgetGeometryError> {
        match text {
            "joystick" => Ok(WidgetKind::Joystick),
            "slider" => Ok(WidgetKind::Slider),
            _ => Err(WidgetGeometryError::InvalidKind),
        }
    }

    /// `DEFAULTS[kind].size`.
    pub(crate) fn default_size(self) -> [f64; 2] {
        match self {
            WidgetKind::Joystick => [160.0, 160.0],
            WidgetKind::Slider => [200.0, 26.0],
        }
    }
}

/// Which axis of a widget a result refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AxisSlot {
    X,
    Y,
}

/// Why an axis has no usable control value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AxisUnavailable {
    /// The axis object itself is missing.
    NoAxis,
    /// The axis has a null or empty control key.
    NoKey,
    /// The control sampling provider is not present.
    ProviderMissing,
    /// The provider has no control under that key (undefined or null sample).
    ControlAbsent,
    /// The provider returned an empty sample.
    EmptySample,
}

/// Typed refusal of an input the kernel will not guess at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WidgetGeometryError {
    /// The widget kind is neither `joystick` nor `slider`.
    InvalidKind,
    /// An axis has no usable control value or key.
    AxisUnavailable {
        axis: AxisSlot,
        reason: AxisUnavailable,
    },
}

/// What the control sampling provider answered for one key at one frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum AxisSample {
    ProviderMissing,
    Absent,
    Values(Vec<f64>),
}

/// Port to the raw control sampler (`SMMotion.valueAtFrame`): the sample of
/// the control `key` of this widget's layer at `frame`.
pub(crate) trait AxisValueSource {
    fn sample(&self, key: &str, frame: f64) -> AxisSample;
}

/// The axis `rest` field: three states the baseline keeps apart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum AxisRest {
    /// `undefined`: rest is the axis minimum.
    Absent,
    /// `null`: reads as `0`.
    Null,
    Value(f64),
}

/// One widget axis: a control key and the presentation range over it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct AxisSpec {
    pub(crate) key: Option<String>,
    pub(crate) min: f64,
    pub(crate) max: f64,
    pub(crate) rest: AxisRest,
}

/// The stored widget record, as far as the geometry reads it.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WidgetSpec {
    pub(crate) kind: WidgetKind,
    pub(crate) size: Option<[f64; 2]>,
    pub(crate) x: AxisSpec,
    pub(crate) y: Option<AxisSpec>,
}

/// The composed placement: world anchor and accumulated angle in degrees.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Placement {
    pub(crate) anchor: [f64; 2],
    pub(crate) angle_degrees: f64,
}

/// `geomFor`'s result for one widget at one frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct WidgetGeom {
    pub(crate) frame: f64,
    pub(crate) cx: f64,
    pub(crate) cy: f64,
    pub(crate) angle: f64,
    pub(crate) cos: f64,
    pub(crate) sin: f64,
    pub(crate) w: f64,
    pub(crate) h: f64,
}

/// One control write the widget asks for.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ControlWrite {
    pub(crate) key: String,
    pub(crate) value: f64,
}

/// `writeFromLocal`'s writes: always X, Y only for a joystick with a Y axis.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WidgetWrites {
    pub(crate) x: ControlWrite,
    pub(crate) y: Option<ControlWrite>,
}

/// `Math.round`: nearest integer, ties toward +infinity, sign of zero kept.
pub(crate) fn js_round(x: f64) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let floor = x.floor();
    let rounded = if x - floor >= 0.5 { floor + 1.0 } else { floor };
    if rounded == 0.0 && x.is_sign_negative() {
        -0.0
    } else {
        rounded
    }
}

/// `clamp01`: NaN is neither below 0 nor above 1 and passes through.
pub(crate) fn clamp01(t: f64) -> f64 {
    if t < 0.0 {
        0.0
    } else if t > 1.0 {
        1.0
    } else {
        t
    }
}

/// `span`: `max - min`, or 1 for a span narrower than `1e-9`.
pub(crate) fn span(min: f64, max: f64) -> f64 {
    let s = max - min;
    if s.abs() < SPAN_EPSILON {
        1.0
    } else {
        s
    }
}

/// `Math.max(8, v)`.
fn pad_edge(v: f64) -> f64 {
    if v.is_nan() || v > MIN_EDGE {
        v
    } else {
        MIN_EDGE
    }
}

/// `geomFor` without the transform composition: pad edges and rotation trig.
pub(crate) fn geom_for(spec: &WidgetSpec, placement: &Placement, frame: f64) -> WidgetGeom {
    let size = spec.size.unwrap_or_else(|| spec.kind.default_size());
    let angle = placement.angle_degrees;
    let radians = angle * PI / 180.0;
    WidgetGeom {
        frame,
        cx: placement.anchor[0],
        cy: placement.anchor[1],
        angle,
        cos: radians.cos(),
        sin: radians.sin(),
        w: pad_edge(size[0]),
        h: pad_edge(size[1]),
    }
}

/// Pad-local offset to world.
pub(crate) fn to_world(g: &WidgetGeom, lx: f64, ly: f64) -> [f64; 2] {
    [
        g.cx + lx * g.cos - ly * g.sin,
        g.cy + lx * g.sin + ly * g.cos,
    ]
}

/// World point to pad-local offset.
pub(crate) fn to_local(g: &WidgetGeom, wx: f64, wy: f64) -> [f64; 2] {
    let dx = wx - g.cx;
    let dy = wy - g.cy;
    [dx * g.cos + dy * g.sin, -dx * g.sin + dy * g.cos]
}

/// `axisValue` without its silent `0`: the first element of the sample.
pub(crate) fn axis_value(
    axis: Option<&AxisSpec>,
    frame: f64,
    source: &dyn AxisValueSource,
) -> Result<f64, AxisUnavailable> {
    let Some(axis) = axis else {
        return Err(AxisUnavailable::NoAxis);
    };
    let key = match axis.key.as_deref() {
        Some(key) if !key.is_empty() => key,
        _ => return Err(AxisUnavailable::NoKey),
    };
    match source.sample(key, frame) {
        AxisSample::ProviderMissing => Err(AxisUnavailable::ProviderMissing),
        AxisSample::Absent => Err(AxisUnavailable::ControlAbsent),
        AxisSample::Values(values) => values.first().copied().ok_or(AxisUnavailable::EmptySample),
    }
}

/// The numeric rest value of an axis.
fn rest_value(axis: &AxisSpec) -> f64 {
    match axis.rest {
        AxisRest::Absent => axis.min,
        AxisRest::Null => 0.0,
        AxisRest::Value(rest) => rest,
    }
}

/// Axis value to a `[0,1]` pad fraction (NaN stays NaN).
fn fraction(axis: &AxisSpec, value: f64) -> f64 {
    clamp01((value - axis.min) / span(axis.min, axis.max))
}

/// The pad-local offset of the given axis values. The shared core of
/// `puckLocal` (values sampled) and `restLocal` (rest values); `vy` is read
/// only for a joystick that has a Y axis.
pub(crate) fn local_from_values(spec: &WidgetSpec, g: &WidgetGeom, vx: f64, vy: f64) -> [f64; 2] {
    let tx = fraction(&spec.x, vx);
    let lx = (tx - 0.5) * g.w;
    let mut ly = 0.0;
    if spec.kind == WidgetKind::Joystick {
        if let Some(y) = &spec.y {
            let ty = fraction(y, vy);
            ly = (0.5 - ty) * g.h;
        }
    }
    [lx, ly]
}

/// `restLocal`: where the puck returns to.
pub(crate) fn rest_local(spec: &WidgetSpec, g: &WidgetGeom) -> [f64; 2] {
    let vy = spec.y.as_ref().map_or(0.0, rest_value);
    local_from_values(spec, g, rest_value(&spec.x), vy)
}

/// `puckLocal`: samples X, then Y (joystick with a Y axis only) and reports
/// the first unavailable axis instead of reading it as `0`.
pub(crate) fn puck_local(
    spec: &WidgetSpec,
    g: &WidgetGeom,
    source: &dyn AxisValueSource,
) -> Result<[f64; 2], WidgetGeometryError> {
    let vx = axis_value(Some(&spec.x), g.frame, source);
    let vy = match (&spec.y, spec.kind) {
        (Some(y), WidgetKind::Joystick) => Some(axis_value(Some(y), g.frame, source)),
        _ => None,
    };
    let vx = vx.map_err(|reason| WidgetGeometryError::AxisUnavailable {
        axis: AxisSlot::X,
        reason,
    })?;
    let vy = match vy {
        Some(result) => result.map_err(|reason| WidgetGeometryError::AxisUnavailable {
            axis: AxisSlot::Y,
            reason,
        })?,
        None => 0.0,
    };
    Ok(local_from_values(spec, g, vx, vy))
}

/// `valueFromLocal`: the control value of a pad fraction, rounded to `1e-3`.
pub(crate) fn value_from_local(axis: &AxisSpec, t: f64) -> f64 {
    let v = axis.min + clamp01(t) * span(axis.min, axis.max);
    js_round(v * 1000.0) / 1000.0
}

/// The control key a write to this axis addresses.
fn write_key(axis: &AxisSpec, slot: AxisSlot) -> Result<&str, WidgetGeometryError> {
    match axis.key.as_deref() {
        Some(key) if !key.is_empty() => Ok(key),
        _ => Err(WidgetGeometryError::AxisUnavailable {
            axis: slot,
            reason: AxisUnavailable::NoKey,
        }),
    }
}

/// `writeFromLocal`: the control writes for a pad-local drag point. Atomic:
/// either every addressed axis yields a write or none does.
pub(crate) fn write_from_local(
    spec: &WidgetSpec,
    g: &WidgetGeom,
    lx: f64,
    ly: f64,
) -> Result<WidgetWrites, WidgetGeometryError> {
    let x_key = write_key(&spec.x, AxisSlot::X)?;
    let y_axis = match (&spec.y, spec.kind) {
        (Some(y), WidgetKind::Joystick) => Some((y, write_key(y, AxisSlot::Y)?)),
        _ => None,
    };
    let x = ControlWrite {
        key: x_key.to_string(),
        value: value_from_local(&spec.x, lx / g.w + 0.5),
    };
    let y = y_axis.map(|(axis, key)| ControlWrite {
        key: key.to_string(),
        value: value_from_local(axis, 0.5 - ly / g.h),
    });
    Ok(WidgetWrites { x, y })
}

#[cfg(test)]
#[path = "../tests/object_widget_geometry_rows.rs"]
mod oracle_rows;

#[cfg(test)]
#[path = "../tests/object_widget_geometry.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_widget_geometry_controls.rs"]
mod controls_tests;
