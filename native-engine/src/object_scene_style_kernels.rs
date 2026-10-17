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

// L07-I: typed item style (record -> fill, stroke, paint order) over the paint and stroke kernels.
#[path = "object_scene_style_item.rs"]
#[allow(dead_code)]
pub(crate) mod object_scene_style_item;

// L07-C: private pinned multi-producer scene join (ScenePin, plan order, unavailable entries, declared resources).
#[path = "object_scene_producer_pin.rs"]
#[allow(dead_code)]
pub(crate) mod object_scene_producer_pin;

// L07-E (#2165): bitmap-brush stamp plan (G06/G17): Paper path bounds + Numerical.solveQuadratic port, then the plan.
#[path = "bitmap_brush_resources_stamp.rs"]
#[allow(dead_code)]
pub(crate) mod bitmap_brush_resources_stamp;
#[path = "bitmap_brush_resources_stamp_bounds.rs"]
#[allow(dead_code)]
pub(crate) mod bitmap_brush_resources_stamp_bounds;

// L07-H: component evaluated-shape kernel (detached copies from a resolved placement, symMatrix + component camera).
#[path = "object_scene_producer_component.rs"]
#[allow(dead_code)]
pub(crate) mod object_scene_producer_component;
#[path = "object_scene_producer_component_shape.rs"]
#[allow(dead_code)]
pub(crate) mod object_scene_producer_component_shape;

// L07-L (#2113): rig-posed stroke and image-mesh producer (I16a, G03a) joined through the L07-C registry.
#[path = "object_scene_producer_rig.rs"]
#[allow(dead_code)]
pub(crate) mod object_scene_producer_rig;

// L07-K (#2113): component evaluated-shape producer join (D32, I16a): select -> scene (layer-slot pin join) -> join (producer).
#[path = "object_scene_producer_component_join.rs"]
#[allow(dead_code)]
pub(crate) mod object_scene_producer_component_join;
#[path = "object_scene_producer_component_scene.rs"]
#[allow(dead_code)]
pub(crate) mod object_scene_producer_component_scene;
#[path = "object_scene_producer_component_select.rs"]
#[allow(dead_code)]
pub(crate) mod object_scene_producer_component_select;
