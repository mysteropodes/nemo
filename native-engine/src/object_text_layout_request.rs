//! Accessors and the layout-kernel view of a text block record (G38a / L06-F):
//! authored (absent / null / value) readings, the typography with defaults
//! applied at evaluation only, and the L06-A `TextLayoutRequest`.

use crate::object_document::object_text_layout_kernels::object_text_layout_kernel::model::{
    TextAlign, TextCase, TextLayoutRequest, TextStyle,
};
use crate::object_document::object_text_layout_kernels::object_text_layout_record::font::FontRef;
use crate::object_document::object_text_layout_kernels::object_text_layout_record::{
    f64_of, TextBlockRecord,
};
use crate::object_document::Presence;
use serde_json::Number;

fn box_extent(value: &Presence<Option<Number>>) -> Option<f64> {
    value.value().and_then(|inner| inner.as_ref()).map(f64_of)
}

impl TextBlockRecord {
    pub(crate) fn text(&self) -> &str {
        &self.text
    }
    pub(crate) fn font(&self) -> &FontRef {
        &self.font
    }
    pub(crate) fn size(&self) -> f64 {
        f64_of(&self.size)
    }
    pub(crate) fn color(&self) -> &str {
        &self.color
    }
    pub(crate) fn align(&self) -> TextAlign {
        TextAlign::from_baseline(&self.align)
    }
    /// The point the glyphs are placed from (`topLeftWorld`).
    pub(crate) fn anchor(&self) -> [f64; 2] {
        [
            f64_of(&self.anchor_top_left.x),
            f64_of(&self.anchor_top_left.y),
        ]
    }
    /// Three states: `None` absent, `Some(None)` explicit `null`, `Some(Some(w))`.
    pub(crate) fn fixed_width_authored(&self) -> Option<Option<f64>> {
        self.fixed_width
            .value()
            .map(|inner| inner.as_ref().map(f64_of))
    }
    /// Three states, as [`TextBlockRecord::fixed_width_authored`].
    pub(crate) fn fixed_height_authored(&self) -> Option<Option<f64>> {
        self.fixed_height
            .value()
            .map(|inner| inner.as_ref().map(f64_of))
    }
    pub(crate) fn letter_spacing_authored(&self) -> Option<f64> {
        self.letter_spacing.value().map(f64_of)
    }
    pub(crate) fn word_spacing_authored(&self) -> Option<f64> {
        self.word_spacing.value().map(f64_of)
    }
    pub(crate) fn line_height_mult_authored(&self) -> Option<f64> {
        self.line_height_mult.value().map(f64_of)
    }

    /// The typography the layout kernel reads, defaults applied HERE and only
    /// here: flags false, spacing 0, case unchanged, and an absent multiplier
    /// stays `None` so the kernel applies its own `1.25`.
    pub(crate) fn style(&self) -> TextStyle {
        let flag = |value: &Presence<bool>| value.value().copied().unwrap_or(false);
        let spacing = |value: &Presence<Number>| value.value().map(f64_of).unwrap_or(0.0);
        TextStyle {
            bold: flag(&self.bold),
            italic: flag(&self.italic),
            underline: flag(&self.underline),
            strike: flag(&self.strike),
            letter_spacing: spacing(&self.letter_spacing),
            word_spacing: spacing(&self.word_spacing),
            line_height_mult: self.line_height_mult.value().map(f64_of),
            text_case: self
                .text_case
                .value()
                .map_or(TextCase::Unchanged, |name| TextCase::from_baseline(name)),
        }
    }

    /// The layout request of this block for the L06-A kernel. `fixedWidth` /
    /// `fixedHeight` that are absent or `null` mean "no box".
    pub(crate) fn layout_request<'a>(&'a self, holder_id: &'a str) -> TextLayoutRequest<'a> {
        TextLayoutRequest {
            holder_id,
            text: &self.text,
            font_key: self.font.layout_key(),
            size: self.size(),
            align: self.align(),
            fixed_width: box_extent(&self.fixed_width),
            fixed_height: box_extent(&self.fixed_height),
            top_left: self.anchor(),
            style: self.style(),
        }
    }
}
