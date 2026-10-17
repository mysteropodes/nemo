//! Retained-font provider seam of the retained text layout (G38a / G38b, L06-J).
//!
//! Assistant2's G41 provider (`font_asset_admission.rs`, `project_asset_inventory.rs`)
//! admits and retains the ENCODED bytes of a Font asset and states that it has no
//! glyph engine. This file is the consumer side of that contract: it resolves a
//! text block's [`FontRef`] to retained bytes at a pinned snapshot, hands them to
//! a [`GlyphSourceOpener`], and reports every way that can fail as a typed
//! [`FontUnavailable`]. It never reads a file, a network, a Paper item or a
//! fallback face, and it never owns font bytes: a [`FontPin`] only clones the
//! provider's `Arc<Vec<u8>>`.
//!
//! * `FontRef::Asset(id)`: the id must name a record of the snapshot's
//!   `AssetInventory`, of kind Font, with an embedded source that carries the
//!   admitted-font proof (`AssetSource::has_font_admission`, set only by the
//!   provider's own codec) and media type `font/ttf`. A linked font is
//!   `Linked` (the provider's `linked_font_unavailable`), never opened.
//! * `FontRef::Bundled(key)`: the eight `VECTOR_FONTS` faces have no bytes in the
//!   native document; a [`BundledFontBytes`] port may supply them, and without
//!   one the face is `BundledNotProvided`.
//! * Weight resolution is the baseline's: `fontKey.replace(/-(Bold|Regular)$/,
//!   '')` plus the requested weight, used only when that key is a bundled face;
//!   a bundled weight that exists but cannot be provided is an error, not a silent
//!   fall back to the other weight (the baseline always has both).

use crate::object_document::object_text_layout_kernels::object_text_layout_kernel::model::{
    FontAdvancePort, FontCatalogPort,
};
use crate::object_document::object_text_layout_kernels::object_text_layout_kernel::resolve_font_key;
use crate::object_document::object_text_layout_kernels::object_text_layout_record::font::FontRef;
use crate::object_document::object_text_layout_kernels::text_glyph_source::{
    GlyphSourceError, GlyphSourceOpener, GlyphSourcePort,
};
use crate::object_document::{AssetId, AssetInventory, AssetKind};
use std::sync::Arc;

/// The media type of an admitted Font asset (`AssetSource::admitted_font`).
const FONT_MEDIA_TYPE: &str = "font/ttf";

/// Why a font could not be provided. No layout, no fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FontUnavailable {
    /// The snapshot is a bare drawing or a project without an asset catalog.
    NoInventory,
    /// No such asset.
    Missing,
    /// The record exists but is not a Font asset.
    WrongKind,
    /// A linked font: its locator is opaque and is never opened.
    Linked,
    /// Embedded bytes without the provider's admitted-font proof, or another
    /// media type than `font/ttf`.
    NotAdmitted,
    /// A bundled face and no [`BundledFontBytes`] port (or no bytes for it).
    BundledNotProvided,
    /// The retained bytes are not a face the glyph source can read.
    Unreadable(GlyphSourceError),
}

/// What a retained layout keeps of its font: the reference and the exact
/// retained allocation (identity is the `Arc`, never equal bytes).
#[derive(Clone, Debug)]
pub(crate) struct FontPin {
    reference: FontRef,
    allocation: Arc<Vec<u8>>,
}

impl FontPin {
    pub(crate) fn new(reference: FontRef, allocation: Arc<Vec<u8>>) -> Self {
        Self {
            reference,
            allocation,
        }
    }
    pub(crate) fn reference(&self) -> &FontRef {
        &self.reference
    }
    pub(crate) fn allocation(&self) -> &Arc<Vec<u8>> {
        &self.allocation
    }
    pub(crate) fn same_allocation_as(&self, other: &FontPin) -> bool {
        Arc::ptr_eq(&self.allocation, &other.allocation)
    }
}

/// A readable face and the pin of the bytes it was read from.
pub(crate) struct RetainedFont {
    pub(crate) pin: FontPin,
    pub(crate) source: Box<dyn GlyphSourcePort>,
}

