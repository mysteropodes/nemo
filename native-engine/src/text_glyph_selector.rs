//! Text range-selector weight kernel (G38b / L06-G): `src/js/text-selector.js`
//! at fbe4b8a3 (`resolveRange`, `weightAt`, `weights`, `unitIndexOf`, the
//! Wiggly branch) plus the unit count of `taUnitCount` in `src/js/motion.js`,
//! as pure data in and out.
//!
//! A text animator multiplies each animated property by a 0..1 weight computed
//! per character / word / line INDEX on the fly (a port of lottie-web's
//! `TextSelectorProperty`). The only input from the glyph side is the identity
//! L06-A stamps on every placed glyph (`charIndex`, `wordIndex`, `lineIndex`);
//! this kernel has no geometry, no clock and no RNG: a weight is a function of
//! the selector, the unit index, the unit count and `timeSec` only, and the
//! same inputs give the same bits on every call.
//!
//! Reproduced on purpose, in the baseline order of operations:
//! * `start`, `offset`, `easeHigh`, `easeLow`, `shape` and `seed` read with
//!   JS `||` (a zero falls to the default); `end`, `amount`, `smooth` and the
//!   four Wiggly fields read with `== null` (an explicit zero stays zero), which
//!   is why those are `Option`.
//! * A reversed range is swapped, not empty; `units: 'index'` divides by 1 and
//!   percent by `100 / max(1, totalUnits)`.
//! * An unknown shape number falls to SQUARE; the six shapes keep their
//!   `e === s` special cases; ease low/high build the bezier applied to the
//!   weight; `smooth !== 100` narrows the band (`smooth * 0.01 || 1e-8`);
//!   `amount` multiplies last.
//! * The Wiggly weight is NOT clamped to 0..1 (a negative `wiggleMin` inverts).
//! * `unitIndexOf` falls back to `charIndex` when `wordIndex` / `lineIndex` is
//!   null; JS answers `undefined` for a missing `charIndex` and `null` for a
//!   missing stroke, both `None` here.
//!
//! Trigonometry: the Wiggly hash (`Math.sin`) and the SMOOTH shape (`Math.cos`)
//! use Rust `f64::sin` / `f64::cos`, like `expression_noise.rs` (#1675): the
//! macOS desktop runs JavaScriptCore, whose `Math.sin` / `Math.cos` agree with
//! them bit for bit there. The oracle is the verbatim baseline run on Node's V8,
//! so rows that reach `sin` / `cos` are held within a bound (never exact) and
//! every other row is bit for bit. No fdlibm or other third-party code is carried.
//!
//! Native-only guards, all refusals before any output: a non-finite selector
//! field, index, unit count or time; more than [`MAX_SELECTOR_UNITS`] units in
//! `weights`; a non-finite result (a huge `wiggleMax - wiggleMin`).
//! Files: this driver (range and Wiggly weights, `weights`, unit indices),
//! `text_glyph_selector_model.rs` (value types, defaults, refusals),
//! `text_glyph_selector_curve.rs` (easer, hash, noise) and
//! `text_glyph_selector_js.rs` (the JS `||` / `Math.max` / `Math.min` rules).
//! The animated selector (`taSelectorAt`), per-property application and the
//! `textAnimators` document fields belong to later joins (Cyril's
//! expression/transform lanes own animation).

#[path = "text_glyph_selector_curve.rs"]
pub(crate) mod curve;
#[path = "text_glyph_selector_js.rs"]
pub(crate) mod js;
#[path = "text_glyph_selector_model.rs"]
pub(crate) mod model;

use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::model::PlacedGlyph;
use crate::evaluation::object_text_layout_kernels::text_glyph_selector::curve::{
    smooth_noise_1d, Easer,
};
use crate::evaluation::object_text_layout_kernels::text_glyph_selector::js::{
    js_max, js_min, js_or,
};
use crate::evaluation::object_text_layout_kernels::text_glyph_selector::model::{
    check_selector, finite,
};

pub(crate) use crate::evaluation::object_text_layout_kernels::text_glyph_selector::model::{
    BasedOn, Selector, SelectorError, SelectorType, Units, MAX_SELECTOR_UNITS, SHAPE_RAMP_DOWN,
    SHAPE_RAMP_UP, SHAPE_ROUND, SHAPE_SMOOTH, SHAPE_SQUARE, SHAPE_TRIANGLE,
};

