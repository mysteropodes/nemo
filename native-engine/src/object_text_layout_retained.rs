//! Immutable retained text layout (G38a / G38b, L06-J): one text block, laid
//! out by the L06-A kernel, outlined by the retained font through the glyph
//! source and turned into contours by the L06-B kernel, held as a value.
//!
//! `RetainedTextLayout::build(snapshot_id, holder_id, record, fonts)` composes
//! what `buildVectorTextGroup` does per block, without a Paper item, a random
//! group id or a document write:
//!
//! 1. the record's typography becomes the kernel request (`layout_request`);
//! 2. the retained-font port supplies the faces ([`FaceSet`]) or a typed
//!    [`FontUnavailable`]: there is no fallback face and no JS/Paper writer;
//! 3. `layout_text` places every glyph (`origin_x`, `baseline_y`, indices);
//! 4. each placed glyph asks its face for `getPath(origin_x, baseline_y, size)`
//!    and `glyph_paths` merges holes (the baseline slit at index (0, 0)) and
//!    shears italic text by the layout's matrix.
//!
//! The result is keyed by `(snapshot id, holder id, requested font reference,
//! the retained allocation of the face used)`. It owns derived numbers only and
//! one clone of the retained `Arc<Vec<u8>>` (the pin that keeps the exact
//! allocation named by the key alive); it holds no parser, face or guest
//! handle. Replacing the font asset, or editing the record, is a NEW `build`
//! and a new value; every clone of an older value stays valid, and its pin is
//! released when its last clone drops.
//!
//! Glyph identity is the kernel's: `(holder id, ordinal)` with the ordinal the
//! baseline `charIndex`, so a one-character edit moves only the ordinals at or
//! after it (`text_glyph_pin::GlyphPin` names and resolves them).
//!
//! `holder_id` is the caller's canonical stable identity of the text holder (a
//! rendering of the F01 holder target, at most 255 bytes, no NUL); deriving it
//! from the document record family belongs to the document join. Not here: the
//! persistent record family, `ClosedCubicPath` construction and its per-glyph
//! refusal policy (`PathFlag` is carried through untouched), decorations as
//! geometry, the scene/export consumption and the retention budget.

#[path = "object_text_layout_retained_fonts.rs"]
pub(crate) mod fonts;

use crate::object_document::object_text_layout_kernels::object_text_layout_kernel::model::{
    valid_id, LaidOutText, TextLayoutError,
};
use crate::object_document::object_text_layout_kernels::object_text_layout_kernel::{
    layout_text, ITALIC_SHEAR,
};
use crate::object_document::object_text_layout_kernels::object_text_layout_record::font::FontRef;
use crate::object_document::object_text_layout_kernels::object_text_layout_record::pin::{
    pins_of, GlyphPin,
};
use crate::object_document::object_text_layout_kernels::object_text_layout_record::TextBlockRecord;
use crate::object_document::object_text_layout_kernels::text_glyph_contours::{
    glyph_paths, GlyphContourError, GlyphPath, ItalicShear,
};
use crate::object_document::object_text_layout_kernels::text_glyph_source::{
    GlyphSourceError, GlyphSourceOpener,
};
use crate::revision::ObjectSnapshot;
use fonts::{
    BundledFontBytes, FaceSet, FontPin, FontUnavailable, InventoryFonts, RetainedFontPort,
};
use std::sync::Arc;

/// Every refusal happens before a value exists.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum RetainedLayoutError {
    /// The snapshot id is blank, over 255 bytes or contains NUL.
    InvalidSnapshotId,
    FontUnavailable(FontUnavailable),
    Layout(TextLayoutError),
    /// The face could not outline the glyph with this ordinal.
    Glyph {
        ordinal: usize,
        error: GlyphSourceError,
    },
    /// The contour kernel refused the outline of the glyph with this ordinal.
    Contour {
        ordinal: usize,
        error: GlyphContourError,
    },
}

/// One non-whitespace glyph of the block with its finished paths. A glyph whose
/// outline is empty (a face without ink for it) keeps its entry with no paths,
/// exactly as the baseline counts it in `charIndex` and builds nothing.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RetainedGlyph {
    pub(crate) ordinal: usize,
    pub(crate) unit: u16,
    pub(crate) line_index: usize,
    pub(crate) word_index: usize,
    pub(crate) origin_x: f64,
    pub(crate) baseline_y: f64,
    pub(crate) paths: Vec<GlyphPath>,
}

/// The identity a retained layout is valid for.
#[derive(Clone, Debug)]
pub(crate) struct RetainedKey {
    snapshot_id: String,
    holder_id: String,
    requested_font: FontRef,
    face: FontPin,
}

impl RetainedKey {
    pub(crate) fn snapshot_id(&self) -> &str {
        &self.snapshot_id
    }
    pub(crate) fn holder_id(&self) -> &str {
        &self.holder_id
    }
    /// The font the record asked for (before weight resolution).
    pub(crate) fn requested_font(&self) -> &FontRef {
        &self.requested_font
    }
    /// The face actually used and its retained allocation.
    pub(crate) fn face(&self) -> &FontPin {
        &self.face
    }
    /// Same snapshot, holder, requested font, resolved face reference and the
    /// very same retained allocation (identity, not equal bytes).
    pub(crate) fn matches(&self, other: &RetainedKey) -> bool {
        self.snapshot_id == other.snapshot_id
            && self.holder_id == other.holder_id
            && self.requested_font == other.requested_font
            && self.face.reference() == other.face.reference()
            && self.face.same_allocation_as(&other.face)
    }
}

