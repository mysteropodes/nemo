//! Text layout kernel (G38a / L06-A): the layout half of `buildVectorTextGroup`
//! in `src/js/vector-text-bridge.js` at fbe4b8a3, as pure data in and out.
//!
//! It reproduces, bit for bit, the case mapping, greedy wrap on a box width,
//! tracking and word spacing, `lineHeight = size * (mult || 1.25)`, baseline
//! `top + li * lineHeight + size * 0.8`, alignment inside
//! `max(widest line, box width)`, area-text line cap with the overflow flag,
//! underline/strike rectangles, the faux-italic shear matrix and the
//! `charIndex` / `wordIndex` / `lineIndex` of every glyph. Glyph outlines,
//! Paper paths, the baseline's random `groupId` and every document write stay
//! out: the result is immutable layout data keyed by a deterministic identity
//! `(holder id, glyph ordinal)`.
//!
//! Files: `object_text_layout_model.rs` (ports, value types, identity keys),
//! `object_text_layout_flow.rs` (case, line split, wrap) and this driver.
//!
//! The font comes through two ports. `FontCatalogPort` answers which keys
//! exist (the bold/regular key resolution of the baseline) and hands out one
//! `FontAdvancePort` per key (`unitsPerEm` and the design advance of a UTF-16
//! unit, as `font.charToGlyph(ch).advanceWidth`). The retained-font provider
//! (Assistant2, G41) plugs in later; without it the layout is the typed
//! `FontUnavailable`, never a fallback metric table.
//!
//! Deliberate differences from the baseline, all refusals made before any
//! output: a non-finite or non-positive `size`, a non-finite spacing, a
//! `lineHeightMult` that is not finite and positive when present (the baseline
//! coerces an explicit `0` to `1.25`; here `None` is the default and `Some(0)`
//! is refused), a non-finite or negative box width or height, and more than
//! `MAX_TEXT_UNITS` units. `Some(0.0)` for a box width or height still means
//! "no box" because the baseline reads `fixedWidth || null`.
//! The underline/strike rectangle centre is the exact `baseline +/- k*size`
//! target (Paper reaches it through a translate that may differ by an ulp).
//! Locale-sensitive casing is out of scope (JS `toUpperCase` is not
//! locale-aware either; Rust's full Unicode mapping is used).

#[path = "object_text_layout_flow.rs"]
pub(crate) mod flow;
#[path = "object_text_layout_model.rs"]
pub(crate) mod model;

use flow::{apply_text_case, continue_sum, is_js_trim_whitespace, run_width, wrap_lines, Span};
use model::{
    valid_id, Decoration, DecorationKind, FontAdvancePort, FontCatalogPort, LaidOutLine,
    LaidOutText, PlacedGlyph, TextAlign, TextLayoutError, TextLayoutRequest, TextStyle,
};

/// Upper bound on the UTF-16 units of one block after case mapping.
pub(crate) const MAX_TEXT_UNITS: usize = 65_536;
/// `opts.lineHeightMult || 1.25` when the multiplier is absent.
pub(crate) const DEFAULT_LINE_HEIGHT_MULT: f64 = 1.25;
/// `Math.tan(12 * Math.PI / 180)` as V8 returns it, pinned so every platform
/// shears identically (std `tan` may differ in the last place).
pub(crate) const ITALIC_SHEAR: f64 = 0.2125565616700221;

/// `opts.fixedWidth || null`: a zero box is no box.
fn present(value: Option<f64>) -> Option<f64> {
    value.filter(|v| *v != 0.0)
}

fn validate(request: &TextLayoutRequest<'_>) -> Result<(), TextLayoutError> {
    if !valid_id(request.holder_id) {
        return Err(TextLayoutError::InvalidHolderId);
    }
    if !(request.size.is_finite() && request.size > 0.0) {
        return Err(TextLayoutError::InvalidSize);
    }
    if !request.top_left.iter().all(|v| v.is_finite()) {
        return Err(TextLayoutError::InvalidOrigin);
    }
    let style = &request.style;
    if !(style.letter_spacing.is_finite() && style.word_spacing.is_finite()) {
        return Err(TextLayoutError::InvalidSpacing);
    }
    if let Some(mult) = style.line_height_mult {
        if !(mult.is_finite() && mult > 0.0) {
            return Err(TextLayoutError::InvalidLineHeightMultiplier);
        }
    }
    let box_ok = |v: Option<f64>| match v {
        None => true,
        Some(v) => v.is_finite() && v >= 0.0,
    };
    if !box_ok(request.fixed_width) {
        return Err(TextLayoutError::InvalidFixedWidth);
    }
    if !box_ok(request.fixed_height) {
        return Err(TextLayoutError::InvalidFixedHeight);
    }
    // 3 bytes per BMP unit is the most UTF-8 spends; case mapping is re-checked below.
    if request.text.len() > MAX_TEXT_UNITS * 3 {
        return Err(TextLayoutError::TextTooLong);
    }
    Ok(())
}

