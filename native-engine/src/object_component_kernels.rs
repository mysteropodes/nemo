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
