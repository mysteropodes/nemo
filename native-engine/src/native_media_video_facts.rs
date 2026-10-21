//! T12: the video facts and time-remap sampler the media plan reads, built
//! from each placement's stored timing (`native_media_frame_export`).
use crate::object_document::media_timing::object_media_link_plan::{
    AudioFacts, MediaFacts, VideoFacts,
};
use crate::object_document::media_timing::object_media_timing_audio::VolumeSampler;
use crate::object_document::media_timing::object_media_timing_video::{
    interp_mode, InterpMode, TimeRemapPort, VideoLayerTiming,
};
use crate::object_document::placement_offset::VideoTiming;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Facts {
    pub(super) video: BTreeMap<String, VideoFacts>,
    pub(super) remaps: BTreeMap<String, Remap>,
}
/// The stored track as the kernel's sampler (`SMMotion.timeRemapValue`).
pub(super) struct Remap(pub(super) VideoTiming);
impl TimeRemapPort for Remap {
    fn remap_value(&self, frame: f64) -> Option<f64> {
        self.0.remap_value(frame)
    }
}
impl MediaFacts for Facts {
    fn video(&self, placement_id: &str) -> Option<VideoFacts> {
        self.video.get(placement_id).cloned()
    }
    fn time_remap(&self, placement_id: &str) -> Option<&dyn TimeRemapPort> {
        self.remaps
            .get(placement_id)
            .map(|remap| remap as &dyn TimeRemapPort)
    }
    fn audio(&self, _placement_id: &str) -> Option<AudioFacts> {
        None
    }
    fn volume_sampler(&self, _placement_id: &str) -> Option<&dyn VolumeSampler> {
        None
    }
}

/// Whether the placement's effective mode is `motion` (`_interpMode`).
pub(super) fn motion_mode(facts: &Facts, placement_id: &str) -> bool {
    facts.video.get(placement_id).is_some_and(|video| {
        interp_mode(&VideoLayerTiming {
            frame_count: video.frame_count,
            offset_frames: 0.0,
            native_video: true,
            time_remap: video.time_remap,
            interp_mode: video.interp_mode.as_deref(),
            frame_blend: video.frame_blend,
        }) == InterpMode::Motion
    })
}
