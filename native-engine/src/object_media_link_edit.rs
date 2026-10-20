//! Placement edit kernel and target admission for the asset catalog (G42, I16b).
//!
//! A placement binds one catalog asset to one target (`AssetPlacement`). The
//! catalog values are immutable; an edit therefore builds the NEXT
//! `AssetInventory` from the current one and leaves the current one untouched,
//! so every older pinned snapshot keeps its own inventory. The records are
//! shared (`Arc<AssetRecord>` clones), never re-encoded: a placement edit
//! cannot change a source or an encoded allocation.
//!
//! [`check_placement`] is the owner join: a placement is admitted only when its
//! asset kind has an owner today and its target exists in the document that
//! carries the catalog. Targets resolve exactly like property holders do
//! (`validate_property_holders`): the context is a context of the order plan
//! (`scene-root` when there is none), the layer is a document layer and an
//! element is an authored record of that context and layer.
//!
//! `Video` and `Image` assets on a drawing are admitted. An image resolves its
//! element through `image_element_exists` (lane 6: an authored path record or
//! raster item). Everything else keeps the typed refusal the codec had before
//! placements existed, and names the owner that has to lift it: `Audio` (a
//! native audio-track record does not exist yet) and `Font` (retained fonts).
//! No decode, file access, clock or history lives here.
//!
//! The relative edit [`PlacementEdit::ShiftOffset`] is the audio-track drag of
//! `src/js/audio-bridge.js` (`startOffset + Math.round(dx / FC)`) in whole
//! frames; like the catalog it refuses a result beyond +-(2^53 - 1) instead of
//! rounding it the way a JS double would.
//!
//! T12: the bounded remap intents (enable, disable, remove key) and
//! [`PlacementEdit::SetInterpMode`] own a video placement's stored timing,
//! each within the 4 KiB request budget; every rebuilt placement keeps it.

use crate::object_document::object_raster_asset_kernels::object_raster_asset_placement::image_element_exists;
use crate::object_document::placement_offset::VideoTiming;
use crate::object_document::{
    AssetId, AssetInventory, AssetKind, AssetPlacement, AssetTarget, FrameScopeKind,
    ObjectDocument, OrderTrack,
};

/// Most catalog records the project wire admits.
pub(crate) const MAX_ASSET_RECORDS: usize = 1024;
/// Most placements the project wire admits.
pub(crate) const MAX_PLACEMENTS: usize = 1024;
/// `AssetPlacement::new` bound: `|offsetFrames| <= 2^53 - 1`.

/// Why a placement cannot enter the owner (typed; the message is stable).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlacementRefusal {
    /// The asset id is not in the catalog (an inventory cannot hold one).
    Asset,
    /// A drawing target of an asset kind that is neither Image nor Video (the catalog constructor never builds one).
    ImageOwner,
    /// Audio placements need a native audio-track record; none exists.
    AudioTrackOwner,
    AudioTrackTarget,
    /// Font placements belong to the retained-font owner.
    FontOwner,
    /// The context is not a context of the document.
    Context,
    /// The layer is not a layer of the document.
    Layer,
    /// The element is not an authored record of that context and layer.
    Element,
}

impl PlacementRefusal {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::Asset => "asset placement asset unresolved",
            Self::ImageOwner => "asset image placement unavailable",
            Self::AudioTrackOwner => "asset audio track unavailable",
            Self::AudioTrackTarget => "asset audio track unresolved",
            Self::FontOwner => "asset font placement unavailable",
            Self::Context => "asset placement context unresolved",
            Self::Layer => "asset placement layer unresolved",
            Self::Element => "asset placement element unresolved",
        }
    }

    /// True when the target is absent from the document (a lookup failure),
    /// false when the asset kind simply has no owner yet.
    pub(crate) fn unresolved(self) -> bool {
        matches!(
            self,
            Self::Asset | Self::AudioTrackTarget | Self::Context | Self::Layer | Self::Element
        )
    }
}

fn has_element(document: &ObjectDocument, context_id: &str, layer_uid: &str, stroke: &str) -> bool {
    document.objects().iter().any(|record| {
        let target = record.target();
        matches!(target.scope_kind(), FrameScopeKind::Authored)
            && target.context_id() == context_id
            && target.layer_uid() == layer_uid
            && target.stroke_id() == stroke
    })
}

