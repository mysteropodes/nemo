//! The pressure-ribbon members of a stroke (`isVectorBrush`, centreline, width profile) as the record's `vectorExt.pressure`.
use super::stroke::point;
use super::LegacyImportError;
use serde_json::{json, Map, Value};

pub(super) fn pressure(path: &str, map: &Map<String, Value>) -> Result<Value, LegacyImportError> {
    let mut fields = Map::new();
    if let Some(centre) = map.get("centerSegments").filter(|v| !v.is_null()) {
        let list = centre.as_array().ok_or_else(|| {
            LegacyImportError::invalid(format!("{path}.centerSegments"), "an array is required")
        })?;
        let mut out = Vec::with_capacity(list.len());
        for (index, segment) in list.iter().enumerate() {
            let spath = format!("{path}.centerSegments[{index}]");
            let parts = super::object(segment, &spath)?;
            let width = parts
                .get("width")
                .filter(|w| w.is_number())
                .ok_or_else(|| {
                    LegacyImportError::invalid(format!("{spath}.width"), "a number is required")
                })?;
            out.push(json!({"point": point(&format!("{spath}.point"), parts.get("point"), false)?,
                "handleIn": point(&format!("{spath}.handleIn"), parts.get("handleIn"), true)?,
                "handleOut": point(&format!("{spath}.handleOut"), parts.get("handleOut"), true)?, "width": width}));
        }
        fields.insert("centerSegments".into(), Value::Array(out));
    }
    if let Some(profile) = map.get("widthProfile").filter(|v| !v.is_null()) {
        let list = profile.as_array().ok_or_else(|| {
            LegacyImportError::invalid(format!("{path}.widthProfile"), "an array is required")
        })?;
        let mut out = Vec::with_capacity(list.len());
        for (index, stop) in list.iter().enumerate() {
            let spath = format!("{path}.widthProfile[{index}]");
            let parts = super::object(stop, &spath)?;
            match (
                parts.get("t").filter(|v| v.is_number()),
                parts.get("width").filter(|v| v.is_number()),
            ) {
                (Some(t), Some(width)) => out.push(json!({"t": t, "width": width})),
                _ => {
                    return Err(LegacyImportError::invalid(
                        spath,
                        "t and width numbers are required",
                    ))
                }
            }
        }
        fields.insert("widthProfile".into(), Value::Array(out));
    }
    if let Some(Value::String(kind)) = map
        .get("strokeProfile")
        .filter(|v| !super::classify::is_default(v))
    {
        fields.insert("strokeProfile".into(), json!(kind));
    }
    if let Some(base) = map.get("profileBase").filter(|v| v.is_number()) {
        fields.insert("profileBase".into(), base.clone());
    }
    let mut ext = Map::new();
    ext.insert("pressure".into(), Value::Object(fields));
    if map
        .get("isFillShape")
        .is_some_and(|v| !super::classify::is_default(v))
    {
        ext.insert("isFillShape".into(), Value::Bool(true));
    }
    Ok(Value::Object(ext))
}
