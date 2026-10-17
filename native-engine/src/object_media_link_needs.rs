//! What one exported frame needs from its media placements (G44 / G42 join,
//! document-layer half): the typed, decodable need of each placement, or the
//! typed refusal that must stop the export BEFORE anything is staged.
//!
//! This is a pure walk over the lane-9 binding plan (`plan_frame`). It adds no
//! timing or link semantics: the source frame of timeline frame `N` is whatever
//! the L09-A kernel computes inside the plan (offset, clamp, time-remap value,
//! blend pair), and a link verdict is whatever the L09-C gate computes.
//!
//! Decisions kept on purpose:
//! - an exported frame is a timeline frame: `plan_frame(input, N)`;
//! - audio placements never enter a frame export. The baseline exports carry no
//!   audio, so an audio placement is skipped whatever its outcome (bound,
//!   unavailable, refused). The audible-span disposition is another leaf's;
//! - a missing link wins over every other refusal of the frame (the first one in
//!   placement order): the export refuses with the typed `MissingLink` and never
//!   draws a blank or placeholder frame for it;
//! - any other non-bound placement (`Unavailable` facts or sampler, a kernel
//!   `Refused`) refuses too, in placement order. A default is never invented;
//! - a still image needs no decode here (its pixels are the raster owner's);
//! - a decodable source frame is an integer in `0..=u32::MAX`. The kernel can
//!   answer a fractional blend partner for a fractional `frameCount` (the
//!   oracle's `X` rows). The need is then `InvalidSourceFrame`, never a floor,
//!   because the baseline would ask its decoder for a frame that does not exist.
//!
//! Layering: this file is in the document layer and imports nothing above it.
//! The decode and release side lives in `native_media_export_job`, which takes
//! plain `(asset id, frames)` specs and never names a document type; the caller
//! that holds both layers maps `MediaNeed::decode_frames` into those specs.
use crate::object_document::media_timing::object_media_link_join::MissingLink;
use crate::object_document::media_timing::object_media_link_plan::{
    plan_frame, Binding, BoundMedia, PlanEntry, PlanError, PlanInput, RefusedBinding,
    UnavailableBinding,
};
use crate::object_document::media_timing::object_media_timing_video::FrameSelection;
use crate::object_document::{AssetId, AssetKind};
use std::ops::RangeInclusive;

/// What one video or image placement supplies to the exported frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum MediaNeed {
    /// An image placement: no decode here, it only had to pass the link gate.
    Still {
        placement_id: String,
        asset_id: AssetId,
    },
    /// One decoded source frame.
    Single {
        placement_id: String,
        asset_id: AssetId,
        frame: u32,
    },
    /// Two source frames mixed by `weight` (the share of `to`, in 1/32 steps).
    Blend {
        placement_id: String,
        asset_id: AssetId,
        from: u32,
        to: u32,
        weight: f64,
    },
}

impl MediaNeed {
    pub(crate) fn asset_id(&self) -> &AssetId {
        match self {
            MediaNeed::Still { asset_id, .. }
            | MediaNeed::Single { asset_id, .. }
            | MediaNeed::Blend { asset_id, .. } => asset_id,
        }
    }
    pub(crate) fn placement_id(&self) -> &str {
        match self {
            MediaNeed::Still { placement_id, .. }
            | MediaNeed::Single { placement_id, .. }
            | MediaNeed::Blend { placement_id, .. } => placement_id,
        }
    }
    /// The source frames a decoder must supply, in draw order, without repeats.
    pub(crate) fn decode_frames(&self) -> Vec<u32> {
        match self {
            MediaNeed::Still { .. } => Vec::new(),
            MediaNeed::Single { frame, .. } => vec![*frame],
            MediaNeed::Blend { from, to, .. } if from == to => vec![*from],
            MediaNeed::Blend { from, to, .. } => vec![*from, *to],
        }
    }
}

/// Why a frame cannot be exported. None of these is recovered into a frame.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum NeedsRefusal {
    /// A linked image or video that cannot supply pixels now.
    MissingLink(MissingLink),
    /// A fact or sampler the provider did not answer.
    Unavailable(UnavailableBinding),
    /// A kernel or link-gate refusal for that placement.
    Refused(RefusedBinding),
    /// The whole plan cannot be derived (dangling asset, kind mismatch).
    Plan(PlanError),
    /// The kernel answered a source frame no decoder can be asked for.
    InvalidSourceFrame { placement_id: String, frame: f64 },
}

