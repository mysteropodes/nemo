//! Oracle tests for the exposed-property key (L04-E, I14/I35).
//!
//! `fixtures/object-component-exposed-key/oracle.txt` was produced by `tools/gen_oracle.cjs` of the
//! L04-E leaf from the verbatim `src/js/domain/component/exposed-properties.js` at baseline
//! `fbe4b8a3` run in a Node `vm`. Rows are tab separated: `T <value> <value.toString(36)>`,
//! `K <clock> <random> <key from expose()>` and `X <clock> <random>` (a magnitude of 2^53 or more:
//! engine dependent, only the typed refusal is pinned). Numbers are JavaScript shortest round-trip
//! strings; `NaN`, `Infinity`, `-Infinity` and `-0` are spelled out.

use super::{carry_back, exposed_key, number_to_string_36, RadixError};

pub(crate) const ORACLE: &str = include_str!("fixtures/object-component-exposed-key/oracle.txt");
pub(crate) const BASELINE: &str = "fbe4b8a33e6d5a229f014154df2ba21988a9fabc";
/// git blob id of `exposed-properties.js` at the baseline (read by the generator).
pub(crate) const BLOB: &str = "dfd93e88afad3d158d62c851f3bfe8238753d542";

pub(crate) fn num(token: &str) -> f64 {
    match token {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        "-0" => -0.0,
        other => other
            .parse::<f64>()
            .unwrap_or_else(|_| panic!("number {other}")),
    }
}

/// Rows of one kind as tab-split columns after the kind letter.
pub(crate) fn rows(kind: &str) -> Vec<Vec<&'static str>> {
    ORACLE
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.split('\t').collect::<Vec<_>>())
        .filter(|cols| cols[0] == kind)
        .map(|cols| cols[1..].to_vec())
        .collect()
}

#[test]
fn fixture_pins_the_baseline_and_its_row_counts() {
    assert!(ORACLE.contains(BASELINE));
    assert!(ORACLE.contains(&format!(
        "src/js/domain/component/exposed-properties.js blob {BLOB}"
    )));
    assert_eq!(rows("T").len(), 1096);
    assert_eq!(rows("K").len(), 411);
    assert_eq!(rows("X").len(), 16);
}

#[test]
fn to_string_36_equals_every_engine_row() {
    for cols in rows("T") {
        let value = num(cols[0]);
        assert_eq!(
            number_to_string_36(value).as_deref(),
            Ok(cols[1]),
            "toString(36) of {}",
            cols[0]
        );
    }
}

#[test]
fn key_equals_every_verbatim_expose_row() {
    for cols in rows("K") {
        assert_eq!(
            exposed_key(num(cols[0]), num(cols[1])).as_deref(),
            Ok(cols[2]),
            "key for clock {} random {}",
            cols[0],
            cols[1]
        );
    }
}

#[test]
fn engine_dependent_magnitudes_are_refused_not_guessed() {
    for cols in rows("X") {
        assert_eq!(
            exposed_key(num(cols[0]), num(cols[1])),
            Err(RadixError::EngineDependent),
            "clock {} random {}",
            cols[0],
            cols[1]
        );
    }
    assert_eq!(
        number_to_string_36(9_007_199_254_740_992.0),
        Err(RadixError::EngineDependent)
    );
    assert_eq!(
        number_to_string_36(-9_007_199_254_740_992.0),
        Err(RadixError::EngineDependent)
    );
    assert_eq!(
        number_to_string_36(f64::MAX),
        Err(RadixError::EngineDependent)
    );
    assert_eq!(
        number_to_string_36(9_007_199_254_740_991.0).as_deref(),
        Ok("2gosa7pa2gv")
    );
}

#[test]
fn pinned_engine_literals() {
    let pins: [(f64, &str); 14] = [
        (0.0, "0"),
        (-0.0, "0"),
        (35.0, "z"),
        (36.0, "10"),
        (1296.0, "100"),
        (0.5, "0.i"),
        (-0.5, "-0.i"),
        (0.25, "0.9"),
        (1.0 / 3.0, "0.c"),
        (0.1, "0.3lllllllllm"),
        (0.999_999_999_999_999_9, "0.zzzzzzzzzza"),
        (1_700_000_000_000.0, "loyw3v28"),
        (123_456.789, "2n9c.sejl0v4"),
        (-36.0, "-10"),
    ];
    for (value, text) in pins {
        assert_eq!(number_to_string_36(value).as_deref(), Ok(text), "{value}");
    }
    assert_eq!(number_to_string_36(f64::NAN).as_deref(), Ok("NaN"));
    assert_eq!(
        number_to_string_36(f64::INFINITY).as_deref(),
        Ok("Infinity")
    );
    assert_eq!(
        number_to_string_36(f64::NEG_INFINITY).as_deref(),
        Ok("-Infinity")
    );
}

#[test]
fn round_to_even_carries_into_the_last_digit() {
    // 2^49 + 3/8 and 2^49 + 7/8: the fraction hits exactly one half after an odd digit and the
    // half-ulp window is wide enough, so the digit is bumped (d -> e, v -> w).
    assert_eq!(
        number_to_string_36(562_949_953_421_312.375).as_deref(),
        Ok("5jjrmzbvnk.e")
    );
    assert_eq!(
        number_to_string_36(562_949_953_421_312.875).as_deref(),
        Ok("5jjrmzbvnk.w")
    );
    // a popped run of z digits: 0.0339506... rounds 0.1z... up to 0.18
    assert_eq!(
        number_to_string_36(0.033_950_617_283_950_615).as_deref(),
        Ok("0.18")
    );
    assert_eq!(
        number_to_string_36(9.707_956_518_394_88e-18).as_deref(),
        Ok("0.00000000001a")
    );
}

#[test]
fn carry_back_literals_including_the_integer_carry() {
    let run = |text: &str| {
        let mut digits = text.as_bytes().to_vec();
        let into_integer = carry_back(&mut digits);
        (String::from_utf8(digits).unwrap(), into_integer)
    };
    assert_eq!(run("a"), ("b".to_string(), false));
    assert_eq!(run("1z"), ("2".to_string(), false));
    assert_eq!(run("0zz"), ("1".to_string(), false));
    assert_eq!(run("az"), ("b".to_string(), false));
    assert_eq!(run("9"), ("a".to_string(), false));
    assert_eq!(run("zz"), (String::new(), true));
    assert_eq!(run("z"), (String::new(), true));
    assert_eq!(run(""), (String::new(), true));
}

#[test]
fn key_shapes_for_the_degenerate_random_values() {
    assert_eq!(exposed_key(5.0, 0.0).as_deref(), Ok("ep_5_"));
    assert_eq!(exposed_key(5.0, -0.0).as_deref(), Ok("ep_5_"));
    assert_eq!(exposed_key(5.0, 0.5).as_deref(), Ok("ep_5_i"));
    assert_eq!(exposed_key(5.0, 0.25).as_deref(), Ok("ep_5_9"));
    assert_eq!(exposed_key(1296.0, 0.5).as_deref(), Ok("ep_100_i"));
    assert_eq!(exposed_key(-1.0, -0.5).as_deref(), Ok("ep_-1_.i"));
    assert_eq!(exposed_key(f64::NAN, 0.5).as_deref(), Ok("ep_NaN_i"));
    assert_eq!(exposed_key(3.0, f64::INFINITY).as_deref(), Ok("ep_3_fini"));
    assert_eq!(exposed_key(1.0, 0.123_456_789).as_deref(), Ok("ep_1_4fzz"));
}
