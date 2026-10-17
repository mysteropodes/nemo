//! Capture of a text block record from a baseline root-data object (G38a /
//! L06-F): the members `buildVectorTextGroup` assigns to `root.data`, read as
//! JSON by the (later) live-Paper adapter, never touching a Paper item here.
//!
//! Read: `text`, `vectorFont` (mapped through `FontRef::from_legacy_key`),
//! `size`, `color`, `align`, `fixedWidth` and `fixedHeight` (number or `null`),
//! `bold`, `italic`, `underline`, `strike`, `letterSpacing`, `wordSpacing`,
//! `lineHeightMult`, `textCase` and `anchorTopLeft` `{x, y}`. A member that is
//! present keeps its presence: the explicit `0`, `false` and `1.25` a fresh
//! build writes are NOT collapsed (the baseline `serP` collapses them; the
//! record does not). `isTextRoot` must be `true`. `textOverflow` is derived
//! output and is ignored; every other key of the shared data bag (`groupId`,
//! `charIndex`, `isVectorText`, ...) is ignored. The result is validated like a
//! decoded record, so a member the baseline tolerates but the record refuses
//! (`align: 'justify'`, `textCase: 'weird'`) is a typed refusal.

use crate::object_document::object_text_layout_kernels::object_text_layout_record::font::FontRef;
use crate::object_document::object_text_layout_kernels::object_text_layout_record::{
    TextAnchor, TextBlockRecord, TextRecordError,
};
use crate::object_document::Presence;
use serde_json::{Map, Number, Value};

fn as_number(value: &Value) -> Option<Number> {
    match value {
        Value::Number(number) => Some(number.clone()),
        _ => None,
    }
}

fn as_bool(value: &Value) -> Option<bool> {
    value.as_bool()
}

fn as_string(value: &Value) -> Option<String> {
    value.as_str().map(str::to_string)
}

/// `null` is an explicit "no box"; anything but a number or `null` is wrong.
fn as_nullable_number(value: &Value) -> Option<Option<Number>> {
    match value {
        Value::Null => Some(None),
        Value::Number(number) => Some(Some(number.clone())),
        _ => None,
    }
}

fn required<T>(
    data: &Map<String, Value>,
    name: &'static str,
    read: fn(&Value) -> Option<T>,
) -> Result<T, TextRecordError> {
    data.get(name)
        .and_then(read)
        .ok_or(TextRecordError::RootField(name))
}

fn optional<T>(
    data: &Map<String, Value>,
    name: &'static str,
    read: fn(&Value) -> Option<T>,
) -> Result<Presence<T>, TextRecordError> {
    match data.get(name) {
        None => Ok(Presence::Missing),
        Some(value) => read(value)
            .map(Presence::Present)
            .ok_or(TextRecordError::RootField(name)),
    }
}

fn anchor(data: &Map<String, Value>) -> Result<TextAnchor, TextRecordError> {
    let wrong = TextRecordError::RootField("anchorTopLeft");
    let object = data
        .get("anchorTopLeft")
        .and_then(Value::as_object)
        .ok_or_else(|| wrong.clone())?;
    Ok(TextAnchor {
        x: required(object, "x", as_number).map_err(|_| wrong.clone())?,
        y: required(object, "y", as_number).map_err(|_| wrong.clone())?,
    })
}

impl TextBlockRecord {
    /// Capture the record of one text root from its `root.data` as JSON.
    pub(crate) fn capture_root_data(root: &Value) -> Result<Self, TextRecordError> {
        let data = root.as_object().ok_or(TextRecordError::NotATextRoot)?;
        if data.get("isTextRoot") != Some(&Value::Bool(true)) {
            return Err(TextRecordError::NotATextRoot);
        }
        let legacy_font = required(data, "vectorFont", as_string)?;
        let record = Self {
            text: required(data, "text", as_string)?,
            font: FontRef::from_legacy_key(&legacy_font).map_err(TextRecordError::Font)?,
            size: required(data, "size", as_number)?,
            color: required(data, "color", as_string)?,
            align: required(data, "align", as_string)?,
            fixed_width: optional(data, "fixedWidth", as_nullable_number)?,
            fixed_height: optional(data, "fixedHeight", as_nullable_number)?,
            bold: optional(data, "bold", as_bool)?,
            italic: optional(data, "italic", as_bool)?,
            underline: optional(data, "underline", as_bool)?,
            strike: optional(data, "strike", as_bool)?,
            letter_spacing: optional(data, "letterSpacing", as_number)?,
            word_spacing: optional(data, "wordSpacing", as_number)?,
            line_height_mult: optional(data, "lineHeightMult", as_number)?,
            text_case: optional(data, "textCase", as_string)?,
            anchor_top_left: anchor(data)?,
        };
        record.validate()?;
        Ok(record)
    }
}
