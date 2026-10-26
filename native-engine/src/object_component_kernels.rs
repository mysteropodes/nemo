//! Lane-4 component/storyboard/duplicator family kernels (package #2106): pure
//! private children, declared here so evaluation.rs needs a single `#[path]` line.

#[path = "object_component_playback.rs"]
#[allow(dead_code)]
pub(crate) mod object_component_playback;

#[path = "duplicator_placement_modes.rs"]
#[allow(dead_code)]
pub(crate) mod duplicator_placement_modes;
#[path = "duplicator_placement_rng.rs"]
#[allow(dead_code)]
pub(crate) mod duplicator_placement_rng;

#[path = "duplicator_placement_apply.rs"]
#[allow(dead_code)]
pub(crate) mod duplicator_placement_apply;
#[path = "duplicator_placement_color.rs"]
#[allow(dead_code)]
pub(crate) mod duplicator_placement_color;
#[path = "duplicator_placement_jsnum.rs"]
#[allow(dead_code)]
pub(crate) mod duplicator_placement_jsnum;
#[path = "duplicator_placement_matrix.rs"]
#[allow(dead_code)]
pub(crate) mod duplicator_placement_matrix;
#[path = "duplicator_placement_temporal.rs"]
#[allow(dead_code)]
pub(crate) mod duplicator_placement_temporal;

#[path = "duplicator_placement_clone.rs"]
#[allow(dead_code)]
pub(crate) mod duplicator_placement_clone;
#[path = "object_effector_falloff.rs"]
#[allow(dead_code)]
pub(crate) mod object_effector_falloff;
#[path = "object_effector_layer.rs"]
#[allow(dead_code)]
pub(crate) mod object_effector_layer;

#[path = "object_component_exposed.rs"]
#[allow(dead_code)]
pub(crate) mod object_component_exposed;
#[path = "object_component_exposed_apply.rs"]
#[allow(dead_code)]
pub(crate) mod object_component_exposed_apply;
#[path = "object_component_exposed_key.rs"]
#[allow(dead_code)]
pub(crate) mod object_component_exposed_key;
#[path = "object_component_override_port.rs"]
#[allow(dead_code)]
pub(crate) mod object_component_override_port;

#[path = "object_duplicator_evaluate.rs"]
#[allow(dead_code)]
pub(crate) mod object_duplicator_evaluate;
#[path = "object_duplicator_evaluate_place.rs"]
#[allow(dead_code)]
pub(crate) mod object_duplicator_evaluate_place;
#[path = "object_duplicator_evaluate_spec.rs"]
#[allow(dead_code)]
pub(crate) mod object_duplicator_evaluate_spec;
#[path = "object_duplicator_evaluate_types.rs"]
#[allow(dead_code)]
pub(crate) mod object_duplicator_evaluate_types;
#[path = "object_effector_layers.rs"]
#[allow(dead_code)]
pub(crate) mod object_effector_layers;

#[path = "object_context_admission.rs"]
#[allow(dead_code)]
pub(crate) mod object_context_admission;
#[path = "object_context_resolver.rs"]
#[allow(dead_code)]
pub(crate) mod object_context_resolver;
#[path = "object_context_selection.rs"]
#[allow(dead_code)]
pub(crate) mod object_context_selection;

#[path = "object_context_split.rs"]
#[allow(dead_code)]
pub(crate) mod object_context_split;
#[path = "object_context_split_facts.rs"]
#[allow(dead_code)]
pub(crate) mod object_context_split_facts;
#[path = "object_context_workspace.rs"]
#[allow(dead_code)]
pub(crate) mod object_context_workspace;

#[path = "object_storyboard_chain.rs"]
#[allow(dead_code)]
pub(crate) mod object_storyboard_chain;

// L04-K: component evaluation join (admission -> frame packet -> component producer join, entered-component records).
#[path = "object_component_evaluate.rs"]
#[allow(dead_code)]
pub(crate) mod object_component_evaluate;
