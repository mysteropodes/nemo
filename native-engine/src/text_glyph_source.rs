//! Glyph source (G38a / G38b, L06-J): the font side of `buildVectorTextGroup`
//! in `src/js/vector-text-bridge.js` at fbe4b8a3, as pure data in and out.
//!
//! The baseline asks opentype.js three things of a font: `font.unitsPerEm`,
//! `font.charToGlyph(ch).advanceWidth` (the L06-A [`FontAdvancePort`]) and
//! `font.charToGlyph(ch).getPath(x, y, size).commands` (this module's
//! [`GlyphSourcePort::outline`], the input of the L06-B contour kernel). This
//! file owns the part of that which is pure arithmetic, ported from the
//! vendored `src/js/opentype.min.js`:
//!
//! * [`design_commands`] is opentype's `glyfToPath`: TrueType contours (on and
//!   off-curve points in font units, y up) to `M`/`L`/`Q`/`Z`. A contour
//!   starts at its LAST point when that is on the curve, else at its first when
//!   that is on the curve, else at the midpoint of the two; an off-curve point
//!   followed by another off-curve point ends at their midpoint.
//! * [`scale_commands`] is `Glyph#getPath`: `scale = 1 / unitsPerEm * size`,
//!   `x + cx * scale`, `y + -cy * scale` (so the result is in the final pixel
//!   space with y down).
//! * [`transform_points`] / [`assemble_composite`] are `Dt` and `Mt` of a
//!   composite glyph: `x' = xScale*x + scale01*y + dx`,
//!   `y' = scale10*x + yScale*y + dy`, parts concatenated in component order.
//!
//! Reading a font file is NOT here: [`GlyphSourcePort`] is the seam, and the
//! retained-font provider's bytes are read by the `font` child (behind the
//! existing `font_asset_admission` feature, over the `read-fonts` parser that
//! Assistant2's provider already depends on). Without that child, or without
//! retained bytes, nothing here can produce an outline: the join reports a
//! typed unavailability and never falls back to another writer.
//!
//! Deliberate differences from opentype.js, all typed refusals before output:
//! a composite that uses point matching instead of offsets, nesting deeper than
//! [`MAX_COMPOSITE_DEPTH`], a component or contour index outside the font, a
//! non-ascending contour end table, a cmap format other than 4 or 12 (opentype
//! throws while parsing), and non-finite output. None of the eight bundled
//! faces uses point matching, scaled or 2x2 components (generator statistics).
//!
//! Files: this one, plus `text_glyph_source_font.rs` (the `read-fonts` reader).

use crate::object_document::object_text_layout_kernels::object_text_layout_kernel::model::FontAdvancePort;
use crate::object_document::object_text_layout_kernels::text_glyph_contours::OutlineCommand;

#[cfg(feature = "font_asset_admission")]
#[path = "text_glyph_source_font.rs"]
pub(crate) mod font;

/// Component nesting limit; the bundled DejaVu faces nest to depth 2.
pub(crate) const MAX_COMPOSITE_DEPTH: usize = 16;

/// One TrueType point (`glyph.points[i]` of opentype.js): font units, y up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DesignPoint {
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) on_curve: bool,
    pub(crate) last_of_contour: bool,
}

/// One command of `glyph.path.commands` in font units (opentype builds only
/// these four from a TrueType glyph).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum DesignCommand {
    Move { x: f64, y: f64 },
    Line { x: f64, y: f64 },
    Quad { x1: f64, y1: f64, x: f64, y: f64 },
    Close,
}

/// A composite component transform, with opentype.js' own field names:
/// `x' = x_scale*x + scale01*y + dx`, `y' = scale10*x + y_scale*y + dy`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ComponentTransform {
    pub(crate) x_scale: f64,
    pub(crate) scale01: f64,
    pub(crate) scale10: f64,
    pub(crate) y_scale: f64,
    pub(crate) dx: f64,
    pub(crate) dy: f64,
}

impl ComponentTransform {
    /// The component of a plain offset record (no scale flags).
    pub(crate) fn offset(dx: f64, dy: f64) -> Self {
        Self {
            x_scale: 1.0,
            scale01: 0.0,
            scale10: 0.0,
            y_scale: 1.0,
            dx,
            dy,
        }
    }
}

/// Why a glyph could not be turned into an outline. Nothing is produced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GlyphSourceError {
    /// The retained bytes are not a readable TrueType face.
    Unreadable,
    /// No Unicode cmap subtable of format 4 or 12 (what opentype.js reads).
    UnsupportedCmap,
    /// `unitsPerEm` is zero or not finite.
    InvalidMetrics,
    /// A glyph index at or beyond the face's glyph count.
    GlyphOutOfRange(u16),
    /// The glyph record is unreadable or its contour table is inconsistent.
    MalformedGlyph(u16),
    /// Points remain after the last contour end.
    UnterminatedContour,
    /// A composite component names a glyph outside the face.
    ComponentOutOfRange(u16),
    /// A composite component is placed by point matching, not by offset.
    PointMatchedComponent(u16),
    /// Component nesting deeper than [`MAX_COMPOSITE_DEPTH`].
    CompositeTooDeep(u16),
    /// The scaled outline holds a NaN or infinite coordinate.
    NonFiniteGeometry,
}

