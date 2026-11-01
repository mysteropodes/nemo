//! Lane-9 native vector/media export aggregator (declaration-only).
//!
//! Declared once from `export_job.rs`; each lane-9 export kernel adds one child
//! here so the shared parent never grows.

#[path = "native_vector_export_job.rs"]
pub(crate) mod native_vector_export_job;

#[path = "native_vector_export_svg.rs"]
pub(crate) mod native_vector_export_svg;

#[path = "native_vector_export_lottie.rs"]
pub(crate) mod native_vector_export_lottie;

#[path = "native_vector_export_rive.rs"]
pub(crate) mod native_vector_export_rive;

#[path = "native_ae_camera_export.rs"]
pub(crate) mod native_ae_camera_export;
