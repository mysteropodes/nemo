//! JavaScript number semantics the duplicator application kernels rely on (L04-D, I08).
//!
//! The baseline (`src/js/app.js` at `fbe4b8a3`) runs these on V8 doubles: `Math.round`
//! (ties toward `+Infinity`, `-0` kept), `Math.min`/`Math.max` (NaN poisons, `-0 < +0`),
//! `x | 0` (ToInt32) and the string a number takes inside `'dup' + seed + '_' + k`
//! (the shortest round-trip digits laid out by `Number::toString`). Each helper is a
//! free function over `f64` so the matrix, color and temporal kernels share ONE copy;
//! the oracle rows in `fixtures/duplicator-placement-jsnum/oracle.txt` pin every one of
//! them against the engine that produced the baseline behavior.

/// JavaScript truthiness of a number: everything but `0`, `-0` and `NaN`.
pub(crate) fn truthy(x: f64) -> bool {
    !(x == 0.0 || x.is_nan())
}

/// `Math.round`: the closest integer, ties toward `+Infinity`, `-0` for the
/// negative values that round to zero.
pub(crate) fn js_round(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let floor = x.floor();
    let rounded = if x - floor >= 0.5 { floor + 1.0 } else { floor };
    if rounded == 0.0 && x < 0.0 {
        -0.0
    } else {
        rounded
    }
}

/// `Math.min(a, b)`: NaN when either side is NaN, `-0` below `+0`.
pub(crate) fn js_min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() {
            a
        } else {
            b
        }
    } else if a < b {
        a
    } else {
        b
    }
}

/// `Math.max(a, b)`: NaN when either side is NaN, `+0` above `-0`.
pub(crate) fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_positive() {
            a
        } else {
            b
        }
    } else if a > b {
        a
    } else {
        b
    }
}

/// `x | 0` (ToInt32): truncate toward zero, reduce modulo 2^32, reinterpret as
/// signed. NaN and the infinities are 0.
pub(crate) fn to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    let wrapped = x.trunc().rem_euclid(4_294_967_296.0);
    (wrapped as u32) as i32
}

/// `String(x)` for a number: `Number::toString` with radix 10.
///
/// With `digits` the shortest round-trip decimal digits (`k` of them) and `n`
/// the decimal exponent such that the value is `0.digits * 10^n`, the layout is
/// plain digits (padded with zeros) up to `n = 21`, a decimal point inside the
/// digits for `0 < n <= 21`, `0.000ddd` down to `n = -5`, and `d.ddde+X`
/// otherwise. Rust's `{:e}` prints the same shortest digits.
pub(crate) fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x == 0.0 {
        return "0".to_string();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-Infinity" } else { "Infinity" }.to_string();
    }
    let scientific = format!("{:e}", x.abs());
    let split = scientific
        .split_once('e')
        .map(|(mantissa, exponent)| (mantissa.to_string(), exponent.to_string()));
    let Some((mantissa, exponent)) = split else {
        return scientific;
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exponent.parse::<i32>().map(|e| e + 1).unwrap_or(0);
    let body = if k <= n && n <= 21 {
        format!("{}{}", digits, "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{}", "0".repeat((-n) as usize), digits)
    } else {
        let e = n - 1;
        let sign = if e < 0 { '-' } else { '+' };
        if k == 1 {
            format!("{}e{}{}", digits, sign, e.abs())
        } else {
            format!("{}.{}e{}{}", &digits[..1], &digits[1..], sign, e.abs())
        }
    };
    if x < 0.0 {
        format!("-{body}")
    } else {
        body
    }
}

#[cfg(test)]
#[path = "../tests/duplicator_placement_jsnum_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/duplicator_placement_jsnum.rs"]
mod tests;
