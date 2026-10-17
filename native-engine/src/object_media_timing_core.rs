//! Media timing kernels (lane 9): pure value mappings from timeline frames to
//! media positions. Children are declared here so the document family needs a
//! single `#[path]` line; later leaves (audio timing, link disposition) add
//! their own child beside `object_media_timing_video`.

#[path = "object_media_timing_video.rs"]
pub(crate) mod object_media_timing_video;

#[path = "object_media_timing_audio.rs"]
pub(crate) mod object_media_timing_audio;

#[path = "object_media_link_state.rs"]
pub(crate) mod object_media_link_state;

#[path = "object_media_link_reopen.rs"]
pub(crate) mod object_media_link_reopen;

#[path = "object_media_link_join.rs"]
pub(crate) mod object_media_link_join;

#[path = "object_media_link_plan.rs"]
pub(crate) mod object_media_link_plan;

#[path = "object_media_link_edit.rs"]
pub(crate) mod object_media_link_edit;