/// A readable face: the L06-A advance port plus the outline of a UTF-16 unit
/// exactly as `font.charToGlyph(ch).getPath(x, y, size).commands` (the
/// notdef outline for an unmapped unit, lone surrogate halves included).
pub(crate) trait GlyphSourcePort: FontAdvancePort {
    /// The same object as an advance port (no trait-object upcast needed).
    fn as_advance(&self) -> &dyn FontAdvancePort;
    fn outline(
        &self,
        utf16_unit: u16,
        x: f64,
        y: f64,
        size: f64,
    ) -> Result<Vec<OutlineCommand>, GlyphSourceError>;
}

/// Opens retained encoded bytes as a face. The parser behind it is the
/// provider's business; this value never copies or owns font bytes.
pub(crate) trait GlyphSourceOpener {
    fn open(
        &self,
        encoded: &std::sync::Arc<Vec<u8>>,
    ) -> Result<Box<dyn GlyphSourcePort>, GlyphSourceError>;
}

fn midpoint(a: &DesignPoint, b: &DesignPoint) -> (f64, f64) {
    (0.5 * (a.x + b.x), 0.5 * (a.y + b.y))
}

fn push_contour(contour: &[DesignPoint], out: &mut Vec<DesignCommand>) {
    let count = contour.len();
    let mut current = contour[count - 1];
    let mut next = contour[0];
    let (start_x, start_y) = if current.on_curve {
        (current.x, current.y)
    } else if next.on_curve {
        (next.x, next.y)
    } else {
        midpoint(&current, &next)
    };
    out.push(DesignCommand::Move {
        x: start_x,
        y: start_y,
    });
    for index in 0..count {
        current = next;
        next = contour[(index + 1) % count];
        if current.on_curve {
            out.push(DesignCommand::Line {
                x: current.x,
                y: current.y,
            });
        } else {
            let (x, y) = if next.on_curve {
                (next.x, next.y)
            } else {
                midpoint(&current, &next)
            };
            out.push(DesignCommand::Quad {
                x1: current.x,
                y1: current.y,
                x,
                y,
            });
        }
    }
    out.push(DesignCommand::Close);
}

/// opentype.js `glyfToPath` over the points of one glyph.
pub(crate) fn design_commands(
    points: &[DesignPoint],
) -> Result<Vec<DesignCommand>, GlyphSourceError> {
    let mut commands = Vec::new();
    let mut start = 0;
    for (index, point) in points.iter().enumerate() {
        if point.last_of_contour {
            push_contour(&points[start..=index], &mut commands);
            start = index + 1;
        }
    }
    if start != points.len() {
        return Err(GlyphSourceError::UnterminatedContour);
    }
    Ok(commands)
}

/// opentype.js `Glyph#getPath(x, y, size)` of design commands.
pub(crate) fn scale_commands(
    commands: &[DesignCommand],
    units_per_em: f64,
    x: f64,
    y: f64,
    size: f64,
) -> Result<Vec<OutlineCommand>, GlyphSourceError> {
    if !(units_per_em.is_finite() && units_per_em > 0.0) {
        return Err(GlyphSourceError::InvalidMetrics);
    }
    let scale = 1.0 / units_per_em * size;
    let px = |cx: f64| x + cx * scale;
    let py = |cy: f64| y + (-cy) * scale;
    let scaled: Vec<OutlineCommand> = commands
        .iter()
        .map(|command| match *command {
            DesignCommand::Move { x: cx, y: cy } => OutlineCommand::Move {
                x: px(cx),
                y: py(cy),
            },
            DesignCommand::Line { x: cx, y: cy } => OutlineCommand::Line {
                x: px(cx),
                y: py(cy),
            },
            DesignCommand::Quad {
                x1,
                y1,
                x: cx,
                y: cy,
            } => OutlineCommand::Quad {
                x1: px(x1),
                y1: py(y1),
                x: px(cx),
                y: py(cy),
            },
            DesignCommand::Close => OutlineCommand::Close,
        })
        .collect();
    let finite = scaled.iter().all(|command| match *command {
        OutlineCommand::Move { x, y } | OutlineCommand::Line { x, y } => {
            x.is_finite() && y.is_finite()
        }
        OutlineCommand::Quad { x1, y1, x, y } => [x1, y1, x, y].iter().all(|v| v.is_finite()),
        OutlineCommand::Cubic { .. } | OutlineCommand::Close => true,
    });
    if finite {
        Ok(scaled)
    } else {
        Err(GlyphSourceError::NonFiniteGeometry)
    }
}

/// opentype.js `Dt`: one component's points under its transform.
pub(crate) fn transform_points(
    points: &[DesignPoint],
    transform: &ComponentTransform,
) -> Vec<DesignPoint> {
    points
        .iter()
        .map(|point| DesignPoint {
            x: transform.x_scale * point.x + transform.scale01 * point.y + transform.dx,
            y: transform.scale10 * point.x + transform.y_scale * point.y + transform.dy,
            on_curve: point.on_curve,
            last_of_contour: point.last_of_contour,
        })
        .collect()
}

/// opentype.js `Mt`: the transformed points of every component, in order.
pub(crate) fn assemble_composite(
    parts: &[(Vec<DesignPoint>, ComponentTransform)],
) -> Vec<DesignPoint> {
    let mut points = Vec::new();
    for (part, transform) in parts {
        points.extend(transform_points(part, transform));
    }
    points
}

#[cfg(test)]
#[path = "../tests/text_glyph_source_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/text_glyph_source_oracle.rs"]
pub(crate) mod oracle;
#[cfg(test)]
#[path = "../tests/text_glyph_source.rs"]
mod tests;
