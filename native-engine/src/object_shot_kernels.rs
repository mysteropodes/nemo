//! Lane-10 shot/montage/import family kernels (package #2106): pure private
//! children, declared here so object_history.rs needs a single `#[path]` line for the lane.

#[path = "native_import_drop_route.rs"]
#[allow(dead_code)]
pub(crate) mod native_import_drop_route;
#[path = "object_shot_chain.rs"]
#[allow(dead_code)]
pub(crate) mod object_shot_chain;

#[path = "native_import_svg_explode.rs"]
#[allow(dead_code)]
pub(crate) mod native_import_svg_explode;

#[path = "object_import_remap_ids.rs"]
#[allow(dead_code)]
pub(crate) mod object_import_remap_ids;

#[path = "native_import_figma_walk.rs"]
#[allow(dead_code)]
pub(crate) mod native_import_figma_walk;

#[path = "native_import_psd_plan.rs"]
#[allow(dead_code)]
pub(crate) mod native_import_psd_plan;

#[path = "native_import_transaction.rs"]
#[allow(dead_code)]
pub(crate) mod native_import_transaction;
