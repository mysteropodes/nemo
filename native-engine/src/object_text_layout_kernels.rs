//! Lane-6 text layout / glyph family kernels (package #2106): pure private
//! children, declared here so evaluation.rs needs a single `#[path]` line.

#[path = "object_text_layout_kernel.rs"]
#[allow(dead_code)]
pub(crate) mod object_text_layout_kernel;

#[path = "text_glyph_contours.rs"]
#[allow(dead_code)]
pub(crate) mod text_glyph_contours;

#[path = "object_text_layout_record.rs"]
#[allow(dead_code)]
pub(crate) mod object_text_layout_record;
