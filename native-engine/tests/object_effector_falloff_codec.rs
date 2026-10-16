//! Token codec shared by the effector and clone oracle tests (L04-C).
//!
//! The oracle files are whitespace separated rows. Numbers are IEEE 754 bit
//! patterns in hex, `u` is `undefined` and `n` is `null`. An effector is one
//! space-free token of ten comma separated fields (see the header of
//! `fixtures/object-effector-falloff/oracle.txt`). Nothing here computes a
//! result: it only reads fixture text into the kernel types and prints kernel
//! results back in the fixture's own notation.

use super::{Channel, Effector, EffectorPos, Falloff};
use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;

/// `value` of an IEEE 754 bit pattern written in hex.
pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

/// Bit pattern with one canonical NaN (the notation the oracle uses).
pub(crate) fn bits(x: f64) -> u64 {
    if x.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        x.to_bits()
    }
}

/// The oracle notation of a plain number.
pub(crate) fn hex_of(x: f64) -> String {
    format!("{:016x}", bits(x))
}

/// `u`, `n` or hex bits.
pub(crate) fn js(tok: &str) -> JsNumber {
    match tok {
        "u" => JsNumber::Undefined,
        "n" => JsNumber::Null,
        h => JsNumber::Value(hex(h)),
    }
}

/// The oracle notation of a [`JsNumber`].
pub(crate) fn js_token(n: JsNumber) -> String {
    match n {
        JsNumber::Undefined => "u".to_string(),
        JsNumber::Null => "n".to_string(),
        JsNumber::Value(v) => hex_of(v),
    }
}

/// A label field: `-` undefined and `nl` null leave it absent, `~` is empty.
pub(crate) fn label(tok: &str) -> Option<&str> {
    match tok {
        "-" | "nl" => None,
        "~" => Some(""),
        m => Some(m),
    }
}

/// The data rows of one kind: the tokens after the kind word.
pub(crate) fn rows(oracle: &'static str, kind: &str) -> Vec<Vec<&'static str>> {
    oracle
        .lines()
        .filter(|l| !l.starts_with('#') && l.split_whitespace().next() == Some(kind))
        .map(|l| l.split_whitespace().skip(1).collect())
        .collect()
}

/// `e` is the empty list, otherwise numbers joined by `/`.
pub(crate) fn parse_nums(tok: &str) -> Vec<JsNumber> {
    if tok == "e" {
        return Vec::new();
    }
    tok.split('/').map(js).collect()
}

/// `-` (undefined) and `n` (null) are absent, otherwise a list.
pub(crate) fn parse_opt_nums(tok: &str) -> Option<Vec<JsNumber>> {
    match tok {
        "-" | "n" => None,
        t => Some(parse_nums(t)),
    }
}

/// `-` or `x/y`.
pub(crate) fn parse_pos(tok: &str) -> Option<EffectorPos> {
    if tok == "-" {
        return None;
    }
    let (x, y) = tok.split_once('/').expect("x/y");
    Some(EffectorPos {
        x: hex(x),
        y: hex(y),
    })
}

/// `-`/`n` absent (migrate), `e` empty, else `prop=VALS` joined by `+`; an
/// undefined `VALS` (`-`) is the empty list, like `ch.value || []`.
pub(crate) fn parse_channels(tok: &str) -> Option<Vec<Channel>> {
    match tok {
        "-" | "n" => None,
        "e" => Some(Vec::new()),
        t => Some(
            t.split('+')
                .map(|item| {
                    let (prop, vals) = item.split_once('=').expect("prop=vals");
                    let value = if vals == "-" {
                        Vec::new()
                    } else {
                        parse_nums(vals)
                    };
                    Channel::new(prop, value)
                })
                .collect(),
        ),
    }
}

/// One effector token.
pub(crate) fn parse_effector(tok: &str) -> Effector {
    let f: Vec<&str> = tok.split(',').collect();
    assert_eq!(f.len(), 10, "effector token {tok}");
    Effector {
        pos: parse_pos(f[0]),
        falloff: Falloff::from_label(label(f[1])),
        radius: js(f[2]),
        strength: js(f[3]),
        angle: js(f[4]),
        channels: parse_channels(f[5]),
        offset_pos: parse_opt_nums(f[6]),
        offset_rot: js(f[7]),
        offset_scale: parse_opt_nums(f[8]),
        offset_opacity: js(f[9]),
        from_layer: false,
    }
}

/// `-` (no list) and `e` are empty, otherwise effectors joined by `;`.
pub(crate) fn parse_effectors(tok: &str) -> Vec<Effector> {
    match tok {
        "-" | "e" => Vec::new(),
        t => t.split(';').map(parse_effector).collect(),
    }
}

/// `e` or `prop=VALS` joined by `+`, the notation of the oracle's outputs.
pub(crate) fn format_channels(list: &[Channel]) -> String {
    if list.is_empty() {
        return "e".to_string();
    }
    let item = |ch: &Channel| {
        let vals = if ch.value.is_empty() {
            "e".to_string()
        } else {
            ch.value
                .iter()
                .map(|n| js_token(*n))
                .collect::<Vec<_>>()
                .join("/")
        };
        format!("{}={}", ch.prop, vals)
    };
    list.iter().map(item).collect::<Vec<_>>().join("+")
}

/// A kernel number equals the oracle's: bit for bit when `tolerant` is false,
/// otherwise within `1e-9 + 1e-12 * |expected|`. Used only for rows that go
/// through the linear falloff's `cos`/`sin`, which are libm calls, not V8's
/// fdlibm. NaN matches NaN; an infinity matches only itself.
pub(crate) fn close(actual: f64, expected: f64, tolerant: bool) -> bool {
    if actual.is_nan() || expected.is_nan() {
        return actual.is_nan() && expected.is_nan();
    }
    if !tolerant {
        return actual.to_bits() == expected.to_bits();
    }
    if actual == expected {
        return true;
    }
    // An infinity matches only itself: the bound below is infinite for it.
    expected.is_finite() && (actual - expected).abs() <= 1e-9 + 1e-12 * expected.abs()
}

/// A list holds an effector that uses the libm-dependent falloff.
pub(crate) fn has_linear(effs: &[Effector]) -> bool {
    effs.iter().any(|e| e.falloff == Falloff::Linear)
}