fn resolve_drawing(
    document: &ObjectDocument,
    context_id: &str,
    layer_uid: &str,
    stroke_id: Option<&str>,
) -> Result<(), PlacementRefusal> {
    let context_known = match document.composition_order.value() {
        Some(plan) => plan
            .contexts
            .iter()
            .any(|context| context.context_id == context_id),
        None => context_id == "scene-root",
    };
    if !context_known {
        return Err(PlacementRefusal::Context);
    }
    if !document
        .layers()
        .iter()
        .any(|layer| layer.layer_uid() == layer_uid)
    {
        return Err(PlacementRefusal::Layer);
    }
    match stroke_id {
        Some(stroke) if !has_element(document, context_id, layer_uid, stroke) => {
            Err(PlacementRefusal::Element)
        }
        _ => Ok(()),
    }
}

/// The owner join for ONE placement of `inventory`.
pub(crate) fn check_placement(
    document: &ObjectDocument,
    inventory: &AssetInventory,
    placement: &AssetPlacement,
) -> Result<(), PlacementRefusal> {
    let kind = inventory
        .get(placement.asset_id())
        .map(|record| record.kind())
        .ok_or(PlacementRefusal::Asset)?;
    match (placement.target(), kind) {
        (
            AssetTarget::Drawing {
                context_id,
                layer_uid,
                stroke_id,
            },
            AssetKind::Video,
        ) => resolve_drawing(
            document,
            context_id.as_str(),
            layer_uid.as_str(),
            stroke_id.as_deref(),
        ),
        (
            AssetTarget::Drawing {
                context_id,
                layer_uid,
                stroke_id,
            },
            AssetKind::Image,
        ) => {
            // Context and layer resolve exactly as for a video; an element must be an image holder.
            resolve_drawing(document, context_id.as_str(), layer_uid.as_str(), None)?;
            match stroke_id.as_deref() {
                Some(stroke) if !image_element_exists(document, context_id, layer_uid, stroke) => {
                    Err(PlacementRefusal::Element)
                }
                _ => Ok(()),
            }
        }
        (AssetTarget::Drawing { .. }, _) => Err(PlacementRefusal::ImageOwner),
        (AssetTarget::AudioTrack { track_uid }, AssetKind::Audio) => {
            let tracks = document
                .audio_tracks
                .value()
                .ok_or(PlacementRefusal::AudioTrackOwner)?;
            let track = tracks
                .get(track_uid)
                .ok_or(PlacementRefusal::AudioTrackTarget)?;
            if track.asset_id != placement.asset_id().as_str() {
                return Err(PlacementRefusal::Asset);
            }
            let source = inventory
                .get(placement.asset_id())
                .ok_or(PlacementRefusal::Asset)?
                .source();
            if !source.has_audio_admission() {
                return Err(PlacementRefusal::AudioTrackOwner);
            }
            Ok(())
        }
        (AssetTarget::AudioTrack { .. }, _) => Err(PlacementRefusal::AudioTrackOwner),
        (AssetTarget::Text { .. }, _) => Err(PlacementRefusal::FontOwner),
    }
}

/// The owner join for every placement, in placement order; the first refusal wins.
pub(crate) fn check_inventory(
    document: &ObjectDocument,
    inventory: &AssetInventory,
) -> Result<(), PlacementRefusal> {
    if let Some(tracks) = document.audio_tracks.value() {
        for track in tracks.records() {
            if inventory
                .records()
                .iter()
                .find(|r| r.id().as_str() == track.asset_id)
                .is_none_or(|r| r.kind() != AssetKind::Audio)
            {
                return Err(PlacementRefusal::Asset);
            }
        }
    }
    for placement in inventory.placements() {
        check_placement(document, inventory, placement)?;
    }
    Ok(())
}

/// The target/kind pairs the catalog constructor accepts
/// (`AssetTarget::accepts`, private to the catalog; a control test keeps the
/// two in agreement).
pub(crate) fn target_accepts(target: &AssetTarget, kind: AssetKind) -> bool {
    matches!(
        (target, kind),
        (
            AssetTarget::Drawing { .. },
            AssetKind::Image | AssetKind::Video
        ) | (AssetTarget::AudioTrack { .. }, AssetKind::Audio)
            | (AssetTarget::Text { .. }, AssetKind::Font)
    )
}

