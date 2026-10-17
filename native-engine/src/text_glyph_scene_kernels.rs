//! Lane-6 text/raster scene join family (package #2106, L06-K): private children
//! of the render-scene parent, declared here so object_render_scene.rs needs a
//! single `#[path]` line for the lane.

#[path = "text_glyph_scene.rs"]
#[allow(dead_code)]
pub(crate) mod text_glyph_scene;

#[path = "object_raster_asset_scene.rs"]
#[allow(dead_code)]
pub(crate) mod object_raster_asset_scene;
