//! Media binding plan (G42): the per-frame list of what each media placement of
//! a pinned asset inventory supplies, built from the lane-9 kernels.
//!
//! The plan is a pure join. For one timeline `frame` it walks
//! `AssetInventory::placements()` in order and, per placement, asks:
//! 1. the link gate (`object_media_link_join::export_gate`) whether the bytes
//!    can be read at all: an unresolved link is a typed [`MissingLink`] entry
//!    (asset id + placement id), never a default frame or silence;
//! 2. the video kernel (`select_frame`) for a `Drawing` + `Video` placement, or
//!    the audio kernel (`plan_start`) for an `AudioTrack` + `Audio` one. An
//!    `Image` placement binds as a still (no timing).
//!
//! Decisions kept on purpose:
//! - `AssetPlacement::offset_frames` is the single offset source. The baseline
//!   kept it twice (`nativeVideo.offsetFrames`, `track.offsetFrames`); the
//!   facts handed to the plan carry no offset and the kernels receive the
//!   placement's i64 as an exact f64 (the catalog bounds it to +-(2^53 - 1));
//! - `Text` placements are fonts, another owner's, and produce no entry; the
//!   entry order is the placement order of the inventory, deterministic;
//! - facts the catalog does not hold (`frameCount`, `timeRemap`, trims, buffer
//!   duration, volume) come through [`MediaFacts`]. A missing fact or sampler
//!   is a typed [`Unavailable`] entry, never a guessed default;
//! - a kernel refusal (`VideoTimingRefusal`, `AudioTimingRefusal`) or a link
//!   gate refusal other than a missing link is a typed [`Refusal`] entry for
//!   THAT placement; the other placements are still planned;
//! - a non-finite `frame` is a [`PlanError`] for the whole plan.
//!
//! Immutability: the plan reads `&AssetInventory` and `&ResolveTable` and
//! returns owned values holding ids and numbers only; it retains no encoded
//! allocation and cannot change the pinned inventory.
//!
//! Not provided: decoding, link resolution (the caller feeds the
//! `ResolveTable`), time-remap and volume sampling (injected), the locator
//! namespace of a placement (the caller names it), and the export job join.

use crate::object_document::media_timing::object_media_link_join::{
    export_gate, LinkGateError, LinkReadiness, LocatorNamespace, MissingLink,
};
use crate::object_document::media_timing::object_media_link_state::ResolveTable;
use crate::object_document::media_timing::object_media_timing_audio::{
    plan_start, AudioTimingRefusal, AudioTrackTiming, NoVolumeSampler, PlaybackEnv, StartOutcome,
    TrackVolume, VolumeSampler,
};
use crate::object_document::media_timing::object_media_timing_video::{
    select_frame, FrameSelection, NoTimeRemap, TimeRemapPort, VideoLayerTiming, VideoTimingRefusal,
};
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetPlacement, AssetRecord, AssetTarget,
};

/// The `nativeVideo` / `timeRemap` fields the catalog does not hold. The offset
/// is deliberately absent: it comes from the placement.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct VideoFacts {
    /// `nativeVideo.frameCount`; `NaN` stands for absent.
    pub frame_count: f64,
    /// The layer's `timeRemap` is present.
    pub time_remap: bool,
    /// `nativeVideo.interpMode` as authored.
    pub interp_mode: Option<String>,
    /// Legacy `nativeVideo.frameBlend`.
    pub frame_blend: bool,
}

/// The audio-track fields the catalog does not hold; the offset is absent.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct AudioFacts {
    pub muted: bool,
    /// Decoded buffer duration; `None` while not decoded.
    pub buffer_duration: Option<f64>,
    /// `trimStart`; `NaN` stands for absent.
    pub trim_start: f64,
    pub trim_end: Option<f64>,
    pub volume: TrackVolume,
}

