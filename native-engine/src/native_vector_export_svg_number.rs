//! The SVG number formatters (G43), a child of `native_vector_export_svg`: baseline `String(x)` digits without an
//! exponent, and channel percentages shifted on the digits.

/// Shortest round-trip decimal, never an exponent; -0 is `0` like JS `String(-0)`.
/// Differs from JS only where JS uses an exponent (|x| < 1e-6 or >= 1e21).
pub(crate) fn format_number(value: f64) -> String {
    if value == 0.0 {
        "0".to_owned()
    } else {
        format!("{value}")
    }
}

/// A channel in 0..=1 as a percentage: the shortest decimal digits with the
/// point moved two places right (exact on the digits, no float multiply).
pub(crate) fn channel_percent(value: f64) -> String {
    let text = format_number(value);
    let (whole, fraction) = text.split_once('.').unwrap_or((text.as_str(), ""));
    let mut digits = fraction.to_owned();
    while digits.len() < 2 {
        digits.push('0');
    }
    let (moved, rest) = digits.split_at(2);
    let joined = format!("{whole}{moved}");
    let int = match joined.trim_start_matches('0') {
        "" => "0",
        trimmed => trimmed,
    };
    let rest = rest.trim_end_matches('0');
    if rest.is_empty() {
        int.to_owned()
    } else {
        format!("{int}.{rest}")
    }
}
