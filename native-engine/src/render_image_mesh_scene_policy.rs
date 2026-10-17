//! Export routing, tag truthiness and widget-slot policy of the image-mesh scene join (F19; leaf L03-S).
//!
//! Document-facing rules of the join, kept in the evaluation layer next to the geometry half so the
//! render-scene half imports no document type:
//!
//! Export. `exportHasImageMesh()` (`src/js/export.js` at fbe4b8a3, 539) is over-inclusive on purpose:
//! the mere presence of a mesh in the STORE (even an orphan) routes an export through the engine,
//! because the opposite error is a flat, unmasked export nobody notices. [`check_export_route`] keeps
//! that rule and refuses an image-only route with the stable code `image-only-export-refused`.
//!
//! Tag. `data.meshId` is read with JavaScript truthiness: an empty tag is no tag, so the raster is an
//! ordinary image ([`tagged_mesh_id`]).
//!
//! Widget layers. A widget (rig control) layer never paints: `getEffectiveStrokes` returns `[]` and
//! `buildSceneJson` pushes an empty slot so later stack indices stay valid (`engine-bridge.js` 910).
//! [`scene_slot`] names that rule on the native layer and [`widget_layers_with_content`] reports the
//! layers that would break it; the pad and puck are viewport-only editor overlays, never scene items.
//! The record layer (L03-K) already refuses a drawing object on a widget layer, so on an admitted
//! document the report is always empty.

use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::RasterItem;
use crate::object_document::{ImageMeshStore, ObjectLayer};

/// `data.meshId` of a raster item, with the baseline truthiness (an empty tag is no tag).
pub(crate) fn tagged_mesh_id(item: &RasterItem) -> Option<&str> {
    item.mesh_id().filter(|id| !id.is_empty())
}

/// How an export reaches its pixels.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExportRoute {
    /// The native scene: raster items go through the mesh join.
    NativeScene,
    /// A path that draws every raster as one plain rect.
    ImageOnly,
}

/// An image-only export of a project that holds meshes: it would emit flat, unmasked images.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ImageOnlyExportRefusal {
    /// Every mesh id in the store, in store order.
    pub(crate) mesh_ids: Vec<String>,
}

impl ImageOnlyExportRefusal {
    pub(crate) const CODE: &'static str = "image-only-export-refused";
}

/// `exportHasImageMesh()`: any entry in the store, orphans included.
pub(crate) fn export_has_image_mesh(store: Option<&ImageMeshStore>) -> bool {
    store.is_some_and(|store| !store.entries().is_empty())
}

/// Refuses an image-only export of a project whose store holds a mesh.
pub(crate) fn check_export_route(
    store: Option<&ImageMeshStore>,
    route: ExportRoute,
) -> Result<(), ImageOnlyExportRefusal> {
    if route == ExportRoute::NativeScene || !export_has_image_mesh(store) {
        return Ok(());
    }
    let mesh_ids: Vec<String> = store
        .map(|store| store.entries().iter().map(|(id, _)| id.clone()).collect())
        .unwrap_or_default();
    Err(ImageOnlyExportRefusal { mesh_ids })
}

/// Stack slot of one document layer in the scene.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayerSceneSlot {
    /// The layer may own scene content.
    Content,
    /// A widget layer: an empty slot that keeps later stack indices valid and paints nothing.
    EmptyWidget,
}

/// `engine-bridge.js` 910: a widget layer gets its own empty slot.
pub(crate) fn scene_slot(layer: &ObjectLayer) -> LayerSceneSlot {
    if layer.widget().is_some() {
        LayerSceneSlot::EmptyWidget
    } else {
        LayerSceneSlot::Content
    }
}

pub(crate) fn scene_slots(layers: &[ObjectLayer]) -> Vec<LayerSceneSlot> {
    layers.iter().map(scene_slot).collect()
}

/// Positions of widget layers that own scene content (`content_layers` are the layer positions of
/// the scene entries). Empty on every admitted document; a non-empty answer is a join defect.
pub(crate) fn widget_layers_with_content(
    layers: &[ObjectLayer],
    content_layers: impl IntoIterator<Item = usize>,
) -> Vec<usize> {
    let mut leaks: Vec<usize> = content_layers
        .into_iter()
        .filter(|index| {
            layers
                .get(*index)
                .is_some_and(|layer| scene_slot(layer) == LayerSceneSlot::EmptyWidget)
        })
        .collect();
    leaks.sort_unstable();
    leaks.dedup();
    leaks
}

#[cfg(test)]
#[path = "../tests/render_image_mesh_scene_policy.rs"]
mod tests;
