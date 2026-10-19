//! Closed metadata DTO; the existing native AudioTrack/history owner remains authoritative.
use super::exact;
use serde_json::Value;

pub(crate) const OP: &str = "command.document.object.audio.track.edit";
fn audio_id(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|id| !id.trim().is_empty() && id.len() <= 255 && !id.contains('\0'))
}
fn name(value: Option<&Value>) -> bool {
    value
        .and_then(Value::as_str)
        .is_some_and(|name| name.len() <= 255 && !name.contains('\0'))
}
pub(super) fn request(value: &Value) -> bool {
    match value.get("kind").and_then(Value::as_str) {
        Some("setMuted") => exact(value, &["kind", "audioId", "muted"], &[])
            .is_some_and(|v| audio_id(v.get("audioId")) && v["muted"].is_boolean()),
        Some("rename") => exact(value, &["kind", "audioId", "name"], &[])
            .is_some_and(|v| audio_id(v.get("audioId")) && name(v.get("name"))),
        Some("setTrim") => exact(value, &["kind", "audioId"], &["trimStart", "trimEnd"])
            .is_some_and(|v| {
                audio_id(v.get("audioId"))
                    && (v.contains_key("trimStart") || v.contains_key("trimEnd"))
                    && v.get("trimStart").is_none_or(Value::is_number)
                    && v.get("trimEnd")
                        .is_none_or(|n| n.is_null() || n.is_number())
            }),
        Some("unset") => exact(value, &["kind", "audioId", "field"], &[]).is_some_and(|v| {
            audio_id(v.get("audioId"))
                && matches!(
                    v["field"].as_str(),
                    Some("name" | "muted" | "trimStart" | "trimEnd")
                )
        }),
        _ => false,
    }
}
pub(super) fn result(value: &Value) -> bool {
    super::object::visibility_result(value)
}
pub(crate) fn revision_matches(expected: Option<u64>, revision: u64, value: &Value) -> bool {
    result(value)
        && expected.and_then(|before| before.checked_add(u64::from(value["applied"] == true)))
            == Some(revision)
}

#[cfg(test)]
#[path = "native_audio_track_contract_tests.rs"]
mod tests;

// Separate original-byte and shared wire-response controls are authored by A2.
#[cfg(test)]
#[path = "native_audio_track_wire_tests.rs"]
mod wire_tests;
