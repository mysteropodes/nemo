//! Lane-1 editable-vector / parametric / path-effect family kernels (package #2106):
//! pure private children, declared here so evaluation.rs needs a single `#[path]` line.

#[path = "object_parametric_geometry.rs"]
#[allow(dead_code)]
pub(crate) mod object_parametric_geometry;
#[path = "object_path_effects_kernel.rs"]
#[allow(dead_code)]
pub(crate) mod object_path_effects_kernel;
// G22/G13 L01-H: evaluation join (path effects first, parametric rebuild after) at a pinned snapshot and frame.
#[path = "render_vector_eval.rs"]
#[allow(dead_code)]
pub(crate) mod render_vector_eval;

// G14 L01-I: stroke-aware scene metadata of the vector render/export join (retained primary solid, stroke reach).
#[path = "render_vector_stroke.rs"]
#[allow(dead_code)]
pub(crate) mod render_vector_stroke;

// G14/G22/G13 L01-C: characterization oracle of the baseline serP/desP stroke normalization (tests only, no production code).
#[cfg(test)]
#[path = "../tests/object_vector_oracle.rs"]
#[allow(dead_code)]
mod object_vector_oracle;
