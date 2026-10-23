//! Holder side of Image asset placements (L06-I, G02 / G27): which document elements an image can sit on, and the
//! pure placement queries the layer and asset lifecycle owners need.
//!
//! A placement binds one catalog asset to one stable holder (`AssetPlacement`: asset id, `AssetTarget`, offset). The
//! baseline media library bound an entry to a layer by `layerUid` first and fell back to the layer NAME
//! (`resolveSrcLayer`, src/js/media-library.js); the native holder is the stable uid only, so a layer that was
//! renamed, deleted or never existed is a dangling target, never a name match.
//!
//! The owner join itself is `check_placement` (lane 9, `object_media_link_edit.rs`): it resolves the context and the
//! layer for every drawing placement and asks [`image_element_exists`] for the element of an Image placement. A
//! drawing element of an image is an AUTHORED path record or an AUTHORED raster item of that context and layer. A
//! reference-scope record is not a holder (the same rule the property holders use), and a text block is not an image
//! holder: it carries a font, and fonts are another owner's placements (`AssetTarget::Text`).
//!
//! Nothing here mutates a document or a catalog, reads a clock, opens a file or owns a resource: the queries borrow
//! the immutable values and return borrowed placements in table order.
use crate::object_document::{
    AssetId, AssetInventory, AssetPlacement, AssetTarget, FrameScopeKind, ObjectDocument,
    ObjectTarget,
};

fn names_element(
    target: &ObjectTarget,
    context_id: &str,
    layer_uid: &str,
    stroke_id: &str,
) -> bool {
    matches!(target.scope_kind(), FrameScopeKind::Authored)
        && target.context_id() == context_id
        && target.layer_uid() == layer_uid
        && target.stroke_id() == stroke_id
}

/// True when `document` holds an authored element with this stable identity that an image can be placed on: a path
/// record of `objects` or a raster item of `items`. The caller has already resolved the context and the layer.
pub(crate) fn image_element_exists(
    document: &ObjectDocument,
    context_id: &str,
    layer_uid: &str,
    stroke_id: &str,
) -> bool {
    document
        .objects()
        .iter()
        .any(|record| names_element(record.target(), context_id, layer_uid, stroke_id))
        || document.item_records().is_some_and(|items| {
            items
                .raster_entries()
                .iter()
                .any(|entry| names_element(&entry.target, context_id, layer_uid, stroke_id))
        })
}

/// The context and layer a target names; `None` for an audio track, which sits on no layer.
pub(crate) fn target_layer(target: &AssetTarget) -> Option<(&str, &str)> {
    match target {
        AssetTarget::Drawing {
            context_id,
            layer_uid,
            ..
        }
        | AssetTarget::Text {
            context_id,
            layer_uid,
            ..
        } => Some((context_id.as_str(), layer_uid.as_str())),
        AssetTarget::AudioTrack { .. } => None,
    }
}

/// The one placement of `target`, if any. The catalog allows one placement per target.
pub(crate) fn placement_for_target<'a>(
    inventory: &'a AssetInventory,
    target: &AssetTarget,
) -> Option<&'a AssetPlacement> {
    inventory
        .placements()
        .iter()
        .find(|placement| placement.target() == target)
}

/// Every placement whose target sits on the layer `layer_uid`, whole-layer and element-level, drawing and text, in
/// table order. Layer uids are unique in a document, so the context is not part of the question. This is what a
/// layer removal has to cascade over or refuse (the layer lifecycle lane owns that decision, not this query).
pub(crate) fn placements_for_layer<'a>(
    inventory: &'a AssetInventory,
    layer_uid: &str,
) -> Vec<&'a AssetPlacement> {
    inventory
        .placements()
        .iter()
        .filter(|placement| {
            target_layer(placement.target()).is_some_and(|(_, layer)| layer == layer_uid)
        })
        .collect()
}

/// Every placement of the asset `asset`, in table order (the same asset may sit on several targets).
pub(crate) fn placements_for_asset<'a>(
    inventory: &'a AssetInventory,
    asset: &AssetId,
) -> Vec<&'a AssetPlacement> {
    inventory
        .placements()
        .iter()
        .filter(|placement| placement.asset_id() == asset)
        .collect()
}

#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/object_raster_asset_placement.rs"]
mod tests;
