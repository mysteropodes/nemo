//! Exposed component properties, instance side (L04-E, I14/I35): `applyOverrides` of
//! `src/js/domain/component/exposed-properties.js` at baseline `fbe4b8a3`, over native stroke records.
//!
//! The baseline resolves overrides LAST, after every transform, on the strokes a component
//! instance produces: the instance layer carries each property's value (read through
//! `SMMotion.valueAtFrame`, here the [`OverridePort`]), a stroke is addressed by its stable
//! `strokeId` only, and the symbol's stored strokes are never touched because a stroke is deep
//! cloned on its first override. The native input is the already transformed `ObjectRecord` slice;
//! the output is a detached copy (`Cow::Owned`), or the very input slice (`Cow::Borrowed`) when the
//! component declares nothing or the port is unavailable, as the baseline returns `out` itself.
//!
//! Order of port calls, from the baseline: `available()` once (skipped for an empty list), then
//! `register` for every declaration in declaration order, then for each record in output order
//! the reads of ITS declarations in declaration order (declarations are grouped by target stroke
//! first, so the global order is by stroke, not by declaration). Per declaration: `__visible`
//! sets a sticky `hide` when the value is below 50 (hide wins over every other override, later
//! visible values cannot undo it, NaN never hides) and writes nothing; `opacity` writes
//! `clamp(value / 100, 0, 1)`; any other field is written verbatim. Hidden records are dropped.
//!
//! Native mapping of `targetField`: `__visible` needs nothing; `opacity` is the vector body's
//! `opacity` (a legacy-family record has none: `NeedsVectorBody`); `fillColor` is the record's
//! fill (`Paint(None)` is "no fill" for a vector record, refused for the legacy family whose fill
//! is required); `strokeColor` is the vector body's stroke and applies only to a REAL stroke,
//! because `hasRealStroke === false` is authoritative in the baseline renderer (`desP`,
//! `app.js:862`), so the written colour is never shown and the record stays as it is. Every
//! other field, a value of the wrong kind, an empty read and a NaN opacity are typed refusals
//! (`ExposedError`); the baseline would write garbage or `undefined` there.

use std::borrow::Cow;
use std::collections::HashMap;

use crate::evaluation::object_component_kernels::object_component_exposed::{
    ExposedEntry, ExposedError, ExposedValue,
};
use crate::object_document::{ObjectRecord, Presence};
use serde_json::Number;

/// What the host supplies: availability of the Motion runtime, the metadata registration and the
/// value of an exposed key on the instance layer at a frame. One object, bound to one instance.
pub(crate) trait OverridePort {
    /// `available()`: false when the property runtime (SMMotion) is not wired.
    fn available(&mut self) -> bool;
    /// `metadata(ep)`: idempotent registration of the property row.
    fn register(&mut self, prop: &ExposedEntry);
    /// `read(ld, key, frame)`: the value components; the kernel uses the first one.
    fn read(&mut self, layer_uid: &str, key: &str, frame: u32)
        -> Result<Vec<ExposedValue>, String>;
}

/// The baseline constants, switchable by the controls only.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct OverrideRules {
    /// `val < 50` hides.
    pub(crate) hide_below: f64,
    /// `val / 100`.
    pub(crate) opacity_divisor: f64,
    /// `Math.max(0, Math.min(1, ...))`.
    pub(crate) clamp_opacity: bool,
    /// `hide = true` is never reset by a later visible value.
    pub(crate) sticky_hide: bool,
    /// A hidden record is dropped (`return null` then `filter`).
    pub(crate) drop_hidden: bool,
}

pub(crate) const BASELINE_OVERRIDE_RULES: OverrideRules = OverrideRules {
    hide_below: 50.0,
    opacity_divisor: 100.0,
    clamp_opacity: true,
    sticky_hide: true,
    drop_hidden: true,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    Visible,
    Opacity,
    FillColor,
    StrokeColor,
}

/// `applyOverrides(sym, out, ld, frameIdx, available, metadata, read)`.
pub(crate) fn apply_overrides<'a>(
    props: &[ExposedEntry],
    out: &'a [ObjectRecord],
    layer_uid: &str,
    frame: u32,
    port: &mut dyn OverridePort,
) -> Result<Cow<'a, [ObjectRecord]>, ExposedError> {
    apply_overrides_with(BASELINE_OVERRIDE_RULES, props, out, layer_uid, frame, port)
}

pub(crate) fn apply_overrides_with<'a>(
    rules: OverrideRules,
    props: &[ExposedEntry],
    out: &'a [ObjectRecord],
    layer_uid: &str,
    frame: u32,
    port: &mut dyn OverridePort,
) -> Result<Cow<'a, [ObjectRecord]>, ExposedError> {
    if props.is_empty() || !port.available() {
        return Ok(Cow::Borrowed(out));
    }
    for prop in props {
        port.register(prop);
    }
    let mut by_stroke: HashMap<&str, Vec<&ExposedEntry>> = HashMap::new();
    for prop in props {
        by_stroke
            .entry(prop.target_stroke_id.as_str())
            .or_default()
            .push(prop);
    }
    let mut result: Vec<ObjectRecord> = Vec::with_capacity(out.len());
    for record in out {
        let id = record.target.stroke_id.as_str();
        // `!sd.strokeId`: an empty id never matches anything.
        let group = if id.is_empty() {
            None
        } else {
            by_stroke.get(id)
        };
        let Some(group) = group else {
            result.push(record.clone());
            continue;
        };
        let (changed, hide) = override_record(rules, record, group, layer_uid, frame, port)?;
        if hide && rules.drop_hidden {
            continue;
        }
        result.push(changed.unwrap_or_else(|| record.clone()));
    }
    Ok(Cow::Owned(result))
}

