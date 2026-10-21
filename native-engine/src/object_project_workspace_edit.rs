//! Explicit original workspace capture on the existing sealed settings/history owner.
use crate::commands::object_fill_owner::project_settings::PreparedProjectSettings;
use crate::commands::ObjectFillOwner;
use crate::object_document::{object_deserialize, ProjectWorkspaceRecord};
use crate::object_snapshot::ObjectSnapshot;
use crate::request_receipts::DispatchErrorCode as Error;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Action {
    Capture { workspace: ProjectWorkspaceRecord },
}
struct Payload {
    command: String,
    action: Action,
}
object_deserialize!(Payload {
    command: String,
    action: Action
});
fn complete_capture(workspace: &ProjectWorkspaceRecord) -> bool {
    !workspace.layer_panel_width.missing()
        && !workspace.tools_panel_width.missing()
        && !workspace.props_panel_width.missing()
        && !workspace.props_panel_collapsed.missing()
        && !workspace.timeline_height.missing()
        && !workspace.panel_order.missing()
    // Original capture() may omit motionColumns if localStorage throws.
}
impl ObjectFillOwner {
    pub(crate) fn prepare_workspace_capture_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedProjectSettings, Error> {
        #[cfg(not(all(feature = "history", feature = "project_media_codec")))]
        {
            let _ = bytes;
            Err(Error::Unavailable)
        }
        #[cfg(all(feature = "history", feature = "project_media_codec"))]
        {
            if bytes.len() > 4096 {
                return Err(Error::InvalidRequest);
            }
            let shape: serde_json::Value =
                serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
            if !shape
                .get("action")
                .is_some_and(serde_json::Value::is_object)
            {
                return Err(Error::InvalidRequest);
            }
            let payload: Payload =
                serde_json::from_slice(bytes).map_err(|_| Error::InvalidRequest)?;
            if payload.command != "object.project.workspace.edit" {
                return Err(Error::InvalidRequest);
            }
            let Action::Capture { workspace } = payload.action;
            if !complete_capture(&workspace) {
                return Err(Error::InvalidRequest);
            }
            let origin = self.current_snapshot();
            let project = origin.project().ok_or(Error::Unavailable)?;
            let before = project.project();
            let metadata = before.clone().with_workspace_metadata(workspace);
            let changed = &metadata != before;
            if changed {
                ObjectSnapshot::check_candidate_saveable(&project.with_metadata(metadata.clone()))?;
            }
            Ok(PreparedProjectSettings {
                origin,
                metadata,
                changed,
            })
        }
    }
}
