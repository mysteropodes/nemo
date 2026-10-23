//! Lane-3 image-mesh scene family (package #2106, L03-S): private children of the render-scene
//! parent, declared here so object_render_scene.rs needs a single `#[path]` line for the lane.

#[path = "render_image_mesh_scene.rs"]
#[allow(dead_code)]
pub(crate) mod render_image_mesh_scene;
#[path = "render_image_mesh_scene_replay.rs"]
#[allow(dead_code)]
pub(crate) mod render_image_mesh_scene_replay;
