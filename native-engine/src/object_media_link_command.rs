//! Staged placement commands through the existing history and request-receipt owner.
//! The parent dispatcher performs envelope, owner, replay, cancel and revision checks.
//! Audio requires its actual persisted record and sealed source; absent owners still refuse.
use crate::history::NativeObjectHistory;
use crate::object_document::media_timing::object_media_link_edit::PlacementEdit;
use crate::object_document::placement_offset::RemapIntent;
use crate::object_document::{AssetTarget, OrderCurvePoint};
use crate::request_receipts::DispatchErrorCode;
use crate::transaction::CompactResult;
use serde::{Deserialize, Deserializer};

pub(super) const OP_PLACEMENT_EDIT: &str = "command.document.object.project.placement.edit";

#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum EditIn {
    #[serde(rename = "place")]
    Place { placement: PlacementIn },
    #[serde(rename = "setOffset")]
    SetOffset {
        #[serde(rename = "placementId")]
        placement_id: String,
        #[serde(rename = "offsetFrames")]
        offset_frames: serde_json::Number,
    },
    #[serde(rename = "shiftOffset")]
    ShiftOffset {
        #[serde(rename = "placementId")]
        placement_id: String,
        #[serde(rename = "deltaFrames")]
        delta_frames: serde_json::Number,
    },
    #[serde(rename = "remove")]
    Remove {
        #[serde(rename = "placementId")]
        placement_id: String,
    },
    // T12 bounded remap intents; each fits the 4 KiB request budget.
    #[serde(rename = "enableTimeRemap")]
    EnableTimeRemap {
        #[serde(rename = "placementId")]
        placement_id: String,
        #[serde(rename = "inFrame")]
        in_frame: serde_json::Number,
        #[serde(rename = "outFrame")]
        out_frame: serde_json::Number,
        #[serde(rename = "lastSourceFrame")]
        last_source_frame: serde_json::Number,
    },
    #[serde(rename = "disableTimeRemap")]
    DisableTimeRemap {
        #[serde(rename = "placementId")]
        placement_id: String,
    },
    #[serde(rename = "removeTimeRemapKey")]
    RemoveTimeRemapKey {
        #[serde(rename = "placementId")]
        placement_id: String,
        frame: serde_json::Number,
    },
    #[serde(rename = "setTimeRemapKey")]
    SetTimeRemapKey {
        #[serde(rename = "placementId")]
        placement_id: String,
        frame: serde_json::Number,
        value: serde_json::Number,
        #[serde(rename = "curvePoints", default)]
        curve: Option<Vec<OrderCurvePoint>>,
    },
    #[serde(rename = "setInterpMode")]
    SetInterpMode {
        #[serde(rename = "placementId")]
        placement_id: String,
        #[serde(rename = "interpMode")]
        interp_mode: String,
    },
}

// Same required persisted placement fields; no absent/null offset coercion.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct PlacementIn {
    id: String,
    asset_id: String,
    target: TargetIn,
    offset_frames: serde_json::Number,
}

#[derive(Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum TargetIn {
    #[serde(rename = "drawing")]
    Drawing {
        #[serde(rename = "contextId")]
        context_id: String,
        #[serde(rename = "layerUid")]
        layer_uid: String,
        #[serde(rename = "strokeId", default, deserialize_with = "present_id")]
        stroke_id: Option<String>,
    },
    #[serde(rename = "audioTrack")]
    AudioTrack {
        #[serde(rename = "trackUid")]
        track_uid: String,
    },
}

// Absent is a whole-layer placement; explicit null is not an absent member.
fn present_id<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

fn remap(placement_id: String, intent: RemapIntent) -> PlacementEdit {
    PlacementEdit::TimeRemap {
        placement_id,
        intent,
    }
}

fn parse_edit(bytes: &[u8]) -> Result<PlacementEdit, DispatchErrorCode> {
    let input: EditIn =
        serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
    Ok(match input {
        EditIn::Place { placement } => PlacementEdit::Place {
            placement_id: placement.id,
            asset_id: placement.asset_id,
            target: match placement.target {
                TargetIn::Drawing {
                    context_id,
                    layer_uid,
                    stroke_id,
                } => AssetTarget::Drawing {
                    context_id,
                    layer_uid,
                    stroke_id,
                },
                TargetIn::AudioTrack { track_uid } => AssetTarget::AudioTrack { track_uid },
            },
            offset_frames: placement.offset_frames,
        },
        EditIn::SetOffset {
            placement_id,
            offset_frames,
        } => PlacementEdit::SetOffset {
            placement_id,
            offset_frames,
        },
        EditIn::ShiftOffset {
            placement_id,
            delta_frames,
        } => PlacementEdit::ShiftOffset {
            placement_id,
            delta_frames,
        },
        EditIn::Remove { placement_id } => PlacementEdit::Remove { placement_id },
        EditIn::EnableTimeRemap {
            placement_id,
            in_frame,
            out_frame,
            last_source_frame,
        } => remap(
            placement_id,
            RemapIntent::Enable {
                in_frame,
                out_frame,
                last_source_frame,
            },
        ),
        EditIn::DisableTimeRemap { placement_id } => remap(placement_id, RemapIntent::Disable),
        EditIn::RemoveTimeRemapKey {
            placement_id,
            frame,
        } => remap(placement_id, RemapIntent::RemoveKey { frame }),
        EditIn::SetTimeRemapKey {
            placement_id,
            frame,
            value,
            curve,
        } => remap(
            placement_id,
            RemapIntent::SetKey {
                frame,
                value,
                curve,
            },
        ),
        EditIn::SetInterpMode {
            placement_id,
            interp_mode,
        } => PlacementEdit::SetInterpMode {
            placement_id,
            interp_mode,
        },
    })
}

impl NativeObjectHistory {
    /// Called only after the common dispatcher checks and receipt lookup. It neither
    /// creates another receipt store nor reconstructs a candidate after publication.
    pub(super) fn dispatch_placement_edit(
        &mut self,
        payload: &serde_json::Value,
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        const REFUSED: &str = "Placement edit payload or target is invalid or unavailable.";
        let bytes = serde_json::to_vec(payload)
            .map_err(|_| (DispatchErrorCode::InvalidRequest, REFUSED))?;
        let edit = parse_edit(&bytes).map_err(|code| (code, REFUSED))?;
        let candidate = self
            .prepare_placement_edit(edit)
            .map_err(|code| (code, REFUSED))?;
        let before = self.content_revision();
        self.commit_placement_edit(candidate)
            .map(|revision| CompactResult::Command {
                applied: revision != before,
                history_entries_added: u64::from(revision != before),
            })
            .map_err(|code| (code, "Placement edit could not commit."))
    }
}

#[cfg(all(test, feature = "project_media_codec"))]
#[path = "../tests/object_media_link_command.rs"]
mod tests;

#[cfg(all(test, feature = "project_media_codec"))]
#[path = "../tests/object_media_link_command_timing.rs"]
mod timing_tests;
