//! Staged actual-owner command dispatch. Public capability admission stays closed.
use crate::commands::MAX_VECTOR_REQUEST_BYTES;
use crate::history::object_history::object_exposure_edit_key::is_exposure_key_operation;
use crate::history::object_history::object_exposure_edit_retime::is_exposure_retime_operation;
use crate::history::object_history::object_frame_bulk_move::is_frame_bulk_move_operation;
use crate::history::object_history::object_frame_clipboard_paste::is_frame_clipboard_operation;
use crate::history::object_history::object_frame_clipboard_repeat::is_frame_repeat_operation;
use crate::history::object_history::object_frame_edit_structure::is_frame_structure_operation;
use crate::history::object_history::object_layer_shift::is_layer_shift_operation;
use crate::history::object_history::object_rig_edit_history::is_rig_edit_operation;
use crate::history::object_history::object_rig_pose_history::is_rig_pose_operation;
use crate::history::NativeObjectHistory;
use crate::request_receipts::frame_clipboard_transport;
use crate::request_receipts::{
    DispatchErrorCode, OpacityRequest, RequestFingerprint, ResponseEnvelope,
    APPLICATION_API_VERSION,
};
use crate::transaction::{CompactFailure, CompactResult, ReceiptLookup, RetainedDisposition};

// #2106 size split: the payload-bearing operation bodies live in the private child below.
#[path = "object_command_dispatch_edits.rs"]
mod edits;
use edits::OP_PROJECT_SETTINGS;

// #2207 staged placement producer; public capability admission remains separate.
#[path = "object_media_link_command.rs"]
mod placement;
use placement::OP_PLACEMENT_EDIT;
#[path = "object_audio_track_edit_command.rs"]
mod audio_track;
use audio_track::OP_AUDIO_TRACK_EDIT;

// CS2 (#2345): command families register by adding a file under object_command_families/.
#[path = "object_command_families/mod.rs"]
pub(crate) mod families;

const OP_OBJECT_FILL: &str = "command.document.object.fill.set";
const OP_OBJECT_UNDO: &str = "command.document.object.undo";
const OP_OBJECT_REDO: &str = "command.document.object.redo";
const OP_LAYER_VISIBILITY: &str = "command.document.object.layer.visibility.set";
const OP_LAYER_CREATE: &str = "command.document.object.layer.create";
const OP_LAYER_PLAYHEAD_CUT: &str = "command.document.object.layer.cut";
const OP_LAYER_POLICY: &str = "command.document.object.layer.policy.edit";
const OP_PROPERTY_EDIT: &str = "command.document.object.property.edit";
const OP_PROPERTY_TRACK_EDIT: &str = "command.document.object.property.track.edit";
const OP_OBJECT_ITEM: &str = "command.document.object.item.edit";
const OP_OBJECT_VECTOR: &str = "command.document.object.vector.edit";
const OP_IMAGE_MESH_EDIT: &str = "command.document.object.image.mesh.edit";
const MAX_MESSAGE_BYTES: usize = families::DEFAULT_REQUEST_BYTES;

/// Request-envelope bound per operation: only the vector and image-mesh edits may carry a bounded body
/// (a hand-drawn mask outline is hundreds of points).
fn request_limit(operation: &str) -> usize {
    if let Some(family) = families::owner(operation) {
        family.request_limit
    } else if frame_clipboard_transport::owns(operation) {
        frame_clipboard_transport::request_limit(operation)
    } else if operation == OP_OBJECT_VECTOR || operation == OP_IMAGE_MESH_EDIT {
        MAX_VECTOR_REQUEST_BYTES
    } else {
        MAX_MESSAGE_BYTES
    }
}
const MAX_SAFE_REVISION: u64 = 9_007_199_254_740_991;

fn identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(byte))
}

