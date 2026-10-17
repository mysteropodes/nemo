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

// L02-H (#2108): selection delete / duplicate / split and clipboard plans (document layer).
#[path = "object_clipboard_plan.rs"]
#[allow(dead_code)]
pub(crate) mod object_clipboard_plan;
#[path = "object_selection_edit_clones.rs"]
#[allow(dead_code)]
pub(crate) mod object_selection_edit_clones;
#[path = "object_selection_edit_frames.rs"]
#[allow(dead_code)]
pub(crate) mod object_selection_edit_frames;
#[path = "object_selection_edit_groups.rs"]
#[allow(dead_code)]
pub(crate) mod object_selection_edit_groups;
#[path = "object_selection_edit_plan.rs"]
#[allow(dead_code)]
pub(crate) mod object_selection_edit_plan;
#[path = "object_selection_edit_split.rs"]
#[allow(dead_code)]
pub(crate) mod object_selection_edit_split;
#[path = "object_selection_edit_split_parts.rs"]
#[allow(dead_code)]
pub(crate) mod object_selection_edit_split_parts;

// L02-G (#2108): stored group table and the detached plan of one group edit.
#[path = "object_group_plan.rs"]
#[allow(dead_code)]
pub(crate) mod object_group_plan;
#[path = "object_group_record.rs"]
#[allow(dead_code)]
pub(crate) mod object_group_record;

// L02-I (#2108): polygon boolean / eraser engine, island rings, refit, Motion bake, document plans,
// and the paint-bucket commit join.
#[path = "object_boolean_ops.rs"]
#[allow(dead_code)]
pub(crate) mod object_boolean_ops;
#[path = "object_fill_trace_ops.rs"]
#[allow(dead_code)]
pub(crate) mod object_fill_trace_ops;
