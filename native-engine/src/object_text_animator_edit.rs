//! Bounded field/key intents against one existing native animator, never a caller-sampled track.
use crate::object_document::object_text_layout_kernels::animator::{AnimatorRecord, SELECTOR_KEYS};
use crate::object_document::{OrderCurvePoint, Presence, PropertyRecord};
use crate::request_receipts::DispatchErrorCode as E;
use serde::Deserialize;
use serde_json::{Number, Value};
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Action {
    SetEnabled {
        enabled: bool,
    },
    SetSelector {
        field: String,
        value: Value,
    },
    SetProps {
        field: String,
        value: Value,
    },
    SetValue {
        property: String,
        frame: Number,
        values: Vec<Number>,
    },
    SetKey {
        property: String,
        frame: Number,
        values: Vec<Number>,
        #[serde(default, rename = "curvePoints")]
        curve_points: Presence<Option<Vec<OrderCurvePoint>>>,
    },
    RemoveKey {
        property: String,
        frame: Number,
    },
    SetAnimated {
        property: String,
        frame: Number,
        animated: bool,
    },
}
#[derive(Debug)]
pub(crate) struct AnimatorEdit {
    action: Action,
}
// Map-only tagged decoding on original bytes (no duplicate-erasing Value for action fields).
impl<'de> Deserialize<'de> for AnimatorEdit {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = AnimatorEdit;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a native animator edit object")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                map: A,
            ) -> Result<Self::Value, A::Error> {
                Action::deserialize(serde::de::value::MapAccessDeserializer::new(map))
                    .map(|action| AnimatorEdit { action })
            }
        }
        d.deserialize_map(Visitor)
    }
}
fn finite(n: &Number) -> Result<f64, E> {
    n.as_f64()
        .filter(|n| n.is_finite())
        .ok_or(E::InvalidRequest)
}
fn values(v: &[Number]) -> Result<Number, E> {
    if v.len() != 1 {
        return Err(E::InvalidRequest);
    }
    finite(&v[0])?;
    Ok(v[0].clone())
}
fn decode(v: Value) -> Result<AnimatorRecord, E> {
    serde_json::from_value(v).map_err(|_| E::InvalidRequest)
}
// Match the existing property editor's JavaScript-number no-op rule while
// retaining missing/null and real structural edits as authored changes.
fn preserve_numbers(before: &Value, after: &mut Value) {
    match (before, after) {
        (Value::Number(a), Value::Number(b)) if a.as_f64() == b.as_f64() => *b = a.clone(),
        (Value::Array(a), Value::Array(b)) => {
            for (a, b) in a.iter().zip(b) {
                preserve_numbers(a, b);
            }
        }
        _ => {}
    }
}
fn set_field(
    record: &AnimatorRecord,
    group: &str,
    field: &str,
    value: &Value,
) -> Result<AnimatorRecord, E> {
    let fields = if group == "selector" {
        vec![
            "selectorType",
            "units",
            "basedOn",
            "start",
            "end",
            "offset",
            "shape",
            "amount",
            "easeHigh",
            "easeLow",
            "smooth",
            "wiggleMin",
            "wiggleMax",
            "wigglesPerSec",
            "correlation",
            "seed",
        ]
    } else {
        vec![
            "position",
            "scale",
            "rotation",
            "opacity",
            "fillColor",
            "strokeColor",
        ]
    };
    if !fields.contains(&field) {
        return Err(E::InvalidRequest);
    }
    let mut out = serde_json::to_value(record).map_err(|_| E::Internal)?;
    let remove =
        group == "props" && matches!(field, "fillColor" | "strokeColor") && value.is_null();
    if remove && out.get(group).is_none() {
        return Ok(record.clone());
    }
    if out.get(group).is_none() {
        out[group] = serde_json::json!({});
    }
    let object = out[group].as_object_mut().ok_or(E::Internal)?;
    if remove {
        object.remove(field);
    } else {
        let mut value = value.clone();
        if let Some(before) = object.get(field) {
            preserve_numbers(before, &mut value);
        }
        object.insert(field.into(), value);
    }
    decode(out)
}
fn current_value(record: &AnimatorRecord, property: &str, at: f64) -> Result<Number, E> {
    #[cfg(feature = "evaluation")]
    {
        let facts = record.selector_facts(at).map_err(|_| E::Unavailable)?;
        return facts[property]
            .as_f64()
            .and_then(Number::from_f64)
            .ok_or(E::Unavailable);
    }
    #[cfg(not(feature = "evaluation"))]
    {
        let _ = (record, property, at);
        Err(E::Unavailable)
    }
}
pub(super) fn apply(record: &AnimatorRecord, edit: &AnimatorEdit) -> Result<AnimatorRecord, E> {
    match &edit.action {
        Action::SetEnabled { enabled } => {
            let mut out = record.clone();
            out.enabled = Presence::Present(*enabled);
            return Ok(out);
        }
        Action::SetSelector { field, value } => return set_field(record, "selector", field, value),
        Action::SetProps { field, value } => return set_field(record, "props", field, value),
        _ => (),
    }
    let (property, frame) = match &edit.action {
        Action::SetValue {
            property, frame, ..
        }
        | Action::SetKey {
            property, frame, ..
        }
        | Action::RemoveKey { property, frame }
        | Action::SetAnimated {
            property, frame, ..
        } => (property, frame),
        _ => unreachable!(),
    };
    if !SELECTOR_KEYS.contains(&property.as_str()) {
        return Err(E::InvalidRequest);
    }
    let at = finite(frame)?;
    let mut out = record.clone();
    let mut properties = record.properties.value().cloned().unwrap_or_default();
    let index = properties.iter().position(|r| r.property() == property);
    let mut edited = index
        .map(|i| properties[i].clone())
        .unwrap_or_else(|| PropertyRecord {
            property: property.clone(),
            static_value: Presence::Missing,
            track: Presence::Missing,
            expression: Presence::Missing,
        });
    let before = edited.clone();
    match &edit.action {
        Action::SetValue { values: v, .. } => {
            edited.scalar_intent_value(frame.clone(), values(v)?, false, None)
        }
        Action::SetKey {
            values: v,
            curve_points,
            ..
        } => edited.scalar_intent_value(
            frame.clone(),
            values(v)?,
            true,
            curve_points.value().cloned().flatten(),
        ),
        Action::RemoveKey { .. } => edited.scalar_intent_remove(at),
        Action::SetAnimated { animated, .. } => {
            if edited.scalar_intent_keyed() != *animated {
                let value = current_value(record, property, at)?;
                if *animated {
                    edited.scalar_intent_value(frame.clone(), value, true, None);
                } else {
                    edited.scalar_intent_freeze(value);
                }
            }
        }
        _ => unreachable!(),
    }
    edited.validate_scalar().map_err(|_| E::InvalidRequest)?;
    if edited == before {
        return Ok(out);
    }
    if let Some(index) = index {
        properties[index] = edited;
    } else {
        properties.push(edited);
    }
    out.properties = Presence::Present(properties);
    decode(serde_json::to_value(out).map_err(|_| E::Internal)?)
}
