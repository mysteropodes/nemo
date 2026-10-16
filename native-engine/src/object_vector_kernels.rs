//! Lane-1 editable-vector / parametric / path-effect family kernels (package #2106):
//! pure private children, declared here so evaluation.rs needs a single `#[path]` line.

#[path = "object_parametric_geometry.rs"]
#[allow(dead_code)]
pub(crate) mod object_parametric_geometry;
#[path = "object_path_effects_kernel.rs"]
#[allow(dead_code)]
pub(crate) mod object_path_effects_kernel;

// G14/G22/G13 L01-C: characterization oracle of the baseline serP/desP stroke normalization (tests only, no production code).
#[cfg(test)]
#[path = "../tests/object_vector_oracle.rs"]
#[allow(dead_code)]
mod object_vector_oracle;