/// Per-placement facts and samplers, keyed by the placement id. `None` means
/// the provider has nothing for that placement, which the plan reports.
pub(crate) trait MediaFacts {
    fn video(&self, placement_id: &str) -> Option<VideoFacts>;
    fn time_remap(&self, placement_id: &str) -> Option<&dyn TimeRemapPort>;
    fn audio(&self, placement_id: &str) -> Option<AudioFacts>;
    fn volume_sampler(&self, placement_id: &str) -> Option<&dyn VolumeSampler>;
}

/// Everything the plan reads for one frame.
#[derive(Clone, Copy)]
pub(crate) struct PlanInput<'a> {
    pub inventory: &'a AssetInventory,
    pub links: &'a ResolveTable,
    pub namespace: LocatorNamespace,
    /// The clock of the link gate, a plain argument.
    pub now_ms: i64,
    pub env: PlaybackEnv,
    pub facts: &'a dyn MediaFacts,
}

/// What a bound placement supplies at the frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BoundMedia {
    Still,
    Video(FrameSelection),
    Audio(StartOutcome),
}

/// A placement whose bytes are readable and whose timing is derived.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Binding {
    pub placement_id: String,
    pub asset_id: AssetId,
    pub link: LinkReadiness,
    pub media: BoundMedia,
}

/// A provider the plan needs that did not answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unavailable {
    VideoFacts,
    TimeRemapSampler,
    AudioFacts,
    VolumeSampler,
}

/// Why a kernel or the link gate refused a placement.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Refusal {
    Link(LinkGateError),
    Video(VideoTimingRefusal),
    Audio(AudioTimingRefusal),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct UnavailableBinding {
    pub placement_id: String,
    pub asset_id: AssetId,
    pub what: Unavailable,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RefusedBinding {
    pub placement_id: String,
    pub asset_id: AssetId,
    pub why: Refusal,
}

/// One placement's outcome.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PlanEntry {
    Bound(Binding),
    MissingLink(MissingLink),
    Unavailable(UnavailableBinding),
    Refused(RefusedBinding),
}

/// The whole plan cannot be derived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PlanError {
    NonFiniteFrame,
    /// A placement names an asset the inventory lacks (the constructor of
    /// `AssetInventory` forbids it; reported, never defaulted).
    DanglingAsset {
        placement_id: String,
    },
    /// A placement target does not accept the asset kind (same).
    KindMismatch {
        placement_id: String,
    },
}

/// The first missing link of a plan, in placement order.
pub(crate) fn first_missing_link(entries: &[PlanEntry]) -> Option<&MissingLink> {
    entries.iter().find_map(|entry| match entry {
        PlanEntry::MissingLink(missing) => Some(missing),
        _ => None,
    })
}

/// The plan for `frame`: one entry per media placement, in placement order.
pub(crate) fn plan_frame(input: &PlanInput<'_>, frame: f64) -> Result<Vec<PlanEntry>, PlanError> {
    if !frame.is_finite() {
        return Err(PlanError::NonFiniteFrame);
    }
    let mut entries = Vec::with_capacity(input.inventory.placements().len());
    for placement in input.inventory.placements() {
        if matches!(placement.target(), AssetTarget::Text { .. }) {
            continue;
        }
        let record =
            input
                .inventory
                .get(placement.asset_id())
                .ok_or_else(|| PlanError::DanglingAsset {
                    placement_id: placement.id().to_owned(),
                })?;
        entries.push(plan_placement(input, placement, record, frame)?);
    }
    Ok(entries)
}

