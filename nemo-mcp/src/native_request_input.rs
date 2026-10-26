//! Pure registered-input checks; success does not authorize execution.
use super::{
    bounded_identifier, NativeApplicationRequest, RequestError, MAX_SAFE_REVISION,
    NATIVE_API_VERSION,
};

impl NativeApplicationRequest {
    /// Check the typed shape, byte bounds and operation registration only.
    /// Public callers must use `validate` to enforce capability availability;
    /// this does not admit a document, authorize dispatch or prove an owner.
    /// Raw duplicate-member checks must precede typed/Value deserialization.
    pub fn validate_input(&self) -> Result<(), RequestError> {
        if self.api_version != NATIVE_API_VERSION {
            return Err(RequestError::InvalidRequest(
                "native request requires apiVersion 2".into(),
            ));
        }
        for (label, value) in [
            ("requestId", self.request_id.as_str()),
            ("instanceId", self.instance_id.as_str()),
            ("documentId", self.document_id.as_str()),
            ("operation", self.operation.as_str()),
        ] {
            if !bounded_identifier(value) {
                return Err(RequestError::InvalidRequest(format!(
                    "{label} must match ^[A-Za-z0-9][A-Za-z0-9._:/-]{{0,127}}$"
                )));
            }
        }
        if self
            .expected_revision
            .is_some_and(|value| value > MAX_SAFE_REVISION)
        {
            return Err(RequestError::InvalidRequest(
                "expectedRevision exceeds the transport maximum".into(),
            ));
        }
        if self.expected_revision.is_some()
            && crate::native_contract::forbids_expected_revision(&self.operation)
        {
            return Err(RequestError::InvalidRequest(
                "native pinned reads forbid expectedRevision".into(),
            ));
        }
        if self.operation == "query.document.object"
            && !crate::native_contract::validate_request(&self.operation, &self.payload)
        {
            return Err(RequestError::InvalidRequest(
                "native object selector does not match its declared contract".into(),
            ));
        }
        if crate::native_contract::object_edit::admitted(&self.operation) {
            if self.expected_revision.is_none() {
                return Err(RequestError::InvalidRequest(
                    "native object edit command requires expectedRevision".into(),
                ));
            }
            if !crate::native_contract::validate_request(&self.operation, &self.payload) {
                return Err(RequestError::InvalidRequest(
                    "native object edit command does not match its declared contract".into(),
                ));
            }
        }
        if crate::native_contract::object_rig::admitted(&self.operation) {
            if self.expected_revision.is_none() {
                return Err(RequestError::InvalidRequest(
                    "native object rig command requires expectedRevision".into(),
                ));
            }
            if !crate::native_contract::validate_request(&self.operation, &self.payload) {
                return Err(RequestError::InvalidRequest(
                    "native object rig command does not match its declared contract".into(),
                ));
            }
        }
        if crate::native_contract::object_component::admitted(&self.operation) {
            if self.expected_revision.is_none() {
                return Err(RequestError::InvalidRequest(
                    "native component command requires expectedRevision".into(),
                ));
            }
            if !crate::native_contract::validate_request(&self.operation, &self.payload) {
                return Err(RequestError::InvalidRequest(
                    "native component command does not match its declared contract".into(),
                ));
            }
        }
        if matches!(
            self.operation.as_str(),
            "command.document.object.layer.visibility.set"
                | "command.document.object.undo"
                | "command.document.object.redo"
                | crate::native_contract::layer_policy::OP
                | crate::native_contract::layer_cut::OP
                | crate::native_contract::property_track::OP
                | crate::native_contract::property_edit::OP
                | "command.document.object.layer.create"
                | "command.document.object.project.settings.edit"
                | crate::native_contract::composition::EDIT
                | crate::native_contract::palette::EDIT
                | crate::native_contract::guide::EDIT
                | crate::native_contract::shadow::EDIT
                | crate::native_contract::perspective::EDIT
                | crate::native_contract::settings::symmetry::EDIT
                | crate::native_contract::workspace::EDIT
                | crate::native_contract::custom_brush::EDIT
                | crate::native_contract::comments::EDIT
                | crate::native_contract::placement::OP
                | crate::native_contract::audio_track::OP
                | crate::native_contract::brush::PUBLISH
                | crate::native_contract::font::IMPORT
                | crate::native_contract::item::ITEM
                | crate::native_contract::item::ATTACH
                | crate::native_contract::item::REMOVE
                | crate::native_contract::import::video_link::OP
                | crate::native_contract::vector::VECTOR
                | crate::native_contract::vector::PARAMETRIC
                | crate::native_contract::vector::EFFECTS
        ) {
            let action = if matches!(
                self.operation.as_str(),
                "command.document.object.undo" | "command.document.object.redo"
            ) {
                "history"
            } else if self.operation == crate::native_contract::font::IMPORT {
                "font import"
            } else if self.operation == crate::native_contract::property_edit::OP {
                "property edit"
            } else if self.operation == crate::native_contract::property_track::OP {
                "property track"
            } else if crate::native_contract::item::owns(&self.operation)
                || self.operation == crate::native_contract::import::video_link::OP
            {
                "item or asset"
            } else if crate::native_contract::vector::owns(&self.operation) {
                "vector"
            } else if self.operation == crate::native_contract::layer_cut::OP {
                "layer cut"
            } else if self.operation == crate::native_contract::layer_policy::OP {
                "layer policy"
            } else if self.operation == crate::native_contract::brush::PUBLISH {
                "brush publication"
            } else if self.operation == crate::native_contract::audio_track::OP {
                "AudioTrack metadata edit"
            } else if self.operation == crate::native_contract::placement::OP {
                "project placement"
            } else if self.operation == crate::native_contract::composition::EDIT {
                "composition metadata"
            } else if self.operation == crate::native_contract::comments::EDIT {
                "team comment metadata"
            } else if self.operation == crate::native_contract::custom_brush::EDIT {
                "custom brush catalog metadata"
            } else if self.operation == crate::native_contract::workspace::EDIT {
                "workspace metadata"
            } else if self.operation == crate::native_contract::perspective::EDIT {
                "perspective metadata"
            } else if self.operation == crate::native_contract::settings::symmetry::EDIT {
                "symmetry metadata"
            } else if self.operation == crate::native_contract::shadow::EDIT {
                "shadow metadata"
            } else if self.operation == crate::native_contract::guide::EDIT {
                "guide metadata"
            } else if self.operation.ends_with(".settings.edit") {
                "project settings"
            } else if self.operation.ends_with(".create") {
                "creation"
            } else {
                "visibility"
            };
            if self.expected_revision.is_none() {
                return Err(RequestError::InvalidRequest(format!(
                    "native object {action} command requires expectedRevision"
                )));
            }
            if !crate::native_contract::validate_request(&self.operation, &self.payload) {
                return Err(RequestError::InvalidRequest(format!(
                    "native object {action} command does not match its declared contract"
                )));
            }
        }
        if crate::native_contract::frame_edit::declares(&self.operation) {
            if self.expected_revision.is_none() {
                return Err(RequestError::InvalidRequest(
                    "native object frame edit command requires expectedRevision".into(),
                ));
            }
            if !crate::native_contract::validate_request(&self.operation, &self.payload) {
                return Err(RequestError::InvalidRequest(
                    "native object frame edit command does not match its declared contract".into(),
                ));
            }
        }
        if crate::native_contract::import::admitted(&self.operation) {
            if self.expected_revision.is_none() {
                return Err(RequestError::InvalidRequest(
                    "native object import command requires expectedRevision".into(),
                ));
            }
            if !crate::native_contract::validate_request(&self.operation, &self.payload) {
                return Err(RequestError::InvalidRequest(
                    "native object import command does not match its declared contract".into(),
                ));
            }
        }
        if !self.payload.is_object() {
            return Err(RequestError::MalformedPayload(
                "native payload must be a JSON object".into(),
            ));
        }
        if !crate::native_contract::validate_request(&self.operation, &self.payload) {
            return Err(RequestError::MalformedPayload(
                "native pinned-read payload does not match its declared contract".into(),
            ));
        }
        let request_limit = crate::wire::native_request_byte_limit(&self.operation);
        if !serde_json::to_vec(self).is_ok_and(|bytes| bytes.len() <= request_limit) {
            return Err(RequestError::InvalidRequest(format!(
                "native requests are limited to {request_limit} encoded bytes"
            )));
        }
        crate::capabilities::validate_native_operation_registration(&self.operation)
    }
}

#[cfg(test)]
#[path = "native_request_input_tests.rs"]
mod tests;