/// `resolveRange`: absolute unit positions `(s, e)`, swapped when `s > e`.
/// Unchecked arithmetic (a non-finite input yields non-finite output).
pub(crate) fn resolve_range(sel: &Selector, total_units: f64) -> (f64, f64) {
    let divisor = if sel.units == Units::Index {
        1.0
    } else {
        100.0 / js_max(1.0, total_units)
    };
    let o = js_or(sel.offset, 0.0) / divisor;
    let mut s = js_or(sel.start, 0.0) / divisor + o;
    let mut e = sel.end.unwrap_or(100.0) / divisor + o;
    if s > e {
        std::mem::swap(&mut s, &mut e);
    }
    (s, e)
}

fn ramp(ind: f64, s: f64, e: f64) -> f64 {
    js_max(0.0, js_min(0.5 / (e - s) + (ind - s) / (e - s), 1.0))
}

fn range_weight(sel: &Selector, ind: f64, total_units: f64) -> f64 {
    let (s, e) = resolve_range(sel, total_units);
    let tot = e - s;
    let shape = js_or(sel.shape, SHAPE_SQUARE);
    let (mut x1, mut y1, mut x2, mut y2) = (0.0, 0.0, 1.0, 1.0);
    let ne = js_or(sel.ease_low, 0.0);
    let xe = js_or(sel.ease_high, 0.0);
    if ne > 0.0 {
        x1 = ne / 100.0;
    } else {
        y1 = -ne / 100.0;
    }
    if xe > 0.0 {
        x2 = 1.0 - xe / 100.0;
    } else {
        y2 = 1.0 + xe / 100.0;
    }
    let easer = Easer::new(x1, y1, x2, y2);

    let mut mult = 0.0;
    if shape == SHAPE_RAMP_UP {
        mult = if e == s {
            if ind >= e {
                1.0
            } else {
                0.0
            }
        } else {
            ramp(ind, s, e)
        };
    } else if shape == SHAPE_RAMP_DOWN {
        mult = if e == s {
            if ind >= e {
                0.0
            } else {
                1.0
            }
        } else {
            1.0 - ramp(ind, s, e)
        };
    } else if shape == SHAPE_TRIANGLE {
        if e != s {
            mult = ramp(ind, s, e);
            mult = if mult < 0.5 {
                mult * 2.0
            } else {
                1.0 - 2.0 * (mult - 0.5)
            };
        }
    } else if shape == SHAPE_ROUND {
        if e != s {
            let i2 = js_min(js_max(0.0, ind + 0.5 - s), tot);
            let x = -tot / 2.0 + i2;
            let a = tot / 2.0;
            mult = if a == 0.0 {
                0.0
            } else {
                js_max(0.0, 1.0 - (x * x) / (a * a)).sqrt()
            };
        }
    } else if shape == SHAPE_SMOOTH {
        if e != s {
            let i2 = js_min(js_max(0.0, ind + 0.5 - s), tot);
            mult = (1.0 + (std::f64::consts::PI + std::f64::consts::PI * 2.0 * i2 / (e - s)).cos())
                / 2.0;
        }
    } else if ind >= s.floor() {
        mult = if ind - s < 0.0 {
            js_max(0.0, js_min(js_min(e, 1.0) - (s - ind), 1.0))
        } else {
            js_max(0.0, js_min(e - ind, 1.0))
        };
    }

    mult = easer.apply(js_max(0.0, js_min(1.0, mult)));

    let sm = sel.smooth.unwrap_or(100.0);
    if sm != 100.0 {
        let smooth = js_or(sm * 0.01, 1e-8);
        let threshold = 0.5 - smooth * 0.5;
        mult = if mult < threshold {
            0.0
        } else {
            js_min(1.0, (mult - threshold) / smooth)
        };
    }
    mult * (sel.amount.unwrap_or(100.0) / 100.0)
}

fn wiggly_weight(sel: &Selector, ind: f64, time_sec: f64) -> f64 {
    let seed = js_or(sel.seed, 1.0);
    let hz = sel.wiggles_per_sec.unwrap_or(2.0);
    let t = js_or(time_sec, 0.0) * hz;
    let local = smooth_noise_1d(seed, t + ind * 37.13);
    let global = smooth_noise_1d(seed + 9973.0, t);
    let corr = js_max(0.0, js_min(100.0, sel.correlation.unwrap_or(50.0))) / 100.0;
    let n = local + (global - local) * corr;
    let lo = sel.wiggle_min.unwrap_or(-50.0);
    let hi = sel.wiggle_max.unwrap_or(50.0);
    (lo + (n * 0.5 + 0.5) * (hi - lo)) / 100.0
}

