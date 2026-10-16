//! Lane-7 typed scene/style family kernels (package #2106): pure private
//! children, declared here so evaluation.rs needs a single `#[path]` line.

#[path = "bitmap_brush_resources_bitmap.rs"]
#[allow(dead_code)]
pub(crate) mod bitmap_brush_resources_bitmap;
#[path = "bitmap_brush_resources_policy.rs"]
#[allow(dead_code)]
pub(crate) mod bitmap_brush_resources_policy;
#[path = "bitmap_brush_resources_store.rs"]
#[allow(dead_code)]
pub(crate) mod bitmap_brush_resources_store;
#[path = "object_scene_style_paint.rs"]
#[allow(dead_code)]
pub(crate) mod object_scene_style_paint;
#[path = "object_scene_style_stroke.rs"]
#[allow(dead_code)]
pub(crate) mod object_scene_style_stroke;

// L07-C: private pinned multi-producer scene join (ScenePin, plan order, unavailable entries, declared resources).
#[path = "object_scene_producer_pin.rs"]
#[allow(dead_code)]
pub(crate) mod object_scene_producer_pin;
