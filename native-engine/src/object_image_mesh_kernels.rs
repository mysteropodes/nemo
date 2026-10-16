//! Lane-3 image-mesh, widget and rig family kernels (package #2106): pure private
//! children, declared here so evaluation.rs needs a single `#[path]` line for the lane.

#[path = "object_image_mesh_geometry.rs"]
#[allow(dead_code)]
pub(crate) mod object_image_mesh_geometry;
#[path = "object_widget_geometry.rs"]
#[allow(dead_code)]
pub(crate) mod object_widget_geometry;
