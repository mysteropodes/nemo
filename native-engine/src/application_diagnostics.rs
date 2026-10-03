//! Bounded terminal metadata at the authority boundary, never request/response payloads.
use crate::application::{ExportResourceResolver, NativeApplication};
use crate::commands::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use crate::export_job::{ExportCompositor, StagedArtifactPort};
use crate::protocol;
use serde::Serialize;
use serde_json::json;
use std::collections::VecDeque;

pub(super) const QUERY: &str = "query.diagnostics.recent";
const CAPACITY: usize = 32;
const MAX_RESPONSE_BYTES: usize = 4096;
const MAX_SEQUENCE: u64 = 9_007_199_254_740_991;

#[derive(Default)]
pub(super) struct RecentDiagnostics {
    records: VecDeque<Record>,
    sequence: u64,
    truncated: bool,
}

impl RecentDiagnostics {
    #[cfg(test)]
    pub(super) fn retained_len(&self) -> usize {
        self.records.len()
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    sequence: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_id_redacted: Option<bool>,
    operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_id: Option<String>,
    content_revision: u64,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error_code: Option<DispatchErrorCode>,
}

fn records_operation(operation: &str) -> bool {
    matches!(
        operation,
        "command.document.apply"
            | "transaction.begin"
            | "transaction.update"
            | "transaction.commit"
            | "transaction.cancel"
            | "history.undo"
            | "history.redo"
    )
}

fn identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._:/-".contains(&byte))
}

impl<P: StagedArtifactPort, C: ExportCompositor, R: ExportResourceResolver>
    NativeApplication<P, C, R>
{
    /// Each admitted attempt has one terminal record. Retried writes keep the
    /// original response revision; sequence orders attempts, not document edits.
    pub fn dispatch(&mut self, request: OpacityRequest) -> ResponseEnvelope {
        let capture = self.prepare_reproduction(&request);
        let eligible = self.release.is_none()
            && self.replacement.is_none()
            && records_operation(&request.operation)
            && identifier(&request.request_id)
            && protocol::preflight_identity(
                &request,
                self.instance_id(),
                self.document_id(),
                self.content_revision(),
            )
            .is_none();
        let metadata = eligible.then(|| {
            let target = request
                .payload
                .get("stableTarget")
                .and_then(|value| value.get("layerUid"))
                .and_then(|value| value.as_str())
                .filter(|value| identifier(value))
                .filter(|value| {
                    self.history
                        .acquire_snapshot(self.content_revision())
                        .is_some_and(|snapshot| {
                            snapshot
                                .document()
                                .layers()
                                .iter()
                                .any(|layer| layer.layer_uid() == *value)
                        })
                })
                .map(str::to_owned)
                .or_else(|| {
                    request
                        .payload
                        .get("transactionId")
                        .and_then(|value| value.as_str())
                        .and_then(|id| self.history.transactions.get(id))
                        .map(|record| record.layer_uid.clone())
                })
                // Imported identifiers can contain path syntax. Targets are
                // optional; never copy such document-derived labels into trace.
                .filter(|value| identifier(value) && !value.contains([':', '/']));
            (
                (!request.request_id.contains([':', '/'])).then(|| request.request_id.clone()),
                request.operation.clone(),
                target,
            )
        });
        let response = self.dispatch_inner(request);
        self.finish_reproduction(capture, &response);
        if let Some((request_id, operation, target_id)) = metadata {
            if self.diagnostics.sequence < MAX_SEQUENCE {
                self.diagnostics.sequence += 1;
                if self.diagnostics.records.len() == CAPACITY {
                    self.diagnostics.records.pop_front();
                    self.diagnostics.truncated = true;
                }
                self.diagnostics.records.push_back(Record {
                    sequence: self.diagnostics.sequence,
                    request_id_redacted: request_id.is_none().then_some(true),
                    request_id,
                    operation,
                    target_id,
                    content_revision: response.content_revision(),
                    ok: response.is_ok(),
                    error_code: response.error().map(|error| error.code()),
                });
            } else {
                self.diagnostics.truncated = true;
            }
        }
        response
    }

    /// Inspection has no receipt: repeated queries cannot grow any store. The
    /// caller already passed lifecycle/identity and retained-write collision checks.
    pub(super) fn recent_diagnostics(&self, request: &OpacityRequest) -> ResponseEnvelope {
        if request.expected_revision.is_some()
            || request
                .payload
                .as_object()
                .is_none_or(|object| !object.is_empty())
        {
            return self.failure(
                request,
                DispatchErrorCode::InvalidRequest,
                "Diagnostics requires an empty payload and forbids expectedRevision.",
            );
        }
        let mut records = self.diagnostics.records.iter().collect::<Vec<_>>();
        let mut truncated = self.diagnostics.truncated;
        loop {
            let response = ResponseEnvelope {
                api_version: 2,
                request_id: request.request_id.clone(),
                instance_id: self.instance_id().into(),
                document_id: self.document_id().into(),
                content_revision: self.content_revision(),
                ok: true,
                result: Some(json!({"records": records, "truncated": truncated})),
                error: None,
            };
            if serde_json::to_vec(&response)
                .expect("bounded metadata is serializable")
                .len()
                <= MAX_RESPONSE_BYTES
            {
                return response;
            }
            // Valid envelope identifiers fit even when the projection is empty.
            records.remove(0);
            truncated = true;
        }
    }
}
