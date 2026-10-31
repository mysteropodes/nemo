//! Closed public numeric edits; the native property owner resolves applicability.
use crate::contract::MAX_SAFE_REVISION;
use crate::native_contract::{exact, object};
use serde_json::Value;
pub(crate) const OP: &str = "command.document.object.property.edit";
fn nonempty(value: Option<&Value>) -> bool {
    value.and_then(Value::as_str).is_some_and(|s| !s.is_empty())
}
fn finite(value: Option<&Value>) -> bool {
    value.and_then(Value::as_f64).is_some_and(f64::is_finite)
}
fn values(value: &Value) -> bool {
    value.as_array().is_some_and(|values| {
        (1..=4).contains(&values.len()) && values.iter().all(|v| finite(Some(v)))
    })
}
// The native owner caps authored code at the evaluator's 4096 UTF-8 bytes; JSON string
// escapes may only shrink the request, so the same bound holds on the wire.
fn code(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|code| code.len() <= 4096)
}
fn action(value: &Value) -> bool {
    match value.get("kind").and_then(Value::as_str) {
        Some("setExpression") => {
            exact(value, &["kind", "code"], &[]).is_some_and(|a| code(a.get("code")))
        }
        Some("setExpressionEnabled") => exact(value, &["kind", "enabled"], &[])
            .is_some_and(|a| a["enabled"].is_boolean()),
        Some("removeExpression") => exact(value, &["kind"], &[]).is_some(),
        Some("bake") => exact(value, &["kind"], &[]).is_some(),
        Some("setValue" | "setKey") => exact(value, &["kind", "frame", "values"], &[])
            .is_some_and(|a| finite(a.get("frame")) && values(&a["values"])),
        Some("removeKey") => {
            exact(value, &["kind", "frame"], &[]).is_some_and(|a| finite(a.get("frame")))
        }
        Some("setAnimated") => exact(value, &["kind", "frame", "animated"], &[])
            .is_some_and(|a| finite(a.get("frame")) && a["animated"].is_boolean()),
        _ => false,
    }
}
pub(super) fn request(value: &Value) -> bool {
    exact(value, &["command", "target", "property", "action"], &[]).is_some_and(|p| {
        p["command"].as_str() == Some("object.property.edit")
            && nonempty(p.get("property"))
            && exact(&p["target"], &["contextId", "layerUid"], &["strokeId"]).is_some_and(|t| {
                nonempty(t.get("contextId"))
                    && nonempty(t.get("layerUid"))
                    && (!t.contains_key("strokeId") || nonempty(t.get("strokeId")))
            })
            && action(&p["action"])
    })
}
pub(super) fn result(value: &Value) -> bool {
    object::visibility_result(value)
}
pub(crate) fn revision_matches(expected: Option<u64>, revision: u64, value: &Value) -> bool {
    result(value)
        && revision <= MAX_SAFE_REVISION
        && expected
            .filter(|v| *v <= MAX_SAFE_REVISION)
            .and_then(|v| v.checked_add(u64::from(value["applied"] == true)))
            == Some(revision)
}
#[cfg(test)]
#[path = "native_property_edit_contract_tests.rs"]
mod tests;
