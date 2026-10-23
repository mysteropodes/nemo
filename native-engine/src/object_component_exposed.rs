//! Exposed component properties, declaration side (L04-E, I14/I35): the `expose()` half of
//! `src/js/domain/component/exposed-properties.js` at baseline `fbe4b8a3`, plus the typed values
//! and refusals shared with the override half (`object_component_exposed_apply.rs`).
//!
//! `expose()` declares a property ON THE COMPONENT (`state.symbols[symId].exposedProps`), bound to
//! one stroke inside it by its stable `strokeId` and to one `targetField`. The kernel is pure: the
//! caller hands over the component's current declarations (absent, empty or listed), the clock and
//! random ports and an optional metadata port, and gets the new entry plus the complete new list
//! back to publish through the one sealed owner commit. Nothing is mutated here, so a refusal can
//! never leave a half-built list (the baseline pushed first and ran the metadata callback after).
//!
//! Baseline order is kept: no symbol means `None` and no port runs; otherwise `clock()`, then
//! `random()`, then the entry is appended, then `metadata(entry)`. The key is
//! `ep_<clock base36>_<4 random base36 digits>` (`object_component_exposed_key.rs`).
//!
//! Deliberate differences, each pinned by a control: the baseline would append a second entry with
//! a key that already exists (same clock and random), which later makes the Motion property rows
//! collide; the native list refuses it (`DuplicateKey`). A clock or random of 2^53 or more has
//! engine-dependent digits and is refused (`Key`). The baseline `default` may be any JavaScript
//! value; the only authored kinds are a number (`Math.round(opacity * 100)` or `100`) and absent.

use crate::evaluation::object_component_kernels::object_component_exposed_key::{
    exposed_key, RadixError,
};
use crate::object_document::{Presence, SolidFill};
use serde_json::Number;

/// One declared property, in the baseline member order `key, label, targetStrokeId, targetField,
/// default`. `default_value` keeps an absent default apart from an explicit zero.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExposedEntry {
    pub(crate) key: String,
    pub(crate) label: String,
    pub(crate) target_stroke_id: String,
    pub(crate) target_field: String,
    pub(crate) default_value: Presence<Number>,
}

/// First component of what `SMMotion.valueAtFrame` returns for an exposed key: a Motion scalar,
/// or a paint already decoded to the native type (`None` is an explicit "no paint").
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ExposedValue {
    Number(f64),
    Paint(Option<SolidFill>),
}

/// Typed refusals of both halves. None of them falls back to a default value.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ExposedError {
    /// A clock or random magnitude of 2^53 or more.
    Key(RadixError),
    /// The generated key is already declared on the component.
    DuplicateKey { key: String },
    /// The value port failed for this key.
    Port { key: String, message: String },
    /// The value port returned an empty array (baseline `[0]` would be `undefined`).
    NoFirstComponent { key: String },
    /// A paint where a number is needed, or a number where a paint is needed.
    ValueKind { key: String, field: String },
    /// The opacity override came out as NaN (the baseline would write NaN, serialised as null).
    NonFiniteOpacity { key: String },
    /// The baseline writes any `targetField` verbatim; the native record has no member for this one.
    UnsupportedField { key: String, field: String },
    /// `opacity` and `strokeColor` live in the vector body; this record is the legacy family.
    NeedsVectorBody { key: String, field: String },
    /// `strokeColor: null` on a real stroke: the baseline then paints the `#fff` fallback.
    NoStrokePaint { key: String },
    /// `fillColor: null` on a legacy record, whose fill is required.
    FillRequired { key: String },
}

/// The arguments of `expose()` that are data: target, label and the authored default.
#[derive(Debug, Clone)]
pub(crate) struct ExposeRequest<'a> {
    pub(crate) target_stroke_id: &'a str,
    pub(crate) target_field: &'a str,
    pub(crate) label: &'a str,
    pub(crate) default_value: Presence<Number>,
}

/// The new entry and the complete declaration list to publish (existing entries first).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ExposedOutcome {
    pub(crate) entry: ExposedEntry,
    pub(crate) props: Vec<ExposedEntry>,
}

/// `expose(symbols, symId, ...)`. `symbol` is the component's declaration slot: `None` when there
/// is no such component (`return null`, no port runs), `Missing` for a component that never
/// exposed anything (the baseline initialises `[]`), `Present` for a list.
pub(crate) fn expose(
    symbol: Option<&Presence<Vec<ExposedEntry>>>,
    request: &ExposeRequest<'_>,
    clock: &mut dyn FnMut() -> f64,
    random: &mut dyn FnMut() -> f64,
    metadata: Option<&mut dyn FnMut(&ExposedEntry)>,
) -> Result<Option<ExposedOutcome>, ExposedError> {
    let Some(symbol) = symbol else {
        return Ok(None);
    };
    let mut props: Vec<ExposedEntry> = symbol.value().cloned().unwrap_or_default();
    let clock_value = clock();
    let random_value = random();
    let key = exposed_key(clock_value, random_value).map_err(ExposedError::Key)?;
    if props.iter().any(|existing| existing.key == key) {
        return Err(ExposedError::DuplicateKey { key });
    }
    let entry = ExposedEntry {
        key,
        label: request.label.to_string(),
        target_stroke_id: request.target_stroke_id.to_string(),
        target_field: request.target_field.to_string(),
        default_value: request.default_value.clone(),
    };
    props.push(entry.clone());
    if let Some(register) = metadata {
        register(&entry);
    }
    Ok(Some(ExposedOutcome { entry, props }))
}

#[cfg(test)]
#[path = "../tests/object_component_exposed.rs"]
mod tests;
