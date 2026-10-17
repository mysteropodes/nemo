//! Native project wire spelling of asset placements (G42, I16b).
//!
//! The v2 catalog wire carries `assetInventory.placements`. Before this module
//! the array had to be empty; now each entry is
//!
//! ```text
//! {"id":P,"assetId":A,"target":T,"offsetFrames":N}
//!   T = {"kind":"drawing","contextId":C,"layerUid":L}            (+ "strokeId":S)
//!     | {"kind":"audioTrack","trackUid":U}
//! ```
//!
//! Every member is required except `strokeId`, whose absence is the one
//! spelling of "the whole layer" (an explicit `null` or empty string is
//! invalid). `offsetFrames` is a plain integer token: the baseline collapses an
//! absent or `null` offset to `0` when it saves (`t.offsetFrames||0`), the
//! native wire keeps absent, `null` and explicit `0` distinct and only the
//! last one is a value. A `text` target (font placements) is `Unsupported`, any
//! other kind is `Invalid`.
//!
//! [`read`] turns the already bounded array into `AssetPlacement` values (the
//! catalog constructor validates ids and the +-(2^53 - 1) offset bound) and
//! [`validate`] checks them against the catalog's records BEFORE any binary is
//! allocated; [`outputs`] is the deterministic writer, in placement order.
//! Whether a target EXISTS is the owner's question (`check_inventory`), not the
//! codec's. No owner, file or clock is involved.

use crate::object_codec::project::{object, text, ProjectWireError};
use crate::object_document::media_timing::object_media_link_edit::target_accepts;
pub(crate) use crate::object_document::media_timing::object_media_link_edit::MAX_PLACEMENTS;
use crate::object_document::{AssetId, AssetInventory, AssetKind, AssetPlacement, AssetTarget};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::value::RawValue;
use std::collections::HashSet;

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PlacementIn<'a> {
    #[serde(borrow)]
    id: &'a RawValue,
    #[serde(borrow)]
    asset_id: &'a RawValue,
    #[serde(borrow)]
    target: &'a RawValue,
    offset_frames: i64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct DrawingIn<'a> {
    #[serde(borrow)]
    kind: &'a RawValue,
    #[serde(borrow)]
    context_id: &'a RawValue,
    #[serde(borrow)]
    layer_uid: &'a RawValue,
    #[serde(borrow, default, deserialize_with = "present")]
    stroke_id: Option<&'a RawValue>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct AudioIn<'a> {
    #[serde(borrow)]
    kind: &'a RawValue,
    #[serde(borrow)]
    track_uid: &'a RawValue,
}

/// Runs only when the member is present, and reads it as raw JSON, so an
/// explicit `null` stays distinguishable from an absent member.
fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<&'de RawValue>, D::Error> {
    <&'de RawValue>::deserialize(deserializer).map(Some)
}

fn target_of(raw: &RawValue) -> Result<AssetTarget, ProjectWireError> {
    // Disjoint strict shapes: a drawing object has members the audio shape
    // forbids and the reverse, so trying both never admits an unknown member.
    if let Ok(audio) = object::<AudioIn<'_>>(raw) {
        if text(audio.kind, 16)? != "audioTrack" {
            return Err(ProjectWireError::Invalid);
        }
        return Ok(AssetTarget::AudioTrack {
            track_uid: text(audio.track_uid, 255)?,
        });
    }
    let drawing: DrawingIn<'_> = object(raw)?;
    match text(drawing.kind, 16)?.as_str() {
        "drawing" => {}
        "text" => return Err(ProjectWireError::Unsupported),
        _ => return Err(ProjectWireError::Invalid),
    }
    let stroke_id = match drawing.stroke_id {
        Some(stroke) => Some(text(stroke, 255)?),
        None => None,
    };
    Ok(AssetTarget::Drawing {
        context_id: text(drawing.context_id, 255)?,
        layer_uid: text(drawing.layer_uid, 255)?,
        stroke_id,
    })
}

/// Placements of an already bounded `placements` array.
pub(crate) fn read(items: Vec<&RawValue>) -> Result<Vec<AssetPlacement>, ProjectWireError> {
    let mut placements = Vec::new();
    placements
        .try_reserve_exact(items.len())
        .map_err(|_| ProjectWireError::Budget)?;
    for raw in items {
        let wire: PlacementIn<'_> = object(raw)?;
        let id = text(wire.id, 255)?;
        let asset_id =
            AssetId::new(text(wire.asset_id, 255)?).map_err(|_| ProjectWireError::Invalid)?;
        let target = target_of(wire.target)?;
        placements.push(
            AssetPlacement::new(id, asset_id, target, wire.offset_frames)
                .map_err(|_| ProjectWireError::Invalid)?,
        );
    }
    Ok(placements)
}

/// The catalog's own cross-checks (known asset, accepted kind, unique id and
/// target), run on the wire values so a bad table fails before the first
/// binary allocation. `kind_of` answers for the asset records of the file.
pub(crate) fn validate(
    placements: &[AssetPlacement],
    kind_of: impl Fn(&str) -> Option<AssetKind>,
) -> Result<(), ProjectWireError> {
    let mut ids = HashSet::new();
    let mut targets = HashSet::new();
    for placement in placements {
        let kind = kind_of(placement.asset_id().as_str()).ok_or(ProjectWireError::Invalid)?;
        if !target_accepts(placement.target(), kind)
            || !ids.insert(placement.id())
            || !targets.insert(placement.target())
        {
            return Err(ProjectWireError::Invalid);
        }
    }
    Ok(())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PlacementOutput<'a> {
    id: &'a str,
    asset_id: &'a str,
    target: TargetOutput<'a>,
    offset_frames: i64,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum TargetOutput<'a> {
    #[serde(rename_all = "camelCase")]
    Drawing {
        context_id: &'a str,
        layer_uid: &'a str,
        #[serde(skip_serializing_if = "Option::is_none")]
        stroke_id: Option<&'a str>,
    },
    #[serde(rename_all = "camelCase")]
    AudioTrack { track_uid: &'a str },
}

/// The wire form of every placement, in placement order. A font (`text`)
/// placement has no wire spelling here and is `Unsupported`.
pub(crate) fn outputs(
    inventory: &AssetInventory,
) -> Result<Vec<PlacementOutput<'_>>, ProjectWireError> {
    let placements = inventory.placements();
    if placements.len() > MAX_PLACEMENTS {
        return Err(ProjectWireError::Budget);
    }
    let mut out = Vec::new();
    out.try_reserve_exact(placements.len())
        .map_err(|_| ProjectWireError::Budget)?;
    for placement in placements {
        let target = match placement.target() {
            AssetTarget::Drawing {
                context_id,
                layer_uid,
                stroke_id,
            } => TargetOutput::Drawing {
                context_id: context_id.as_str(),
                layer_uid: layer_uid.as_str(),
                stroke_id: stroke_id.as_deref(),
            },
            AssetTarget::AudioTrack { track_uid } => TargetOutput::AudioTrack {
                track_uid: track_uid.as_str(),
            },
            AssetTarget::Text { .. } => return Err(ProjectWireError::Unsupported),
        };
        out.push(PlacementOutput {
            id: placement.id(),
            asset_id: placement.asset_id().as_str(),
            target,
            offset_frames: placement.offset_frames(),
        });
    }
    Ok(out)
}

#[cfg(test)]
#[path = "../tests/object_media_link_track.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_media_link_track_controls.rs"]
mod controls;
