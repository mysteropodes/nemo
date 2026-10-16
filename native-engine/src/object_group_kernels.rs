//! Lane-2 group/boolean/clipboard/lifecycle family kernels (package #2106):
//! pure private children, declared here so object_document.rs needs a single
//! `#[path]` line for the family.

#[path = "object_group_combine_plan.rs"]
#[allow(dead_code)]
pub(crate) mod object_group_combine_plan;
#[path = "object_group_hierarchy.rs"]
#[allow(dead_code)]
pub(crate) mod object_group_hierarchy;

#[path = "object_clipboard_remap.rs"]
#[allow(dead_code)]
pub(crate) mod object_clipboard_remap;

#[path = "object_boolean_topology.rs"]
#[allow(dead_code)]
pub(crate) mod object_boolean_topology;

#[path = "object_layer_lifecycle_plan.rs"]
#[allow(dead_code)]
pub(crate) mod object_layer_lifecycle_plan;
