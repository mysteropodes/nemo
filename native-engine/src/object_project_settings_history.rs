//! Project settings are one transaction in the existing object history.
use crate::commands::PreparedProjectSettings;
use crate::history::object_history::{NativeObjectHistory, ObjectHistoryEntry};
use crate::request_receipts::DispatchErrorCode;

impl NativeObjectHistory {
    pub(crate) fn prepare_perspective_delta_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedProjectSettings, DispatchErrorCode> {
        self.owner.prepare_perspective_delta_json(bytes)
    }
    pub(crate) fn prepare_shadow_delta_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedProjectSettings, DispatchErrorCode> {
        self.owner.prepare_shadow_delta_json(bytes)
    }
    pub(crate) fn prepare_guide_delta_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedProjectSettings, DispatchErrorCode> {
        self.owner.prepare_guide_delta_json(bytes)
    }
    pub(crate) fn prepare_palette_delta_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedProjectSettings, DispatchErrorCode> {
        self.owner.prepare_palette_delta_json(bytes)
    }
    pub(crate) fn prepare_composition_delta_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedProjectSettings, DispatchErrorCode> {
        self.owner.prepare_composition_delta_json(bytes)
    }
    pub(crate) fn prepare_composition_metadata_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedProjectSettings, DispatchErrorCode> {
        self.owner.prepare_composition_metadata_json(bytes)
    }
    pub(crate) fn prepare_project_settings_json(
        &self,
        bytes: &[u8],
    ) -> Result<PreparedProjectSettings, DispatchErrorCode> {
        self.owner.prepare_project_settings_json(bytes)
    }
    pub(crate) fn commit_project_settings(
        &mut self,
        candidate: PreparedProjectSettings,
    ) -> Result<u64, DispatchErrorCode> {
        let before_revision = self.content_revision();
        let after_revision = self.owner.commit_project_settings(candidate)?;
        if before_revision != after_revision {
            self.undo.push(ObjectHistoryEntry {
                before_revision,
                after_revision,
            });
            self.redo.clear();
        }
        Ok(after_revision)
    }
}

#[cfg(all(test, feature = "project_media_codec"))]
#[path = "../tests/object_composition_dispatch.rs"]
mod composition_delta_tests;

// The actual Baker History public route is application-scoped.
#[cfg(all(test, feature = "application", feature = "project_media_codec"))]
#[path = "../tests/object_composition_public.rs"]
mod composition_public_tests;

#[cfg(all(test, feature = "project_media_codec"))]
#[path = "../tests/object_palette_edit.rs"]
mod palette_delta_tests;

// Public envelope coverage shares the existing metadata/history owner.
#[cfg(all(test, feature = "application", feature = "project_media_codec"))]
#[path = "../tests/object_palette_public.rs"]
mod palette_public_tests;

#[cfg(all(test, feature = "application", feature = "project_media_codec"))]
#[path = "../tests/object_guide_public.rs"]
mod guide_public_tests;

#[cfg(all(test, feature = "application", feature = "project_media_codec"))]
#[path = "../tests/object_shadow_public.rs"]
mod shadow_public_tests;

#[cfg(all(test, feature = "application", feature = "project_media_codec"))]
#[path = "../tests/object_perspective_public.rs"]
mod perspective_public_tests;
