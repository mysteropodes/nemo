//! Glyph identity pins (G38a / L06-F): the stable name of one glyph of one text
//! block, and its checked resolution against an immutable layout.
//!
//! The layout kernel (L06-A) names a glyph `"<holder>/glyph/<ordinal>"`
//! (`glyph_key`); the ordinal is the baseline `charIndex`, counted over the
//! non-whitespace UTF-16 units of the whole block. Text selectors, animators
//! and scene entries refer to glyphs through that key, never through a frame
//! local index, so they need the inverse: [`GlyphPin::parse`] splits at the LAST
//! `/glyph/` (an ordinal contains no `/`, so the split is unambiguous even when
//! the holder id itself contains the separator) and accepts only the canonical
//! spelling `glyph_key` writes (digits, no sign, no leading zero). A pin never
//! carries a position: [`GlyphPin::resolve`] reads it from a layout, and a layout
//! of another holder or a smaller block refuses instead of answering.

use crate::object_document::object_text_layout_kernels::object_text_layout_kernel::model::{
    glyph_key, valid_id, LaidOutText, PlacedGlyph,
};

const SEPARATOR: &str = "/glyph/";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GlyphPin {
    holder_id: String,
    ordinal: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlyphPinError {
    /// The holder id is blank, over 255 bytes or contains NUL.
    InvalidHolder,
    /// The key is not the canonical `glyph_key` spelling.
    MalformedKey,
    /// The layout belongs to another text block.
    WrongHolder,
    /// The block has no glyph with this ordinal.
    OrdinalOutOfRange,
}

impl GlyphPin {
    pub(crate) fn new(holder_id: &str, ordinal: usize) -> Result<Self, GlyphPinError> {
        if !valid_id(holder_id) {
            return Err(GlyphPinError::InvalidHolder);
        }
        Ok(Self {
            holder_id: holder_id.to_string(),
            ordinal,
        })
    }

    pub(crate) fn holder_id(&self) -> &str {
        &self.holder_id
    }

    pub(crate) fn ordinal(&self) -> usize {
        self.ordinal
    }

    /// The kernel's identity key of this glyph.
    pub(crate) fn key(&self) -> String {
        glyph_key(&self.holder_id, self.ordinal)
    }

    /// Exact inverse of [`GlyphPin::key`].
    pub(crate) fn parse(key: &str) -> Result<Self, GlyphPinError> {
        let at = key.rfind(SEPARATOR).ok_or(GlyphPinError::MalformedKey)?;
        let digits = &key[at + SEPARATOR.len()..];
        let canonical = !digits.is_empty()
            && digits.bytes().all(|byte| byte.is_ascii_digit())
            && (digits == "0" || !digits.starts_with('0'));
        if !canonical {
            return Err(GlyphPinError::MalformedKey);
        }
        let ordinal: usize = digits.parse().map_err(|_| GlyphPinError::MalformedKey)?;
        Self::new(&key[..at], ordinal).map_err(|_| GlyphPinError::MalformedKey)
    }

    /// The placed glyph this pin names in `layout`.
    pub(crate) fn resolve<'a>(
        &self,
        layout: &'a LaidOutText,
    ) -> Result<&'a PlacedGlyph, GlyphPinError> {
        if layout.holder_id != self.holder_id {
            return Err(GlyphPinError::WrongHolder);
        }
        layout
            .lines
            .iter()
            .flat_map(|line| line.glyphs.iter())
            .find(|glyph| glyph.ordinal == self.ordinal)
            .ok_or(GlyphPinError::OrdinalOutOfRange)
    }
}

/// One pin per placed glyph of `layout`, in layout (paint) order.
pub(crate) fn pins_of(layout: &LaidOutText) -> Vec<GlyphPin> {
    layout
        .lines
        .iter()
        .flat_map(|line| line.glyphs.iter())
        .map(|glyph| GlyphPin {
            holder_id: layout.holder_id.clone(),
            ordinal: glyph.ordinal,
        })
        .collect()
}

#[cfg(test)]
#[path = "../tests/text_glyph_pin.rs"]
mod tests;
