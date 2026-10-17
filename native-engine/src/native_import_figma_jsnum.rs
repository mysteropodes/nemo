//! JavaScript scalar semantics the Figma import baseline relies on (L10-E, G36).
//!
//! `src/js/figma-import.js` at the agreed desktop baseline `fbe4b8a3` builds its SVG text and its
//! `rgba(...)` strings by concatenating JavaScript numbers, rounds with `Math.round` and trims with
//! `String.prototype.trim`. Rust's `{}` formatting, `f64::round` and `str::trim` differ from those in
//! observable ways (exponent notation from 1e21 and below 1e-6, `-0`, round-half-up of negative
//! halves, U+FEFF and U+0085 in trim), and every such difference would change the tags the SVG path
//! receives. These four helpers reproduce the JavaScript results bit for bit; the oracle rows
//! `J`, `R` and `U` pin them against the verbatim engine.

/// `String(x)` / `'' + x` for a double: ECMAScript `Number::toString` with radix 10.
pub(crate) fn js_number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x == 0.0 {
        return "0".to_string();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-Infinity" } else { "Infinity" }.to_string();
    }
    if x < 0.0 {
        let mut text = String::from("-");
        text.push_str(&js_number_to_string(-x));
        return text;
    }
    // `{:e}` prints the shortest digits that round-trip, as `d.ddde<exp>`; ECMAScript wants the same
    // digit string, laid out by the position `n` of the decimal point.
    let scientific = format!("{:e}", x);
    let (mantissa, exponent_text) = match scientific.split_once('e') {
        Some(parts) => parts,
        None => return scientific,
    };
    let exponent: i32 = match exponent_text.parse() {
        Ok(value) => value,
        Err(_) => return scientific,
    };
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exponent + 1;
    if k <= n && n <= 21 {
        let mut text = digits;
        for _ in 0..(n - k) {
            text.push('0');
        }
        text
    } else if 0 < n && n <= 21 {
        let split = n as usize;
        format!("{}.{}", &digits[..split], &digits[split..])
    } else if -6 < n && n <= 0 {
        let mut text = String::from("0.");
        for _ in 0..(-n) {
            text.push('0');
        }
        text.push_str(&digits);
        text
    } else {
        let e = n - 1;
        let sign = if e < 0 { '-' } else { '+' };
        if k == 1 {
            format!("{}e{}{}", digits, sign, e.abs())
        } else {
            format!("{}.{}e{}{}", &digits[..1], &digits[1..], sign, e.abs())
        }
    }
}

/// `Math.round(x)`: round half toward +Infinity, keeping `-0` for the values in `[-0.5, 0)`.
pub(crate) fn js_round(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() || x == 0.0 {
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

/// `Math.max(a, b)` for two operands: NaN if either is NaN.
pub(crate) fn js_max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else {
        b
    }
}

/// `true` for the code points `String.prototype.trim` strips (WhiteSpace and LineTerminator).
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\u{9}' | '\u{a}' | '\u{b}' | '\u{c}' | '\u{d}' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// `s.trim()`.
pub(crate) fn js_trim(s: &str) -> &str {
    s.trim_matches(is_js_space)
}

#[cfg(test)]
#[path = "../tests/native_import_figma_jsnum.rs"]
mod tests;
