//! Typed v2 commands and queries for the admitted native opacity document.

use crate::document::OpacityDocument;
use crate::request_receipts::{
    ApplyOpacity, QueryOpacity, ReceiptDisposition, ReceiptLookup, RequestFingerprint,
    RequestReceipts, RevisionQuery, SnapshotQuery,
};
pub use crate::request_receipts::{
    DispatchError, DispatchErrorCode, OpacityRequest, ResponseEnvelope, APPLICATION_API_VERSION,
    OP_COMMAND_APPLY, OP_QUERY_OPACITY, OP_QUERY_REVISION, OP_QUERY_SNAPSHOT,
};
use crate::revision::{DocumentSnapshot, RevisionOwner};
use serde_json::{json, Value};

#[derive(Debug)]
pub struct NativeOpacityApplication {
    owner: RevisionOwner,
    receipts: RequestReceipts,
}

impl NativeOpacityApplication {
    pub fn new(
        instance_id: impl Into<String>,
        document: OpacityDocument,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            owner: RevisionOwner::new(instance_id, document)?,
            receipts: RequestReceipts::default(),
        })
    }

    pub fn instance_id(&self) -> &str {
        self.owner.instance_id()
    }

    pub fn document_id(&self) -> &str {
        self.owner.document_id()
    }

    pub fn content_revision(&self) -> u64 {
        self.owner.content_revision()
    }

    pub fn acquire_snapshot(&self, revision: u64) -> Option<DocumentSnapshot> {
        self.owner.acquire(revision)
    }

    pub fn replace_document(&mut self, document: OpacityDocument) -> Result<(), &'static str> {
        self.owner.replace(document)?;
        self.receipts.clear();
        Ok(())
    }

    pub fn handle(&mut self, request: OpacityRequest) -> ResponseEnvelope {
        if let Some(response) = self.preflight(&request) {
            return response;
        }
        let fingerprint = RequestFingerprint::new(&request)
            .expect("preflight admitted a serializable bounded request");
        match self.receipts.lookup(&request.request_id, &fingerprint) {
            ReceiptLookup::Retained(disposition) => {
                return self.retained_receipt(&request, disposition);
            }
            ReceiptLookup::ChangedBody => {
                return self.invalid(&request, "requestId was reused with a changed body.");
            }
            ReceiptLookup::Missing => {}
        }
        if request.cancelled_before_dispatch {
            return self.failure(
                &request,
                DispatchErrorCode::CancelledBeforeDispatch,
                "Request was cancelled before dispatch.",
                None,
            );
        }

        match request.operation.as_str() {
            OP_COMMAND_APPLY => self.apply_opacity(&request, fingerprint),
            OP_QUERY_OPACITY => self.query_opacity(&request, fingerprint),
            OP_QUERY_REVISION => self.query_revision(&request, fingerprint),
            OP_QUERY_SNAPSHOT => self.query_snapshot(&request, fingerprint),
            _ => self.failure(
                &request,
                DispatchErrorCode::InvalidRequest,
                "Operation is not admitted by the native opacity slice.",
                None,
            ),
        }
    }

    fn preflight(&self, request: &OpacityRequest) -> Option<ResponseEnvelope> {
        if request.api_version != APPLICATION_API_VERSION
            || request.request_id.is_empty()
            || request.request_id.len() > 128
            || request.instance_id.is_empty()
            || request.document_id.is_empty()
            || !request.payload.is_object()
        {
            return Some(self.failure(
                request,
                DispatchErrorCode::InvalidRequest,
                "Invalid native opacity request.",
                None,
            ));
        }
        let request_size = serde_json::to_vec(request).map(|bytes| bytes.len());
        if !matches!(request_size, Ok(0..=4096)) {
            return Some(self.failure(
                request,
                DispatchErrorCode::InvalidRequest,
                "Native opacity requests are limited to 4096 encoded bytes.",
                None,
            ));
        }
        if request.instance_id != self.owner.instance_id() {
            return Some(self.failure(
                request,
                DispatchErrorCode::WrongInstance,
                "Request targets a different native engine instance.",
                None,
            ));
        }
        if request.document_id != self.owner.document_id() {
            return Some(self.failure(
                request,
                DispatchErrorCode::WrongDocument,
                "Request targets a replaced document.",
                Some(json!({ "requestedDocumentId": request.document_id })),
            ));
        }
        None
    }

    fn apply_opacity(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
    ) -> ResponseEnvelope {
        let payload: ApplyOpacity = match serde_json::from_value(request.payload.clone()) {
            Ok(payload) => payload,
            Err(_) => {
                return self.retain_command_failure(
                    request,
                    fingerprint,
                    DispatchErrorCode::InvalidRequest,
                    "Invalid opacity command payload.",
                );
            }
        };
        if payload.command != "layer.opacity.set" {
            return self.retain_command_failure(
                request,
                fingerprint,
                DispatchErrorCode::InvalidRequest,
                "Only layer.opacity.set is admitted.",
            );
        }
        let Some(value) = payload.value.as_f64() else {
            return self.retain_command_failure(
                request,
                fingerprint,
                DispatchErrorCode::InvalidRequest,
                "Opacity must be a finite number.",
            );
        };
        if !(0.0..=100.0).contains(&value) {
            return self.retain_command_failure(
                request,
                fingerprint,
                DispatchErrorCode::InvalidRequest,
                "Opacity must be in the range 0..100.",
            );
        }
        if request.expected_revision.is_none() {
            return self.retain_command_failure(
                request,
                fingerprint,
                DispatchErrorCode::InvalidRequest,
                "A command requires expectedRevision.",
            );
        }
        if request.expected_revision != Some(self.owner.content_revision()) {
            return self.retain_command_failure(
                request,
                fingerprint,
                DispatchErrorCode::StaleRevision,
                "Read the current content revision before writing.",
            );
        }

        let mut document = (*self.owner.current_document()).clone();
        let Some(layer) = document
            .layers
            .iter_mut()
            .find(|layer| layer.layer_uid == payload.stable_target.layer_uid)
        else {
            return self.retain_command_failure(
                request,
                fingerprint,
                DispatchErrorCode::NotFound,
                "The stable layer target was not found.",
            );
        };
        let Some(motion) = layer.motion_static.as_mut() else {
            return self.retain_command_failure(
                request,
                fingerprint,
                DispatchErrorCode::Unavailable,
                "Static opacity is unavailable for this admitted layer.",
            );
        };
        if motion.opacity[0].as_f64() == Some(value) {
            return self.retain_command(request, fingerprint, false);
        }
        motion.opacity[0] = payload.value;
        if self.owner.commit(document).is_err() {
            return self.retain_command_failure(
                request,
                fingerprint,
                DispatchErrorCode::Internal,
                "The native content revision could not advance.",
            );
        }
        self.retain_command(request, fingerprint, true)
    }

    fn query_opacity(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
    ) -> ResponseEnvelope {
        if request.expected_revision.is_some() {
            return self.retain_failure(
                request,
                fingerprint,
                DispatchErrorCode::InvalidRequest,
                "A query must not carry expectedRevision.",
            );
        }
        let payload: QueryOpacity = match serde_json::from_value(request.payload.clone()) {
            Ok(payload) => payload,
            Err(_) => {
                return self.retain_failure(
                    request,
                    fingerprint,
                    DispatchErrorCode::InvalidRequest,
                    "Invalid opacity query payload.",
                );
            }
        };
        let revision = payload
            .at_revision
            .unwrap_or_else(|| self.owner.content_revision());
        let Some(snapshot) = self.owner.acquire(revision) else {
            return self.retain_failure(
                request,
                fingerprint,
                DispatchErrorCode::NotFound,
                "The selected content revision is unavailable.",
            );
        };
        let Some(value) = snapshot.static_opacity(&payload.stable_target.layer_uid) else {
            return self.retain_failure(
                request,
                fingerprint,
                DispatchErrorCode::NotFound,
                "The stable layer or its static opacity was not found.",
            );
        };
        let disposition = ReceiptDisposition::OpacityQuerySuccess {
            content_revision: self.owner.content_revision(),
            at_revision: revision,
            layer_uid: payload.stable_target.layer_uid,
            value: serde_json::Number::from_f64(value).expect("admitted opacity is finite"),
        };
        self.retain_disposition(request, fingerprint, disposition)
    }

    fn query_revision(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
    ) -> ResponseEnvelope {
        if request.expected_revision.is_some() {
            return self.retain_failure(
                request,
                fingerprint,
                DispatchErrorCode::InvalidRequest,
                "A query must not carry expectedRevision.",
            );
        }
        let payload: RevisionQuery = match serde_json::from_value(request.payload.clone()) {
            Ok(payload) => payload,
            Err(_) => {
                return self.retain_failure(
                    request,
                    fingerprint,
                    DispatchErrorCode::InvalidRequest,
                    "Invalid revision query payload.",
                );
            }
        };
        let disposition = ReceiptDisposition::RevisionQuerySuccess {
            content_revision: self.owner.content_revision(),
            frame: payload.frame,
            context_id: payload.context_id,
            view_generation: payload.view_generation,
        };
        self.retain_disposition(request, fingerprint, disposition)
    }

    fn query_snapshot(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
    ) -> ResponseEnvelope {
        if request.expected_revision.is_some() {
            return self.retain_failure(
                request,
                fingerprint,
                DispatchErrorCode::InvalidRequest,
                "A query must not carry expectedRevision.",
            );
        }
        let payload: SnapshotQuery = match serde_json::from_value(request.payload.clone()) {
            Ok(payload) => payload,
            Err(_) => {
                return self.retain_failure(
                    request,
                    fingerprint,
                    DispatchErrorCode::InvalidRequest,
                    "Invalid snapshot query payload.",
                );
            }
        };
        let revision = payload
            .at_revision
            .unwrap_or_else(|| self.owner.content_revision());
        let Some(snapshot) = self.owner.acquire(revision) else {
            return self.retain_failure(
                request,
                fingerprint,
                DispatchErrorCode::NotFound,
                "The selected content revision is unavailable.",
            );
        };
        let disposition = ReceiptDisposition::SnapshotQuerySuccess {
            content_revision: self.owner.content_revision(),
            at_revision: revision,
            document_snapshot_id: snapshot.id().to_owned(),
        };
        self.retain_disposition(request, fingerprint, disposition)
    }

    fn invalid(&self, request: &OpacityRequest, message: &str) -> ResponseEnvelope {
        self.failure(request, DispatchErrorCode::InvalidRequest, message, None)
    }

    fn retain_command(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
        applied: bool,
    ) -> ResponseEnvelope {
        self.retain_disposition(
            request,
            fingerprint,
            ReceiptDisposition::success(self.owner.content_revision(), applied),
        )
    }

    fn retain_command_failure(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
        code: DispatchErrorCode,
        message: &'static str,
    ) -> ResponseEnvelope {
        self.retain_failure(request, fingerprint, code, message)
    }

    fn retain_failure(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
        code: DispatchErrorCode,
        message: &'static str,
    ) -> ResponseEnvelope {
        self.retain_disposition(
            request,
            fingerprint,
            ReceiptDisposition::failure(self.owner.content_revision(), code, message),
        )
    }

    fn retain_disposition(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
        disposition: ReceiptDisposition,
    ) -> ResponseEnvelope {
        let response = self.retained_receipt(request, disposition.clone());
        self.receipts
            .retain(request.request_id.clone(), fingerprint, disposition);
        response
    }

    fn retained_receipt(
        &self,
        request: &OpacityRequest,
        disposition: ReceiptDisposition,
    ) -> ResponseEnvelope {
        disposition.response(request, self.owner.instance_id(), self.owner.document_id())
    }

    fn failure(
        &self,
        request: &OpacityRequest,
        code: DispatchErrorCode,
        message: &str,
        details: Option<Value>,
    ) -> ResponseEnvelope {
        ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: self.owner.instance_id().to_owned(),
            document_id: self.owner.document_id().to_owned(),
            content_revision: self.owner.content_revision(),
            ok: false,
            result: None,
            error: Some(DispatchError {
                code,
                message: message.to_owned(),
                details,
            }),
        }
    }
}
