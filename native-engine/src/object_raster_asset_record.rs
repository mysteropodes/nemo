//! Document members of the lane-6 items (G01 / G38a, L06-L): raster items and text blocks as part of the ONE
//! `ObjectDocument`, next to `objects` and never in a second document.
//!
//! Wire (document member `items`, absent when the document holds none; an empty container is never written):
//! `{"raster":[{"target":{..},"item":{..}}],"text":[{"target":{..},"block":{..}}]}` -- each list absent or non-empty.
//! `target` is the stable native identity (`ObjectTarget`: context, authored frame scope, layer, stroke id) shared by
//! every record kind; `item` is the strict L06-C `RasterItem`; `block` is the strict L06-F `TextBlockRecord`. Both
//! strict records decode from the original bytes (unknown, duplicate and `null` members refused), so a document that
//! holds an item is exactly as strict as the item's own codec.
//!
//! Not a record of `objects`: those are path records every existing consumer (scene, export, frame edit, order plan)
//! reads as geometry. Items are a separate, additive member, invisible to those readers until each one joins.
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::geometry::RasterItemError;
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::RasterItem;
use crate::object_document::object_text_layout_kernels::object_text_layout_record::font::FontRef;
use crate::object_document::object_text_layout_kernels::object_text_layout_record::TextBlockRecord;
use crate::object_document::{
    object_deserialize, AssetId, AssetInventory, AssetKind, ObjectDocument, ObjectTarget, Presence,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RasterEntry {
    pub(crate) target: ObjectTarget,
    pub(crate) item: RasterItem,
}
object_deserialize!(RasterEntry {
    target: ObjectTarget,
    item: RasterItem
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TextEntry {
    pub(crate) target: ObjectTarget,
    pub(crate) block: TextBlockRecord,
}
object_deserialize!(TextEntry {
    target: ObjectTarget,
    block: TextBlockRecord
});

/// The `items` document member. Both lists absent is not a value: the container itself is then absent.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ItemRecords {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) raster: Presence<Vec<RasterEntry>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) text: Presence<Vec<TextEntry>>,
}
object_deserialize!(ItemRecords {
    #[serde(default)] raster: Presence<Vec<RasterEntry>>,
    #[serde(default)] text: Presence<Vec<TextEntry>>
});

fn present_if_any<T>(entries: Vec<T>) -> Presence<Vec<T>> {
    if entries.is_empty() {
        Presence::Missing
    } else {
        Presence::Present(entries)
    }
}

impl ItemRecords {
    pub(crate) fn raster_entries(&self) -> &[RasterEntry] {
        self.raster.value().map_or(&[], Vec::as_slice)
    }
    pub(crate) fn text_entries(&self) -> &[TextEntry] {
        self.text.value().map_or(&[], Vec::as_slice)
    }
    /// Every item target, raster first, in stored order.
    pub(crate) fn targets(&self) -> impl Iterator<Item = &ObjectTarget> {
        self.raster_entries()
            .iter()
            .map(|entry| &entry.target)
            .chain(self.text_entries().iter().map(|entry| &entry.target))
    }
    /// Whether any raster item names this asset (an asset in use cannot be removed).
    pub(crate) fn references_asset(&self, asset: &AssetId) -> bool {
        self.raster_entries()
            .iter()
            .any(|entry| entry.item.asset_id() == asset)
    }
    /// The canonical container for the given lists: an empty list is absent, and no list at all is no container.
    pub(crate) fn normalized(
        raster: Vec<RasterEntry>,
        text: Vec<TextEntry>,
    ) -> Presence<ItemRecords> {
        if raster.is_empty() && text.is_empty() {
            return Presence::Missing;
        }
        Presence::Present(ItemRecords {
            raster: present_if_any(raster),
            text: present_if_any(text),
        })
    }
}

/// The stable holder name of a text block: the identity its glyph pins (`<holder>/glyph/<ordinal>`) are keyed by. It
/// is derived from the stable native target only (never a frame-local presentation id) and an edit never changes it.
pub(crate) fn text_holder_id(target: &ObjectTarget) -> String {
    format!(
        "{}:{}:{}",
        target.context_id(),
        target.layer_uid(),
        target.stroke_id()
    )
}

/// Why an asset reference of an item does not resolve against the project inventory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ItemAssetIssue {
    /// No project inventory exists (a bare drawing).
    NoInventory,
    /// A raster item names an asset that is not in the inventory.
    MissingAsset,
    /// A raster item names an asset that is not an image.
    NotImage,
    /// A text block names a font asset that is not in the inventory.
    MissingFont,
    /// A text block names an asset that is not a font.
    NotFont,
}

/// `None` when the raster item's asset is an image of the inventory (embedded or linked: the form is not an issue).
pub(crate) fn raster_asset_issue(
    item: &RasterItem,
    inventory: Option<&AssetInventory>,
) -> Option<ItemAssetIssue> {
    let Some(inventory) = inventory else {
        return Some(ItemAssetIssue::NoInventory);
    };
    match item.source_form(inventory) {
        Ok(_) => None,
        Err(RasterItemError::NotImage) => Some(ItemAssetIssue::NotImage),
        Err(_) => Some(ItemAssetIssue::MissingAsset),
    }
}

/// `None` for a bundled font and for a font asset of kind font. Whether the retained font provider can lay it out is a
/// separate, explicit unavailability; this only resolves the stable reference.
pub(crate) fn font_asset_issue(
    font: &FontRef,
    inventory: Option<&AssetInventory>,
) -> Option<ItemAssetIssue> {
    let FontRef::Asset(id) = font else {
        return None;
    };
    let Some(inventory) = inventory else {
        return Some(ItemAssetIssue::NoInventory);
    };
    match inventory.get(id) {
        None => Some(ItemAssetIssue::MissingFont),
        Some(record) if record.kind() != AssetKind::Font => Some(ItemAssetIssue::NotFont),
        Some(_) => None,
    }
}

/// The first unresolved asset reference of the document's items, raster items before text blocks.
pub(crate) fn item_asset_issue(
    document: &ObjectDocument,
    inventory: Option<&AssetInventory>,
) -> Option<ItemAssetIssue> {
    let items = document.item_records()?;
    items
        .raster_entries()
        .iter()
        .find_map(|entry| raster_asset_issue(&entry.item, inventory))
        .or_else(|| {
            items
                .text_entries()
                .iter()
                .find_map(|entry| font_asset_issue(entry.block.font(), inventory))
        })
}

impl ObjectDocument {
    /// The raster items and text blocks of this document; `None` when it holds none.
    pub(crate) fn item_records(&self) -> Option<&ItemRecords> {
        self.items.value()
    }
}
