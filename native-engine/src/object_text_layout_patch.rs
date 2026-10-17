//! L06-L (G38a): the member patch of a text block edit, applied through the strict L06-F codec.
//!
//! A patch names the members to replace and nothing else: `anchorTopLeft` (the point the glyphs are placed FROM) is
//! not a member, so an edit can never re-anchor a block (baseline feedback #37: re-deriving the anchor from ink
//! bounds sank the text on every typography edit). The result is the stored record with exactly the patched members
//! replaced, re-decoded by `TextBlockRecord::decode`; absent / default / explicit values and number spelling classes
//! stay as authored. A patch is a no-op when every patched member already has that EFFECTIVE value (an absent bold reads
//! false, an absent letterSpacing reads 0, an absent lineHeightMult reads the kernel's 1.25, an absent box is null).
use crate::object_document::object_text_layout_kernels::object_text_layout_record::font::FontRef;
use crate::object_document::object_text_layout_kernels::object_text_layout_record::TextBlockRecord;
use crate::object_document::{object_deserialize, Presence};
use crate::request_receipts::DispatchErrorCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Number, Value};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextPatch {
    #[serde(skip_serializing_if = "Presence::missing")]
    text: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    font: Presence<FontRef>,
    #[serde(skip_serializing_if = "Presence::missing")]
    size: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    color: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    align: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    fixed_width: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    fixed_height: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    bold: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    italic: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    underline: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    strike: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    letter_spacing: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    word_spacing: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    line_height_mult: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    text_case: Presence<String>,
}
object_deserialize!(TextPatch {
    #[serde(default)] text: Presence<String>,
    #[serde(default)] font: Presence<FontRef>,
    #[serde(default)] size: Presence<Number>,
    #[serde(default)] color: Presence<String>,
    #[serde(default)] align: Presence<String>,
    #[serde(default)] fixed_width: Presence<Option<Number>>,
    #[serde(default)] fixed_height: Presence<Option<Number>>,
    #[serde(default)] bold: Presence<bool>,
    #[serde(default)] italic: Presence<bool>,
    #[serde(default)] underline: Presence<bool>,
    #[serde(default)] strike: Presence<bool>,
    #[serde(default)] letter_spacing: Presence<Number>,
    #[serde(default)] word_spacing: Presence<Number>,
    #[serde(default)] line_height_mult: Presence<Number>,
    #[serde(default)] text_case: Presence<String>
});

/// What an absent member reads as at evaluation time (the record never writes these back).
fn default_of(member: &str) -> Option<Value> {
    match member {
        "bold" | "italic" | "underline" | "strike" => Some(Value::Bool(false)),
        "letterSpacing" | "wordSpacing" => Some(json!(0)),
        "lineHeightMult" => Some(json!(1.25)),
        "textCase" => Some(json!("none")),
        "fixedWidth" | "fixedHeight" => Some(Value::Null),
        _ => None,
    }
}

fn same_value(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        _ => a == b,
    }
}

/// `Ok(None)` when the patch changes nothing; otherwise the strictly re-decoded record. An empty patch, a value the
/// record refuses (blank text, a non-positive size, an unknown case, ...) is `InvalidRequest`.
pub(crate) fn apply_patch(
    block: &TextBlockRecord,
    patch: &TextPatch,
) -> Result<Option<TextBlockRecord>, DispatchErrorCode> {
    let Value::Object(changes) =
        serde_json::to_value(patch).map_err(|_| DispatchErrorCode::Internal)?
    else {
        return Err(DispatchErrorCode::Internal);
    };
    if changes.is_empty() {
        return Err(DispatchErrorCode::InvalidRequest);
    }
    let Value::Object(mut members) =
        serde_json::to_value(block).map_err(|_| DispatchErrorCode::Internal)?
    else {
        return Err(DispatchErrorCode::Internal);
    };
    let mut changed = false;
    for (member, wanted) in changes {
        let effective = members
            .get(&member)
            .cloned()
            .or_else(|| default_of(&member));
        if !effective.is_some_and(|current| same_value(&current, &wanted)) {
            members.insert(member, wanted);
            changed = true;
        }
    }
    // Strict re-decode even for a no-op patch: a refused value is a refusal, never a silent no-op.
    let text = Value::Object(members).to_string();
    let edited = TextBlockRecord::decode(&text).map_err(|_| DispatchErrorCode::InvalidRequest)?;
    Ok(changed.then_some(edited))
}
