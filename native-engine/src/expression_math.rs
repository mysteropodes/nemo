//! Pure math and vector built-ins for Motion expressions.
//!
//! This preserves the characterized desktop JavaScript baseline of
//! `motion.js`: the coercion helpers `_vec`/`_num`/`_vecOp`/`_mixVals`/
//! `_remapArgs`/`_normT` and `exprClamp`, `exprRemap*`, `exprRadians`,
//! `exprDegrees`, `exprAdd`, `exprSub`, `exprMul`, `exprDiv`, `exprDot`,
//! `exprLength`, `exprNormalize` and `exprCross`. The compatibility aliases
//! (`linear`/`ease`/`easeIn`/`easeOut`, `degreesToRadians`/`radiansToDegrees`)
//! are the same functions under other names.
//!
//! Everything here is IEEE 754 basic arithmetic plus `sqrt`, so results are
//! bit-identical to JavaScript on every host. Two JavaScript coercions are
//! kept on purpose because they change signed zeros:
//! - `_num` turns non-finite values into `0` and keeps `-0`;
//! - `_vec` reads a scalar as `[Number(v) || 0]`, which also turns `-0`
//!   into `+0`.
//!
//! Callers convert JavaScript values to [`ExprValue`] first (`Number(v)` for
//! scalars, one `f64` per array entry); nothing here reads a context or clock.

/// One expression value: a number or a flat array of numbers.
#[derive(Clone, Debug, PartialEq)]
pub enum ExprValue {
    Scalar(f64),
    Vector(Vec<f64>),
}

impl ExprValue {
    fn is_vector(&self) -> bool {
        matches!(self, ExprValue::Vector(_))
    }
}

/// `_num`: non-finite becomes `0`; `-0` survives.
fn num(x: f64) -> f64 {
    if x.is_finite() {
        x
    } else {
        0.0
    }
}

/// `_num` applied to a whole value, as JavaScript `Number(value)` would see
/// it: an array of one entry reads as that entry (through its string form,
/// which drops a `-0` sign), any other array as `NaN`, hence `0`.
fn num_of(value: &ExprValue) -> f64 {
    match value {
        ExprValue::Scalar(x) => num(*x),
        ExprValue::Vector(xs) => match xs.as_slice() {
            [] => 0.0,
            [x] => {
                let x = num(*x);
                if x == 0.0 {
                    0.0
                } else {
                    x
                }
            }
            _ => 0.0,
        },
    }
}

/// `_vec`: an array as is, a scalar as `[Number(v) || 0]`.
fn vec_of(value: &ExprValue) -> Vec<f64> {
    match value {
        ExprValue::Scalar(x) => vec![if *x == 0.0 || x.is_nan() { 0.0 } else { *x }],
        ExprValue::Vector(xs) => xs.clone(),
    }
}

/// Entry `i` of a `_vec` result through `_num`; a missing entry is `0`.
fn at(xs: &[f64], i: usize) -> f64 {
    xs.get(i).copied().map_or(0.0, num)
}

/// JavaScript `Math.max` on finite inputs: `+0` beats `-0`.
fn js_max(a: f64, b: f64) -> f64 {
    if a > b || (a == b && b.is_sign_negative()) {
        a
    } else {
        b
    }
}

/// JavaScript `Math.min` on finite inputs: `-0` beats `+0`.
fn js_min(a: f64, b: f64) -> f64 {
    if a < b || (a == b && b.is_sign_positive()) {
        a
    } else {
        b
    }
}

/// `_vecOp`: component-wise over the longer length; a scalar result only
/// when both sides are scalars.
fn vec_op(a: &ExprValue, b: &ExprValue, f: impl Fn(f64, f64) -> f64) -> ExprValue {
    let (va, vb) = (vec_of(a), vec_of(b));
    let out: Vec<f64> = (0..va.len().max(vb.len()))
        .map(|i| f(at(&va, i), at(&vb, i)))
        .collect();
    if !a.is_vector() && !b.is_vector() {
        ExprValue::Scalar(out[0])
    } else {
        ExprValue::Vector(out)
    }
}

