//! Lane-5 frame/exposure/clipboard family kernels (package #2106): pure private
//! children, declared here so evaluation.rs needs a single `#[path]` line for the lane.

#[path = "object_frame_clipboard_packet.rs"]
#[allow(dead_code)]
pub(crate) mod object_frame_clipboard_packet;
#[path = "object_frame_edit_ranges.rs"]
#[allow(dead_code)]
pub(crate) mod object_frame_edit_ranges;
