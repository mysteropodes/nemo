//! Effector falloff weights, legacy channel migration and channel summation
//! (L04-C, I08/I09).
//!
//! Mirrors the effector half of `_duplicatorClonePlacement` and
//! `effectorChannels` (`src/js/app.js`) at baseline `fbe4b8a3`: how strongly an
//! effector reaches clone `k` (radial, linear or step falloff times strength),
//! which channel list an effector contributes (`eff.channels` wins, otherwise
//! the legacy `offsetPos/offsetRot/offsetScale/offsetOpacity` fields are folded
//! into channels) and how those channels are added into the running sums of
//! the clone placement.
//!
//! JavaScript semantics are kept on `f64`: `x || d` (undefined, null, 0, -0
//! and NaN read as the default) through [`JsNumber`], `Math.min`/`Math.max`
//! with their NaN and signed-zero rules, and `Math.hypot` as V8 computes it
//! (scale by the larger magnitude, Kahan sum of squares) because `f64::hypot`
//! is not bit-identical to it. `cos`/`sin` of the linear falloff are libm
//! calls, so only values that pass through them can differ from V8's.
//!
//! The in-place migration of the baseline (`eff.channels = ch`) is a returned
//! value here ([`migrated`]); nothing in this module owns shared state.

use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use std::borrow::Cow;
use std::f64::consts::PI;

/// Falloff of an effector. Only `'step'` and `'linear'` are compared with
/// `===` in the baseline: every other label, absent or not, is radial.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Falloff {
    #[default]
    Radial,
    Linear,
    Step,
}

impl Falloff {
    /// `eff.falloff` decided against the two labels the baseline tests.
    pub(crate) fn from_label(label: Option<&str>) -> Falloff {
        match label {
            Some("step") => Falloff::Step,
            Some("linear") => Falloff::Linear,
            _ => Falloff::Radial,
        }
    }

    /// The canonical label of the kind.
    pub(crate) fn label(self) -> &'static str {
        match self {
            Falloff::Radial => "radial",
            Falloff::Linear => "linear",
            Falloff::Step => "step",
        }
    }
}

/// Property a channel adds into, from the `switch (ch.prop)` of the baseline.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChannelProp {
    Position,
    PositionZ,
    Rotation,
    RotationX,
    RotationY,
    Scale,
    Opacity,
    Hue,
    /// Any other label: the `switch` has no case and the channel is ignored.
    Unknown,
}

impl ChannelProp {
    /// Exact, case-sensitive match like the `switch`.
    pub(crate) fn from_label(label: &str) -> ChannelProp {
        match label {
            "position" => ChannelProp::Position,
            "positionZ" => ChannelProp::PositionZ,
            "rotation" => ChannelProp::Rotation,
            "rotationX" => ChannelProp::RotationX,
            "rotationY" => ChannelProp::RotationY,
            "scale" => ChannelProp::Scale,
            "opacity" => ChannelProp::Opacity,
            "hue" => ChannelProp::Hue,
            _ => ChannelProp::Unknown,
        }
    }
}

/// `{prop, value}`. An absent `value` (`ch.value || []`) is the empty list.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Channel {
    pub prop: String,
    pub value: Vec<JsNumber>,
}

impl Channel {
    pub(crate) fn new(prop: &str, value: Vec<JsNumber>) -> Channel {
        Channel {
            prop: prop.to_string(),
            value,
        }
    }

    /// `v[i] || 0`.
    fn component(&self, i: usize) -> f64 {
        at(&self.value, i).or_default(0.0)
    }
}

/// `eff.pos`: a point in canvas coordinates. A missing coordinate of a saved
/// effector is NaN in the arithmetic of the baseline (`undefined`), so the
/// record leaf maps it to NaN.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct EffectorPos {
    pub x: f64,
    pub y: f64,
}

/// An effector as `dup.effectors[i]` or a resolved effector layer holds it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Effector {
    pub pos: Option<EffectorPos>,
    pub falloff: Falloff,
    pub radius: JsNumber,
    pub strength: JsNumber,
    pub angle: JsNumber,
    /// `eff.channels`; `None` (absent or null) means "migrate the legacy fields".
    pub channels: Option<Vec<Channel>>,
    pub offset_pos: Option<Vec<JsNumber>>,
    pub offset_rot: JsNumber,
    pub offset_scale: Option<Vec<JsNumber>>,
    pub offset_opacity: JsNumber,
    /// `_fromLayer`: set on records resolved from an effector layer.
    pub from_layer: bool,
}