/// `_mixVals`: `x + (y - x) * k`, component-wise when either side is an array.
fn mix(v1: &ExprValue, v2: &ExprValue, k: f64) -> ExprValue {
    if v1.is_vector() || v2.is_vector() {
        let (va, vb) = (vec_of(v1), vec_of(v2));
        ExprValue::Vector(
            (0..va.len().max(vb.len()))
                .map(|i| {
                    let (x, y) = (at(&va, i), at(&vb, i));
                    x + (y - x) * k
                })
                .collect(),
        )
    } else {
        let (x, y) = (num_of(v1), num_of(v2));
        ExprValue::Scalar(x + (y - x) * k)
    }
}

/// `_normT`: `t` placed on `t_min..t_max` and clamped to `0..1`; an empty
/// range is a step at `t_min`.
fn norm_t(t: f64, t_min: f64, t_max: f64) -> f64 {
    if t_max == t_min {
        return if t <= t_min { 0.0 } else { 1.0 };
    }
    // Same as `k < 0 ? 0 : (k > 1 ? 1 : k)`, `-0` and `NaN` included.
    ((t - t_min) / (t_max - t_min)).clamp(0.0, 1.0)
}

/// The four remap blend shapes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RemapShape {
    /// `remap`/`linear`: straight.
    Linear,
    /// `remapEase`/`ease`: soft at both ends (`k*k*(3 - 2k)`).
    Ease,
    /// `remapEaseIn`/`easeIn`: soft at the start (`-k^3 + 2k^2`).
    EaseIn,
    /// `remapEaseOut`/`easeOut`: soft at the end (`-k^3 + k^2 + k`).
    EaseOut,
}

fn shape(k: f64, shape: RemapShape) -> f64 {
    match shape {
        RemapShape::Linear => k,
        RemapShape::Ease => k * k * (3.0 - 2.0 * k),
        RemapShape::EaseIn => -k * k * k + 2.0 * k * k,
        RemapShape::EaseOut => -k * k * k + k * k + k,
    }
}

/// Three-argument remap `(t, to_lo, to_hi)`: `t` read on a `0..1` scale.
pub fn remap(shape_kind: RemapShape, t: f64, to_lo: &ExprValue, to_hi: &ExprValue) -> ExprValue {
    remap_range(shape_kind, t, 0.0, 1.0, to_lo, to_hi)
}

/// Five-argument remap `(t, from_lo, from_hi, to_lo, to_hi)`. A four-argument
/// JavaScript call reaches this form too, with `to_hi` as `Scalar(0.0)`.
pub fn remap_range(
    shape_kind: RemapShape,
    t: f64,
    from_lo: f64,
    from_hi: f64,
    to_lo: &ExprValue,
    to_hi: &ExprValue,
) -> ExprValue {
    let k = shape(norm_t(num(t), num(from_lo), num(from_hi)), shape_kind);
    mix(to_lo, to_hi, k)
}

/// `clamp(v, lo, hi)`: reversed bounds are swapped; an array `v` is clamped
/// per component, with array bounds read per component.
pub fn clamp(v: &ExprValue, lo: &ExprValue, hi: &ExprValue) -> ExprValue {
    let bound = |b: &ExprValue, i: usize| match b {
        ExprValue::Vector(xs) => at(xs, i),
        ExprValue::Scalar(x) => num(*x),
    };
    match v {
        ExprValue::Vector(xs) => ExprValue::Vector(
            xs.iter()
                .enumerate()
                .map(|(i, x)| clamp_scalar(num(*x), bound(lo, i), bound(hi, i)))
                .collect(),
        ),
        ExprValue::Scalar(x) => ExprValue::Scalar(clamp_scalar(num(*x), num_of(lo), num_of(hi))),
    }
}

fn clamp_scalar(v: f64, lo: f64, hi: f64) -> f64 {
    let (lo, hi) = if hi < lo { (hi, lo) } else { (lo, hi) };
    js_min(js_max(v, lo), hi)
}

/// `radians(d)`: `(d * PI) / 180`, in that order, per component.
pub fn radians(d: &ExprValue) -> ExprValue {
    map_components(d, |x| x * std::f64::consts::PI / 180.0)
}