/// One typed edit of the placement table. Raw fields in; the kernel validates.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum PlacementEdit {
    /// Add a placement. The identical placement again is a no-op.
    Place {
        placement_id: String,
        asset_id: String,
        target: AssetTarget,
        offset_frames: serde_json::Number,
    },
    /// Set the offset to an absolute number of frames.
    SetOffset {
        placement_id: String,
        offset_frames: serde_json::Number,
    },
    /// Add a signed number of frames to the offset (the timeline drag).
    ShiftOffset {
        placement_id: String,
        delta_frames: serde_json::Number,
    },
    /// Remove a placement; the catalog records stay.
    Remove { placement_id: String },
    /// T12 `enableTimeRemap` (motion.js): keys `{in, v:[0]}` and
    /// `{max(in + 1, out), v:[last]}`, linear; refused when one exists.
    EnableTimeRemap {
        placement_id: String,
        in_frame: serde_json::Number,
        out_frame: serde_json::Number,
        last_source_frame: serde_json::Number,
    },
    /// T12 `disableTimeRemap`: `delete ld.timeRemap`.
    DisableTimeRemap { placement_id: String },
    /// T12: remove the remap key at `frame` (the property `removeKey` rule).
    RemoveTimeRemapKey {
        placement_id: String,
        frame: serde_json::Number,
    },
    /// T12: the footage panel's mode selector (timeline.js), which writes
    /// `interpMode` and `frameBlend = interpMode !== 'none'` together.
    SetInterpMode {
        placement_id: String,
        interp_mode: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PlacementEditError {
    /// A field fails the catalog constructor (blank id, offset beyond 2^53 - 1, ...).
    Invalid,
    UnknownPlacement,
    UnknownAsset,
    /// The target does not accept the asset kind.
    KindMismatch,
    /// The placement id or the target is already used by a different placement.
    Conflict,
    /// A shifted offset leaves +-(2^53 - 1).
    Overflow,
    Limit,
    Refused(PlacementRefusal),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum PlacementOutcome {
    /// The edit asks for what is already there: nothing to publish.
    Unchanged,
    Changed(AssetInventory),
}

fn position(placements: &[AssetPlacement], id: &str) -> Result<usize, PlacementEditError> {
    placements
        .iter()
        .position(|placement| placement.id() == id)
        .ok_or(PlacementEditError::UnknownPlacement)
}

fn with_offset(
    placement: &AssetPlacement,
    offset_frames: serde_json::Number,
) -> Result<AssetPlacement, PlacementEditError> {
    // The stored timing travels with every rebuilt placement.
    AssetPlacement::new_number(
        placement.id().to_owned(),
        placement.asset_id().clone(),
        placement.target().clone(),
        offset_frames,
    )
    .and_then(|next| next.with_timing(placement.timing().clone()))
    .map_err(|_| PlacementEditError::Invalid)
}

/// Replace the timing of the video placement at `at`; `Ok(false)` when it is the same.
fn replace_timing(
    inventory: &AssetInventory,
    placements: &mut [AssetPlacement],
    at: usize,
    edit: impl FnOnce(&mut VideoTiming) -> Result<(), PlacementEditError>,
) -> Result<bool, PlacementEditError> {
    let kind = inventory.get(placements[at].asset_id()).map(|r| r.kind());
    if kind != Some(AssetKind::Video) {
        return Err(PlacementEditError::KindMismatch);
    }
    let mut timing = placements[at].timing().clone();
    edit(&mut timing)?;
    let next = placements[at]
        .clone()
        .with_timing(timing)
        .map_err(|_| PlacementEditError::Invalid)?;
    if next == placements[at] {
        return Ok(false);
    }
    placements[at] = next;
    Ok(true)
}

/// Replace the offset of the placement at `at`; `Ok(false)` when it is the same.
fn replace_offset(
    placements: &mut [AssetPlacement],
    at: usize,
    offset_frames: serde_json::Number,
) -> Result<bool, PlacementEditError> {
    let next = with_offset(&placements[at], offset_frames)?;
    if next == placements[at] {
        return Ok(false);
    }
    placements[at] = next;
    Ok(true)
}

/// Apply one edit to `inventory` for the document that owns its targets.
/// The inputs are never changed; a refusal leaves no partial value.
pub(crate) fn apply_edit(
    inventory: &AssetInventory,
    document: &ObjectDocument,
    edit: PlacementEdit,
) -> Result<PlacementOutcome, PlacementEditError> {
    let mut placements: Vec<AssetPlacement> = inventory.placements().to_vec();
    match edit {
        PlacementEdit::Place {
            placement_id,
            asset_id,
            target,
            offset_frames,
        } => {
            let asset_id = AssetId::new(asset_id).map_err(|_| PlacementEditError::Invalid)?;
            let candidate =
                AssetPlacement::new_number(placement_id, asset_id, target, offset_frames)
                    .map_err(|_| PlacementEditError::Invalid)?;
            if let Some(existing) = placements.iter().find(|p| p.id() == candidate.id()) {
                return if *existing == candidate {
                    Ok(PlacementOutcome::Unchanged)
                } else {
                    Err(PlacementEditError::Conflict)
                };
            }
            let record = inventory
                .get(candidate.asset_id())
                .ok_or(PlacementEditError::UnknownAsset)?;
            if !target_accepts(candidate.target(), record.kind()) {
                return Err(PlacementEditError::KindMismatch);
            }
            if placements.iter().any(|p| p.target() == candidate.target()) {
                return Err(PlacementEditError::Conflict);
            }
            if placements.len() >= MAX_PLACEMENTS {
                return Err(PlacementEditError::Limit);
            }
            check_placement(document, inventory, &candidate)
                .map_err(PlacementEditError::Refused)?;
            placements.push(candidate);
        }
        PlacementEdit::SetOffset {
            placement_id,
            offset_frames,
        } => {
            let at = position(&placements, &placement_id)?;
            if !replace_offset(&mut placements, at, offset_frames)? {
                return Ok(PlacementOutcome::Unchanged);
            }
        }
        PlacementEdit::ShiftOffset {
            placement_id,
            delta_frames,
        } => {
            let at = position(&placements, &placement_id)?;
            let sum = crate::object_document::placement_offset::shifted(
                placements[at].target(),
                placements[at].offset_frames(),
                &delta_frames,
            )
            .ok_or(PlacementEditError::Overflow)?;
            if !replace_offset(&mut placements, at, sum)? {
                return Ok(PlacementOutcome::Unchanged);
            }
        }
        PlacementEdit::Remove { placement_id } => {
            let at = position(&placements, &placement_id)?;
            placements.remove(at);
        }
        PlacementEdit::EnableTimeRemap {
            placement_id,
            in_frame,
            out_frame,
            last_source_frame,
        } => {
            let at = position(&placements, &placement_id)?;
            let track = VideoTiming::enabled_track(&in_frame, &out_frame, &last_source_frame)
                .ok_or(PlacementEditError::Invalid)?;
            let set = |timing: &mut VideoTiming| match timing.time_remap {
                Some(_) => Err(PlacementEditError::Conflict),
                None => Ok(timing.time_remap = Some(track)),
            };
            replace_timing(inventory, &mut placements, at, set)?;
        }
        PlacementEdit::DisableTimeRemap { placement_id } => {
            let at = position(&placements, &placement_id)?;
            let set = |timing: &mut VideoTiming| Ok(timing.time_remap = None);
            if !replace_timing(inventory, &mut placements, at, set)? {
                return Ok(PlacementOutcome::Unchanged);
            }
        }
        PlacementEdit::RemoveTimeRemapKey {
            placement_id,
            frame,
        } => {
            let at = position(&placements, &placement_id)?;
            let frame = frame.as_f64().filter(|f| f.is_finite());
            let set = |timing: &mut VideoTiming| {
                let (Some(track), Some(at)) = (timing.time_remap.as_mut(), frame) else {
                    return Err(PlacementEditError::Invalid);
                };
                Ok(track.keys.retain(|key| key.frame.as_f64() != Some(at)))
            };
            if !replace_timing(inventory, &mut placements, at, set)? {
                return Ok(PlacementOutcome::Unchanged);
            }
        }
        PlacementEdit::SetInterpMode {
            placement_id,
            interp_mode,
        } => {
            let at = position(&placements, &placement_id)?;
            let set = |timing: &mut VideoTiming| {
                timing.frame_blend = Some(interp_mode != "none");
                timing.interp_mode = Some(interp_mode);
                Ok(())
            };
            if !replace_timing(inventory, &mut placements, at, set)? {
                return Ok(PlacementOutcome::Unchanged);
            }
        }
    }
    AssetInventory::new(
        inventory.records().to_vec(),
        placements,
        MAX_ASSET_RECORDS,
        MAX_PLACEMENTS,
    )
    .map(PlacementOutcome::Changed)
    .map_err(|_| PlacementEditError::Invalid)
}

#[cfg(test)]
#[path = "../tests/object_media_link_edit.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_media_link_edit_check.rs"]
mod check_tests;

#[cfg(test)]
#[path = "../tests/object_media_link_edit_timing.rs"]
mod timing_tests;