#[derive(Debug)]
struct RetainedValue {
    key: RetainedKey,
    layout: LaidOutText,
    glyphs: Vec<RetainedGlyph>,
}

/// Aliases share one value; the value never changes after `build`.
#[derive(Clone, Debug)]
pub(crate) struct RetainedTextLayout(Arc<RetainedValue>);

impl RetainedTextLayout {
    pub(crate) fn build(
        snapshot_id: &str,
        holder_id: &str,
        record: &TextBlockRecord,
        fonts: &dyn RetainedFontPort,
    ) -> Result<Self, RetainedLayoutError> {
        if !valid_id(snapshot_id) {
            return Err(RetainedLayoutError::InvalidSnapshotId);
        }
        let request = record.layout_request(holder_id);
        let faces = FaceSet::load(fonts, record.font(), request.style.bold)
            .map_err(RetainedLayoutError::FontUnavailable)?;
        let layout = layout_text(&request, Some(&faces)).map_err(RetainedLayoutError::Layout)?;
        let Some(face) = faces.get(&layout.resolved_font_key) else {
            return Err(RetainedLayoutError::Layout(
                TextLayoutError::FontFaceUnavailable(layout.resolved_font_key.clone()),
            ));
        };
        let mut glyphs = Vec::new();
        for line in &layout.lines {
            for placed in &line.glyphs {
                let ordinal = placed.ordinal;
                let commands = face
                    .source
                    .outline(placed.unit, placed.origin_x, placed.baseline_y, placed.size)
                    .map_err(|error| RetainedLayoutError::Glyph { ordinal, error })?;
                let shear = match placed.shear {
                    Some(_) => Some(
                        ItalicShear::new(ITALIC_SHEAR, placed.baseline_y)
                            .map_err(|error| RetainedLayoutError::Contour { ordinal, error })?,
                    ),
                    None => None,
                };
                let paths = glyph_paths(&commands, shear.as_ref())
                    .map_err(|error| RetainedLayoutError::Contour { ordinal, error })?;
                glyphs.push(RetainedGlyph {
                    ordinal,
                    unit: placed.unit,
                    line_index: placed.line_index,
                    word_index: placed.word_index,
                    origin_x: placed.origin_x,
                    baseline_y: placed.baseline_y,
                    paths,
                });
            }
        }
        let key = RetainedKey {
            snapshot_id: snapshot_id.to_string(),
            holder_id: holder_id.to_string(),
            requested_font: record.font().clone(),
            face: face.pin.clone(),
        };
        Ok(Self(Arc::new(RetainedValue {
            key,
            layout,
            glyphs,
        })))
    }

    /// `build` over one pinned snapshot: its id, and its asset catalog for the
    /// font assets (a bare drawing has none).
    pub(crate) fn build_for_snapshot(
        snapshot: &ObjectSnapshot,
        holder_id: &str,
        record: &TextBlockRecord,
        bundled: Option<&dyn BundledFontBytes>,
        opener: &dyn GlyphSourceOpener,
    ) -> Result<Self, RetainedLayoutError> {
        let inventory = snapshot
            .project()
            .and_then(|project| project.asset_inventory());
        let fonts = InventoryFonts {
            inventory,
            bundled,
            opener,
        };
        Self::build(snapshot.snapshot_id(), holder_id, record, &fonts)
    }

    pub(crate) fn key(&self) -> &RetainedKey {
        &self.0.key
    }

    /// Lines, placed glyphs and decorations, bounds (`width`, `height`) and the
    /// overflow flag, exactly as the L06-A kernel returned them.
    pub(crate) fn layout(&self) -> &LaidOutText {
        &self.0.layout
    }

    /// The glyphs in layout (paint) order.
    pub(crate) fn glyphs(&self) -> &[RetainedGlyph] {
        &self.0.glyphs
    }

    pub(crate) fn glyph(&self, ordinal: usize) -> Option<&RetainedGlyph> {
        self.0.glyphs.iter().find(|glyph| glyph.ordinal == ordinal)
    }

    /// One identity pin per glyph, in layout order.
    pub(crate) fn pins(&self) -> Vec<GlyphPin> {
        pins_of(&self.0.layout)
    }

    /// Number of live handles on this value (clones of this layout).
    pub(crate) fn handle_count(&self) -> usize {
        Arc::strong_count(&self.0)
    }

    pub(crate) fn same_value_as(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

#[cfg(test)]
#[path = "../tests/object_text_layout_retained_controls.rs"]
mod controls;
#[cfg(all(test, feature = "font_asset_wire"))]
#[path = "../tests/object_text_layout_retained_inventory.rs"]
mod inventory;
#[cfg(test)]
#[path = "../tests/object_text_layout_retained.rs"]
pub(crate) mod tests;
