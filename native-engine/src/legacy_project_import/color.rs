//! Baseline colour text (`#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`) to the stored solid paint (channels 0..1).
use serde_json::{json, Number, Value};

/// A whole value stays an integer, as a hand-written record spells it.
pub(super) fn number(value: f64) -> Value {
    if value.is_finite() && value.fract() == 0.0 && value.abs() < 9_007_199_254_740_992.0 {
        return Value::from(value as i64);
    }
    Number::from_f64(value).map_or(Value::Null, Value::Number)
}

pub(super) fn bytes(text: &str) -> Option<[u8; 4]> {
    let digits = text.strip_prefix('#')?;
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let pair = |at: usize| u8::from_str_radix(&digits[at..at + 2], 16).ok();
    let single = |at: usize| {
        u8::from_str_radix(&digits[at..=at], 16)
            .ok()
            .map(|v| v * 17)
    };
    match digits.len() {
        3 => Some([single(0)?, single(1)?, single(2)?, 255]),
        4 => Some([single(0)?, single(1)?, single(2)?, single(3)?]),
        6 => Some([pair(0)?, pair(2)?, pair(4)?, 255]),
        8 => Some([pair(0)?, pair(2)?, pair(4)?, pair(6)?]),
        _ => None,
    }
}

pub(super) fn solid(text: &str) -> Option<Value> {
    let [r, g, b, a] = bytes(text)?;
    let channel = |v: u8| number(f64::from(v) / 255.0);
    Some(
        json!({"kind": "solid", "r": channel(r), "g": channel(g), "b": channel(b), "a": channel(a)}),
    )
}

/// `#rrggbb` for the layer colour, dropping any alpha byte.
pub(super) fn layer_hex(text: &str) -> Option<String> {
    let [r, g, b, _] = bytes(text)?;
    Some(format!("#{r:02X}{g:02X}{b:02X}"))
}
