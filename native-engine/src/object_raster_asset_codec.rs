//! L06-L (G01 / G38a): admission of the `items` document member (raster items and text blocks). Pure checks over the
//! decoded document; the single native codec (object_codec.rs) delegates here and stays byte-identical for a document
//! without `items`. Every refusal is a typed codec error; nothing is clamped, defaulted or inferred.
//!
//! Two stages, because the asset inventory lives in the project envelope and not in the drawing:
//! * [`validate_items`] / [`validate_item_parts`] (drawing only): container shape, scoped targets, layer resolution,
//!   uniqueness across every record kind, glyph-holder identity of every text block (the records themselves were decoded
//!   strictly). A v2 document keeps its composition order plan, which must also cover every item (object_order_codec.rs);
//! * [`validate_item_assets`] (drawing + inventory): every raster asset reference and every font asset reference
//!   resolves to an asset of the right kind. Called by the project wire decode and by the sealed transactions.
use crate::object_codec::{ObjectCodecError, ObjectCodecErrorKind};
use crate::object_document::object_text_layout_kernels::object_text_layout_record::pin::GlyphPin;
use crate::object_document::{
    item_asset_issue, text_holder_id, AssetInventory, ComponentTables, FrameScopeKind,
    ItemAssetIssue, ItemRecords, ObjectDocument, ObjectLayer, ObjectRecord, ObjectTarget, Presence,
};
use std::collections::{HashMap, HashSet};

fn invalid(message: &str) -> ObjectCodecError {
    ObjectCodecError::new(ObjectCodecErrorKind::Invalid, message)
}
fn unsupported(message: &str) -> ObjectCodecError {
    ObjectCodecError::new(ObjectCodecErrorKind::Unsupported, message)
}

/// Admit the `items` member of a storage document (v1, or v2 with its composition order plan).
pub(crate) fn validate_items(document: &ObjectDocument) -> Result<(), ObjectCodecError> {
    validate_item_parts(
        &document.items,
        document.format_version,
        document.total_frames,
        &document.layers,
        &document.objects,
        document.component_tables.value(),
    )
}

/// The same admission over the parts of an ordered (v2) wire document, which is not an `ObjectDocument`.
/// A declared component resolves only against its own layer list and frame bound; it never borrows
/// an identically named root layer. Every context's path targets participate in item uniqueness.
pub(crate) fn validate_item_parts(
    members: &Presence<ItemRecords>,
    format_version: u32,
    total_frames: u32,
    layers: &[ObjectLayer],
    objects: &[ObjectRecord],
    component_tables: Option<&ComponentTables>,
) -> Result<(), ObjectCodecError> {
    let Some(items) = members.value() else {
        return Ok(());
    };
    let empty = |list_present: bool, len: usize| list_present && len == 0;
    if empty(items.raster.value().is_some(), items.raster_entries().len())
        || empty(items.text.value().is_some(), items.text_entries().len())
        || (items.raster_entries().is_empty() && items.text_entries().is_empty())
    {
        return Err(invalid("an items container must hold at least one entry"));
    }
    let component_set = component_tables.and_then(|tables| tables.components.value());
    let components: HashMap<_, _> = component_set
        .into_iter()
        .flat_map(|set| &set.components)
        .map(|component| (component.symbol_id(), component))
        .collect();
    let inner_objects = component_set
        .into_iter()
        .flat_map(|set| &set.components)
        .flat_map(|component| component.objects());
    let mut seen: HashSet<&ObjectTarget> = objects
        .iter()
        .chain(inner_objects)
        .map(|record| &record.target)
        .collect();
    for target in items.targets() {
        let valid = if format_version == 2 {
            !target.context_id.is_empty()
                && !target.layer_uid.is_empty()
                && !target.stroke_id.is_empty()
        } else {
            target.valid()
        };
        if !valid {
            return Err(invalid("invalid scoped item target or frame"));
        }
        // Declared components always use their own layers and frame bounds. Other v2
        // contexts retain the protected storage-only contract against the root tables;
        // executable context resolution remains separate and refuses opaque identities.
        let (local_layers, local_frames) = components
            .get(target.context_id.as_str())
            .map_or((layers, total_frames), |component| {
                (component.layers(), component.total_frames())
            });
        if target.frame_scope.frame >= local_frames {
            return Err(invalid("invalid scoped item target or frame"));
        }
        if target.frame_scope.kind != FrameScopeKind::Authored {
            return Err(unsupported("items are authored records, never reference"));
        }
        let layer = local_layers
            .iter()
            .find(|layer| layer.layer_uid == target.layer_uid)
            .ok_or_else(|| invalid("item layer reference is unresolved"))?;
        if layer.widget.value().is_some() {
            return Err(invalid("a widget layer holds no items"));
        }
        if layer.is_null_layer() {
            return Err(invalid("a Null layer holds no items"));
        }
        if !seen.insert(target) {
            return Err(ObjectCodecError::new(
                ObjectCodecErrorKind::DuplicateId,
                "duplicate scoped item key",
            ));
        }
    }
    for entry in items.text_entries() {
        if GlyphPin::new(&text_holder_id(&entry.target), 0).is_err() {
            return Err(invalid(
                "text block holder identity is not a valid glyph holder",
            ));
        }
    }
    Ok(())
}

/// Resolve every asset reference of the drawing's items against the project inventory.
pub(crate) fn validate_item_assets(
    document: &ObjectDocument,
    inventory: Option<&AssetInventory>,
) -> Result<(), ObjectCodecError> {
    match item_asset_issue(document, inventory) {
        None => Ok(()),
        Some(ItemAssetIssue::NoInventory) => {
            Err(invalid("an item needs the project asset inventory"))
        }
        Some(ItemAssetIssue::MissingAsset) => {
            Err(invalid("a raster item asset is not in the inventory"))
        }
        Some(ItemAssetIssue::NotImage) => Err(invalid("a raster item asset is not an image")),
        Some(ItemAssetIssue::MissingFont) => {
            Err(invalid("a text block font asset is not in the inventory"))
        }
        Some(ItemAssetIssue::NotFont) => Err(invalid("a text block font asset is not a font")),
    }
}

#[cfg(test)]
#[path = "../tests/object_raster_asset_codec.rs"]
mod tests;
