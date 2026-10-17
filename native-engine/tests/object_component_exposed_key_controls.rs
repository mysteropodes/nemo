//! Controls for the exposed-property key: each rule of the real kernel is switched off through
//! `RadixRules` / `KeyRules` and the engine oracle must notice. The unmutated rules must reproduce
//! every row first, so a miss below is caused by the mutation and by nothing else.

use super::tests::{num, rows};
use super::{
    exposed_key_with, number_to_string_36_with, KeyRules, RadixRules, BASELINE_KEY_RULES,
    BASELINE_RADIX_RULES,
};

fn radix_misses(rules: RadixRules) -> usize {
    rows("T")
        .iter()
        .filter(|cols| number_to_string_36_with(num(cols[0]), rules).as_deref() != Ok(cols[1]))
        .count()
}

fn key_misses(rules: KeyRules) -> usize {
    rows("K")
        .iter()
        .filter(|cols| {
            exposed_key_with(num(cols[0]), num(cols[1]), rules).as_deref() != Ok(cols[2])
        })
        .count()
}

#[test]
fn the_unmutated_rules_reproduce_every_row() {
    assert_eq!(radix_misses(BASELINE_RADIX_RULES), 0);
    assert_eq!(key_misses(BASELINE_KEY_RULES), 0);
}

#[test]
fn without_the_half_ulp_stop_the_digits_run_on() {
    let rules = RadixRules {
        stop_at_delta: false,
        ..BASELINE_RADIX_RULES
    };
    assert!(radix_misses(rules) > 0);
    // 0.5 is exactly "0.i"; without the stop the zero remainder keeps emitting digits
    assert_ne!(number_to_string_36_with(0.5, rules).as_deref(), Ok("0.i"));
    assert_ne!(number_to_string_36_with(0.25, rules).as_deref(), Ok("0.9"));
}

#[test]
fn without_round_to_even_the_last_digit_is_not_bumped() {
    let rules = RadixRules {
        round_last_digit: false,
        ..BASELINE_RADIX_RULES
    };
    assert!(radix_misses(rules) > 0);
    // 2^49 + 3/8: digit d, remainder exactly one half, odd digit, wide window -> e
    assert_eq!(
        number_to_string_36_with(562_949_953_421_312.375, rules).as_deref(),
        Ok("5jjrmzbvnk.d")
    );
    assert_ne!(
        number_to_string_36_with(0.1, rules).as_deref(),
        Ok("0.3lllllllllm")
    );
}

#[test]
fn a_shifted_slice_changes_the_key() {
    for (skip, len) in [(1, 4), (3, 4), (2, 3), (2, 5)] {
        let rules = KeyRules {
            tail_skip: skip,
            tail_len: len,
            ..BASELINE_KEY_RULES
        };
        assert!(key_misses(rules) > 0, "slice({skip}, {})", skip + len);
    }
    let one_off = KeyRules {
        tail_skip: 1,
        ..BASELINE_KEY_RULES
    };
    assert_eq!(
        exposed_key_with(5.0, 0.5, one_off).as_deref(),
        Ok("ep_5_.i")
    );
}

#[test]
fn a_mutated_radix_rule_reaches_the_keys() {
    // the same conversion builds both parts of the key. No oracle K row has a clock that needs
    // the round-up (their clocks are whole or dyadic), so the reach is pinned with literals:
    // clock 0.1 converts to "0.3lllllllllm" and, with round-to-even off, to "0.3llllllllll".
    let rules = KeyRules {
        radix: RadixRules {
            round_last_digit: false,
            ..BASELINE_RADIX_RULES
        },
        ..BASELINE_KEY_RULES
    };
    assert_eq!(
        exposed_key_with(0.1, 0.5, BASELINE_KEY_RULES).as_deref(),
        Ok("ep_0.3lllllllllm_i")
    );
    assert_eq!(
        exposed_key_with(0.1, 0.5, rules).as_deref(),
        Ok("ep_0.3llllllllll_i")
    );
    assert_ne!(
        exposed_key_with(0.1, 0.5, rules),
        exposed_key_with(0.1, 0.5, BASELINE_KEY_RULES)
    );
}
