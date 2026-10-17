//! Oracle tests for the JavaScript number helpers (L04-D, I08).
//!
//! `fixtures/duplicator-placement-jsnum/oracle.txt` was produced by
//! `tools/gen_oracle.cjs` of the L04-D leaf running `Math.round`, `x | 0` and
//! `String(x)` in the Node vm that also ran the verbatim baseline functions.
//! Numbers are IEEE 754 bit patterns in hex (NaN canonical). Rows:
//! `rnd X => Y`, `i32 X => N` (decimal) and `str X => UTF8HEX`.

use super::{js_max, js_min, js_round, number_to_string, to_int32, truthy};

pub(crate) const ORACLE: &str = include_str!("fixtures/duplicator-placement-jsnum/oracle.txt");

pub(crate) fn hex(h: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(h, 16).expect("hex bits"))
}

pub(crate) fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

pub(crate) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|l| l.split_whitespace().next() == Some(kind))
        .map(|l| l.split_whitespace().skip(1).collect())
        .collect()
}

pub(crate) fn utf8(h: &str) -> String {
    let bytes: Vec<u8> = (0..h.len() / 2)
        .map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).expect("utf8 hex"))
        .collect();
    String::from_utf8(bytes).expect("utf8")
}

#[test]
fn math_round_rows_match_the_engine() {
    let all = rows("rnd");
    assert_eq!(all.len(), 429);
    for t in all {
        let (x, want) = (hex(t[0]), hex(t[2]));
        assert!(
            same(js_round(x), want),
            "round {x:e}: {} vs {want:e}",
            js_round(x)
        );
    }
}

#[test]
fn to_int32_rows_match_the_engine() {
    let all = rows("i32");
    assert_eq!(all.len(), 142);
    for t in all {
        let want: i32 = t[2].parse().expect("int");
        assert_eq!(to_int32(hex(t[0])), want, "x|0 of {}", t[0]);
    }
}

#[test]
fn number_to_string_rows_match_the_engine() {
    let all = rows("str");
    assert_eq!(all.len(), 408);
    for t in all {
        let x = hex(t[0]);
        assert_eq!(number_to_string(x), utf8(t[2]), "String({x:e})");
    }
}

#[test]
fn pinned_strings_are_the_ecmascript_layouts() {
    // (value, String(value)) taken from the engine; each is also an oracle row.
    let pins: [(f64, &str); 14] = [
        (0.0, "0"),
        (-0.0, "0"),
        (42.0, "42"),
        (-5.0, "-5"),
        (0.5, "0.5"),
        (1e21, "1e+21"),
        (1e20, "100000000000000000000"),
        (123456789012345680000.0, "123456789012345680000"),
        (1e-6, "0.000001"),
        (1e-7, "1e-7"),
        (1.2345e-7, "1.2345e-7"),
        (5e-324, "5e-324"),
        (f64::MAX, "1.7976931348623157e+308"),
        (0.1 + 0.2, "0.30000000000000004"),
    ];
    let oracle = rows("str");
    for (x, want) in pins {
        assert_eq!(number_to_string(x), want);
        let bits = format!("{:016x}", x.to_bits());
        let row = oracle.iter().find(|t| t[0] == bits).expect("oracle row");
        assert_eq!(utf8(row[2]), want, "oracle row of {want}");
    }
    assert_eq!(number_to_string(f64::NAN), "NaN");
    assert_eq!(number_to_string(f64::INFINITY), "Infinity");
    assert_eq!(number_to_string(f64::NEG_INFINITY), "-Infinity");
}

#[test]
fn round_keeps_the_signed_zero_and_the_ties_toward_positive_infinity() {
    assert!(same(js_round(-0.5), -0.0));
    assert!(same(js_round(-0.25), -0.0));
    assert_eq!(js_round(2.5), 3.0);
    assert_eq!(js_round(-2.5), -2.0);
    assert_eq!(js_round(0.49999999999999994), 0.0);
    assert!(same(js_round(-0.0), -0.0));
}

#[test]
fn min_and_max_follow_the_signed_zero_and_nan_rules() {
    assert!(same(js_min(0.0, -0.0), -0.0));
    assert!(same(js_min(-0.0, 0.0), -0.0));
    assert!(same(js_max(-0.0, 0.0), 0.0));
    assert!(same(js_max(0.0, -0.0), 0.0));
    assert!(js_min(f64::NAN, 1.0).is_nan());
    assert!(js_max(1.0, f64::NAN).is_nan());
    assert_eq!(js_min(2.0, 3.0), 2.0);
    assert_eq!(js_max(2.0, 3.0), 3.0);
}

#[test]
fn truthiness_is_false_for_zero_negative_zero_and_nan_only() {
    for falsy in [0.0, -0.0, f64::NAN] {
        assert!(!truthy(falsy));
    }
    for truthy_value in [1.0, -1.0, 1e-300, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(truthy(truthy_value));
    }
}
