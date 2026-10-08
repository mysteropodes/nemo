//! Pure registered-input checks; success does not authorize execution.
use super::{
    bounded_identifier, NativeApplicationRequest, RequestError, MAX_SAFE_REVISION,
    NATIVE_API_VERSION, NATIVE_MAX_MESSAGE_BYTES,
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
        if !matches!(
            serde_json::to_vec(self).map(|bytes| bytes.len()),
            Ok(0..=NATIVE_MAX_MESSAGE_BYTES)
        ) {
            return Err(RequestError::InvalidRequest(
                "native requests are limited to 4096 encoded bytes".into(),
            ));
        }
        crate::capabilities::validate_native_operation_registration(&self.operation)
    }
}

#[cfg(test)]
#[path = "native_request_input_tests.rs"]
mod tests;