/// `list[i]` with `undefined` past the end.
pub(crate) fn at(list: &[JsNumber], i: usize) -> JsNumber {
    list.get(i).copied().unwrap_or(JsNumber::Undefined)
}

/// JavaScript truthiness of a number-or-nothing field.
pub(crate) fn truthy(n: JsNumber) -> bool {
    matches!(n, JsNumber::Value(v) if !(v == 0.0 || v.is_nan()))
}

/// `Math.min`: NaN when either side is NaN, `-0` below `+0`.
pub(crate) fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else if b < a {
        b
    } else if a.is_sign_negative() {
        a
    } else {
        b
    }
}

/// `Math.max`: NaN when either side is NaN, `+0` above `-0`.
pub(crate) fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else if b > a {
        b
    } else if a.is_sign_positive() {
        a
    } else {
        b
    }
}

/// `Math.max(0, Math.min(1, x))`.
pub(crate) fn clamp01(x: f64) -> f64 {
    js_max(0.0, js_min(1.0, x))
}

/// `Math.hypot(x, y)` the way V8 computes it: infinity wins over NaN, a zero
/// maximum is 0, otherwise `sqrt(sum((|v| / max)^2)) * max` with a Kahan sum.
pub(crate) fn js_hypot(x: f64, y: f64) -> f64 {
    let values = [x.abs(), y.abs()];
    let mut max = 0.0_f64;
    let mut any_nan = false;
    for v in values {
        if v.is_nan() {
            any_nan = true;
        } else if v > max {
            max = v;
        }
    }
    if max == f64::INFINITY {
        return f64::INFINITY;
    }
    if any_nan {
        return f64::NAN;
    }
    if max == 0.0 {
        return 0.0;
    }
    let mut sum = 0.0_f64;
    let mut compensation = 0.0_f64;
    for v in values {
        let n = v / max;
        let summand = n * n - compensation;
        let preliminary = sum + summand;
        compensation = (preliminary - sum) - summand;
        sum = preliminary;
    }
    sum.sqrt() * max
}

/// The channels folded out of the legacy offset fields, in the baseline order
/// position, rotation, scale, opacity, with the same truthiness filters.
pub(crate) fn legacy_channels(eff: &Effector) -> Vec<Channel> {
    let mut out = Vec::new();
    if let Some(p) = &eff.offset_pos {
        if truthy(at(p, 0)) || truthy(at(p, 1)) {
            out.push(pair_channel("position", p));
        }
    }
    if truthy(eff.offset_rot) {
        out.push(Channel::new("rotation", vec![eff.offset_rot]));
    }
    if let Some(s) = &eff.offset_scale {
        if truthy(at(s, 0)) || truthy(at(s, 1)) {
            out.push(pair_channel("scale", s));
        }
    }
    if truthy(eff.offset_opacity) {
        out.push(Channel::new("opacity", vec![eff.offset_opacity]));
    }
    out
}

/// `[v[0] || 0, v[1] || 0]`.
fn pair_channel(prop: &str, v: &[JsNumber]) -> Channel {
    let pair = |i: usize| JsNumber::Value(at(v, i).or_default(0.0));
    Channel::new(prop, vec![pair(0), pair(1)])
}

/// `effectorChannels(eff)`: `eff.channels` when present (an empty list too),
/// otherwise the migrated legacy list.
pub(crate) fn effector_channels(eff: &Effector) -> Cow<'_, [Channel]> {
    match &eff.channels {
        Some(list) => Cow::Borrowed(list.as_slice()),
        None => Cow::Owned(legacy_channels(eff)),
    }
}

/// The effector after `effectorChannels` stored its result (`eff.channels =
/// ch`): the legacy fields stay, `channels` is set, a second call changes
/// nothing.
pub(crate) fn migrated(eff: &Effector) -> Effector {
    let mut out = eff.clone();
    if out.channels.is_none() {
        out.channels = Some(legacy_channels(eff));
    }
    out
}

