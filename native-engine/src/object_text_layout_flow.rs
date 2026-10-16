//! Case mapping, line splitting and greedy wrap for the text layout kernel
//! (G38a / L06-A): the pure string-flow half of `src/js/vector-text-bridge.js`
//! at fbe4b8a3 (`applyTextCase`, `wrapVectorLines`, the `runWidth` closure of
//! `buildVectorTextGroup` and JS `String.prototype.trim` whitespace).
//!
//! Text is handled as the UTF-16 code units JS iterates with `split('')`; a
//! line is a half-open `Span` of those units. Per-unit advances are resolved
//! once by the caller, so every width here is a left-to-right sum of the same
//! `f64` additions the baseline performs on a freshly measured string. The
//! greedy wrap keeps one running raw sum for the current line and continues it
//! (`cur + ' ' + word`), which repeats the baseline's addition sequence bit
//! for bit instead of re-measuring the joined string each word.

use super::model::TextCase;

/// Half-open range of UTF-16 units of the displayed (case-mapped) text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Span {
    pub(crate) start: usize,
    pub(crate) end: usize,
}

/// `applyTextCase`: `toUpperCase`, `toLowerCase`, or `replace(/\b\w/g, ...)`
/// whose `\b\w` is the ASCII word character that follows a non-word unit
/// (or the start); anything else leaves the text untouched.
pub(crate) fn apply_text_case(text: &str, text_case: TextCase) -> String {
    match text_case {
        TextCase::Upper => text.to_uppercase(),
        TextCase::Lower => text.to_lowercase(),
        TextCase::Capitalize => capitalize(text),
        TextCase::Unchanged => text.to_string(),
    }
}

fn capitalize(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut previous_is_word = false;
    for ch in text.chars() {
        let is_word = ch.is_ascii_alphanumeric() || ch == '_';
        if is_word && !previous_is_word {
            out.push(ch.to_ascii_uppercase());
        } else {
            out.push(ch);
        }
        previous_is_word = is_word;
    }
    out
}

/// `ch.trim() !== ''` is false exactly for ECMAScript WhiteSpace and
/// LineTerminator code units; a lone surrogate half is not whitespace.
pub(crate) fn is_js_trim_whitespace(unit: u16) -> bool {
    matches!(
        unit,
        0x0009..=0x000D
            | 0x0020
            | 0x00A0
            | 0x1680
            | 0x2000..=0x200A
            | 0x2028
            | 0x2029
            | 0x202F
            | 0x205F
            | 0x3000
            | 0xFEFF
    )
}

/// `text.split('\n')`: always at least one (possibly empty) line.
pub(crate) fn split_source_lines(units: &[u16]) -> Vec<Span> {
    let mut lines = Vec::new();
    let mut start = 0;
    for (index, unit) in units.iter().enumerate() {
        if *unit == 10 {
            lines.push(Span { start, end: index });
            start = index + 1;
        }
    }
    lines.push(Span {
        start,
        end: units.len(),
    });
    lines
}

/// Continues a left-to-right `+=` sum of per-unit advances over `span`.
pub(crate) fn continue_sum(advances: &[f64], start: f64, span: Span) -> f64 {
    let mut sum = start;
    for advance in &advances[span.start..span.end] {
        sum += *advance;
    }
    sum
}

/// `runWidth` of a raw advance sum: the trailing tracking is removed and the
/// result never goes below zero (so an empty run is `max(0, -letterSpacing)`).
pub(crate) fn run_width(raw_sum: f64, letter_spacing: f64) -> f64 {
    (raw_sum - letter_spacing).max(0.0)
}

/// `wrapVectorLines`. Without a (truthy) box width the source lines are
/// returned unchanged. Otherwise each non-empty source line is split at single
/// spaces and words are appended while `runWidth(cur + ' ' + word)` is not
/// greater than the box; an over-long word is never split, an empty current
/// line is replaced by the next word (so leading and doubled spaces vanish at
/// a break exactly as in the baseline), and empty source lines stay empty.
pub(crate) fn wrap_lines(
    units: &[u16],
    advances: &[f64],
    fixed_width: Option<f64>,
    letter_spacing: f64,
) -> Vec<Span> {
    let sources = split_source_lines(units);
    let limit = match fixed_width {
        Some(width) if width != 0.0 => width,
        _ => return sources,
    };
    let mut wrapped = Vec::new();
    for line in sources {
        if line.start == line.end {
            wrapped.push(line);
            continue;
        }
        let mut current = Span {
            start: line.start,
            end: line.start,
        };
        let mut current_raw = 0.0_f64;
        let mut word_start = line.start;
        loop {
            let word_end = units[word_start..line.end]
                .iter()
                .position(|unit| *unit == 32)
                .map_or(line.end, |offset| word_start + offset);
            let word = Span {
                start: word_start,
                end: word_end,
            };
            let word_raw = continue_sum(advances, 0.0, word);
            if current.end > current.start {
                // The separator is the single space just before this word.
                let test_raw = continue_sum(advances, current_raw + advances[word_start - 1], word);
                if run_width(test_raw, letter_spacing) > limit {
                    wrapped.push(current);
                    current = word;
                    current_raw = word_raw;
                } else {
                    current.end = word_end;
                    current_raw = test_raw;
                }
            } else {
                current = word;
                current_raw = word_raw;
            }
            if word_end == line.end {
                break;
            }
            word_start = word_end + 1;
        }
        wrapped.push(current);
    }
    wrapped
}
