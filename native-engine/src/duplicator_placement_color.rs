//! Duplicator hue shift (L04-D, I08).
//!
//! Mirrors `hexHueRotate` of `src/js/app.js` at baseline `fbe4b8a3`: rotate the
//! HUE of a `#rrggbb` / `#rrggbbaa` color by `deg` degrees through the standard
//! RGB-HSL round trip, keeping saturation, lightness and any trailing alpha
//! byte. A falsy input, a falsy `deg` or a string that is not exactly
//! `^#([0-9a-f]{6})([0-9a-f]{2})?$` (case-insensitive) passes through
//! unchanged and never throws. All arithmetic is plain IEEE `f64` in the
//! operand order of the baseline, so every row of the oracle is compared bit
//! for bit, including the `Math.round` of the byte and its `toString(16)`.
//!
//! `rotate_fill_hue` joins the same algorithm to the immutable native
//! [`SolidFill`]: its red, green and blue channels are quantized to the 8-bit
//! value the hex color had (`Math.round(v * 255)`), rotated, and written back
//! as `byte / 255`; the alpha channel is never touched. For a fill that came
//! from a hex color this is exactly `hexHueRotate` of that color.

use crate::evaluation::object_component_kernels::duplicator_placement_jsnum::{
    js_round, number_to_string, truthy,
};
use crate::object_document::SolidFill;
use serde_json::Number;

/// The hue period of the baseline, `h = (h + deg) % 360`.
pub(crate) const HUE_WRAP: f64 = 360.0;

/// A native fill that cannot be rotated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FillHueError {
    /// A red, green or blue channel is not a finite number in `[0, 1]`.
    ChannelOutOfRange,
    /// A rotated channel is not representable as a JSON number.
    NonFinite,
}

fn hue_to_rgb(p: f64, q: f64, t: f64) -> f64 {
    let mut t = t;
    if t < 0.0 {
        t += 1.0;
    }
    if t > 1.0 {
        t -= 1.0;
    }
    if t < 1.0 / 6.0 {
        return p + (q - p) * 6.0 * t;
    }
    if t < 1.0 / 2.0 {
        return q;
    }
    if t < 2.0 / 3.0 {
        return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
    }
    p
}

/// The rotated channels in `[0, 1]` (before the byte rounding) of `r, g, b`
/// in `[0, 1]`, with the hue period as a parameter so a control can move it.
fn rotated_channels(r: f64, g: f64, b: f64, deg: f64, wrap: f64) -> [f64; 3] {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let l = (max + min) / 2.0;
    let mut h = 0.0;
    let mut s = 0.0;
    if d != 0.0 {
        s = if l > 0.5 {
            d / (2.0 - max - min)
        } else {
            d / (max + min)
        };
        if max == r {
            h = (g - b) / d + if g < b { 6.0 } else { 0.0 };
        } else if max == g {
            h = (b - r) / d + 2.0;
        } else {
            h = (r - g) / d + 4.0;
        }
        h *= 60.0;
    }
    h = (h + deg) % wrap;
    if h < 0.0 {
        h += wrap;
    }
    if s == 0.0 {
        return [l, l, l];
    }
    let q = if l < 0.5 {
        l * (1.0 + s)
    } else {
        l + s - l * s
    };
    let p = 2.0 * l - q;
    [
        hue_to_rgb(p, q, h / 360.0 + 1.0 / 3.0),
        hue_to_rgb(p, q, h / 360.0),
        hue_to_rgb(p, q, h / 360.0 - 1.0 / 3.0),
    ]
}

/// `Math.round(v * 255)`, the byte value `toHex` formats.
fn byte_value(v: f64) -> f64 {
    js_round(v * 255.0)
}

/// `Math.round(v * 255).toString(16)` padded to two digits with `0`.
fn to_hex2(v: f64) -> String {
    let rounded = byte_value(v);
    let digits = if rounded.is_finite() {
        let whole = rounded as i64;
        if whole < 0 {
            format!("-{:x}", whole.unsigned_abs())
        } else {
            format!("{whole:x}")
        }
    } else {
        number_to_string(rounded)
    };
    if digits.len() < 2 {
        format!("0{digits}")
    } else {
        digits
    }
}

/// `m[1]` as three bytes and `m[2]` of `/^#([0-9a-f]{6})([0-9a-f]{2})?$/i`.
fn parse_hex(hex: &str) -> Option<([u8; 3], &str)> {
    let bytes = hex.as_bytes();
    if !(bytes.len() == 7 || bytes.len() == 9) || bytes[0] != b'#' {
        return None;
    }
    if !bytes[1..].iter().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let channel = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).ok();
    Some(([channel(1)?, channel(3)?, channel(5)?], &hex[7..]))
}

/// `hexHueRotate(hex, deg)` for a string color.
pub(crate) fn hex_hue_rotate(hex: &str, deg: f64) -> String {
    hex_hue_rotate_with_wrap(hex, deg, HUE_WRAP)
}

/// [`hex_hue_rotate`] with an explicit hue period, so a control can feed the
/// wrong one through the same code path.
pub(crate) fn hex_hue_rotate_with_wrap(hex: &str, deg: f64, wrap: f64) -> String {
    if hex.is_empty() || !truthy(deg) {
        return hex.to_string();
    }
    let Some((rgb, alpha)) = parse_hex(hex) else {
        return hex.to_string();
    };
    let [r, g, b] = rotated_channels(
        f64::from(rgb[0]) / 255.0,
        f64::from(rgb[1]) / 255.0,
        f64::from(rgb[2]) / 255.0,
        deg,
        wrap,
    );
    format!("#{}{}{}{}", to_hex2(r), to_hex2(g), to_hex2(b), alpha)
}

/// `hexHueRotate` of a color that may be `null` or `undefined`: those pass
/// through as `None`.
pub(crate) fn hex_hue_rotate_opt(hex: Option<&str>, deg: f64) -> Option<String> {
    hex.map(|h| hex_hue_rotate(h, deg))
}

fn rgb_channel(n: &Number) -> Result<f64, FillHueError> {
    n.as_f64()
        .filter(|v| v.is_finite() && (0.0..=1.0).contains(v))
        .ok_or(FillHueError::ChannelOutOfRange)
}

fn byte_channel(v: f64) -> Result<Number, FillHueError> {
    Number::from_f64(byte_value(v) / 255.0).ok_or(FillHueError::NonFinite)
}

/// The fill with its hue rotated by `deg`; a falsy `deg` returns it unchanged
/// (the baseline only rotates when `hueDeg` is truthy).
pub(crate) fn rotate_fill_hue(fill: &SolidFill, deg: f64) -> Result<SolidFill, FillHueError> {
    if !truthy(deg) {
        return Ok(fill.clone());
    }
    let r = byte_value(rgb_channel(&fill.r)?) / 255.0;
    let g = byte_value(rgb_channel(&fill.g)?) / 255.0;
    let b = byte_value(rgb_channel(&fill.b)?) / 255.0;
    let [r2, g2, b2] = rotated_channels(r, g, b, deg, HUE_WRAP);
    Ok(SolidFill {
        kind: fill.kind.clone(),
        r: byte_channel(r2)?,
        g: byte_channel(g2)?,
        b: byte_channel(b2)?,
        a: fill.a.clone(),
    })
}

#[cfg(test)]
#[path = "../tests/duplicator_placement_color_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/duplicator_placement_color.rs"]
mod tests;