/// The first frame of a range that cannot be exported, and why.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PreflightFailure {
    pub(crate) frame: u32,
    pub(crate) refusal: NeedsRefusal,
}

/// The needs of exported (timeline) frame `frame`, in placement order.
pub(crate) fn needs_for_frame(
    input: &PlanInput<'_>,
    frame: u32,
) -> Result<Vec<MediaNeed>, NeedsRefusal> {
    let entries = plan_frame(input, f64::from(frame)).map_err(NeedsRefusal::Plan)?;
    let relevant: Vec<&PlanEntry> = entries
        .iter()
        .filter(|entry| !is_audio(input, entry))
        .collect();
    // A missing link outranks every other refusal of the frame.
    for entry in &relevant {
        if let PlanEntry::MissingLink(missing) = entry {
            return Err(NeedsRefusal::MissingLink(missing.clone()));
        }
    }
    let mut needs = Vec::with_capacity(relevant.len());
    for entry in relevant {
        match entry {
            PlanEntry::Bound(binding) => {
                if let Some(need) = need_of(binding)? {
                    needs.push(need);
                }
            }
            PlanEntry::MissingLink(missing) => {
                return Err(NeedsRefusal::MissingLink(missing.clone()))
            }
            PlanEntry::Unavailable(unavailable) => {
                return Err(NeedsRefusal::Unavailable(unavailable.clone()))
            }
            PlanEntry::Refused(refused) => return Err(NeedsRefusal::Refused(refused.clone())),
        }
    }
    Ok(needs)
}

/// Every frame of `frames` must be exportable; the first one that is not is
/// reported with its frame number. Nothing is acquired or staged here.
pub(crate) fn preflight(
    input: &PlanInput<'_>,
    frames: RangeInclusive<u32>,
) -> Result<(), PreflightFailure> {
    for frame in frames {
        needs_for_frame(input, frame).map_err(|refusal| PreflightFailure { frame, refusal })?;
    }
    Ok(())
}

fn asset_of(entry: &PlanEntry) -> &AssetId {
    match entry {
        PlanEntry::Bound(binding) => &binding.asset_id,
        PlanEntry::MissingLink(missing) => &missing.asset_id,
        PlanEntry::Unavailable(unavailable) => &unavailable.asset_id,
        PlanEntry::Refused(refused) => &refused.asset_id,
    }
}

fn is_audio(input: &PlanInput<'_>, entry: &PlanEntry) -> bool {
    input
        .inventory
        .get(asset_of(entry))
        .is_some_and(|record| record.kind() == AssetKind::Audio)
}

/// `Ok(None)` is a bound audio track: it contributes no pixels.
fn need_of(binding: &Binding) -> Result<Option<MediaNeed>, NeedsRefusal> {
    let placement_id = binding.placement_id.clone();
    let asset_id = binding.asset_id.clone();
    Ok(match &binding.media {
        BoundMedia::Still => Some(MediaNeed::Still {
            placement_id,
            asset_id,
        }),
        BoundMedia::Video(FrameSelection::Single { frame }) => {
            let frame = source_index(&binding.placement_id, *frame)?;
            Some(MediaNeed::Single {
                placement_id,
                asset_id,
                frame,
            })
        }
        BoundMedia::Video(FrameSelection::Blend(pair)) => {
            let from = source_index(&binding.placement_id, pair.from)?;
            let to = source_index(&binding.placement_id, pair.to)?;
            Some(MediaNeed::Blend {
                placement_id,
                asset_id,
                from,
                to,
                weight: pair.weight,
            })
        }
        BoundMedia::Audio(_) => None,
    })
}

/// A decodable index: a finite integer in `0..=u32::MAX` (`-0` is `0`).
fn source_index(placement_id: &str, frame: f64) -> Result<u32, NeedsRefusal> {
    if frame.is_finite() && frame >= 0.0 && frame.fract() == 0.0 && frame <= f64::from(u32::MAX) {
        Ok(frame as u32)
    } else {
        Err(NeedsRefusal::InvalidSourceFrame {
            placement_id: placement_id.to_owned(),
            frame,
        })
    }
}

#[cfg(test)]
#[path = "../tests/object_media_link_needs_cases.rs"]
mod cases;
#[cfg(test)]
#[path = "../tests/object_media_link_needs_controls.rs"]
mod controls;
/// `pub(crate)`: the export-job tests reuse its builders (they live in another layer's tests).
#[cfg(test)]
#[path = "../tests/object_media_link_needs.rs"]
pub(crate) mod tests;
