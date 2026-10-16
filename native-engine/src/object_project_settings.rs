//! Sealed project settings edits through the existing revision owner.
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, ProjectMetadata};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode;
use serde::Deserialize;
use serde_json::Number;

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum SettingsAction {
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
        if bytes.len() > 4096 {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let payload: SettingsPayload =
            serde_json::from_slice(bytes).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        if payload.command != "object.project.settings.edit" {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let origin = self.current_snapshot();
        let before = *origin
            .project()
            .ok_or(DispatchErrorCode::Unavailable)?
            .project();
        let metadata = match payload.action {
            SettingsAction::SetCanvasSize { width, height } => {
                before.with_canvas_dimensions(width, height)
            }
            // The UI setter clamps this command; storage keeps positive fractional FPS.
            SettingsAction::SetFps { fps } => before.with_fps(finite(fps)?.clamp(1., 60.)),
            SettingsAction::SetCanvasBackground { color } => {
                before.with_canvas_background(Some(&color))
            }
            SettingsAction::SetWorkArea {
                in_frame,
                out_frame,
            } => before.with_work_area(Some(finite(in_frame)?), Some(finite(out_frame)?)),
        }
        .map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let changed = metadata != before;
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
