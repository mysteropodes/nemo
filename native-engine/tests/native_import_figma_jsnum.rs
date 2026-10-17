//! Tests of the JavaScript scalar helpers against the oracle rows `J`, `R` and `U` (L10-E, G36).
//! Every literal pin is also asserted to be a row of the oracle, i.e. a value the verbatim baseline
//! engine produced, so a pin cannot drift into a Rust-only expectation.

use super::super::rows::{escape, has_line, hex, singles, unescape, unhex};
use super::{js_max, js_number_to_string, js_round, js_trim};

fn pin_number(x: f64, text: &str) {
    assert_eq!(js_number_to_string(x), text, "production for {}", text);
    let line = format!("J {} {}", hex(x), escape(text));
    assert!(has_line(&line), "pin must be an oracle row: {}", line);
}

fn pin_round(x: f64, expected: f64) {
    assert_eq!(js_round(x).to_bits(), expected.to_bits(), "round {}", x);
    let line = format!("R {} {}", hex(x), hex(expected));
    assert!(has_line(&line), "pin must be an oracle row: {}", line);
}

#[test]
fn number_to_string_matches_every_oracle_row() {
    let rows = singles("J");
    assert_eq!(rows.len(), 49);
    for row in &rows {
        let x = unhex(row[0]);
        assert_eq!(escape(&js_number_to_string(x)), row[1], "J {}", row[0]);
    }
}

#[test]
fn number_to_string_literal_pins() {
    pin_number(1e21, "1e+21");
    pin_number(-1e21, "-1e+21");
    pin_number(1e-7, "1e-7");
    pin_number(1.5e-7, "1.5e-7");
    pin_number(0.000001, "0.000001");
    pin_number(123456789012345680000.0, "123456789012345680000");
    pin_number(-0.0, "0");
    pin_number(0.1 + 0.2, "0.30000000000000004");
    pin_number(5e-324, "5e-324");
    pin_number(1.7976931348623157e308, "1.7976931348623157e+308");
    pin_number(100.0, "100");
    pin_number(f64::NAN, "NaN");
    pin_number(f64::INFINITY, "Infinity");
    pin_number(f64::NEG_INFINITY, "-Infinity");
}

#[test]
fn number_to_string_parses_back_to_the_same_double() {
    // Independent of the oracle: the printed text must round-trip for finite values of every magnitude.
    let mut state: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut checked = 0;
    while checked < 3000 {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let x = f64::from_bits(state);
        if !x.is_finite() || x == 0.0 {
            continue;
        }
        let text = js_number_to_string(x);
        let back: f64 = text.parse().expect("printed number parses");
        assert_eq!(back.to_bits(), x.to_bits(), "round trip of {}", text);
        checked += 1;
    }
}

#[test]
fn round_matches_every_oracle_row() {
    let rows = singles("R");
    assert_eq!(rows.len(), 31);
    for row in &rows {
        assert_eq!(hex(js_round(unhex(row[0]))), row[1], "R {}", row[0]);
    }
}

#[test]
fn round_literal_pins() {
    pin_round(2.5, 3.0);
    pin_round(-2.5, -2.0);
    pin_round(0.5, 1.0);
    pin_round(-0.5, -0.0);
    pin_round(-0.4, -0.0);
    pin_round(0.49999999999999994, 0.0);
    pin_round(127.5, 128.0);
    pin_round(f64::NAN, f64::NAN);
}

#[test]
fn trim_matches_every_oracle_row() {
    let rows = singles("U");
    assert_eq!(rows.len(), 17);
    for row in &rows {
        assert_eq!(escape(js_trim(&unescape(row[0]))), row[1], "U {}", row[0]);
    }
}

#[test]
fn trim_differs_from_rust_trim_exactly_where_the_baseline_does() {
    // U+FEFF is JavaScript whitespace but not Rust's; U+0085 is Rust's but not JavaScript's.
    assert_eq!(js_trim("\u{feff}x\u{feff}"), "x");
    assert_eq!("\u{feff}x\u{feff}".trim(), "\u{feff}x\u{feff}");
    assert_eq!(js_trim("\u{85}x\u{85}"), "\u{85}x\u{85}");
    assert_eq!("\u{85}x\u{85}".trim(), "x");
    assert!(has_line(&format!(
        "U {} {}",
        escape("\u{feff}x\u{feff}"),
        escape("x")
    )));
    assert!(has_line(&format!(
        "U {} {}",
        escape("\u{85}x\u{85}"),
        escape("\u{85}x\u{85}")
    )));
}

#[test]
fn max_propagates_nan_like_math_max() {
    assert_eq!(js_max(1.0, 0.2), 1.0);
    assert_eq!(js_max(1.0, 5.0), 5.0);
    assert!(js_max(1.0, f64::NAN).is_nan());
    assert!(js_max(f64::NAN, 1.0).is_nan());
}