/// `baseFamily = fontKey.replace(/-(Bold|Regular)$/, '')`, then the
/// requested weight, falling back to the key as given when that is unknown.
pub(crate) fn resolve_font_key(
    font_key: &str,
    bold: bool,
    catalog: &dyn FontCatalogPort,
) -> String {
    let base = font_key
        .strip_suffix("-Bold")
        .or_else(|| font_key.strip_suffix("-Regular"))
        .unwrap_or(font_key);
    let weight = if bold { "Bold" } else { "Regular" };
    let resolved = format!("{}-{}", base, weight);
    if catalog.knows_key(&resolved) {
        resolved
    } else {
        font_key.to_string()
    }
}

/// `advance(ch, ord)` of every unit: `advanceWidth * (size / unitsPerEm)` plus
/// tracking, plus word spacing for a space. `\n` is never measured.
pub(crate) fn unit_advances(
    face: &dyn FontAdvancePort,
    units: &[u16],
    size: f64,
    style: &TextStyle,
) -> Result<Vec<f64>, TextLayoutError> {
    let upm = face.units_per_em();
    if !(upm.is_finite() && upm > 0.0) {
        return Err(TextLayoutError::InvalidFontMetrics);
    }
    let scale = size / upm;
    let mut advances = Vec::with_capacity(units.len());
    for unit in units {
        if *unit == 10 {
            advances.push(0.0);
            continue;
        }
        let design = face
            .advance_units(*unit)
            .ok_or(TextLayoutError::GlyphAdvanceUnavailable(*unit))?;
        if !(design.is_finite() && design >= 0.0) {
            return Err(TextLayoutError::InvalidFontMetrics);
        }
        let mut advance = design * scale;
        if *unit != 32 {
            advance += style.letter_spacing;
        } else {
            advance += style.letter_spacing + style.word_spacing;
        }
        if !advance.is_finite() {
            return Err(TextLayoutError::InvalidFontMetrics);
        }
        advances.push(advance);
    }
    Ok(advances)
}

/// Area text: keep `max(1, floor(fixedHeight / lineHeight))` lines; true only
/// when lines were dropped.
fn apply_area_limit(spans: &mut Vec<Span>, fixed_height: Option<f64>, line_height: f64) -> bool {
    let height = match present(fixed_height) {
        Some(height) => height,
        None => return false,
    };
    let fits = (height / line_height).floor().max(1.0);
    if (spans.len() as f64) > fits {
        spans.truncate(fits as usize);
        true
    } else {
        false
    }
}

struct Pen {
    words: usize,
    chars: usize,
}

struct Flow<'a> {
    request: &'a TextLayoutRequest<'a>,
    units: &'a [u16],
    advances: &'a [f64],
    line_height: f64,
    box_width: f64,
}

impl Flow<'_> {
    fn line(&self, index: usize, span: Span, pen: &mut Pen) -> LaidOutLine {
        let request = self.request;
        let style = &request.style;
        let width = run_width(continue_sum(self.advances, 0.0, span), style.letter_spacing);
        let start_x = match request.align {
            TextAlign::Left => request.top_left[0],
            TextAlign::Center => request.top_left[0] + (self.box_width - width) / 2.0,
            TextAlign::Right => request.top_left[0] + (self.box_width - width),
        };
        let baseline_y =
            request.top_left[1] + (index as f64) * self.line_height + request.size * 0.8;
        let units = &self.units[span.start..span.end];
        let mut decorations = Vec::new();
        let has_ink = units.iter().any(|unit| !is_js_trim_whitespace(*unit));
        if (style.underline || style.strike) && has_ink {
            let thickness = (request.size * 0.06).max(1.0);
            let mut push = |kind, center_y| {
                decorations.push(Decoration {
                    kind,
                    left: start_x,
                    width,
                    thickness,
                    center_y,
                });
            };
            if style.underline {
                push(DecorationKind::Underline, baseline_y + request.size * 0.08);
            }
            if style.strike {
                push(DecorationKind::Strike, baseline_y - request.size * 0.28);
            }
        }
        let shear = if style.italic {
            Some([1.0, 0.0, -ITALIC_SHEAR, 1.0, ITALIC_SHEAR * baseline_y, 0.0])
        } else {
            None
        };
        let mut glyphs = Vec::new();
        let mut cursor_x = start_x;
        let mut at_word_start = true;
        for (offset, unit) in units.iter().enumerate() {
            if !is_js_trim_whitespace(*unit) {
                if at_word_start {
                    pen.words += 1;
                    at_word_start = false;
                }
                glyphs.push(PlacedGlyph {
                    ordinal: pen.chars,
                    unit: *unit,
                    origin_x: cursor_x,
                    baseline_y,
                    size: request.size,
                    word_index: pen.words,
                    line_index: index,
                    shear,
                });
                pen.chars += 1;
            } else {
                at_word_start = true;
            }
            cursor_x += self.advances[span.start + offset];
        }
        LaidOutLine {
            line_index: index,
            text: String::from_utf16_lossy(units),
            width,
            start_x,
            baseline_y,
            decorations,
            glyphs,
        }
    }
}

