//! Selector value types of the text range-selector kernel (G38b / L06-G):
//! the shape ids, the three string-valued fields as enums, `Selector` with the
//! baseline defaults, the typed refusals and the finiteness check every checked
//! entry point runs before it computes anything. Plain data, no behavior of the
//! baseline beyond `DEFAULT_SELECTOR` and the string comparisons it makes
//! (`=== 'wiggly'`, `=== 'index'`, `'words'` / `'lines'`).
//!
//! This module imports nothing from the crate.

/// Same bound as one text block (`MAX_TEXT_UNITS` of the layout kernel).
pub(crate) const MAX_SELECTOR_UNITS: usize = 65_536;

pub(crate) const SHAPE_SQUARE: f64 = 1.0;
pub(crate) const SHAPE_RAMP_UP: f64 = 2.0;
pub(crate) const SHAPE_RAMP_DOWN: f64 = 3.0;
pub(crate) const SHAPE_TRIANGLE: f64 = 4.0;
pub(crate) const SHAPE_ROUND: f64 = 5.0;
pub(crate) const SHAPE_SMOOTH: f64 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectorType {
    Range,
    Wiggly,
}

impl SelectorType {
    /// `sel.selectorType === 'wiggly'`; anything else is the range selector.
    pub(crate) fn from_baseline(value: &str) -> Self {
        if value == "wiggly" {
            SelectorType::Wiggly
        } else {
            SelectorType::Range
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Units {
    Percent,
    Index,
}

impl Units {
    /// `sel.units === 'index'`; anything else is percent.
    pub(crate) fn from_baseline(value: &str) -> Self {
        if value == "index" {
            Units::Index
        } else {
            Units::Percent
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BasedOn {
    Chars,
    Words,
    Lines,
}

impl BasedOn {
    /// `'words'` / `'lines'`; anything else reads `charIndex`.
    pub(crate) fn from_baseline(value: &str) -> Self {
        match value {
            "words" => BasedOn::Words,
            "lines" => BasedOn::Lines,
            _ => BasedOn::Chars,
        }
    }
}

/// One selector. `None` is JS `null` / `undefined` for the `== null` fields.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Selector {
    pub selector_type: SelectorType,
    pub start: f64,
    pub end: Option<f64>,
    pub offset: f64,
    pub units: Units,
    pub based_on: BasedOn,
    pub shape: f64,
    pub amount: Option<f64>,
    pub ease_high: f64,
    pub ease_low: f64,
    pub smooth: Option<f64>,
    pub wiggle_min: Option<f64>,
    pub wiggle_max: Option<f64>,
    pub wiggles_per_sec: Option<f64>,
    pub correlation: Option<f64>,
    pub seed: f64,
}

impl Selector {
    /// `defaultSelector()` (`DEFAULT_SELECTOR`).
    pub(crate) fn default_selector() -> Self {
        Selector {
            selector_type: SelectorType::Range,
            start: 0.0,
            end: Some(100.0),
            offset: 0.0,
            units: Units::Percent,
            based_on: BasedOn::Chars,
            shape: SHAPE_SQUARE,
            amount: Some(100.0),
            ease_high: 0.0,
            ease_low: 0.0,
            smooth: Some(100.0),
            wiggle_min: Some(-50.0),
            wiggle_max: Some(50.0),
            wiggles_per_sec: Some(2.0),
            correlation: Some(50.0),
            seed: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SelectorError {
    /// A selector field, index, unit count or time that is NaN or infinite.
    NonFinite(&'static str),
    TooManyUnits,
    NonFiniteWeight,
}

pub(crate) fn finite(name: &'static str, value: f64) -> Result<(), SelectorError> {
    if value.is_finite() {
        Ok(())
    } else {
        Err(SelectorError::NonFinite(name))
    }
}

fn finite_opt(name: &'static str, value: Option<f64>) -> Result<(), SelectorError> {
    value.map_or(Ok(()), |v| finite(name, v))
}

/// Refuses a non-finite value in ANY of the 13 numeric fields, including one the
/// selected branch never reads (a NaN `wiggleMin` on a Range selector, which the
/// baseline answers). This is a deliberate native guard, not a baseline rule:
/// a selector with a non-finite field is malformed input, and refusing it up
/// front keeps "which field is bad" independent of the selector type and of the
/// unit index (`a_non_finite_field_the_selected_branch_never_reads_is_refused`).
pub(crate) fn check_selector(sel: &Selector) -> Result<(), SelectorError> {
    finite("start", sel.start)?;
    finite_opt("end", sel.end)?;
    finite("offset", sel.offset)?;
    finite("shape", sel.shape)?;
    finite_opt("amount", sel.amount)?;
    finite("easeHigh", sel.ease_high)?;
    finite("easeLow", sel.ease_low)?;
    finite_opt("smooth", sel.smooth)?;
    finite_opt("wiggleMin", sel.wiggle_min)?;
    finite_opt("wiggleMax", sel.wiggle_max)?;
    finite_opt("wigglesPerSec", sel.wiggles_per_sec)?;
    finite_opt("correlation", sel.correlation)?;
    finite("seed", sel.seed)
}
