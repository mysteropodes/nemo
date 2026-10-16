//! Lane-8 scene composition/effect family kernels (package #2106): pure private
//! children, declared here so object_render_scene.rs needs a single `#[path]` line.

#[path = "scene_effect_order.rs"]
#[allow(dead_code)]
pub(crate) mod scene_effect_order;
