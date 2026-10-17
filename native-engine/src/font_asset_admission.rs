//! G41 provider: borrowed existing parser, retained encoded font bytes.
//! No glyph/layout/render engine, network, state owner or resource budget.
use read_fonts::{FontRef, TableProvider};
use std::sync::Arc;
#[cfg(feature = "font_asset_wire")]
#[path = "font_asset_project.rs"]
mod project_bridge;
#[cfg(feature = "font_asset_wire")]
pub(crate) use project_bridge::admitted_font_source;

// Ceiling only: the caller still reserves actual CAPACITY in the existing
// encoded ledger. This validator neither creates nor enforces a second quota.
const MAX_ENCODED: usize = 128 * 1024 * 1024;
const MAX_TABLES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FontAssetError {
    Limit,
    Unsupported,
    Invalid,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FontAssetLimits {
    encoded: usize,
    tables: usize,
}
impl FontAssetLimits {
    pub(crate) fn encoded_byte_limit(self) -> usize {
        self.encoded
    }
    pub(crate) fn new(encoded: usize, tables: usize) -> Result<Self, FontAssetError> {
        if encoded == 0 || encoded > MAX_ENCODED || tables == 0 || tables > MAX_TABLES {
            return Err(FontAssetError::Limit);
        }
        Ok(Self { encoded, tables })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct FontAssetMetadata {
    pub(crate) units_per_em: u16,
    pub(crate) glyph_count: u16,
}

/// Construction proof and immutable allocation only, never a document owner.
/// No public/raw DTO can forge this type's fields. Floor types never import it;
/// the later codec-to-document bridge owns the one-way retention conversion.
#[derive(Debug, Clone)]
pub(crate) struct AdmittedFont {
    encoded: Arc<Vec<u8>>,
    metadata: FontAssetMetadata,
}
impl AdmittedFont {
    pub(crate) fn encoded_allocation(&self) -> &Arc<Vec<u8>> {
        &self.encoded
    }
    pub(crate) fn metadata(&self) -> FontAssetMetadata {
        self.metadata
    }
    pub(crate) fn media_type(&self) -> &'static str {
        "font/ttf"
    }
    pub(crate) fn into_encoded(self) -> Arc<Vec<u8>> {
        self.encoded
    }
}

/// Byte admission only. Required tables/bounds are checked by the retained
/// fontations parser; glyph interpretation remains the existing text provider.
/// The characterized eight bundled files are all SFNT1.0 TrueType, matching
/// the retained Google-font TTF transport. Other containers explicitly refuse;
/// no CFF/collection/WOFF/glyph/layout support is claimed by this provider.
pub(crate) fn validate_font(
    bytes: &[u8],
    limits: FontAssetLimits,
) -> Result<FontAssetMetadata, FontAssetError> {
    if bytes.is_empty() || bytes.len() > limits.encoded {
        return Err(FontAssetError::Limit);
    }
    if !bytes.starts_with(b"\x00\x01\x00\x00") {
        return Err(FontAssetError::Unsupported);
    }
    let font = FontRef::new(bytes).map_err(|_| FontAssetError::Invalid)?;
    let tables = font.table_directory().table_records();
    if tables.is_empty() || tables.len() > limits.tables {
        return Err(FontAssetError::Limit);
    }
    for (index, table) in tables.iter().enumerate() {
        // Duplicate tags are ambiguous. Unsorted valid directories are retained
        // as supported by FontRef; no normalization or table rewriting occurs.
        if tables[..index]
            .iter()
            .any(|prior| prior.tag() == table.tag())
            || font.table_data(table.tag()).is_none()
        {
            return Err(FontAssetError::Invalid);
        }
    }
    let head = font.head().map_err(|_| FontAssetError::Invalid)?;
    let maxp = font.maxp().map_err(|_| FontAssetError::Invalid)?;
    let hhea = font.hhea().map_err(|_| FontAssetError::Invalid)?;
    let hmtx = font.hmtx().map_err(|_| FontAssetError::Invalid)?;
    font.cmap().map_err(|_| FontAssetError::Invalid)?;
    font.name().map_err(|_| FontAssetError::Invalid)?;
    let units_per_em = head.units_per_em();
    let glyph_count = maxp.num_glyphs();
    let metrics = hhea.number_of_h_metrics();
    if head.magic_number() != 0x5f0f3cf5
        || !(16..=16384).contains(&units_per_em)
        || glyph_count == 0
        || metrics == 0
        || metrics > glyph_count
        || hmtx.h_metrics().len() != usize::from(metrics)
        || hmtx.left_side_bearings().len() < usize::from(glyph_count - metrics)
    {
        return Err(FontAssetError::Invalid);
    }
    let location_format = head.index_to_loc_format();
    if !matches!(location_format, 0 | 1) {
        return Err(FontAssetError::Invalid);
    }
    let locations = font
        .loca(Some(location_format == 1))
        .map_err(|_| FontAssetError::Invalid)?;
    font.glyf().map_err(|_| FontAssetError::Invalid)?;
    let glyph_data = font
        .table_data(read_fonts::types::Tag::new(b"glyf"))
        .ok_or(FontAssetError::Invalid)?;
    if locations.len() < usize::from(glyph_count)
        || !locations.all_offsets_are_ascending()
        || locations
            .get_raw(usize::from(glyph_count))
            .is_none_or(|last| last as usize > glyph_data.len())
    {
        return Err(FontAssetError::Invalid);
    }
    Ok(FontAssetMetadata {
        units_per_em,
        glyph_count,
    })
}

/// PRECONDITION: the same existing native owner reserved this allocation's
/// CAPACITY before copying its bytes. Validation borrows; nothing is copied.
/// This factory cannot publish, issue IDs, relax the current PNG guard, or call
/// a document-floor factory. The paired wire/ledger bridge is a later window.
pub(crate) fn admit_reserved_font(
    bytes: Arc<Vec<u8>>,
    limits: FontAssetLimits,
) -> Result<AdmittedFont, FontAssetError> {
    if bytes.capacity() > limits.encoded {
        return Err(FontAssetError::Limit);
    }
    let metadata = validate_font(&bytes, limits)?;
    Ok(AdmittedFont {
        encoded: bytes,
        metadata,
    })
}

#[cfg(test)]
#[path = "../tests/font_asset_admission.rs"]
mod tests;