fn override_record(
    rules: OverrideRules,
    record: &ObjectRecord,
    group: &[&ExposedEntry],
    layer_uid: &str,
    frame: u32,
    port: &mut dyn OverridePort,
) -> Result<(Option<ObjectRecord>, bool), ExposedError> {
    let mut changed: Option<ObjectRecord> = None;
    let mut hide = false;
    for prop in group {
        let field = classify(prop, record)?;
        let first = read_first(port, layer_uid, prop, frame)?;
        if field == Field::Visible {
            let below = number(first, prop)? < rules.hide_below;
            hide = if rules.sticky_hide {
                hide || below
            } else {
                below
            };
            continue;
        }
        let target = changed.get_or_insert_with(|| record.clone());
        write_field(rules, target, field, first, prop)?;
    }
    Ok((changed, hide))
}

/// Maps `targetField` to a native member before any value is read.
fn classify(prop: &ExposedEntry, record: &ObjectRecord) -> Result<Field, ExposedError> {
    let field = match prop.target_field.as_str() {
        "__visible" => Field::Visible,
        "opacity" => Field::Opacity,
        "fillColor" => Field::FillColor,
        "strokeColor" => Field::StrokeColor,
        other => {
            return Err(ExposedError::UnsupportedField {
                key: prop.key.clone(),
                field: other.to_string(),
            })
        }
    };
    let needs_vector = matches!(field, Field::Opacity | Field::StrokeColor);
    if needs_vector && record.vector.missing() {
        return Err(ExposedError::NeedsVectorBody {
            key: prop.key.clone(),
            field: prop.target_field.clone(),
        });
    }
    Ok(field)
}

fn read_first(
    port: &mut dyn OverridePort,
    layer_uid: &str,
    prop: &ExposedEntry,
    frame: u32,
) -> Result<ExposedValue, ExposedError> {
    let values = port
        .read(layer_uid, &prop.key, frame)
        .map_err(|message| ExposedError::Port {
            key: prop.key.clone(),
            message,
        })?;
    values
        .into_iter()
        .next()
        .ok_or_else(|| ExposedError::NoFirstComponent {
            key: prop.key.clone(),
        })
}

fn number(value: ExposedValue, prop: &ExposedEntry) -> Result<f64, ExposedError> {
    match value {
        ExposedValue::Number(number) => Ok(number),
        ExposedValue::Paint(_) => Err(kind_error(prop)),
    }
}

fn kind_error(prop: &ExposedEntry) -> ExposedError {
    ExposedError::ValueKind {
        key: prop.key.clone(),
        field: prop.target_field.clone(),
    }
}

fn write_field(
    rules: OverrideRules,
    target: &mut ObjectRecord,
    field: Field,
    first: ExposedValue,
    prop: &ExposedEntry,
) -> Result<(), ExposedError> {
    match field {
        Field::Visible => {}
        Field::Opacity => {
            let scaled = number(first, prop)? / rules.opacity_divisor;
            let value = if rules.clamp_opacity {
                clamp_unit(scaled)
            } else {
                scaled
            };
            let opacity = js_number(value).ok_or_else(|| ExposedError::NonFiniteOpacity {
                key: prop.key.clone(),
            })?;
            if let Presence::Present(body) = &mut target.vector {
                body.opacity = opacity;
            }
        }
        Field::FillColor => {
            let ExposedValue::Paint(paint) = first else {
                return Err(kind_error(prop));
            };
            if paint.is_none() && target.vector.missing() {
                return Err(ExposedError::FillRequired {
                    key: prop.key.clone(),
                });
            }
            target.fill = Presence::Present(paint);
        }
        Field::StrokeColor => {
            let ExposedValue::Paint(paint) = first else {
                return Err(kind_error(prop));
            };
            if let Presence::Present(body) = &mut target.vector {
                if body.has_real_stroke {
                    let Some(paint) = paint else {
                        return Err(ExposedError::NoStrokePaint {
                            key: prop.key.clone(),
                        });
                    };
                    body.stroke = Presence::Present(Some(paint));
                }
            }
        }
    }
    Ok(())
}

/// `Math.max(0, Math.min(1, x))`: NaN stays NaN, `-0` becomes `+0`.
fn clamp_unit(x: f64) -> f64 {
    if x.is_nan() {
        return f64::NAN;
    }
    let capped = if x < 1.0 { x } else { 1.0 };
    if capped > 0.0 {
        capped
    } else {
        0.0
    }
}

/// The JSON number `JSON.stringify` would write: an integral value is an integer, the rest a
/// float. Non-finite values have no JSON form.
fn js_number(value: f64) -> Option<Number> {
    if !value.is_finite() {
        return None;
    }
    if value == value.trunc() && value.abs() < 9_007_199_254_740_992.0 {
        return Some(Number::from(value as i64));
    }
    Number::from_f64(value)
}

#[cfg(test)]
#[path = "../tests/object_component_exposed_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_component_exposed_support.rs"]
mod support;
#[cfg(test)]
#[path = "../tests/object_component_exposed_apply.rs"]
mod tests;
