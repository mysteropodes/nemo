//! Local retained-font adapter. No registry, fallback face, parser cache or document writer.
use crate::evaluation::object_text_layout_kernels::{
    object_text_layout_kernel::resolve_font_key,
    object_text_layout_record::{font::FontRef, TextBlockRecord},
    object_text_layout_retained::fonts::{BundledFontBytes, FaceSet, FontPin, InventoryFonts},
    text_glyph_contours::OutlineCommand,
    text_glyph_source::font::TrueTypeOpener,
};
use crate::object_render_scene::text_glyph_scene_kernels::text_glyph_scene::GlyphOutlinePort;
use crate::object_render_scene::ObjectSceneError;
use crate::object_snapshot::{
    object_text_font_resources::{font_version, BundledFontAllocation},
    ObjectSnapshot,
};
use std::{collections::BTreeMap, sync::Arc};

#[path = "object_text_scene_bundled.rs"]
mod bundled;

// Bundled storage is charged and retained by the existing read/resource layer.
#[derive(Default)]
pub(super) struct BundledInputs(BTreeMap<String, BundledFontAllocation>);
impl BundledFontBytes for BundledInputs {
    fn bundled_bytes(&self, key: &str) -> Option<Arc<Vec<u8>>> {
        self.0.get(key).map(|face| Arc::clone(face.allocation()))
    }
}
impl BundledInputs {
    /// Construction-local dedup only. Only requested/resolved real bundled files
    /// enter the existing shared CPU payload quota; no font substitution or I/O.
    pub(super) fn include(
        &mut self,
        record: &TextBlockRecord,
        index: usize,
    ) -> Result<(), ObjectSceneError> {
        if !matches!(record.font(), FontRef::Bundled(_)) {
            return Ok(());
        }
        let given = record.font().layout_key();
        // The same layout resolver and actual bundled table decide the weight.
        let resolved = resolve_font_key(given, record.style().bold, &bundled::Keys);
        for key in [given, resolved.as_str()] {
            if self.0.contains_key(key) {
                continue;
            }
            let bytes = bundled::bytes(key)
                .ok_or_else(|| unavailable(index, "requested bundled font bytes unavailable"))?;
            let allocation = BundledFontAllocation::retain(bytes)
                .map_err(|_| unavailable(index, "bundled font resource budget unavailable"))?;
            self.0.insert(key.to_owned(), allocation);
        }
        Ok(())
    }
}

/// Retains exact encoded identity AND its original quota owner. No face/parser
/// crosses into the scene; the glyph producer returns detached contours.
#[derive(Debug)]
pub(super) struct FontResource {
    pub(super) pin: FontPin,
    _snapshot: ObjectSnapshot,
    _bundled: Option<BundledFontAllocation>,
}
impl FontResource {
    pub(super) fn same_allocation_as(&self, other: &Self) -> bool {
        self.pin.same_allocation_as(&other.pin)
    }
    pub(super) fn bytes(&self) -> u64 {
        self.pin.allocation().len() as u64
    }
    pub(super) fn version(&self) -> String {
        font_version(self.pin.allocation())
    }
}

pub(super) struct PreparedFont {
    pub(super) faces: FaceSet,
    pub(super) key: String,
    pub(super) resource: Arc<FontResource>,
}
pub(super) fn prepare(
    snapshot: &ObjectSnapshot,
    record: &TextBlockRecord,
    bundled: &BundledInputs,
    index: usize,
) -> Result<PreparedFont, ObjectSceneError> {
    let provider = InventoryFonts {
        inventory: snapshot
            .project()
            .and_then(|project| project.asset_inventory()),
        bundled: Some(bundled),
        opener: &TrueTypeOpener,
    };
    let style = record.style();
    let faces = FaceSet::load(&provider, record.font(), style.bold).map_err(|_| {
        unavailable(
            index,
            "requested retained font is unavailable or unreadable",
        )
    })?;
    let key = resolve_font_key(record.font().layout_key(), style.bold, &faces);
    let face = faces
        .get(&key)
        .ok_or_else(|| unavailable(index, "resolved retained font is unavailable"))?;
    let resource = Arc::new(FontResource {
        pin: face.pin.clone(),
        _snapshot: snapshot.clone(),
        _bundled: bundled.0.get(&key).cloned(),
    });
    Ok(PreparedFont {
        faces,
        key,
        resource,
    })
}
pub(super) struct Outlines<'a>(pub(super) &'a FaceSet);
impl GlyphOutlinePort for Outlines<'_> {
    fn outline(
        &self,
        key: &str,
        unit: u16,
        x: f64,
        y: f64,
        size: f64,
    ) -> Option<Vec<OutlineCommand>> {
        self.0.get(key)?.source.outline(unit, x, y, size).ok()
    }
}
fn unavailable(index: usize, reason: &'static str) -> ObjectSceneError {
    ObjectSceneError::UnsupportedGeometry {
        source_object_index: index,
        reason,
    }
}
