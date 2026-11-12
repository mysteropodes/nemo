//! G16 option C (#2361): the baseline vector-brush dab stamper, ported to native. A textured stroke stays ONE anchor
//! record (preset, brush group and seed); its dabs are produced from it, never stored. The evaluation facade composes shared document geometry: the walked path, the preset table, the random and width helpers, the
//! Paper dab geometry and the stamper.
#[path = "brush_dab_paint.rs"]
pub(crate) mod paint;
// Pure geometry is shared with commands; paint composition stays in evaluation.
pub(crate) use crate::object_document::brush_dab::{path, presets, sampling, shape, stamp};

#[cfg(test)]
#[path = "../tests/brush_dab_oracle.rs"]
mod oracle;
