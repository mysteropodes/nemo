//! Project-saved UI workspace and comment attribution. Local identity/preferences
//! are not copied here. Values publish only with the enclosing native revision.
use super::{ProjectMetadata, ProjectMetadataError};
use crate::object_document::{object_deserialize, Presence};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectWorkspaceRecord {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) layer_panel_width: Presence<Option<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) tools_panel_width: Presence<Option<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) props_panel_width: Presence<Option<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) props_panel_collapsed: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) timeline_height: Presence<Option<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) panel_order: Presence<Vec<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) motion_columns: Presence<Option<String>>,
}
object_deserialize!(ProjectWorkspaceRecord {
    #[serde(default)] layer_panel_width:Presence<Option<String>>,
    #[serde(default)] tools_panel_width:Presence<Option<String>>,
    #[serde(default)] props_panel_width:Presence<Option<String>>,
    #[serde(default)] props_panel_collapsed:Presence<bool>,
    #[serde(default)] timeline_height:Presence<Option<String>>,
    #[serde(default)] panel_order:Presence<Vec<String>>,
    #[serde(default)] motion_columns:Presence<Option<String>>
});

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectCommentRecord {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) id: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) x: Presence<f64>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) y: Presence<f64>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) frame: Presence<f64>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) author_id: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) author_name: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) author_color: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) text: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) created_at: Presence<f64>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) resolved: Presence<bool>,
}
object_deserialize!(ProjectCommentRecord {
    #[serde(default)] id:Presence<String>,
    #[serde(default)] x:Presence<f64>,#[serde(default)] y:Presence<f64>,#[serde(default)] frame:Presence<f64>,
    #[serde(default)] author_id:Presence<String>,#[serde(default)] author_name:Presence<String>,#[serde(default)] author_color:Presence<String>,
    #[serde(default)] text:Presence<String>,#[serde(default)] created_at:Presence<f64>,#[serde(default)] resolved:Presence<bool>
});
#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct Annotations {
    workspace: Option<Arc<ProjectWorkspaceRecord>>,
    comments: Option<Arc<[ProjectCommentRecord]>>,
}
impl ProjectMetadata {
    pub(crate) fn with_annotations(
        mut self,
        workspace: Option<ProjectWorkspaceRecord>,
        comments: Option<Vec<ProjectCommentRecord>>,
    ) -> Result<Self, ProjectMetadataError> {
        if comments.as_ref().is_some_and(|values| {
            values.iter().any(|comment| {
                [&comment.x, &comment.y, &comment.frame, &comment.created_at]
                    .iter()
                    .any(|v| v.value().is_some_and(|n| !n.is_finite()))
            })
        }) {
            return Err(ProjectMetadataError::InvalidAnnotations);
        }
        self.annotations.workspace = workspace.map(Arc::new);
        self.annotations.comments = comments.map(Arc::from);
        Ok(self)
    }
    pub(crate) fn annotations(
        &self,
    ) -> (
        Option<&ProjectWorkspaceRecord>,
        Option<&[ProjectCommentRecord]>,
    ) {
        (
            self.annotations.workspace.as_deref(),
            self.annotations.comments.as_deref(),
        )
    }
}
#[cfg(test)]
#[path = "../tests/project_metadata_annotations.rs"]
mod tests;
