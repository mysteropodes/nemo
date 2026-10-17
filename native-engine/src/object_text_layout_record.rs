//! Text block persistent record (G38a / L06-F): the root metadata of one vector
//! text block as `buildVectorTextGroup` (src/js/vector-text-bridge.js at
//! fbe4b8a3) assigns it to `root.data`, as a strict, presence-aware value.
//!
//! Members, in canonical order (camelCase): `text`, `font` ([`FontRef`], the
//! mapped `vectorFont`), `size`, `color` (`#rrggbb` or `#rrggbbaa`), `align`
//! (`left` | `center` | `right`), `fixedWidth` and `fixedHeight` (a positive
//! number or an explicit `null` for "no box"), `bold`, `italic`, `underline`,
//! `strike`, `letterSpacing`, `wordSpacing`, `lineHeightMult` (positive),
//! `textCase` (`none` | `upper` | `lower` | `capitalize`) and `anchorTopLeft`
//! (`{x, y}`, the point the glyphs were placed FROM). `text`, `font`, `size`,
//! `color`, `align` and `anchorTopLeft` are required; every other member may be
//! absent, and absent, default and explicit stay distinct on a round trip
//! (`fixedWidth` absent is not `null`; an explicit `0` letter spacing and an
//! explicit `1.25` multiplier are kept; the spelling class of a number is kept).
//! An absent member means the baseline default at EVALUATION time only (see
//! [`TextBlockRecord::style`]); nothing is ever written back for it.
//!
//! `textOverflow` is NOT a member: it is derived output (`LaidOutText::overflow`
//! of the layout), and `isText` / `isTextRoot` are the record's type, not data.
//! The per-glyph members (`groupId`, `charIndex`, ...) belong to the glyph
//! items, whose identity is `text_glyph_pin`.
//!
//! Strict decode: object only, unknown, duplicate or `null` (outside
//! `fixedWidth` / `fixedHeight`) members refused, non-empty text of at most
//! `MAX_TEXT_UNITS` UTF-16 units with no NUL and at least one non-whitespace
//! unit (the baseline never creates a root without a glyph; a lone surrogate
//! cannot be spelled: the JSON decoder refuses it). Deliberate differences from
//! the baseline, all refusals: an `align` or `textCase` outside the lists above,
//! a non-hex `color`, a `lineHeightMult` of 0 (the baseline coerces it to 1.25),
//! a `size` of 0 (coerced to 48 on reload) and a font key outside the table.
//!
//! Files: `object_text_layout_font.rs` (font reference), `text_glyph_pin.rs`
//! (glyph identity pins), `object_text_layout_request.rs` (accessors, the
//! typography and the L06-A layout request), `object_text_layout_root.rs`
//! (capture from a live `root.data` object) and this record.