fn weight_checked(
    sel: &Selector,
    ind: f64,
    total_units: f64,
    time_sec: f64,
) -> Result<f64, SelectorError> {
    let weight = if sel.selector_type == SelectorType::Wiggly {
        wiggly_weight(sel, ind, time_sec)
    } else {
        range_weight(sel, ind, total_units)
    };
    if weight.is_finite() {
        Ok(weight)
    } else {
        Err(SelectorError::NonFiniteWeight)
    }
}

/// `weightAt(sel, ind, totalUnits, timeSec)`; `time_sec` is ignored by the range selector.
pub(crate) fn weight_at(
    sel: &Selector,
    ind: f64,
    total_units: f64,
    time_sec: f64,
) -> Result<f64, SelectorError> {
    check_selector(sel)?;
    finite("ind", ind)?;
    finite("totalUnits", total_units)?;
    finite("timeSec", time_sec)?;
    weight_checked(sel, ind, total_units, time_sec)
}

/// `weights(sel, totalUnits, timeSec)`: one weight per index `0 <= i < totalUnits`.
pub(crate) fn weights(
    sel: &Selector,
    total_units: f64,
    time_sec: f64,
) -> Result<Vec<f64>, SelectorError> {
    check_selector(sel)?;
    finite("totalUnits", total_units)?;
    finite("timeSec", time_sec)?;
    if total_units > MAX_SELECTOR_UNITS as f64 {
        return Err(SelectorError::TooManyUnits);
    }
    let mut out = Vec::new();
    let mut i: u32 = 0;
    while f64::from(i) < total_units {
        out.push(weight_checked(sel, f64::from(i), total_units, time_sec)?);
        i += 1;
    }
    Ok(out)
}

/// The three indices a glyph stroke carries; `None` is `null` / `undefined`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct UnitIndices {
    pub char_index: Option<f64>,
    pub word_index: Option<f64>,
    pub line_index: Option<f64>,
}

impl UnitIndices {
    /// The identity L06-A stamps: `charIndex` is the ordinal, the others as laid out.
    pub(crate) fn of_glyph(glyph: &PlacedGlyph) -> Self {
        UnitIndices {
            char_index: Some(glyph.ordinal as f64),
            word_index: Some(glyph.word_index as f64),
            line_index: Some(glyph.line_index as f64),
        }
    }
}

/// `unitIndexOf(sd, basedOn)`: the index a glyph counts as; `!= null` fallback to `charIndex`.
pub(crate) fn unit_index_of(sd: Option<&UnitIndices>, based_on: BasedOn) -> Option<f64> {
    let sd = sd?;
    match based_on {
        BasedOn::Words => sd.word_index.or(sd.char_index),
        BasedOn::Lines => sd.line_index.or(sd.char_index),
        BasedOn::Chars => sd.char_index,
    }
}

/// The loop of `taUnitCount`: highest unit index plus one (0 when no glyph has one).
pub(crate) fn unit_count(glyphs: &[UnitIndices], based_on: BasedOn) -> f64 {
    let mut max = -1.0;
    for glyph in glyphs {
        if let Some(u) = unit_index_of(Some(glyph), based_on) {
            if u > max {
                max = u;
            }
        }
    }
    max + 1.0
}

#[cfg(test)]
#[path = "../tests/text_glyph_selector_controls.rs"]
mod controls;

#[cfg(test)]
#[path = "../tests/text_glyph_selector_defaults.rs"]
mod defaults;

#[cfg(test)]
#[path = "../tests/text_glyph_selector_literals.rs"]
mod literals;

#[cfg(test)]
#[path = "../tests/text_glyph_selector_mutations.rs"]
mod mutations;

#[cfg(test)]
#[path = "../tests/text_glyph_selector_oracle.rs"]
pub(crate) mod oracle;

#[cfg(test)]
#[path = "../tests/text_glyph_selector_scenarios.rs"]
pub(crate) mod scenarios;

#[cfg(test)]
#[path = "../tests/text_glyph_selector.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/text_glyph_selector_wiggly.rs"]
mod wiggly_tests;
