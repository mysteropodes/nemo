//! The `read-fonts` reader behind [`GlyphSourcePort`] (G38a / G38b, L06-J):
//! the bytes of an admitted font, read as opentype.js reads them.
//!
//! Compiled only with the existing `font_asset_admission` feature, i.e. over the
//! already pinned `read-fonts =0.39.2` that Assistant2's provider validates
//! with. It adds no dependency, state, cache, budget or font bytes of its own:
//! [`TrueTypeGlyphSource`] holds the retained `Arc<Vec<u8>>` and reparses the
//! table directory per call, so a face is a few words and no parsed handle
//! outlives the call that needed it.
//!
//! opentype.js behavior followed on purpose:
//! * `charToGlyph(ch)` looks `ch.codePointAt(0)` up in ONE cmap subtable, the
//!   LAST encoding record that is (platform 3, encoding 0/1/10) or (platform 0,
//!   encoding 0..=4) (`ue.parse`: the loop runs from the end of the record
//!   array). A UTF-16 unit is looked up as the code point of that unit, so a
//!   lone surrogate half is an ordinary (unmapped) code point. An unmapped
//!   unit, or a glyph index at or past the glyph count, is glyph 0 (`.notdef`).
//! * `advanceWidth` is the `hmtx` advance, the last long metric for a glyph past
//!   `numberOfHMetrics`; variable faces (Manrope, Inter) are read at the default
//!   master because `gvar`/`HVAR` are never consulted.
//! * A simple glyph is its points with `lastPointOfContour` from the end table;
//!   a composite glyph is the concatenation of its components' points, each under
//!   `Dt` (offset components only, see the parent file for the refusals).
//!   `read-fonts` names the 2x2 members `xx, yx, xy, yy`; opentype.js reads the
//!   same four in file order as `xScale, scale01, scale10, yScale` and applies
//!   `scale01` to y in the x row, which is `yx`, so that is the mapping used.

use crate::object_document::object_text_layout_kernels::object_text_layout_kernel::model::FontAdvancePort;
use crate::object_document::object_text_layout_kernels::text_glyph_contours::OutlineCommand;
use crate::object_document::object_text_layout_kernels::text_glyph_source::{
    design_commands, scale_commands, transform_points, ComponentTransform, DesignPoint,
    GlyphSourceError, GlyphSourceOpener, GlyphSourcePort, MAX_COMPOSITE_DEPTH,
};
use read_fonts::tables::cmap::{Cmap, CmapSubtable, PlatformId};
use read_fonts::tables::glyf::{Anchor, Glyph, SimpleGlyph};
use read_fonts::types::GlyphId;
use read_fonts::{FontRef, TableProvider};
use std::sync::Arc;

/// F2Dot14 raw bits to the double opentype.js computes (`parseShort() / 16384`).
fn f2dot14(raw: i16) -> f64 {
    f64::from(raw) / 16384.0
}

/// ue.parse: the last matching record, and only formats 4 and 12 are readable.
fn select_cmap_record(cmap: &Cmap<'_>) -> Result<usize, GlyphSourceError> {
    let records = cmap.encoding_records();
    for index in (0..records.len()).rev() {
        let record = &records[index];
        let windows = record.platform_id() == PlatformId::Windows
            && matches!(record.encoding_id(), 0 | 1 | 10);
        let unicode = record.platform_id() == PlatformId::Unicode && record.encoding_id() <= 4;
        if windows || unicode {
            return match record.subtable(cmap.offset_data()) {
                Ok(CmapSubtable::Format4(_)) | Ok(CmapSubtable::Format12(_)) => Ok(index),
                _ => Err(GlyphSourceError::UnsupportedCmap),
            };
        }
    }
    Err(GlyphSourceError::UnsupportedCmap)
}

