//! Lane-4 owner-side family (package #2106, L04-M): the setup / convert
//! transaction children of `object_fill_owner.rs`, declared here so the parent
//! (498 of 500 nonblank lines) needs a single two-line declaration (ruling R4).
//! Declarations and re-exports only.
#![allow(dead_code)]

#[path = "object_component_convert.rs"]
pub(crate) mod object_component_convert;
#[path = "object_component_convert_owner.rs"]
pub(crate) mod object_component_convert_owner;
#[path = "object_component_convert_support.rs"]
pub(crate) mod object_component_convert_support;
#[path = "object_component_instance_set.rs"]
pub(crate) mod object_component_instance_set;
#[path = "object_component_place.rs"]
pub(crate) mod object_component_place;
#[path = "object_duplicator_config.rs"]
pub(crate) mod object_duplicator_config;
#[path = "object_duplicator_convert.rs"]
pub(crate) mod object_duplicator_convert;

pub(crate) use object_component_convert_owner::{PreparedComponentSetup, SetupKind, SetupRefusal};
pub(crate) use object_component_convert_support::{SetupError, SourceRefusal};
