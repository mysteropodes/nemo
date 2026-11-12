//! Pure seeded brush geometry shared by native commands and evaluation. No document mutation or paint owner.
#[path = "brush_dab_path.rs"]
pub(crate) mod path;
#[path = "brush_dab_presets.rs"]
pub(crate) mod presets;
#[path = "brush_dab_sampling.rs"]
pub(crate) mod sampling;
#[path = "brush_dab_shape.rs"]
pub(crate) mod shape;
#[path = "brush_dab_stamp.rs"]
pub(crate) mod stamp;