fn simple_points(gid: u16, glyph: &SimpleGlyph<'_>) -> Result<Vec<DesignPoint>, GlyphSourceError> {
    let ends: Vec<usize> = glyph
        .end_pts_of_contours()
        .iter()
        .map(|end| usize::from(end.get()))
        .collect();
    if ends.windows(2).any(|pair| pair[1] <= pair[0]) {
        return Err(GlyphSourceError::MalformedGlyph(gid));
    }
    let raw: Vec<_> = glyph.points().collect();
    if ends.last().map_or(0, |last| last + 1) != raw.len() {
        return Err(GlyphSourceError::MalformedGlyph(gid));
    }
    Ok(raw
        .iter()
        .enumerate()
        .map(|(index, point)| DesignPoint {
            x: f64::from(point.x),
            y: f64::from(point.y),
            on_curve: point.on_curve,
            last_of_contour: ends.binary_search(&index).is_ok(),
        })
        .collect())
}

/// A face over retained encoded bytes. Identity is the allocation: two faces
/// opened from one `Arc` read the same immutable bytes.
#[derive(Debug, Clone)]
pub(crate) struct TrueTypeGlyphSource {
    bytes: Arc<Vec<u8>>,
    units_per_em: u16,
    glyph_count: u16,
    cmap_record: usize,
}

impl TrueTypeGlyphSource {
    /// Reads the head, maxp, hmtx, loca, glyf and cmap tables once, so a face
    /// that opens can answer every later call from the same bytes.
    pub(crate) fn open(bytes: Arc<Vec<u8>>) -> Result<Self, GlyphSourceError> {
        let font = FontRef::new(bytes.as_slice()).map_err(|_| GlyphSourceError::Unreadable)?;
        let units_per_em = font
            .head()
            .map_err(|_| GlyphSourceError::Unreadable)?
            .units_per_em();
        if units_per_em == 0 {
            return Err(GlyphSourceError::InvalidMetrics);
        }
        let glyph_count = font
            .maxp()
            .map_err(|_| GlyphSourceError::Unreadable)?
            .num_glyphs();
        font.hmtx().map_err(|_| GlyphSourceError::Unreadable)?;
        font.loca(None).map_err(|_| GlyphSourceError::Unreadable)?;
        font.glyf().map_err(|_| GlyphSourceError::Unreadable)?;
        let cmap = font.cmap().map_err(|_| GlyphSourceError::Unreadable)?;
        let cmap_record = select_cmap_record(&cmap)?;
        Ok(Self {
            bytes,
            units_per_em,
            glyph_count,
            cmap_record,
        })
    }

    pub(crate) fn encoded(&self) -> &Arc<Vec<u8>> {
        &self.bytes
    }

    pub(crate) fn glyph_count(&self) -> u16 {
        self.glyph_count
    }

