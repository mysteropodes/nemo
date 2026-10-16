//! Ports, value types and identity keys of the text layout kernel (G38a /
//! L06-A). Pure data: the font ports the retained-font provider will fill,
//! the request, the immutable layout result and the typed refusals.

const MAX_ID_BYTES: usize = 255;

/// A font face as the layout needs it: `font.unitsPerEm` and the design
/// advance of one UTF-16 code unit (`font.charToGlyph(unit).advanceWidth`,
/// the `.notdef` advance for an unmapped unit). `None` is a provider failure.
pub(crate) trait FontAdvancePort {
    fn units_per_em(&self) -> f64;
    fn advance_units(&self, utf16_unit: u16) -> Option<f64>;
}

/// The font keys a layout may name (`VECTOR_FONTS`) and their faces.
pub(crate) trait FontCatalogPort {
    fn knows_key(&self, key: &str) -> bool;
    fn face(&self, key: &str) -> Option<&dyn FontAdvancePort>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextAlign {
    Left,
    Center,
    Right,
}

impl TextAlign {
    /// `align === 'center'` / `'right'`; any other value is left.
    pub(crate) fn from_baseline(value: &str) -> Self {
        match value {
            "center" => TextAlign::Center,
            "right" => TextAlign::Right,
            _ => TextAlign::Left,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TextCase {
    Unchanged,
    Upper,
    Lower,
    Capitalize,
}

impl TextCase {
    /// `'upper'`, `'lower'`, `'capitalize'`; anything else leaves the text.
    pub(crate) fn from_baseline(value: &str) -> Self {
        match value {
            "upper" => TextCase::Upper,
            "lower" => TextCase::Lower,
            "capitalize" => TextCase::Capitalize,
            _ => TextCase::Unchanged,
        }
    }
}

/// The typography `opts` of the baseline with their defaults.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct TextStyle {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub letter_spacing: f64,
    pub word_spacing: f64,
    /// `None` is the default `1.25`; an explicit value must be finite and > 0.
    pub line_height_mult: Option<f64>,
    pub text_case: TextCase,
}

impl Default for TextStyle {
    fn default() -> Self {
        TextStyle {
            bold: false,
            italic: false,
            underline: false,
            strike: false,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            line_height_mult: None,
            text_case: TextCase::Unchanged,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct TextLayoutRequest<'a> {
    /// Stable identity of the text block; seeds every glyph/decoration key.
    pub holder_id: &'a str,
    pub text: &'a str,
    pub font_key: &'a str,
    pub size: f64,
    pub align: TextAlign,
    pub fixed_width: Option<f64>,
    pub fixed_height: Option<f64>,
    /// `topLeftWorld` as `[x, y]`.
    pub top_left: [f64; 2],
    pub style: TextStyle,
}

/// Every refusal happens before any output exists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TextLayoutError {
    InvalidHolderId,
    InvalidSize,
    InvalidOrigin,
    InvalidSpacing,
    InvalidLineHeightMultiplier,
    InvalidFixedWidth,
    InvalidFixedHeight,
    TextTooLong,
    /// No font catalog port is installed (retained-font provider absent).
    FontUnavailable,
    /// The resolved key names no face (baseline: `unknown vector font`).
    FontFaceUnavailable(String),
    InvalidFontMetrics,
    GlyphAdvanceUnavailable(u16),
    NonFiniteLayout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DecorationKind {
    Underline,
    Strike,
}

impl DecorationKind {
    fn slug(self) -> &'static str {
        match self {
            DecorationKind::Underline => "underline",
            DecorationKind::Strike => "strike",
        }
    }
}

/// One underline/strike rectangle: `left`, `width`, `thickness` and the Y of
/// its CENTRE (`deco.position.y = ...`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Decoration {
    pub kind: DecorationKind,
    pub left: f64,
    pub width: f64,
    pub thickness: f64,
    pub center_y: f64,
}

/// One non-whitespace UTF-16 unit placed at `(origin_x, baseline_y)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PlacedGlyph {
    /// Glyph ordinal in layout order; equals the baseline `charIndex`.
    pub ordinal: usize,
    pub unit: u16,
    pub origin_x: f64,
    pub baseline_y: f64,
    pub size: f64,
    /// 1-based, running over the whole block (not reset per line).
    pub word_index: usize,
    pub line_index: usize,
    /// Paper `Matrix(a, b, c, d, tx, ty)` of the faux-italic shear.
    pub shear: Option<[f64; 6]>,
}

/// Paint order is baseline order: decorations first, then the glyphs.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LaidOutLine {
    pub line_index: usize,
    pub text: String,
    pub width: f64,
    pub start_x: f64,
    pub baseline_y: f64,
    pub decorations: Vec<Decoration>,
    pub glyphs: Vec<PlacedGlyph>,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LaidOutText {
    pub holder_id: String,
    pub resolved_font_key: String,
    pub line_height: f64,
    /// Widest kept line (`maxW`).
    pub width: f64,
    /// `keptLines * lineHeight`.
    pub height: f64,
    pub overflow: bool,
    pub lines: Vec<LaidOutLine>,
}

pub(crate) fn glyph_key(holder_id: &str, ordinal: usize) -> String {
    format!("{}/glyph/{}", holder_id, ordinal)
}

pub(crate) fn decoration_key(holder_id: &str, line_index: usize, kind: DecorationKind) -> String {
    format!("{}/line/{}/{}", holder_id, line_index, kind.slug())
}

pub(crate) fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_ID_BYTES && !value.contains('\0')
}
