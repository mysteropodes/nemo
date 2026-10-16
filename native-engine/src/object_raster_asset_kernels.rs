//! Lane-6 raster/text asset family kernels on the document parent (package
//! #2106): pure private children, declared here so object_document.rs needs a
//! single `#[path]` line for the lane.

#[path = "object_raster_asset_item.rs"]
#[allow(dead_code)]
pub(crate) mod object_raster_asset_item;