fn all_finite(text: &LaidOutText) -> bool {
    text.line_height.is_finite()
        && text.width.is_finite()
        && text.height.is_finite()
        && text.lines.iter().all(|line| {
            line.width.is_finite()
                && line.start_x.is_finite()
                && line.baseline_y.is_finite()
                && line
                    .decorations
                    .iter()
                    .all(|d| d.left.is_finite() && d.width.is_finite() && d.center_y.is_finite())
                && line.glyphs.iter().all(|g| g.origin_x.is_finite())
        })
}

/// Lays out one text block. Pure: the same request and font port give the
/// same bytes of output; nothing is published or retained.
pub(crate) fn layout_text(
    request: &TextLayoutRequest<'_>,
    catalog: Option<&dyn FontCatalogPort>,
) -> Result<LaidOutText, TextLayoutError> {
    validate(request)?;
    let catalog = catalog.ok_or(TextLayoutError::FontUnavailable)?;
    let resolved = resolve_font_key(request.font_key, request.style.bold, catalog);
    let face = catalog
        .face(&resolved)
        .ok_or_else(|| TextLayoutError::FontFaceUnavailable(resolved.clone()))?;
    let display = apply_text_case(request.text, request.style.text_case);
    let units: Vec<u16> = display.encode_utf16().collect();
    if units.len() > MAX_TEXT_UNITS {
        return Err(TextLayoutError::TextTooLong);
    }
    let advances = unit_advances(face, &units, request.size, &request.style)?;
    let mult = request
        .style
        .line_height_mult
        .unwrap_or(DEFAULT_LINE_HEIGHT_MULT);
    let line_height = request.size * mult;
    let fixed_width = present(request.fixed_width);
    let letter_spacing = request.style.letter_spacing;
    let mut spans = wrap_lines(&units, &advances, fixed_width, letter_spacing);
    let overflow = apply_area_limit(&mut spans, request.fixed_height, line_height);
    let max_width = spans.iter().fold(0.0_f64, |widest, span| {
        widest.max(run_width(
            continue_sum(&advances, 0.0, *span),
            letter_spacing,
        ))
    });
    let flow = Flow {
        request,
        units: &units,
        advances: &advances,
        line_height,
        box_width: max_width.max(fixed_width.unwrap_or(0.0)),
    };
    let mut pen = Pen { words: 0, chars: 0 };
    let lines: Vec<LaidOutLine> = spans
        .iter()
        .enumerate()
        .map(|(index, span)| flow.line(index, *span, &mut pen))
        .collect();
    let laid_out = LaidOutText {
        holder_id: request.holder_id.to_string(),
        resolved_font_key: resolved,
        line_height,
        width: max_width,
        height: (lines.len() as f64) * line_height,
        overflow,
        lines,
    };
    if !all_finite(&laid_out) {
        return Err(TextLayoutError::NonFiniteLayout);
    }
    Ok(laid_out)
}

#[cfg(test)]
#[path = "../tests/object_text_layout_kernel_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_text_layout_kernel_oracle.rs"]
pub(crate) mod oracle;
#[cfg(test)]
#[path = "../tests/object_text_layout_kernel_refusals.rs"]
mod refusals;
#[cfg(test)]
#[path = "../tests/object_text_layout_kernel.rs"]
mod tests;