impl NativeObjectHistory {
    /// Dispatch only the distinct staged fill operation in this actual owner.
    /// `None` leaves other operations to their existing host route. This typed
    /// entry needs original-byte ingress before deserialization; it cannot
    /// detect duplicate raw members already discarded into a Value.
    pub fn try_dispatch_object_fill(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<Option<ResponseEnvelope>, DispatchErrorCode> {
        if request.operation != OP_OBJECT_FILL {
            return Ok(None);
        }
        self.try_dispatch_object_command(request)
    }

    /// Dispatch staged fill, layer creation/visibility and undo/redo through one owner and
    /// receipt store. Typed input still requires the unchanged raw ingress gate.
    pub fn try_dispatch_object_command(
        &mut self,
        request: &OpacityRequest,
    ) -> Result<Option<ResponseEnvelope>, DispatchErrorCode> {
        self.try_dispatch_object_command_with_bake(request, |_, _| {
            Err(DispatchErrorCode::Unavailable)
        })
    }

    /// True when the hand-routed chain below does NOT admit the request. Registry families
    /// (`families::owner`) are consulted first and are not listed here.
    pub(crate) fn legacy_chain_declines(request: &OpacityRequest) -> bool {
        !Self::layer_lifecycle_operation(&request.operation)
            && !edits::is_project_settings_operation(&request.operation)
            && !Self::group_operation(&request.operation)
            && !Self::component_setup_operation(&request.operation)
            && !Self::compositing_operation(&request.operation)
            && !Self::boolean_operation(&request.operation)
            && !Self::selection_edit_operation(&request.operation)
            && !Self::import_batch_operation(&request.operation)
            && !Self::vector_param_operation(&request.operation)
            && !matches!(
                request.operation.as_str(),
                OP_OBJECT_FILL
                    | OP_OBJECT_UNDO
                    | OP_OBJECT_REDO
                    | OP_LAYER_VISIBILITY
                    | OP_LAYER_CREATE
                    | OP_LAYER_PLAYHEAD_CUT
                    | OP_LAYER_POLICY
                    | OP_PROPERTY_EDIT
                    | OP_PROPERTY_TRACK_EDIT
                    | OP_PLACEMENT_EDIT
                    | OP_AUDIO_TRACK_EDIT
                    | OP_OBJECT_VECTOR
                    | OP_IMAGE_MESH_EDIT
                    | OP_OBJECT_ITEM
            )
            && !is_frame_structure_operation(request.operation.as_str())
            && !is_frame_clipboard_operation(request.operation.as_str())
            && !is_exposure_key_operation(request.operation.as_str())
            && !is_exposure_retime_operation(request.operation.as_str())
            && !is_rig_edit_operation(request.operation.as_str())
            && !is_rig_pose_operation(request.operation.as_str())
            && !is_frame_repeat_operation(request.operation.as_str())
            && !is_frame_bulk_move_operation(request.operation.as_str())
            && !is_layer_shift_operation(request.operation.as_str())
    }

    pub(crate) fn try_dispatch_object_command_with_bake(
        &mut self,
        request: &OpacityRequest,
        bake: impl FnOnce(
            &Self,
            &[u8],
        ) -> Result<crate::commands::PreparedPropertyEdit, DispatchErrorCode>,
    ) -> Result<Option<ResponseEnvelope>, DispatchErrorCode> {
        if families::owner(&request.operation).is_none() && Self::legacy_chain_declines(request) {
            return Ok(None);
        }
        if [
            request.request_id.as_str(),
            request.instance_id.as_str(),
            request.document_id.as_str(),
            self.instance_id(),
            self.document_id(),
        ]
        .iter()
        .any(|id| !identifier(id))
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let message = |fill, history| {
            if request.operation == OP_OBJECT_ITEM {
                "Item edit request refused."
            } else if request.operation == OP_OBJECT_VECTOR {
                "Vector edit request refused."
            } else if Self::vector_param_operation(&request.operation) {
                "Vector parameter request refused."
            } else if request.operation == OP_IMAGE_MESH_EDIT {
                "Image mesh edit request refused."
            } else if let Some(family) = families::owner(&request.operation) {
                family.refusal
            } else if request.operation == OP_AUDIO_TRACK_EDIT {
                "AudioTrack edit request refused."
            } else if request.operation == OP_PLACEMENT_EDIT {
                "Placement edit request refused."
            } else if request.operation == OP_PROJECT_SETTINGS {
                "Project settings request refused."
            } else if is_frame_structure_operation(request.operation.as_str()) {
                "Frame structure request refused."
            } else if is_frame_clipboard_operation(request.operation.as_str()) {
                "Frame clipboard request refused."
            } else if matches!(
                request.operation.as_str(),
                OP_PROPERTY_EDIT | OP_PROPERTY_TRACK_EDIT
            ) {
                "Property edit request refused."
            } else if Self::layer_lifecycle_operation(&request.operation) {
                "Layer lifecycle request refused."
            } else if Self::component_setup_operation(&request.operation) {
                "Component setup request refused."
            } else if Self::group_operation(&request.operation) {
                "Group request refused."
            } else if Self::compositing_operation(&request.operation) {
                "Layer compositing request refused."
            } else if Self::boolean_operation(&request.operation) {
                "Boolean request refused."
            } else if Self::selection_edit_operation(&request.operation) {
                "Selection edit request refused."
            } else if is_exposure_key_operation(request.operation.as_str()) {
                "Exposure key request refused."
            } else if is_exposure_retime_operation(request.operation.as_str()) {
                "Key retime request refused."
            } else if is_layer_shift_operation(request.operation.as_str()) {
                "Layer range drag request refused."
            } else if is_frame_bulk_move_operation(request.operation.as_str()) {
                "Frame move request refused."
            } else if is_frame_repeat_operation(request.operation.as_str()) {
                "Frame repeat request refused."
            } else if Self::import_batch_operation(&request.operation) {
                "Import batch request refused."
            } else if request.operation == OP_LAYER_CREATE {
                "Layer creation request refused."
            } else if request.operation == OP_LAYER_POLICY {
                "Layer policy request refused."
            } else if request.operation == OP_LAYER_VISIBILITY {
                "Layer visibility request refused."
            } else if request.operation == OP_OBJECT_FILL {
                fill
            } else {
                history
            }
        };
        let reject = |owner: &Self, code, message| {
            owner.object_reply(request, &owner.object_failure(code, message))
        };
        if request.api_version != APPLICATION_API_VERSION
            || !request.payload.is_object()
            || serde_json::to_vec(request).map_or(true, |bytes| {
                bytes.len() > request_limit(&request.operation)
            })
            || request
                .expected_revision
                .is_some_and(|revision| revision > MAX_SAFE_REVISION)
        {
            return reject(
                self,
                DispatchErrorCode::InvalidRequest,
                message(
                    "Invalid bounded object fill request.",
                    "Invalid bounded object history request.",
                ),
            )
            .map(Some);
        }
        if request.instance_id != self.instance_id() {
            return reject(
                self,
                DispatchErrorCode::WrongInstance,
                message(
                    "Object fill targets a different instance.",
                    "Object history targets a different instance.",
                ),
            )
            .map(Some);
        }
        if request.document_id != self.document_id() {
            return reject(
                self,
                DispatchErrorCode::WrongDocument,
                message(
                    "Object fill targets a replaced document.",
                    "Object history targets a replaced document.",
                ),
            )
            .map(Some);
        }
        let fingerprint =
            RequestFingerprint::new(request).ok_or(DispatchErrorCode::InvalidRequest)?;
        match self.receipts.lookup(&request.request_id, &fingerprint) {
            ReceiptLookup::Retained(disposition) => {
                return self.object_reply(request, disposition).map(Some)
            }
            ReceiptLookup::ChangedBody => {
                return reject(
                    self,
                    DispatchErrorCode::InvalidRequest,
                    message(
                        "Object fill requestId was reused with a changed body.",
                        "Object history requestId was reused with a changed body.",
                    ),
                )
                .map(Some)
            }
            ReceiptLookup::Missing => {}
        }
        let outcome = if request.cancelled_before_dispatch {
            Err((
                DispatchErrorCode::CancelledBeforeDispatch,
                message(
                    "Object fill was cancelled before dispatch.",
                    "Object history was cancelled before dispatch.",
                ),
            ))
        } else if request.expected_revision.is_none() {
            Err((
                DispatchErrorCode::InvalidRequest,
                message(
                    "Object fill requires expectedRevision.",
                    "Object history requires expectedRevision.",
                ),
            ))
        } else if request.expected_revision != Some(self.content_revision()) {
            Err((
                DispatchErrorCode::StaleRevision,
                message(
                    "Object fill expected revision is stale.",
                    "Object history expected revision is stale.",
                ),
            ))
        } else {
            if let Some(family) = families::owner(&request.operation) {
                (family.dispatch)(self, request)?
            } else if request.operation == OP_OBJECT_ITEM {
                let payload = serde_json::to_vec(&request.payload)
                    .map_err(|_| DispatchErrorCode::InvalidRequest)?;
                self.dispatch_item_edit_command(request, &payload)
            } else if request.operation == OP_OBJECT_VECTOR {
                let payload = serde_json::to_vec(&request.payload)
                    .map_err(|_| DispatchErrorCode::InvalidRequest)?;
                self.dispatch_vector_edit_command(request, &payload)
            } else if Self::vector_param_operation(&request.operation) {
                self.dispatch_vector_param(&request.operation, &request.payload)
            } else if request.operation == OP_LAYER_PLAYHEAD_CUT {
                self.dispatch_layer_playhead_cut(&request.operation, &request.payload)
            } else if Self::layer_lifecycle_operation(&request.operation) {
                self.dispatch_layer_lifecycle(&request.operation, &request.payload)
            } else if Self::component_setup_operation(&request.operation) {
                self.dispatch_component_setup(&request.operation, &request.payload)
            } else if Self::group_operation(&request.operation) {
                self.dispatch_group(&request.operation, &request.payload)
            } else if Self::compositing_operation(&request.operation) {
                self.dispatch_compositing(&request.operation, &request.payload)
            } else if Self::boolean_operation(&request.operation) {
                self.dispatch_boolean(&request.operation, &request.payload)
            } else if Self::selection_edit_operation(&request.operation) {
                self.dispatch_selection_edit(request)
            } else if request.operation == OP_IMAGE_MESH_EDIT {
                let payload = serde_json::to_vec(&request.payload)
                    .map_err(|_| DispatchErrorCode::InvalidRequest)?;
                self.dispatch_image_mesh_edit(&payload)
            } else if Self::import_batch_operation(&request.operation) {
                self.dispatch_import_batch(&request.payload)
            } else if request.operation == OP_LAYER_CREATE {
                let payload = serde_json::to_vec(&request.payload)
                    .map_err(|_| DispatchErrorCode::InvalidRequest)?;
                self.dispatch_layer_create_command(&payload)
            } else if request.operation == OP_LAYER_POLICY {
                let payload = serde_json::to_vec(&request.payload)
                    .map_err(|_| DispatchErrorCode::InvalidRequest)?;
                self.dispatch_layer_policy_command(&payload)
            } else if is_exposure_key_operation(request.operation.as_str()) {
                self.dispatch_exposure_key(request.operation.as_str(), &request.payload)
            } else if is_exposure_retime_operation(request.operation.as_str()) {
                self.dispatch_exposure_retime(&request.payload)
            } else if is_rig_pose_operation(request.operation.as_str()) {
                self.dispatch_rig_pose(&request.payload)
            } else if is_rig_edit_operation(request.operation.as_str()) {
                self.dispatch_rig_edit(&request.payload)
            } else if is_layer_shift_operation(request.operation.as_str()) {
                self.dispatch_layer_shift(request)
            } else if is_frame_bulk_move_operation(request.operation.as_str()) {
                self.dispatch_frame_bulk_move(request)
            } else if is_frame_repeat_operation(request.operation.as_str()) {
                self.dispatch_frame_repeat(request.operation.as_str(), &request.payload)
            } else if request.operation == OP_LAYER_VISIBILITY {
                let payload = serde_json::to_vec(&request.payload)
                    .map_err(|_| DispatchErrorCode::InvalidRequest)?;
                self.dispatch_layer_visibility_command(&payload)
            } else if is_frame_structure_operation(request.operation.as_str()) {
                self.dispatch_frame_structure(request.operation.as_str(), &request.payload)
            } else if is_frame_clipboard_operation(request.operation.as_str()) {
                self.dispatch_frame_clipboard(request)
            } else if request.operation == OP_AUDIO_TRACK_EDIT {
                self.dispatch_audio_track_edit(&request.payload)
            } else if request.operation == OP_PLACEMENT_EDIT {
                self.dispatch_placement_edit(&request.payload)
            } else if edits::is_project_settings_operation(&request.operation) {
                self.dispatch_project_settings_command(request)
            } else if matches!(
                request.operation.as_str(),
                OP_PROPERTY_EDIT | OP_PROPERTY_TRACK_EDIT
            ) {
                let payload = serde_json::to_vec(&request.payload)
                    .map_err(|_| DispatchErrorCode::InvalidRequest)?;
                if request.operation == OP_PROPERTY_EDIT
                    && request.payload["action"]["kind"] == "bake"
                {
                    self.dispatch_property_bake_command(&payload, bake)
                } else {
                    self.dispatch_property_edit_command(request, &payload)
                }
            } else if request.operation == OP_OBJECT_FILL {
                let payload = serde_json::to_vec(&request.payload)
                    .map_err(|_| DispatchErrorCode::InvalidRequest)?;
                self.dispatch_fill_command(&payload)
            } else {
                let undo = request.operation == OP_OBJECT_UNDO;
                let command = if undo { "object.undo" } else { "object.redo" };
                if request.payload != serde_json::json!({"command":command}) {
                    Err((
                        DispatchErrorCode::InvalidRequest,
                        "Object history payload is invalid.",
                    ))
                } else {
                    let revision = request
                        .expected_revision
                        .ok_or(DispatchErrorCode::InvalidRequest)?;
                    let moved = if undo {
                        self.undo(revision)
                    } else {
                        self.redo(revision)
                    };
                    moved
                        .map(|_| CompactResult::Command {
                            applied: true,
                            history_entries_added: 0,
                        })
                        .map_err(|code| (code, "Object history could not move."))
                }
            }
        };
        let disposition = RetainedDisposition::Exact {
            content_revision: self.content_revision(),
            outcome: outcome.map_err(|(code, message)| CompactFailure {
                code,
                message: message.into(),
                details: None,
            }),
        };
        self.receipts
            .retain(request.request_id.clone(), fingerprint, disposition.clone());
        self.object_reply(request, &disposition).map(Some)
    }

    fn object_failure(&self, code: DispatchErrorCode, message: &str) -> RetainedDisposition {
        RetainedDisposition::Exact {
            content_revision: self.content_revision(),
            outcome: Err(CompactFailure {
                code,
                message: message.into(),
                details: None,
            }),
        }
    }

    pub(super) fn object_reply(
        &self,
        request: &OpacityRequest,
        disposition: &RetainedDisposition,
    ) -> Result<ResponseEnvelope, DispatchErrorCode> {
        let response = disposition
            .response(request, self.instance_id(), self.document_id())
            .ok_or(DispatchErrorCode::Internal)?;
        if !serde_json::to_vec(&response).is_ok_and(|bytes| {
            bytes.len() <= frame_clipboard_transport::response_limit(&request.operation)
        }) {
            return Err(DispatchErrorCode::Internal);
        }
        Ok(response)
    }
}

// CS2 (#2345): every declared object operation has exactly one owner.
#[cfg(test)]
#[path = "../tests/object_command_ownership.rs"]
mod ownership_tests;
