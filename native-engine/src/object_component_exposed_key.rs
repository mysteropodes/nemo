//! Exposed-property key (L04-E, I14/I35): `'ep_' + clock().toString(36) + '_' +
//! random().toString(36).slice(2, 6)` of `expose()` in
//! `src/js/domain/component/exposed-properties.js` at baseline `fbe4b8a3`.
//!
//! The key is the only observable the clock and random ports reach, so the
//! kernel must reproduce `Number.prototype.toString(36)` for any double the
//! ports may hand over. `number_to_string_36` ports the radix algorithm every
//! engine uses for a non-decimal radix (V8's `DoubleToRadixCString`): split the
//! magnitude into integer and fraction, emit fractional digits while the
//! remaining fraction is at least half an ulp (`delta`, scaled with every
//! digit), and round the last digit to even, carrying back through the digits
//! already written. `NaN`, `0`/`-0` and the infinities are the spec's special
//! cases. A magnitude of 2^53 or more has no fraction, but engines disagree on
//! its digits (older V8 zero-filled the low digits, the oracle engine prints
//! them exactly), so it is refused instead of guessed; `Date.now()` and a
//! `Math.random()` value are far below that bound.
//!
//! Real behaviour: `fixtures/object-component-exposed-key/oracle.txt` pins 1,096
//! `toString(36)` rows and 411 key rows executed in a Node `vm` from the
//! verbatim baseline module, and the tests walk every branch of this port. The
//! rules struct exists so the controls can switch each rule off in the real
//! code and prove the oracle notices.

const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
const RADIX: f64 = 36.0;
/// 2^53: first magnitude whose digits are engine dependent.
const ENGINE_DEPENDENT_FROM: f64 = 9_007_199_254_740_992.0;

/// A magnitude of 2^53 or more: `toString(36)` digits differ between engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RadixError {
    EngineDependent,
}

/// The two rules of the fractional loop, switchable by the controls only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RadixRules {
    /// Stop once the remaining fraction is below `delta` (the shortest digits).
    pub(crate) stop_at_delta: bool,
    /// Round the last digit to even and carry back through the digits written.
    pub(crate) round_last_digit: bool,
}

pub(crate) const BASELINE_RADIX_RULES: RadixRules = RadixRules {
    stop_at_delta: true,
    round_last_digit: true,
};

/// `value.toString(36)`.
pub(crate) fn number_to_string_36(value: f64) -> Result<String, RadixError> {
    number_to_string_36_with(value, BASELINE_RADIX_RULES)
}

pub(crate) fn number_to_string_36_with(
    value: f64,
    rules: RadixRules,
) -> Result<String, RadixError> {
    if value.is_nan() {
        return Ok("NaN".to_string());
    }
    if value == 0.0 {
        return Ok("0".to_string());
    }
    if value.is_infinite() {
        return Ok(if value > 0.0 { "Infinity" } else { "-Infinity" }.to_string());
    }
    if value.abs() >= ENGINE_DEPENDENT_FROM {
        return Err(RadixError::EngineDependent);
    }
    let negative = value < 0.0;
    let magnitude = value.abs();
    let mut integer = magnitude.floor();
    let mut fraction = magnitude - integer;
    // Half the distance to the next double, never below the smallest subnormal.
    let mut delta = 0.5 * (f64::from_bits(magnitude.to_bits() + 1) - magnitude);
    if delta < f64::from_bits(1) {
        delta = f64::from_bits(1);
    }
    let mut digits: Vec<u8> = Vec::new();
    if fraction >= delta {
        loop {
            fraction *= RADIX;
            delta *= RADIX;
            // `fraction < 1` before the multiply keeps this below 36; the clamp only
            // guards the index (an unreachable rounding of 35.99999... to 36.0).
            let digit = (fraction as u32).min(35);
            digits.push(DIGITS[digit as usize]);
            fraction -= f64::from(digit);
            let round_up = fraction > 0.5 || (fraction == 0.5 && digit & 1 == 1);
            if rules.round_last_digit && round_up && fraction + delta > 1.0 {
                if carry_back(&mut digits) {
                    integer += 1.0;
                }
                break;
            }
            if rules.stop_at_delta && fraction < delta {
                break;
            }
            if !rules.stop_at_delta && digits.len() >= 52 {
                break;
            }
        }
    }
    let mut whole: Vec<u8> = Vec::new();
    loop {
        let remainder = integer % RADIX;
        whole.push(DIGITS[remainder as usize]);
        integer = (integer - remainder) / RADIX;
        if integer <= 0.0 {
            break;
        }
    }
    let mut text = String::new();
    if negative {
        text.push('-');
    }
    text.extend(whole.iter().rev().map(|byte| char::from(*byte)));
    if !digits.is_empty() {
        text.push('.');
        text.extend(digits.iter().map(|byte| char::from(*byte)));
    }
    Ok(text)
}

/// V8's back-trace after a round-up: drop trailing `z` digits and bump the first
/// other digit. Returns true when every digit was `z` (the carry enters the
/// integer part and no fraction is left).
pub(crate) fn carry_back(digits: &mut Vec<u8>) -> bool {
    loop {
        match digits.pop() {
            None => return true,
            Some(byte) => {
                let value = digit_value(byte);
                if value + 1 < 36 {
                    digits.push(DIGITS[(value + 1) as usize]);
                    return false;
                }
            }
        }
    }
}

fn digit_value(byte: u8) -> u32 {
    if byte > b'9' {
        u32::from(byte - b'a') + 10
    } else {
        u32::from(byte - b'0')
    }
}

/// Rules of the key builder, switchable by the controls only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KeyRules {
    pub(crate) radix: RadixRules,
    /// `.slice(2, 6)` of the random part: first character kept, number of characters kept.
    pub(crate) tail_skip: usize,
    pub(crate) tail_len: usize,
}

pub(crate) const BASELINE_KEY_RULES: KeyRules = KeyRules {
    radix: BASELINE_RADIX_RULES,
    tail_skip: 2,
    tail_len: 4,
};

/// `'ep_' + clock.toString(36) + '_' + random.toString(36).slice(2, 6)`.
pub(crate) fn exposed_key(clock: f64, random: f64) -> Result<String, RadixError> {
    exposed_key_with(clock, random, BASELINE_KEY_RULES)
}

pub(crate) fn exposed_key_with(
    clock: f64,
    random: f64,
    rules: KeyRules,
) -> Result<String, RadixError> {
    let clock_part = number_to_string_36_with(clock, rules.radix)?;
    let random_part = number_to_string_36_with(random, rules.radix)?;
    let tail: String = random_part
        .chars()
        .skip(rules.tail_skip)
        .take(rules.tail_len)
        .collect();
    Ok(format!("ep_{clock_part}_{tail}"))
}

#[cfg(test)]
#[path = "../tests/object_component_exposed_key_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_component_exposed_key.rs"]
mod tests;
