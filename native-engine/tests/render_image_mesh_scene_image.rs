//! Scene-side test builders for the image-mesh scene join (L03-S; 0 tests): a fixed-size pixel port
//! and the prepared raster image of the real raster scene producer, which applies the D30 stage chain.

use super::{prepare_mesh_scene_item, MeshSceneItem, MeshSceneSource};
use crate::evaluation::motion_parent_chain::{ChainEntry, ImageRect};
use crate::evaluation::object_image_mesh_kernels::render_image_mesh_scene_source::oracle_rows::{
    store_of, MeshRow,
};
use crate::object_render_scene::text_glyph_scene_kernels::object_raster_asset_scene::{
    prepare_raster_scene_image, RasterPixelPort, RasterPixels, RasterSceneImage, RasterSceneSource,
};

/// A pixel port that hands out a fixed-size image under the version `v1`.
pub(crate) struct SizedPort {
    pub(crate) width: u32,
    pub(crate) height: u32,
}

impl RasterPixelPort for SizedPort {
    type Handle = ();
    fn pixels(&self, _asset_id: &str) -> Option<RasterPixels<()>> {
        Some(RasterPixels {
            handle: (),
            width: self.width,
            height: self.height,
            version: "v1".to_owned(),
        })
    }
}

/// The prepared raster image of the raster scene producer (stage chain applied by the real producer).
pub(crate) fn raster_image(
    base: ImageRect,
    stages: &[ChainEntry],
    opacity: f64,
    image: (u32, u32),
) -> RasterSceneImage<()> {
    let source = RasterSceneSource {
        asset_id: "asset-1".to_owned(),
        rect: base,
        opacity,
    };
    let port = SizedPort {
        width: image.0,
        height: image.1,
    };
    prepare_raster_scene_image(&source, stages, &port).expect("raster image prepares")
}

/// Runs the scene join for one oracle row exactly as the oracle describes it.
pub(crate) fn prepare_image_row(row: &MeshRow) -> (RasterSceneImage<()>, MeshSceneItem<()>) {
    let image = raster_image(row.base, &row.stages, row.opacity, row.image);
    let store = store_of(&row.mesh);
    let source = MeshSceneSource {
        mesh_id: "im_1",
        view: row.view,
        offsets: row.override_offsets.as_deref(),
        pose: row.pose.as_deref(),
    };
    let item = prepare_mesh_scene_item(&image, &source, Some(&store))
        .expect("row prepares")
        .expect("row has a mesh draw");
    (image, item)
}