/// The retained-font provider as the layout join sees it.
pub(crate) trait RetainedFontPort {
    fn retained_font(&self, font: &FontRef) -> Result<RetainedFont, FontUnavailable>;
}

/// Bytes of the eight bundled faces, when the host has them.
pub(crate) trait BundledFontBytes {
    fn bundled_bytes(&self, key: &str) -> Option<Arc<Vec<u8>>>;
}

/// The provider over one snapshot's asset catalog.
pub(crate) struct InventoryFonts<'a> {
    pub(crate) inventory: Option<&'a AssetInventory>,
    pub(crate) bundled: Option<&'a dyn BundledFontBytes>,
    pub(crate) opener: &'a dyn GlyphSourceOpener,
}

fn asset_allocation(
    inventory: Option<&AssetInventory>,
    id: &AssetId,
) -> Result<Arc<Vec<u8>>, FontUnavailable> {
    let inventory = inventory.ok_or(FontUnavailable::NoInventory)?;
    let record = inventory.get(id).ok_or(FontUnavailable::Missing)?;
    if record.kind() != AssetKind::Font {
        return Err(FontUnavailable::WrongKind);
    }
    let source = record.source();
    let Some(allocation) = source.encoded_allocation() else {
        return Err(FontUnavailable::Linked);
    };
    if !source.has_font_admission() || source.media_type() != Some(FONT_MEDIA_TYPE) {
        return Err(FontUnavailable::NotAdmitted);
    }
    Ok(Arc::clone(allocation))
}

impl RetainedFontPort for InventoryFonts<'_> {
    fn retained_font(&self, font: &FontRef) -> Result<RetainedFont, FontUnavailable> {
        let allocation = match font {
            FontRef::Bundled(bundled) => self
                .bundled
                .and_then(|port| port.bundled_bytes(bundled.key()))
                .ok_or(FontUnavailable::BundledNotProvided)?,
            FontRef::Asset(id) => asset_allocation(self.inventory, id)?,
        };
        let source = self
            .opener
            .open(&allocation)
            .map_err(FontUnavailable::Unreadable)?;
        Ok(RetainedFont {
            pin: FontPin::new(font.clone(), allocation),
            source,
        })
    }
}

/// Answers `knows_key` for every key, so `resolve_font_key` can be asked which
/// weight key the baseline would look for without a second copy of its rule.
struct AnyKey;

impl FontCatalogPort for AnyKey {
    fn knows_key(&self, _key: &str) -> bool {
        true
    }
    fn face(&self, _key: &str) -> Option<&dyn FontAdvancePort> {
        None
    }
}

/// The faces one layout may name, loaded up front: the weight-resolved bundled
/// key (when it is a bundled face) and the key as given. Implements the L06-A
/// catalog, so `layout_text` resolves the face exactly as the baseline does.
pub(crate) struct FaceSet {
    faces: Vec<(String, RetainedFont)>,
}

impl FaceSet {
    pub(crate) fn load(
        port: &dyn RetainedFontPort,
        font: &FontRef,
        bold: bool,
    ) -> Result<Self, FontUnavailable> {
        let given = font.layout_key().to_string();
        let mut faces = Vec::new();
        if matches!(font, FontRef::Bundled(_)) {
            let weighted = resolve_font_key(&given, bold, &AnyKey);
            if weighted != given {
                if let Ok(reference) = FontRef::from_legacy_key(&weighted) {
                    faces.push((weighted, port.retained_font(&reference)?));
                }
            }
        }
        faces.push((given, port.retained_font(font)?));
        Ok(Self { faces })
    }

    pub(crate) fn get(&self, key: &str) -> Option<&RetainedFont> {
        self.faces
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, face)| face)
    }

    pub(crate) fn len(&self) -> usize {
        self.faces.len()
    }
}

impl FontCatalogPort for FaceSet {
    fn knows_key(&self, key: &str) -> bool {
        self.get(key).is_some()
    }
    fn face(&self, key: &str) -> Option<&dyn FontAdvancePort> {
        self.get(key).map(|font| font.source.as_advance())
    }
}

#[cfg(test)]
#[path = "../tests/object_text_layout_retained_fonts.rs"]
mod tests;