/// `degrees(r)`: `(r * 180) / PI`, in that order, per component.
pub fn degrees(r: &ExprValue) -> ExprValue {
    map_components(r, |x| x * 180.0 / std::f64::consts::PI)
}

fn map_components(value: &ExprValue, f: impl Fn(f64) -> f64) -> ExprValue {
    match value {
        ExprValue::Scalar(x) => ExprValue::Scalar(f(num(*x))),
        ExprValue::Vector(xs) => ExprValue::Vector(xs.iter().map(|x| f(num(*x))).collect()),
    }
}

/// `add(a, b)`.
pub fn add(a: &ExprValue, b: &ExprValue) -> ExprValue {
    vec_op(a, b, |x, y| x + y)
}

/// `sub(a, b)`.
pub fn sub(a: &ExprValue, b: &ExprValue) -> ExprValue {
    vec_op(a, b, |x, y| x - y)
}

/// `mul(a, s)`: component-wise for an array `s`, otherwise scaled by `s`.
pub fn mul(a: &ExprValue, s: &ExprValue) -> ExprValue {
    match s {
        ExprValue::Vector(_) => vec_op(a, s, |x, y| x * y),
        ExprValue::Scalar(k) => scale(a, num(*k), |x, k| x * k),
    }
}

/// `div(a, s)`: like [`mul`], with any division by zero giving `0`.
pub fn div(a: &ExprValue, s: &ExprValue) -> ExprValue {
    match s {
        ExprValue::Vector(_) => vec_op(a, s, |x, y| if y == 0.0 { 0.0 } else { x / y }),
        ExprValue::Scalar(k) => scale(a, num(*k), |x, k| if k == 0.0 { 0.0 } else { x / k }),
    }
}

fn scale(a: &ExprValue, k: f64, f: impl Fn(f64, f64) -> f64) -> ExprValue {
    let out: Vec<f64> = vec_of(a).into_iter().map(|x| f(num(x), k)).collect();
    if a.is_vector() {
        ExprValue::Vector(out)
    } else {
        ExprValue::Scalar(out[0])
    }
}

/// `dot(a, b)` over the longer length, summed in index order from `+0`.
pub fn dot(a: &ExprValue, b: &ExprValue) -> f64 {
    let (va, vb) = (vec_of(a), vec_of(b));
    (0..va.len().max(vb.len())).fold(0.0, |s, i| s + at(&va, i) * at(&vb, i))
}

/// `length(a)`: the magnitude, squares summed in index order from `+0`.
pub fn length(a: &ExprValue) -> f64 {
    vec_of(a)
        .into_iter()
        .fold(0.0, |s, x| s + num(x) * num(x))
        .sqrt()
}

/// `length(a, b)`: the distance between two points.
pub fn distance(a: &ExprValue, b: &ExprValue) -> f64 {
    length(&sub(a, b))
}

/// `normalize(a)`: `a / length(a)`; a zero-length vector gives zeros of the
/// same shape. An overflowed length is infinite, which [`div`] reads as `0`.
pub fn normalize(a: &ExprValue) -> ExprValue {
    let l = length(a);
    if l == 0.0 || l.is_nan() {
        return match a {
            ExprValue::Vector(_) => ExprValue::Vector(vec![0.0; vec_of(a).len()]),
            ExprValue::Scalar(_) => ExprValue::Scalar(0.0),
        };
    }
    div(a, &ExprValue::Scalar(l))
}

/// `cross(a, b)`: the 3D cross product when either side has three or more
/// entries, otherwise the z component of the 2D one as a scalar.
pub fn cross(a: &ExprValue, b: &ExprValue) -> ExprValue {
    let (va, vb) = (vec_of(a), vec_of(b));
    let (a0, a1, a2) = (at(&va, 0), at(&va, 1), at(&va, 2));
    let (b0, b1, b2) = (at(&vb, 0), at(&vb, 1), at(&vb, 2));
    if va.len() >= 3 || vb.len() >= 3 {
        ExprValue::Vector(vec![
            a1 * b2 - a2 * b1,
            a2 * b0 - a0 * b2,
            a0 * b1 - a1 * b0,
        ])
    } else {
        ExprValue::Scalar(a0 * b1 - a1 * b0)
    }
}
