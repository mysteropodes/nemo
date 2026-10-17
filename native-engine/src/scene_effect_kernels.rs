//! Lane-8 scene composition/effect family kernels (package #2106): pure private
//! children, declared here so object_render_scene.rs needs a single `#[path]` line.

#[path = "object_scene_resource_set.rs"]
#[allow(dead_code)]
pub(crate) mod object_scene_resource_set;
#[path = "scene_effect_blend_ref.rs"]
#[allow(dead_code)]
pub(crate) mod scene_effect_blend_ref;
#[path = "scene_effect_order.rs"]
#[allow(dead_code)]
pub(crate) mod scene_effect_order;
#[path = "scene_effect_params.rs"]
#[allow(dead_code)]
pub(crate) mod scene_effect_params;
