//! Sealed project settings edits through the existing revision owner.
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, Presence, ProjectMetadata};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;

#[path = "object_project_composition_edit.rs"]
mod composition_delta;
#[path = "object_project_guide_edit.rs"]
mod guide_delta;
#[path = "object_project_palette_edit.rs"]
mod palette_delta;
#[path = "object_project_shadow_edit.rs"]
mod shadow_delta;

#[derive(Debug)]
struct CompositionMarkerInput {
    frame: f64,
    name: Presence<String>,
    color: Presence<String>,
}
object_deserialize!(CompositionMarkerInput {
    frame: f64,
    #[serde(default)] name: Presence<String>,
    #[serde(default)] color: Presence<String>
});
#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum SettingsAction {
    SetCompositionMetadata {
        #[serde(default)]
        markers: Presence<Vec<CompositionMarkerInput>>,
        #[serde(default)]
        bpm: Presence<f64>,
        #[serde(default, rename = "bpmOffset")]
        bpm_offset: Presence<f64>,
        #[serde(default, rename = "bpmShow")]
        bpm_show: Presence<bool>,
    },
    SetCanvasSize {
        width: u32,
        height: u32,
    },
    SetFps {
        fps: Number,
    },
    SetCanvasBackground {
        color: String,
    },
    SetWorkArea {
        #[serde(rename = "inFrame")]
        in_frame: Number,
        #[serde(rename = "outFrame")]
        out_frame: Number,
    },
}
#[derive(Debug)]
struct SettingsPayload {
    command: String,
    action: SettingsAction,
}
object_deserialize!(SettingsPayload {
    command: String,
    action: SettingsAction
});

/// Only this module can create the candidate or replace its origin.
#[derive(Debug, Clone)]
pub(crate) struct PreparedProjectSettings {
    origin: ObjectSnapshot,
    metadata: ProjectMetadata,
    changed: bool,
}
fn finite(value: Number) -> Result<f64, DispatchErrorCode> {
    value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or(DispatchErrorCode::InvalidRequest)
}
impl ObjectFillOwner {
    pub(crate) fn prepare_project_settings_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedProjectSettings, DispatchErrorCode> {
        self.prepare_settings_json(bytes, false)
    }
    /// Private provider entry; the public settings dispatcher cannot select it.
    pub(crate) fn prepare_composition_metadata_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedProjectSettings, DispatchErrorCode> {
        #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
        {
            let _ = bytes;
            Err(DispatchErrorCode::Unavailable)
        }
        #[cfg(all(feature = "history", feature = "project_media_codec"))]
        {
            self.prepare_settings_json(bytes, true)
        }
    }
    fn prepare_settings_json(
        &self,
        bytes: &[u8],
        composition: bool,
    ) -> Result<PreparedProjectSettings, DispatchErrorCode> {
        if bytes.len() > 4096 {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        if composition {
            let shape: serde_json::Value =
                serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
            if !shape
                .get("action")
                .is_some_and(serde_json::Value::is_object)
            {
                return Err(DispatchErrorCode::InvalidRequest);
            }
        }
        let payload: SettingsPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let command = if composition {
            "object.project.composition.edit"
        } else {
            "object.project.settings.edit"
        };
        if payload.command != command
            || matches!(
                &payload.action,
                SettingsAction::SetCompositionMetadata { .. }
            ) != composition
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let before = origin
            .project()
            .ok_or(DispatchErrorCode::Unavailable)?
            .project()
            .clone();
        let metadata = match payload.action {
            SettingsAction::SetCompositionMetadata {
                markers,
                bpm,
                bpm_offset,
                bpm_show,
            } => before.clone().with_composition_metadata(
                markers.value().map(|values| {
                    values
                        .iter()
                        .map(|m| (m.frame, m.name.value().cloned(), m.color.value().cloned()))
                        .collect()
                }),
                bpm.value().copied(),
                bpm_offset.value().copied(),
                bpm_show.value().copied(),
            ),
            SettingsAction::SetCanvasSize { width, height } => {
                before.clone().with_canvas_dimensions(width, height)
            }
            // The UI setter clamps this command; storage keeps positive fractional FPS.
            SettingsAction::SetFps { fps } => before.clone().with_fps(finite(fps)?.clamp(1., 60.)),
            SettingsAction::SetCanvasBackground { color } => {
                before.clone().with_canvas_background(Some(&color))
            }
            SettingsAction::SetWorkArea {
                in_frame,
                out_frame,
            } => before
                .clone()
                .with_work_area(Some(finite(in_frame)?), Some(finite(out_frame)?)),
        }
        .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let changed = metadata != before;
        if composition && changed {
            #[cfg(all(feature = "history", feature = "project_media_codec"))]
            {
                let project = origin.project().ok_or(DispatchErrorCode::Unavailable)?;
                ObjectSnapshot::check_candidate_saveable(&project.with_metadata(metadata.clone()))?;
            }
            #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
            return Err(DispatchErrorCode::Unavailable);
        }
        Ok(PreparedProjectSettings {
            origin,
            metadata,
            changed,
        })
    }
    pub(crate) fn commit_project_settings(
        &mut self,
        candidate: PreparedProjectSettings,
    ) -> Result<u64, DispatchErrorCode> {
        let current = self.current_snapshot();
        if candidate.origin.instance_id() != current.instance_id() {
            return Err(DispatchErrorCode::WrongInstance);
        }
        if candidate.origin.document_id() != current.document_id() {
            return Err(DispatchErrorCode::WrongDocument);
        }
        if candidate.origin.snapshot_id() != current.snapshot_id()
            || candidate.origin.content_revision() != current.content_revision()
        {
            return Err(DispatchErrorCode::StaleRevision);
        }
        if !candidate.changed {
            return Ok(current.content_revision());
        }
        self.revisions
            .publish_project_settings(candidate.metadata)
            .map_err(|_| DispatchErrorCode::Unavailable)
    }
}
