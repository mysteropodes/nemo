use crate::commands::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use crate::document::OpacityDocument;
use crate::export_job::{
    ExportBegin, ExportCompositor, ExportFrameInput, ExportJobError, ExportJobErrorKind,
    ExportJobManager, JobReceipt, PendingFrame, ReconciliationStage, StagedArtifactPort,
};
pub use crate::export_job_lifecycle::ApplicationReleaseReceipt;
use crate::history::NativeOpacityHistory;
use crate::protocol::{
    self, ExportBeginPayload, JobIdPayload, OpaqueResourceHandle, OP_JOB_EXPORT_PNG_BEGIN,
    OP_JOB_EXPORT_PNG_CANCEL, OP_JOB_EXPORT_PNG_STATUS,
};
use crate::render_scene::GeometryPaintInput;
use crate::request_receipts::RequestFingerprint;
use crate::revision::DocumentSnapshot;
use crate::transaction::TerminalDisposition;
use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceResolutionErrorKind {
    InvalidRequest,
    Unavailable,
    NotFound,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceResolutionError {
    kind: ResourceResolutionErrorKind,
    message: String,
}

impl ResourceResolutionError {
    pub fn new(kind: ResourceResolutionErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

pub trait ExportResourceResolver {
    fn resolve_geometry(
        &mut self,
        handle: &OpaqueResourceHandle,
    ) -> Result<GeometryPaintInput, ResourceResolutionError>;
}

#[derive(Debug, Clone)]
enum RecordedRequest {
    History(RequestFingerprint),
    JobBegin(RequestFingerprint, u64, JobReceipt),
    JobReceipt(RequestFingerprint, u64, JobReceipt),
    Failure(RequestFingerprint, u64, DispatchErrorCode, String),
}

impl RecordedRequest {
    fn fingerprint(&self) -> &RequestFingerprint {
        match self {
            Self::History(value)
            | Self::JobBegin(value, _, _)
            | Self::JobReceipt(value, _, _)
            | Self::Failure(value, _, _, _) => value,
        }
    }
}

pub struct NativeApplication<P, C, R> {
    history: NativeOpacityHistory,
    exports: ExportJobManager<P, C>,
    resources: R,
    requests: HashMap<String, RecordedRequest>,
    release: Option<ApplicationReleaseReceipt>,
}

impl<P: StagedArtifactPort, C: ExportCompositor, R: ExportResourceResolver>
    NativeApplication<P, C, R>
{
    pub fn new(
        instance_id: impl Into<String>,
        document: OpacityDocument,
        artifact_port: P,
        compositor: C,
        resources: R,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            history: NativeOpacityHistory::new(instance_id, document)?,
            exports: ExportJobManager::new(artifact_port, compositor),
            resources,
            requests: HashMap::new(),
            release: None,
        })
    }

    pub fn instance_id(&self) -> &str {
        self.history.instance_id()
    }

    pub fn document_id(&self) -> &str {
        self.history.document_id()
    }

    pub fn content_revision(&self) -> u64 {
        self.history.content_revision()
    }

    pub fn acquire_snapshot(&self, revision: u64) -> Option<DocumentSnapshot> {
        self.history.acquire_snapshot(revision)
    }

    pub fn dispatch(&mut self, request: OpacityRequest) -> ResponseEnvelope {
        if self.release.is_some() {
            return self.failure(
                &request,
                DispatchErrorCode::Unavailable,
                "Native application authority has been released.",
            );
        }
        if let Some(response) = protocol::preflight_identity(
            &request,
            self.instance_id(),
            self.document_id(),
            self.content_revision(),
        ) {
            return response;
        }
        let fingerprint = RequestFingerprint::new(&request)
            .expect("preflight admitted one bounded serializable request");
        if let Some(recorded) = self.requests.get(&request.request_id).cloned() {
            if recorded.fingerprint() != &fingerprint {
                return self.failure(
                    &request,
                    DispatchErrorCode::InvalidRequest,
                    "requestId was reused with a changed body.",
                );
            }
            return match recorded {
                RecordedRequest::History(_) => self.history.handle(request),
                RecordedRequest::JobBegin(_, revision, receipt) => protocol::job_response(
                    &request,
                    self.instance_id(),
                    self.document_id(),
                    revision,
                    &receipt,
                ),
                RecordedRequest::JobReceipt(_, revision, receipt) => protocol::job_response(
                    &request,
                    self.instance_id(),
                    self.document_id(),
                    revision,
                    &receipt,
                ),
                RecordedRequest::Failure(_, revision, code, message) => protocol::failure(
                    &request,
                    self.instance_id(),
                    self.document_id(),
                    revision,
                    code,
                    message,
                    None,
                ),
            };
        }
        if request.cancelled_before_dispatch {
            return self.failure(
                &request,
                DispatchErrorCode::CancelledBeforeDispatch,
                "Request was cancelled before dispatch.",
            );
        }

        match request.operation.as_str() {
            OP_JOB_EXPORT_PNG_BEGIN => self.begin_export(&request, fingerprint),
            OP_JOB_EXPORT_PNG_STATUS => self.job_stage(&request, fingerprint, false),
            OP_JOB_EXPORT_PNG_CANCEL => self.job_stage(&request, fingerprint, true),
            _ => self.history_stage(request, fingerprint),
        }
    }

    pub fn run_export_to_completion(&mut self, job_id: &str) -> Result<JobReceipt, ExportJobError> {
        self.require_active()?;
        self.exports.run_to_completion(job_id)
    }

    pub fn start_next_export_frame(
        &mut self,
        job_id: &str,
    ) -> Result<Option<PendingFrame>, ExportJobError> {
        self.require_active()?;
        self.exports.start_next_frame(job_id)
    }

    pub fn finish_export_frame(
        &mut self,
        pending: PendingFrame,
    ) -> Result<JobReceipt, ExportJobError> {
        self.exports.finish_frame(pending)
    }

    pub fn replace_document(
        &mut self,
        document: OpacityDocument,
    ) -> Result<Vec<JobReceipt>, String> {
        if self.release.is_some() {
            return Err("native application authority has been released".into());
        }
        self.history
            .replace_document(document)
            .map_err(str::to_owned)?;
        let document_id = self.history.document_id().to_owned();
        let reconciled = self
            .exports
            .replace_document(&document_id)
            .map_err(|error| error.message)?;
        self.requests.clear();
        Ok(reconciled)
    }

    pub fn export_receipt(&self, job_id: &str) -> Result<JobReceipt, ExportJobError> {
        self.exports.status(job_id)
    }

    pub fn artifact_port(&self) -> &P {
        self.exports.port()
    }

    pub fn resource_resolver_mut(&mut self) -> &mut R {
        &mut self.resources
    }

    pub fn release_authority(&mut self) -> ApplicationReleaseReceipt {
        self.release_transaction_stage();
        self.release_export_stage()
    }

    pub fn release_transaction_stage(&mut self) -> ApplicationReleaseReceipt {
        if matches!(self.release.as_ref(), Some(receipt) if receipt.transaction_stage == ReconciliationStage::Complete)
        {
            return self.release.clone().unwrap();
        }
        let (undo_depth, redo_depth) = self.history.history_depths();
        let cancelled_transaction_id = self.history.transactions.active_id().map(str::to_owned);
        let mut receipt = ApplicationReleaseReceipt {
            instance_id: self.instance_id().to_owned(),
            document_id: self.document_id().to_owned(),
            content_revision: self.content_revision(),
            cancelled_transaction_id: cancelled_transaction_id.clone(),
            cancelled_transaction: None,
            undo_depth,
            redo_depth,
            transaction_stage: ReconciliationStage::Pending,
            export_stage: ReconciliationStage::Pending,
            exports: self.exports.release_snapshot(),
        };
        self.release = Some(receipt.clone());
        let cancelled_transaction = cancelled_transaction_id.as_deref().map(|id| {
            let record = self
                .history
                .transactions
                .finish(id, TerminalDisposition::Cancelled, None)
                .expect("the observed active transaction cancels without commit");
            let mut result = record.result();
            result.as_object_mut().unwrap().insert(
                "committedValue".into(),
                serde_json::Value::Number(record.base_value),
            );
            result
        });
        receipt.cancelled_transaction = cancelled_transaction;
        (receipt.undo_depth, receipt.redo_depth) = self.history.history_depths();
        receipt.transaction_stage = ReconciliationStage::Complete;
        self.release = Some(receipt.clone());
        receipt
    }

    pub fn release_export_stage(&mut self) -> ApplicationReleaseReceipt {
        self.release_export_stage_with_checkpoint(|| {})
    }

    pub fn release_export_stage_with_checkpoint<F: FnMut()>(
        &mut self,
        checkpoint: F,
    ) -> ApplicationReleaseReceipt {
        let mut receipt = self.release_transaction_stage();
        if receipt.export_stage == ReconciliationStage::Complete {
            return receipt;
        }
        receipt.exports = self.exports.reconcile_release_with(checkpoint);
        receipt.export_stage = ReconciliationStage::Complete;
        self.requests.clear();
        self.release = Some(receipt.clone());
        receipt
    }

    pub fn release_progress(&self) -> Option<ApplicationReleaseReceipt> {
        let mut receipt = self.release.clone()?;
        if receipt.export_stage == ReconciliationStage::Pending {
            receipt.exports = self.exports.release_snapshot();
        }
        Some(receipt)
    }

    fn history_stage(
        &mut self,
        request: OpacityRequest,
        fingerprint: RequestFingerprint,
    ) -> ResponseEnvelope {
        let response = self.history.handle(request.clone());
        self.requests
            .insert(request.request_id, RecordedRequest::History(fingerprint));
        response
    }

    fn begin_export(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
    ) -> ResponseEnvelope {
        if request.expected_revision.is_none() {
            return self.retain_failure(
                request,
                fingerprint,
                DispatchErrorCode::InvalidRequest,
                "A PNG job begin requires expectedRevision.",
            );
        }
        if request.expected_revision != Some(self.content_revision()) {
            return self.retain_failure(
                request,
                fingerprint,
                DispatchErrorCode::StaleRevision,
                "Read the current content revision before beginning export.",
            );
        }
        let payload = match ExportBeginPayload::parse(request.payload.clone()) {
            Ok(payload) => payload,
            Err(message) => {
                return self.retain_failure(
                    request,
                    fingerprint,
                    DispatchErrorCode::InvalidRequest,
                    message,
                )
            }
        };
        let mut frames = Vec::with_capacity(payload.frames.len());
        for frame in payload.frames {
            let geometry = match self.resources.resolve_geometry(&frame.geometry_handle) {
                Ok(value)
                    if value.resource_id() == frame.geometry_handle.resource_id()
                        && value.resource_version() == frame.geometry_handle.resource_version() =>
                {
                    value
                }
                Ok(_) => {
                    return self.retain_failure(
                        request,
                        fingerprint,
                        DispatchErrorCode::Internal,
                        "Resource resolver returned different opaque identity.",
                    )
                }
                Err(error) => {
                    return self.retain_failure(
                        request,
                        fingerprint,
                        resolution_code(error.kind),
                        error.message,
                    )
                }
            };
            frames.push(ExportFrameInput::new(frame.source_frame, geometry));
        }
        let begin = ExportBegin {
            request_id: request.request_id.clone(),
            expected_revision: request.expected_revision.unwrap(),
            context_id: payload.context_id,
            quality: payload.quality,
            target: payload.output_handle,
            frames,
        };
        match self.exports.begin(&self.history, begin) {
            Ok(receipt) => {
                let response = self.job_response(request, &receipt);
                self.requests.insert(
                    request.request_id.clone(),
                    RecordedRequest::JobBegin(fingerprint, response.content_revision(), receipt),
                );
                response
            }
            Err(error) => self.retain_export_error(request, fingerprint, error),
        }
    }

    fn job_stage(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
        cancel: bool,
    ) -> ResponseEnvelope {
        if request.expected_revision.is_some() {
            return self.retain_failure(
                request,
                fingerprint,
                DispatchErrorCode::InvalidRequest,
                "Job status and cancel forbid expectedRevision.",
            );
        }
        let payload = match JobIdPayload::parse(request.payload.clone()) {
            Ok(payload) => payload,
            Err(message) => {
                return self.retain_failure(
                    request,
                    fingerprint,
                    DispatchErrorCode::InvalidRequest,
                    message,
                )
            }
        };
        let result = if cancel {
            self.exports.cancel(&payload.job_id)
        } else {
            self.exports.status(&payload.job_id)
        };
        match result {
            Ok(receipt) => {
                let response = self.job_response(request, &receipt);
                self.requests.insert(
                    request.request_id.clone(),
                    RecordedRequest::JobReceipt(fingerprint, response.content_revision(), receipt),
                );
                response
            }
            Err(error) => self.retain_export_error(request, fingerprint, error),
        }
    }

    fn retain_export_error(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
        error: ExportJobError,
    ) -> ResponseEnvelope {
        self.retain_failure(
            request,
            fingerprint,
            export_error_code(error.kind),
            error.message,
        )
    }

    fn retain_failure(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
        code: DispatchErrorCode,
        message: impl Into<String>,
    ) -> ResponseEnvelope {
        let message = message.into();
        let response = self.failure(request, code, &message);
        self.requests.insert(
            request.request_id.clone(),
            RecordedRequest::Failure(fingerprint, response.content_revision(), code, message),
        );
        response
    }

    fn failure(
        &self,
        request: &OpacityRequest,
        code: DispatchErrorCode,
        message: impl Into<String>,
    ) -> ResponseEnvelope {
        protocol::failure(
            request,
            self.instance_id(),
            self.document_id(),
            self.content_revision(),
            code,
            message,
            None,
        )
    }

    fn job_response(&self, request: &OpacityRequest, receipt: &JobReceipt) -> ResponseEnvelope {
        protocol::job_response(
            request,
            self.instance_id(),
            self.document_id(),
            self.content_revision(),
            receipt,
        )
    }

    fn require_active(&self) -> Result<(), ExportJobError> {
        if self.release.is_some() {
            Err(ExportJobError {
                kind: ExportJobErrorKind::Released,
                message: "native application authority has been released".into(),
            })
        } else {
            Ok(())
        }
    }
}

fn resolution_code(kind: ResourceResolutionErrorKind) -> DispatchErrorCode {
    match kind {
        ResourceResolutionErrorKind::InvalidRequest => DispatchErrorCode::InvalidRequest,
        ResourceResolutionErrorKind::Unavailable => DispatchErrorCode::Unavailable,
        ResourceResolutionErrorKind::NotFound => DispatchErrorCode::NotFound,
        ResourceResolutionErrorKind::Internal => DispatchErrorCode::Internal,
    }
}

fn export_error_code(kind: ExportJobErrorKind) -> DispatchErrorCode {
    match kind {
        ExportJobErrorKind::InvalidRequest | ExportJobErrorKind::ChangedRequestId => {
            DispatchErrorCode::InvalidRequest
        }
        ExportJobErrorKind::Busy => DispatchErrorCode::BusyConflict,
        ExportJobErrorKind::NotFound => DispatchErrorCode::NotFound,
        ExportJobErrorKind::WrongDocument => DispatchErrorCode::WrongDocument,
        ExportJobErrorKind::Released => DispatchErrorCode::Unavailable,
    }
}
