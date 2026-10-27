//! G16 option C (#2361): the baseline vector-brush dab stamper, ported to native. A textured stroke stays ONE anchor
//! record (preset, brush group and seed); its dabs are produced from it, never stored. Declared once from
//! `evaluation.rs`; each part is one child: the walked path, the preset table, the random and width helpers, the
//! Paper dab geometry and the stamper.
#[path = "brush_dab_paint.rs"]
pub(crate) mod paint;
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

#[cfg(test)]
#[path = "../tests/brush_dab_oracle.rs"]
mod oracle;