use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::flow::is_js_trim_whitespace;
use crate::evaluation::object_text_layout_kernels::object_text_layout_kernel::MAX_TEXT_UNITS;
use crate::object_document::{object_deserialize, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;

#[path = "object_text_layout_font.rs"]
pub(crate) mod font;
#[path = "text_glyph_pin.rs"]
pub(crate) mod pin;
#[path = "object_text_layout_request.rs"]
pub(crate) mod request;
#[path = "object_text_layout_root.rs"]
pub(crate) mod root;

use font::{FontRef, FontRefError, FontRefWire};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TextRecordError {
    /// Not JSON, not an object, trailing data, or an unknown, duplicate,
    /// `null` or wrongly typed member.
    Malformed,
    Font(FontRefError),
    EmptyText,
    TextContainsNul,
    TextTooLong,
    /// Only whitespace: the baseline never creates a root without a glyph.
    NoVisibleUnit,
    Size,
    Color,
    Align,
    FixedWidth,
    FixedHeight,
    LineHeightMult,
    TextCase,
    /// Capture: the value is not a root-data object with `isTextRoot === true`.
    NotATextRoot,
    /// Capture: this root-data member is missing or has the wrong JSON type.
    RootField(&'static str),
}

impl std::fmt::Display for TextRecordError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "text block record refused: {:?}", self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct TextAnchor {
    x: Number,
    y: Number,
}
object_deserialize!(TextAnchor {
    x: Number,
    y: Number
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextBlockRecord {
    text: String,
    font: FontRef,
    size: Number,
    color: String,
    align: String,
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
    anchor_top_left: TextAnchor,
}

struct TextBlockWire {
    text: String,
    font: FontRefWire,
    size: Number,
    color: String,
    align: String,
    fixed_width: Presence<Option<Number>>,
    fixed_height: Presence<Option<Number>>,
    bold: Presence<bool>,
    italic: Presence<bool>,
    underline: Presence<bool>,
    strike: Presence<bool>,
    letter_spacing: Presence<Number>,
    word_spacing: Presence<Number>,
    line_height_mult: Presence<Number>,
    text_case: Presence<String>,
    anchor_top_left: TextAnchor,
}
object_deserialize!(TextBlockWire {
    text: String, font: FontRefWire, size: Number, color: String, align: String,
    #[serde(default)] fixed_width: Presence<Option<Number>>,
    #[serde(default)] fixed_height: Presence<Option<Number>>,
    #[serde(default)] bold: Presence<bool>,
    #[serde(default)] italic: Presence<bool>,
    #[serde(default)] underline: Presence<bool>,
    #[serde(default)] strike: Presence<bool>,
    #[serde(default)] letter_spacing: Presence<Number>,
    #[serde(default)] word_spacing: Presence<Number>,
    #[serde(default)] line_height_mult: Presence<Number>,
    #[serde(default)] text_case: Presence<String>,
    anchor_top_left: TextAnchor
});

impl<'de> Deserialize<'de> for TextBlockRecord {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = TextBlockWire::deserialize(deserializer)?;
        TextBlockRecord::from_wire(wire).map_err(serde::de::Error::custom)
    }
}

fn f64_of(number: &Number) -> f64 {
    number.as_f64().unwrap_or(f64::NAN)
}

fn positive(number: &Number) -> bool {
    let value = f64_of(number);
    value.is_finite() && value > 0.0
}

fn valid_color(color: &str) -> bool {
    match color.strip_prefix('#') {
        Some(hex) => {
            (hex.len() == 6 || hex.len() == 8) && hex.bytes().all(|b| b.is_ascii_hexdigit())
        }
        None => false,
    }
}

fn check_text(text: &str) -> Result<(), TextRecordError> {
    if text.is_empty() {
        return Err(TextRecordError::EmptyText);
    }
    if text.contains('\0') {
        return Err(TextRecordError::TextContainsNul);
    }
    let mut units = 0_usize;
    let mut visible = false;
    for unit in text.encode_utf16() {
        units += 1;
        if units > MAX_TEXT_UNITS {
            return Err(TextRecordError::TextTooLong);
        }
        if !is_js_trim_whitespace(unit) {
            visible = true;
        }
    }
    if visible {
        Ok(())
    } else {
        Err(TextRecordError::NoVisibleUnit)
    }
}

impl TextBlockRecord {
    fn from_wire(wire: TextBlockWire) -> Result<Self, TextRecordError> {
        let record = Self {
            text: wire.text,
            font: FontRef::from_wire(wire.font).map_err(TextRecordError::Font)?,
            size: wire.size,
            color: wire.color,
            align: wire.align,
            fixed_width: wire.fixed_width,
            fixed_height: wire.fixed_height,
            bold: wire.bold,
            italic: wire.italic,
            underline: wire.underline,
            strike: wire.strike,
            letter_spacing: wire.letter_spacing,
            word_spacing: wire.word_spacing,
            line_height_mult: wire.line_height_mult,
            text_case: wire.text_case,
            anchor_top_left: wire.anchor_top_left,
        };
        record.validate()?;
        Ok(record)
    }

    fn validate(&self) -> Result<(), TextRecordError> {
        check_text(&self.text)?;
        if !positive(&self.size) {
            return Err(TextRecordError::Size);
        }
        if !valid_color(&self.color) {
            return Err(TextRecordError::Color);
        }
        if !matches!(self.align.as_str(), "left" | "center" | "right") {
            return Err(TextRecordError::Align);
        }
        if let Some(Some(width)) = self.fixed_width.value() {
            if !positive(width) {
                return Err(TextRecordError::FixedWidth);
            }
        }
        if let Some(Some(height)) = self.fixed_height.value() {
            if !positive(height) {
                return Err(TextRecordError::FixedHeight);
            }
        }
        if self
            .line_height_mult
            .value()
            .is_some_and(|number| !positive(number))
        {
            return Err(TextRecordError::LineHeightMult);
        }
        if let Some(case) = self.text_case.value() {
            if !matches!(case.as_str(), "none" | "upper" | "lower" | "capitalize") {
                return Err(TextRecordError::TextCase);
            }
        }
        Ok(())
    }

    /// Strict decode of one record's JSON text (see the module notes).
    pub(crate) fn decode(text: &str) -> Result<Self, TextRecordError> {
        let wire: TextBlockWire =
            serde_json::from_str(text).map_err(|_| TextRecordError::Malformed)?;
        Self::from_wire(wire)
    }

    /// Canonical JSON text; member order is fixed, absent members stay absent.
    pub(crate) fn encode(&self) -> Result<String, TextRecordError> {
        serde_json::to_string(self).map_err(|_| TextRecordError::Malformed)
    }
}

#[cfg(test)]
#[path = "../tests/object_text_layout_record_capture.rs"]
mod capture_tests;
#[cfg(test)]
#[path = "../tests/object_text_layout_record_codec.rs"]
mod codec_tests;
#[cfg(test)]
#[path = "../tests/object_text_layout_record_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_text_layout_record_oracle.rs"]
pub(crate) mod oracle;
#[cfg(test)]
#[path = "../tests/object_text_layout_record_refusals.rs"]
mod refusals;
#[cfg(test)]
#[path = "../tests/object_text_layout_record.rs"]
mod tests;
