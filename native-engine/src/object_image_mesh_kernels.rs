//! Lane-3 image-mesh, widget and rig family kernels (package #2106): pure private
//! children, declared here so evaluation.rs needs a single `#[path]` line for the lane.

#[path = "object_image_mesh_geometry.rs"]
#[allow(dead_code)]
pub(crate) mod object_image_mesh_geometry;
#[path = "object_image_mesh_topology.rs"]
#[allow(dead_code)]
pub(crate) mod object_image_mesh_topology;
#[path = "object_widget_geometry.rs"]
#[allow(dead_code)]
pub(crate) mod object_widget_geometry;
#[path = "rig_ik_solve.rs"]
#[allow(dead_code)]
pub(crate) mod rig_ik_solve;

#[path = "object_image_mesh_delaunay.rs"]
#[allow(dead_code)]
pub(crate) mod object_image_mesh_delaunay;
#[path = "object_image_mesh_delaunay_geom.rs"]
#[allow(dead_code)]
pub(crate) mod object_image_mesh_delaunay_geom;
#[path = "object_image_mesh_orient.rs"]
#[allow(dead_code)]
pub(crate) mod object_image_mesh_orient;
#[path = "rig_pose_weights.rs"]
#[allow(dead_code)]
pub(crate) mod rig_pose_weights;

#[path = "render_image_mesh_plan.rs"]
#[allow(dead_code)]
pub(crate) mod render_image_mesh_plan;
#[path = "rig_pose_deform.rs"]
#[allow(dead_code)]
pub(crate) mod rig_pose_deform;
#[path = "rig_pose_deform_anchor.rs"]
#[allow(dead_code)]
pub(crate) mod rig_pose_deform_anchor;
// L03-S (#2109, F17/F18/F19): document-facing halves of the image-mesh scene and export join
// (geometry from the stored mesh, export routing, meshId truthiness, widget slots).
#[path = "render_image_mesh_scene_policy.rs"]
#[allow(dead_code)]
pub(crate) mod render_image_mesh_scene_policy;
#[path = "render_image_mesh_scene_source.rs"]
#[allow(dead_code)]
pub(crate) mod render_image_mesh_scene_source;
// L03-R (#2109, F17/F18): image-mesh / raster binding kernel (propagate, attach, detach, refcount release,
// duplicate-layer remap, video-layer mesh id behind the typed VideoFamily port).
#[path = "object_image_mesh_binding.rs"]
#[allow(dead_code)]
pub(crate) mod object_image_mesh_binding;
#[path = "object_image_mesh_binding_ops.rs"]
#[allow(dead_code)]
pub(crate) mod object_image_mesh_binding_ops;
// L03-P (#2109): pinned rig pose evaluation join (stored rig + snapshot -> detached posed geometry).
#[path = "rig_pose_eval.rs"]
#[allow(dead_code)]
pub(crate) mod rig_pose_eval;
#[path = "rig_pose_nearest.rs"]
#[allow(dead_code)]
pub(crate) mod rig_pose_nearest;
#[path = "rig_pose_nearest_curve.rs"]
#[allow(dead_code)]
pub(crate) mod rig_pose_nearest_curve;
#[path = "rig_pose_nearest_solve.rs"]
#[allow(dead_code)]
pub(crate) mod rig_pose_nearest_solve;
#[path = "rig_pose_nearest_weigh.rs"]
#[allow(dead_code)]
pub(crate) mod rig_pose_nearest_weigh;
