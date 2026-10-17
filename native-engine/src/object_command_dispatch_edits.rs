//! Per-operation bodies of the staged object command dispatcher, moved verbatim out of
//! object_command_dispatch.rs (#2106). The common envelope, replay and revision checks already ran
//! there, so only the typed preparation and the sealed commit of each operation remain.
use crate::history::object_history::object_command_dispatch::{
    MAX_SAFE_REVISION, OP_PROPERTY_TRACK_EDIT,
};
use crate::history::NativeObjectHistory;
use crate::request_receipts::{DispatchErrorCode, OpacityRequest};
use crate::transaction::{CompactResult, RetainedDisposition};

impl NativeObjectHistory {
    /// Raster-item / text-block edit: the typed preparation, the response-bound preflight and the sealed commit.
    pub(super) fn dispatch_item_edit_command(
        &mut self,
        request: &OpacityRequest,
        payload: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        match self.prepare_item_edit_json(payload) {
            Ok(candidate) => {
                let result = candidate.result();
                // The echoed stroke id comes from the request; check the complete success envelope
                // before publishing. This temporary reply is never retained.
                let preflight = RetainedDisposition::Exact {
                    content_revision: MAX_SAFE_REVISION,
                    outcome: Ok(CompactResult::ItemEdit(result.clone())),
                };
                if self.object_reply(request, &preflight).is_err() {
                    Err((
                        DispatchErrorCode::InvalidRequest,
                        "Item edit result exceeds the response bound.",
                    ))
                } else {
                    self.commit_item_edit(candidate)
                        .map(|_| CompactResult::ItemEdit(result))
                        .map_err(|code| (code, "Item edit could not commit."))
                }
            }
            Err(code) => Err((
                code,
                "Item edit payload or target is invalid or unavailable.",
            )),
        }
    }
    /// Editable-vector edit: the typed preparation, the response-bound preflight and the sealed commit.
    pub(super) fn dispatch_vector_edit_command(
        &mut self,
        request: &OpacityRequest,
        payload: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        match self.prepare_vector_edit_json(payload) {
            Ok(candidate) => {
                let result = candidate.result();
                // Imported stroke IDs may be large. Check the complete success
                // envelope before publishing; this temporary reply is never retained.
                let preflight = RetainedDisposition::Exact {
                    content_revision: MAX_SAFE_REVISION,
                    outcome: Ok(CompactResult::VectorEdit(result.clone())),
                };
                if self.object_reply(request, &preflight).is_err() {
                    Err((
                        DispatchErrorCode::InvalidRequest,
                        "Vector edit result exceeds the response bound.",
                    ))
                } else {
                    self.commit_vector_edit(candidate)
                        .map(|_| CompactResult::VectorEdit(result))
                        .map_err(|code| (code, "Vector edit could not commit."))
                }
            }
            Err(code) => Err((
                code,
                "Vector edit payload or target is invalid or unavailable.",
            )),
        }
    }
    /// Layer creation: the typed preparation and the sealed commit that reports the minted layer id.
    pub(super) fn dispatch_layer_create_command(
        &mut self,
        payload: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        match self.prepare_layer_create_json(payload) {
            Ok(candidate) => {
                let layer_uid = candidate.layer_uid().to_owned();
                self.commit_layer_create(candidate)
                    .map(|_| CompactResult::LayerCreated { layer_uid })
                    .map_err(|code| (code, "Layer creation could not commit."))
            }
            Err(code) => Err((code, "Layer creation payload is invalid.")),
        }
    }
    /// Persisted layer flags: the typed preparation and the sealed commit.
    pub(super) fn dispatch_layer_policy_command(
        &mut self,
        payload: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        match self.prepare_layer_policy_json(payload) {
            Ok(candidate) => {
                let before = self.content_revision();
                self.commit_layer_policy(candidate)
                    .map(|revision| CompactResult::Command {
                        applied: revision != before,
                        history_entries_added: u64::from(revision != before),
                    })
                    .map_err(|code| (code, "Layer policy could not commit."))
            }
            Err(code) => Err((code, "Layer policy payload is invalid or target is absent.")),
        }
    }
    /// Layer visibility: the typed preparation and the sealed commit.
    pub(super) fn dispatch_layer_visibility_command(
        &mut self,
        payload: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        match self.prepare_layer_visibility_json(payload) {
            Ok(candidate) => {
                let before = self.content_revision();
                self.commit_layer_visibility(candidate)
                    .map(|revision| CompactResult::Command {
                        applied: revision != before,
                        history_entries_added: u64::from(revision != before),
                    })
                    .map_err(|code| (code, "Layer visibility could not commit."))
            }
            Err(code) => Err((
                code,
                "Layer visibility payload is invalid or target is absent.",
            )),
        }
    }
    /// Project settings: the typed preparation and the sealed commit.
    pub(super) fn dispatch_project_settings_command(
        &mut self,
        payload: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        match self.prepare_project_settings_json(payload) {
            Ok(candidate) => {
                let before = self.content_revision();
                self.commit_project_settings(candidate)
                    .map(|revision| CompactResult::Command {
                        applied: revision != before,
                        history_entries_added: u64::from(revision != before),
                    })
                    .map_err(|code| (code, "Project settings could not commit."))
            }
            Err(code) => Err((
                code,
                "Project settings payload is invalid or project is absent.",
            )),
        }
    }
    /// Property value and track edits: the typed preparation and the sealed commit.
    pub(super) fn dispatch_property_edit_command(
        &mut self,
        request: &OpacityRequest,
        payload: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        let candidate = if request.operation == OP_PROPERTY_TRACK_EDIT {
            self.prepare_property_track_edit_json(payload)
        } else {
            self.prepare_property_edit_json(payload)
        };
        match candidate {
            Ok(candidate) => {
                let before = self.content_revision();
                self.commit_property_edit(candidate)
                    .map(|revision| CompactResult::Command {
                        applied: revision != before,
                        history_entries_added: u64::from(revision != before),
                    })
                    .map_err(|code| (code, "Property edit could not commit."))
            }
            Err(code) => Err((
                code,
                "Property edit payload or target is invalid or unavailable.",
            )),
        }
    }
    /// Object fill: the typed preparation and the sealed commit.
    pub(super) fn dispatch_fill_command(
        &mut self,
        payload: &[u8],
    ) -> Result<CompactResult, (DispatchErrorCode, &'static str)> {
        match self.prepare_fill_json(payload) {
            Ok(candidate) => {
                let before = self.content_revision();
                self.commit_fill(candidate)
                    .map(|revision| CompactResult::Command {
                        applied: revision != before,
                        history_entries_added: u64::from(revision != before),
                    })
                    .map_err(|code| (code, "Object fill could not commit."))
            }
            Err(code) => Err((code, "Object fill payload is invalid or target is absent.")),
        }
    }
}
