//! Shared bounded object-read identity and refusal envelope, factored unchanged for G21.
use crate::object_snapshot::{ObjectSnapshot, ReadAdmission, MAX_MESSAGE_BYTES};
use crate::request_receipts::{
    DispatchError, DispatchErrorCode, OpacityRequest, ResponseEnvelope, APPLICATION_API_VERSION,
};
/// The capability declarations that name opacity-only operations. The refusal below is
/// built from them, so it cannot drift from the `documentKinds` and `useInstead` that
/// discovery reports; a test reads every declaration and checks both directions.
const OPACITY_DECLARATIONS: [&str; 3] = [
    include_str!("../../engineering/application/capabilities-v2/native-opacity.json"),
    include_str!("../../engineering/application/capabilities-v2/native-diagnostics.json"),
    include_str!("../../engineering/application/capabilities-v2/native-reproduction.json"),
];

/// Operation -> the object operation to use instead, where the declaration names one.
fn opacity_document_alternatives() -> &'static [(String, Option<String>)] {
    static TABLE: std::sync::OnceLock<Vec<(String, Option<String>)>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        let mut table = Vec::new();
        for source in OPACITY_DECLARATIONS {
            let declaration: serde_json::Value =
                serde_json::from_str(source).expect("capability declaration is valid JSON");
            if declaration["documentKinds"] != serde_json::json!(["opacity"]) {
                continue;
            }
            for operation in declaration["operations"].as_array().into_iter().flatten() {
                let Some(operation) = operation.as_str() else { continue };
                let other = declaration["useInstead"]["object"][operation].as_str();
                table.push((operation.to_owned(), other.map(str::to_owned)));
            }
        }
        table
    })
}

fn opacity_document_refusal(operation: &str) -> Option<String> {
    let (_, other) = opacity_document_alternatives().iter().find(|(op, _)| op == operation)?;
    let hint = other.as_ref().map_or_else(
        || "No equivalent exists on this document.".to_owned(),
        |other| format!("Use {other} instead."),
    );
    Some(format!(
        "Unsupported object read operation. {operation} applies to opacity documents only; this is an object document. {hint}"
    ))
}

pub(super) fn envelope_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 128
        && bytes[0].is_ascii_alphanumeric()
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(byte))
}

impl ObjectSnapshot {
    pub(super) fn read_admission(
        &self,
        request: &OpacityRequest,
    ) -> Result<ReadAdmission, DispatchErrorCode> {
        if [
            request.request_id.as_str(),
            request.instance_id.as_str(),
            request.document_id.as_str(),
            self.instance_id(),
            self.document_id(),
        ]
        .iter()
        .any(|value| !envelope_identifier(value))
        {
            return Err(DispatchErrorCode::InvalidRequest);
        }
        let encoded =
            serde_json::to_vec(&request).map_err(|_| DispatchErrorCode::InvalidRequest)?;
        let failure = |code, message| {
            Ok(ReadAdmission::Refused(
                self.read_failure(request, code, message),
            ))
        };
        if request.api_version != APPLICATION_API_VERSION
            || !envelope_identifier(&request.operation)
            || !request.payload.is_object()
            || encoded.len() > MAX_MESSAGE_BYTES
        {
            return failure(
                DispatchErrorCode::InvalidRequest,
                "Invalid bounded object read request.",
            );
        }
        if request.instance_id != self.instance_id() {
            return failure(
                DispatchErrorCode::WrongInstance,
                "Object read targets a different instance.",
            );
        }
        if request.document_id != self.document_id() {
            return failure(
                DispatchErrorCode::WrongDocument,
                "Object read targets a different document.",
            );
        }
        if let Some(message) = opacity_document_refusal(&request.operation) {
            return failure(DispatchErrorCode::InvalidRequest, &message);
        }
        if request.cancelled_before_dispatch {
            return failure(
                DispatchErrorCode::CancelledBeforeDispatch,
                "Object read was cancelled before dispatch.",
            );
        }
        if request.expected_revision.is_some() {
            return failure(
                DispatchErrorCode::InvalidRequest,
                "Only pinned object reads without expectedRevision are supported.",
            );
        }
        Ok(ReadAdmission::Accepted(encoded))
    }

    pub(super) fn read_failure(
        &self,
        request: &OpacityRequest,
        code: DispatchErrorCode,
        message: &str,
    ) -> ResponseEnvelope {
        ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().to_owned(),
            document_id: self.document_id().to_owned(),
            content_revision: self.content_revision(),
            ok: false,
            result: None,
            error: Some(DispatchError {
                code,
                message: message.into(),
                details: (code == DispatchErrorCode::WrongDocument)
                    .then(|| serde_json::json!({"requestedDocumentId":request.document_id})),
            }),
        }
    }
}

#[cfg(test)]
#[path = "../tests/object_read_admission_declarations.rs"]
mod declarations;