    fn font(&self) -> Result<FontRef<'_>, GlyphSourceError> {
        FontRef::new(self.bytes.as_slice()).map_err(|_| GlyphSourceError::Unreadable)
    }

    /// `charToGlyph(ch).index` for a UTF-16 unit; 0 (`.notdef`) when unmapped.
    pub(crate) fn glyph_for_unit(&self, unit: u16) -> u16 {
        let Ok(font) = self.font() else { return 0 };
        let Ok(cmap) = font.cmap() else { return 0 };
        let Some(record) = cmap.encoding_records().get(self.cmap_record) else {
            return 0;
        };
        let Ok(subtable) = record.subtable(cmap.offset_data()) else {
            return 0;
        };
        match subtable.map_codepoint(u32::from(unit)) {
            Some(gid) if gid.to_u32() < u32::from(self.glyph_count) => gid.to_u32() as u16,
            _ => 0,
        }
    }

    /// `glyph.points` of opentype.js: the glyph's points in font units.
    pub(crate) fn design_points(&self, gid: u16) -> Result<Vec<DesignPoint>, GlyphSourceError> {
        if gid >= self.glyph_count {
            return Err(GlyphSourceError::GlyphOutOfRange(gid));
        }
        self.points_at(gid, 0)
    }

    fn points_at(&self, gid: u16, depth: usize) -> Result<Vec<DesignPoint>, GlyphSourceError> {
        if depth > MAX_COMPOSITE_DEPTH {
            return Err(GlyphSourceError::CompositeTooDeep(gid));
        }
        let font = self.font()?;
        let loca = font.loca(None).map_err(|_| GlyphSourceError::Unreadable)?;
        let glyf = font.glyf().map_err(|_| GlyphSourceError::Unreadable)?;
        let glyph = loca
            .get_glyf(GlyphId::new(u32::from(gid)), &glyf)
            .map_err(|_| GlyphSourceError::MalformedGlyph(gid))?;
        match glyph {
            None => Ok(Vec::new()),
            Some(Glyph::Simple(simple)) => simple_points(gid, &simple),
            Some(Glyph::Composite(composite)) => {
                let mut points = Vec::new();
                for component in composite.components() {
                    let (dx, dy) = match component.anchor {
                        Anchor::Offset { x, y } => (f64::from(x), f64::from(y)),
                        Anchor::Point { .. } => {
                            return Err(GlyphSourceError::PointMatchedComponent(gid))
                        }
                    };
                    let child = component.glyph.to_u16();
                    if child >= self.glyph_count {
                        return Err(GlyphSourceError::ComponentOutOfRange(gid));
                    }
                    let transform = ComponentTransform {
                        x_scale: f2dot14(component.transform.xx.to_bits()),
                        scale01: f2dot14(component.transform.yx.to_bits()),
                        scale10: f2dot14(component.transform.xy.to_bits()),
                        y_scale: f2dot14(component.transform.yy.to_bits()),
                        dx,
                        dy,
                    };
                    let part = self.points_at(child, depth + 1)?;
                    points.extend(transform_points(&part, &transform));
                }
                Ok(points)
            }
        }
    }

    /// `font.glyphs.get(gid).getPath(x, y, size).commands`.
    pub(crate) fn glyph_commands(
        &self,
        gid: u16,
        x: f64,
        y: f64,
        size: f64,
    ) -> Result<Vec<OutlineCommand>, GlyphSourceError> {
        let design = design_commands(&self.design_points(gid)?)?;
        scale_commands(&design, f64::from(self.units_per_em), x, y, size)
    }
}

impl FontAdvancePort for TrueTypeGlyphSource {
    fn units_per_em(&self) -> f64 {
        f64::from(self.units_per_em)
    }

    fn advance_units(&self, utf16_unit: u16) -> Option<f64> {
        let gid = self.glyph_for_unit(utf16_unit);
        let font = self.font().ok()?;
        let hmtx = font.hmtx().ok()?;
        hmtx.advance(GlyphId::new(u32::from(gid))).map(f64::from)
    }
}

impl GlyphSourcePort for TrueTypeGlyphSource {
    fn as_advance(&self) -> &dyn FontAdvancePort {
        self
    }

    fn outline(
        &self,
        utf16_unit: u16,
        x: f64,
        y: f64,
        size: f64,
    ) -> Result<Vec<OutlineCommand>, GlyphSourceError> {
        self.glyph_commands(self.glyph_for_unit(utf16_unit), x, y, size)
    }
}

/// The opener the join installs over retained font bytes.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TrueTypeOpener;

impl GlyphSourceOpener for TrueTypeOpener {
    fn open(&self, encoded: &Arc<Vec<u8>>) -> Result<Box<dyn GlyphSourcePort>, GlyphSourceError> {
        let source = TrueTypeGlyphSource::open(Arc::clone(encoded))?;
        Ok(Box::new(source))
    }
}

#[cfg(test)]
#[path = "../tests/text_glyph_source_font_cases.rs"]
mod cases;
#[cfg(test)]
#[path = "../tests/text_glyph_source_font.rs"]
mod tests;
