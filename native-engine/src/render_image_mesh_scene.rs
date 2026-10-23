//! Scene item of the image-mesh join (F17, F18, F19; leaf L03-S): one prepared raster image carrying
//! a `meshId` becomes the deformed, outline-masked draw the native scene and the native export both
//! replay.
//!
//! The input is the [`RasterSceneImage`] of the raster scene producer: its `rect()` IS the final
//! display rect (every Motion, parent, 3D and duplicator step already folded in, engine-bridge.js
//! 1465-1485), its `opacity_f32()` the sequential product of the stage opacities, its `pixel_size()`
//! the decoded image size, and its handle keeps the pixels alive. Nothing here multiplies a Motion
//! matrix: the mesh follows that one rect. The world payload and the planned draw come from the
//! geometry half (`render_image_mesh_scene_source`); the item keeps the payload so
//! [`MeshSceneItem::for_view`] re-plans the SAME geometry for another output (the export), and only the
//! half-device-pixel seam dilation can differ between outputs.
//!
//! The item is detached from the document and the mesh store (a payload copy), holds the pixel handle
//! through the image it embeds, and is the single type the scene replay and the export replay consume.
//!
//! A raster the mesh producer claims is NOT also reported by the plain raster producer: the registry
//! rejects a double claim, and `Ok(None)` (a mesh without triangles, drawn as the plain image by the
//! baseline) reports nothing so the raster producer keeps the item.
//!
//! This file sits in the render-scene layer, so it imports no document type: the mesh store reaches it
//! as the [`MeshStoreRef`] port of the geometry half. Not provided: the GPU replay
//! (`render_image_mesh_scene_replay`), the image retention (the pixel handle belongs to L06/L08), the
//! producer registry call (the caller owns the pin and the source object index) and any Motion math.

use crate::evaluation::object_image_mesh_kernels::object_image_mesh_geometry::ScenePayload;
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_plan::{
    AffineCoeffs, MeshDrawPlan,
};
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_scene_source::{
    plan_draw, prepare_mesh_geometry,
};
pub(crate) use crate::evaluation::object_image_mesh_kernels::render_image_mesh_scene_source::{
    MeshDraw, MeshGeometry, MeshSceneError, MeshSceneSource, MeshStoreRef,
};
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    ProducerOutput, ScenePin,
};
use crate::object_render_scene::text_glyph_scene_kernels::object_raster_asset_scene::{
    resource_id, RasterSceneImage,
};
use crate::object_render_scene::ObjectSceneError;

/// Producer name in the pinned scene registry.
pub(crate) const PRODUCER: &str = "mesh-image";

/// One deformed, masked image draw, detached from the document and the store.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MeshSceneItem<H> {
    image: RasterSceneImage<H>,
    mesh_id: String,
    view: AffineCoeffs,
    geometry: MeshGeometry,
}

/// Builds the mesh draw of one prepared raster image.
///
/// `store` is the document's mesh store. Returns `Ok(None)` when the baseline draws the plain image
/// (a mesh without triangles); see the module notes.
pub(crate) fn prepare_mesh_scene_item<H: Clone>(
    image: &RasterSceneImage<H>,
    source: &MeshSceneSource<'_>,
    store: MeshStoreRef<'_>,
) -> Result<Option<MeshSceneItem<H>>, MeshSceneError> {
    let geometry = prepare_mesh_geometry(
        &image.rect(),
        image.pixel_size(),
        image.opacity_f32(),
        source,
        store,
    )?;
    Ok(geometry.map(|geometry| MeshSceneItem {
        image: image.clone(),
        mesh_id: source.mesh_id.to_owned(),
        view: source.view,
        geometry,
    }))
}

impl<H> MeshSceneItem<H> {
    /// The prepared raster image: final rect, opacity, pixel size, retention handle and version.
    pub(crate) fn image(&self) -> &RasterSceneImage<H> {
        &self.image
    }
    pub(crate) fn mesh_id(&self) -> &str {
        &self.mesh_id
    }
    /// The view the plan (and the replay transform) was built for.
    pub(crate) fn view(&self) -> AffineCoeffs {
        self.view
    }
    pub(crate) fn geometry(&self) -> &MeshGeometry {
        &self.geometry
    }
    /// The world-space payload every output is planned from (`scenePayloadFor`).
    pub(crate) fn payload(&self) -> &ScenePayload {
        &self.geometry.payload
    }
    pub(crate) fn draw(&self) -> &MeshDraw {
        &self.geometry.draw
    }
    pub(crate) fn plan(&self) -> Option<&MeshDrawPlan> {
        self.geometry.plan()
    }
    /// Triangles that draw (0 when the renderer draws nothing).
    pub(crate) fn triangle_count(&self) -> usize {
        self.geometry.triangle_count()
    }
}

impl<H: Clone> MeshSceneItem<H> {
    /// The same geometry planned for another output (the export): same payload, same image, same
    /// opacity, only the view, and with it the seam dilation, changes.
    pub(crate) fn for_view(&self, view: AffineCoeffs) -> MeshSceneItem<H> {
        let payload = self.geometry.payload.clone();
        let draw = plan_draw(
            &payload,
            self.image.pixel_size(),
            self.image.opacity_f32(),
            &view,
        );
        MeshSceneItem {
            image: self.image.clone(),
            mesh_id: self.mesh_id.clone(),
            view,
            geometry: MeshGeometry { payload, draw },
        }
    }
}

/// Reports one raster item to the pinned scene registry: the mesh item plus the pixel resource it
/// declares on success, nothing for `Ok(None)` (the raster producer claims the plain image), and a
/// typed unavailable entry for a refusal.
pub(crate) fn mesh_producer_output<H>(
    pin: ScenePin,
    source_object_index: usize,
    result: Result<Option<MeshSceneItem<H>>, MeshSceneError>,
) -> ProducerOutput<MeshSceneItem<H>> {
    let output = ProducerOutput::new(PRODUCER, pin);
    match result {
        Ok(Some(item)) => {
            let id = resource_id(item.image.asset_id());
            let version = item.image.version().to_owned();
            output
                .with_item(source_object_index, item)
                .with_resource(id, version)
        }
        Ok(None) => output,
        Err(error) => output.with_unavailable(source_object_index, error.reason()),
    }
}

/// The frame refusal for a failed item: the existing atomic scene error.
pub(crate) fn scene_refusal(
    error: &MeshSceneError,
    source_object_index: usize,
) -> ObjectSceneError {
    ObjectSceneError::UnsupportedGeometry {
        source_object_index,
        reason: error.reason(),
    }
}

#[cfg(test)]
#[path = "../tests/render_image_mesh_scene_image.rs"]
pub(crate) mod image_helpers;
#[cfg(test)]
#[path = "../tests/render_image_mesh_scene.rs"]
mod tests;
#[cfg(test)]
#[path = "../tests/render_image_mesh_scene_widget.rs"]
mod widget_tests;