/// The falloff weight of an effector at a clone, before strength:
/// step ramps by clone index (`k / (count - 1)`, 0 for `count <= 1`), the
/// others compare the clone position (`pivot + base offset`) with `eff.pos`
/// (origin when absent) over `radius || 1`.
pub(crate) fn falloff_weight(eff: &Effector, inst_x: f64, inst_y: f64, k: u32, count: f64) -> f64 {
    if eff.falloff == Falloff::Step {
        return if count > 1.0 {
            k as f64 / (count - 1.0)
        } else {
            0.0
        };
    }
    let (px, py) = match eff.pos {
        Some(p) => (p.x, p.y),
        None => (0.0, 0.0),
    };
    let ddx = inst_x - px;
    let ddy = inst_y - py;
    let radius = eff.radius.or_default(1.0);
    if eff.falloff == Falloff::Linear {
        let rad = eff.angle.or_default(0.0) * PI / 180.0;
        let proj = ddx * rad.cos() + ddy * rad.sin();
        clamp01(1.0 - proj / radius)
    } else {
        clamp01(1.0 - js_hypot(ddx, ddy) / radius)
    }
}

/// `eff.strength != null ? eff.strength : 100`: only undefined and null fall
/// back, a strength of 0 stays 0.
pub(crate) fn strength_percent(eff: &Effector) -> f64 {
    match eff.strength {
        JsNumber::Value(v) => v,
        _ => 100.0,
    }
}

/// `w *= strength / 100` after the falloff. A weight of 0 or NaN is skipped by
/// the baseline (`if (!w) return`): see [`is_skipped`].
pub(crate) fn effector_weight(eff: &Effector, inst_x: f64, inst_y: f64, k: u32, count: f64) -> f64 {
    falloff_weight(eff, inst_x, inst_y, k, count) * (strength_percent(eff) / 100.0)
}

/// `!w`.
pub(crate) fn is_skipped(w: f64) -> bool {
    w == 0.0 || w.is_nan()
}

/// The running per-clone sums the effectors add into, in the baseline's
/// names: `posK`, `dzK`, `rotK`, `drxK`, `dryK`, `scaleK`, `opK`, `hueK`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ChannelSums {
    pub pos: [f64; 2],
    pub pos_z: f64,
    pub rot: f64,
    pub rot_x: f64,
    pub rot_y: f64,
    pub scale: [f64; 2],
    pub opacity: f64,
    pub hue: f64,
}

impl ChannelSums {
    /// All sums at `+0`: the state of a clone whose own deltas are 0.
    pub(crate) fn zero() -> ChannelSums {
        ChannelSums {
            pos: [0.0, 0.0],
            pos_z: 0.0,
            rot: 0.0,
            rot_x: 0.0,
            rot_y: 0.0,
            scale: [0.0, 0.0],
            opacity: 0.0,
            hue: 0.0,
        }
    }
}

/// One iteration of the effector loop: weight, skip when falsy, then add
/// `w * (v[i] || 0)` for every channel in list order. Unknown props add nothing.
pub(crate) fn apply_effector(
    sums: &mut ChannelSums,
    eff: &Effector,
    inst_x: f64,
    inst_y: f64,
    k: u32,
    count: f64,
) {
    let w = effector_weight(eff, inst_x, inst_y, k, count);
    if is_skipped(w) {
        return;
    }
    for ch in effector_channels(eff).iter() {
        match ChannelProp::from_label(&ch.prop) {
            ChannelProp::Position => {
                sums.pos[0] += w * ch.component(0);
                sums.pos[1] += w * ch.component(1);
            }
            ChannelProp::PositionZ => sums.pos_z += w * ch.component(0),
            ChannelProp::Rotation => sums.rot += w * ch.component(0),
            ChannelProp::RotationX => sums.rot_x += w * ch.component(0),
            ChannelProp::RotationY => sums.rot_y += w * ch.component(0),
            ChannelProp::Scale => {
                sums.scale[0] += w * ch.component(0);
                sums.scale[1] += w * ch.component(1);
            }
            ChannelProp::Opacity => sums.opacity += w * ch.component(0),
            ChannelProp::Hue => sums.hue += w * ch.component(0),
            ChannelProp::Unknown => {}
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_effector_falloff_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_effector_falloff_pins.rs"]
mod pins_tests;
#[cfg(test)]
#[path = "../tests/object_effector_falloff_codec.rs"]
pub(crate) mod test_codec;
#[cfg(test)]
#[path = "../tests/object_effector_falloff.rs"]
mod tests;