fn plan_placement(
    input: &PlanInput<'_>,
    placement: &AssetPlacement,
    record: &AssetRecord,
    frame: f64,
) -> Result<PlanEntry, PlanError> {
    let mismatch = || PlanError::KindMismatch {
        placement_id: placement.id().to_owned(),
    };
    match (placement.target(), record.kind()) {
        (AssetTarget::Drawing { .. }, AssetKind::Image | AssetKind::Video)
        | (AssetTarget::AudioTrack { .. }, AssetKind::Audio) => {}
        _ => return Err(mismatch()),
    }
    let link = match export_gate(
        input.links,
        record,
        placement,
        input.namespace,
        input.now_ms,
    ) {
        Ok(readiness) => readiness,
        Err(LinkGateError::MissingLink(missing)) => return Ok(PlanEntry::MissingLink(missing)),
        Err(other) => return Ok(refused(placement, Refusal::Link(other))),
    };
    let media = match record.kind() {
        AssetKind::Video => match video_selection(input, placement, frame) {
            Ok(selection) => BoundMedia::Video(selection),
            Err(entry) => return Ok(entry),
        },
        AssetKind::Audio => match audio_start(input, placement, frame) {
            Ok(outcome) => BoundMedia::Audio(outcome),
            Err(entry) => return Ok(entry),
        },
        _ => BoundMedia::Still,
    };
    Ok(PlanEntry::Bound(Binding {
        placement_id: placement.id().to_owned(),
        asset_id: record.id().clone(),
        link,
        media,
    }))
}

fn refused(placement: &AssetPlacement, why: Refusal) -> PlanEntry {
    PlanEntry::Refused(RefusedBinding {
        placement_id: placement.id().to_owned(),
        asset_id: placement.asset_id().clone(),
        why,
    })
}

fn unavailable(placement: &AssetPlacement, what: Unavailable) -> PlanEntry {
    PlanEntry::Unavailable(UnavailableBinding {
        placement_id: placement.id().to_owned(),
        asset_id: placement.asset_id().clone(),
        what,
    })
}

/// The video kernel at the placement offset; `Err` carries the finished entry.
fn video_selection(
    input: &PlanInput<'_>,
    placement: &AssetPlacement,
    frame: f64,
) -> Result<FrameSelection, PlanEntry> {
    let id = placement.id();
    let Some(facts) = input.facts.video(id) else {
        return Err(unavailable(placement, Unavailable::VideoFacts));
    };
    let idle = NoTimeRemap;
    let port: &dyn TimeRemapPort = if facts.time_remap {
        match input.facts.time_remap(id) {
            Some(port) => port,
            None => return Err(unavailable(placement, Unavailable::TimeRemapSampler)),
        }
    } else {
        &idle
    };
    let layer = VideoLayerTiming {
        frame_count: facts.frame_count,
        offset_frames: placement.offset_frames() as f64,
        native_video: true,
        time_remap: facts.time_remap,
        interp_mode: facts.interp_mode.as_deref(),
        frame_blend: facts.frame_blend,
    };
    select_frame(&layer, frame, port).map_err(|e| refused(placement, Refusal::Video(e)))
}

/// The audio kernel at the placement offset; `Err` carries the finished entry.
fn audio_start(
    input: &PlanInput<'_>,
    placement: &AssetPlacement,
    frame: f64,
) -> Result<StartOutcome, PlanEntry> {
    let id = placement.id();
    let Some(facts) = input.facts.audio(id) else {
        return Err(unavailable(placement, Unavailable::AudioFacts));
    };
    let idle = NoVolumeSampler;
    let sampler: &dyn VolumeSampler = match (facts.volume, input.facts.volume_sampler(id)) {
        (TrackVolume::Animated, None) => {
            return Err(unavailable(placement, Unavailable::VolumeSampler))
        }
        (_, Some(sampler)) => sampler,
        (TrackVolume::Flat(_), None) => &idle,
    };
    let track = AudioTrackTiming {
        muted: facts.muted,
        buffer_duration: facts.buffer_duration,
        offset_frames: placement.offset_frames() as f64,
        trim_start: facts.trim_start,
        trim_end: facts.trim_end,
        volume: facts.volume,
    };
    plan_start(&input.env, &track, frame, sampler)
        .map_err(|e| refused(placement, Refusal::Audio(e)))
}

#[cfg(test)]
#[path = "../tests/object_media_link_plan_controls.rs"]
mod controls;
#[cfg(test)]
#[path = "../tests/object_media_link_plan_links.rs"]
mod links;
#[cfg(test)]
#[path = "../tests/object_media_link_plan_pins.rs"]
mod pins;
#[cfg(test)]
#[path = "../tests/object_media_link_plan.rs"]
mod tests;
