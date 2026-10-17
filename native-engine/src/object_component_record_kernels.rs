//! Lane-4 component/StoryBoard/duplicator record family (package #2106): private
//! storage-record children, declared here so object_document.rs needs a single
//! `#[path]` line for the family (ruling R4). Declarations only.

#[path = "object_component_records.rs"]
#[allow(dead_code)]
pub(crate) mod object_component_records;
#[path = "object_duplicator_records.rs"]
#[allow(dead_code)]
pub(crate) mod object_duplicator_records;
#[path = "object_storyboard_records.rs"]
#[allow(dead_code)]
pub(crate) mod object_storyboard_records;
