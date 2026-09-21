//! N09 history-aware facade over the N08 native opacity application.

use crate::commands::{
    DispatchError, DispatchErrorCode, NativeOpacityApplication, OpacityRequest, ResponseEnvelope,
    APPLICATION_API_VERSION, OP_COMMAND_APPLY, OP_QUERY_OPACITY, OP_QUERY_REVISION,
    OP_QUERY_SNAPSHOT,
};
use crate::document::OpacityDocument;
use crate::request_receipts::{ApplyOpacity, RequestFingerprint};
use crate::revision::DocumentSnapshot;
use crate::transaction::{
    transaction_id, CompactFailure, CompactResult, ReceiptLookup, RetainedDisposition,
    StageReceipts, TerminalDisposition, Transactions, OP_HISTORY_REDO, OP_HISTORY_UNDO,
    OP_TRANSACTION_BEGIN, OP_TRANSACTION_CANCEL, OP_TRANSACTION_COMMIT, OP_TRANSACTION_STATUS,
    OP_TRANSACTION_UPDATE,
};
use serde_json::{json, Number, Value};

#[derive(Debug, Clone)]
struct HistoryEntry {
    layer_uid: String,
    before_revision: u64,
    after_revision: u64,
}

/// Staged owner for N09 only. Production JavaScript remains authoritative until N20.
#[derive(Debug)]
pub struct NativeOpacityHistory {
    application: NativeOpacityApplication,
    pub(crate) transactions: Transactions,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
    receipts: StageReceipts,
}

impl NativeOpacityHistory {
    pub fn new(
        instance_id: impl Into<String>,
        document: OpacityDocument,
    ) -> Result<Self, &'static str> {
        Ok(Self {
            application: NativeOpacityApplication::new(instance_id, document)?,
            transactions: Transactions::default(),
            undo: Vec::new(),
            redo: Vec::new(),
            receipts: StageReceipts::default(),
        })
    }

    pub fn instance_id(&self) -> &str {
        self.application.instance_id()
    }

    pub fn document_id(&self) -> &str {
        self.application.document_id()
    }

    pub fn content_revision(&self) -> u64 {
        self.application.content_revision()
    }

    pub fn acquire_snapshot(&self, revision: u64) -> Option<DocumentSnapshot> {
        self.application.acquire_snapshot(revision)
    }

    #[cfg(test)]
    pub(crate) fn test_application_mut(&mut self) -> &mut NativeOpacityApplication {
        &mut self.application
    }

    pub fn history_depths(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }

    pub fn replace_document(&mut self, document: OpacityDocument) -> Result<(), &'static str> {
        self.application.replace_document(document)?;
        self.transactions.clear();
        self.undo.clear();
        self.redo.clear();
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
                let disposition = disposition.clone();
                if disposition == RetainedDisposition::Forwarded {
                    return self.application.handle(request);
                }
                return disposition
                    .response(&request, self.instance_id(), self.document_id())
                    .expect("a native N09 disposition reconstructs a response");
            }
            ReceiptLookup::ChangedBody => {
                return self.failure(
                    &request,
                    DispatchErrorCode::InvalidRequest,
                    "requestId was reused with a changed body.",
                    None,
                );
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
            OP_COMMAND_APPLY => self.apply_command(&request, fingerprint),
            OP_QUERY_OPACITY | OP_QUERY_REVISION | OP_QUERY_SNAPSHOT => {
                self.forward_query(request, fingerprint)
            }
            #[cfg(feature = "application")]
            crate::commands::OP_QUERY_SERIALIZE | crate::commands::OP_QUERY_EVALUATE => {
                self.forward_query(request, fingerprint)
            }
            OP_TRANSACTION_BEGIN => self.begin(&request, fingerprint),
            OP_TRANSACTION_UPDATE => self.update(&request, fingerprint),
            OP_TRANSACTION_COMMIT => self.commit(&request, fingerprint),
            OP_TRANSACTION_CANCEL => self.cancel(&request, fingerprint),
            OP_TRANSACTION_STATUS => self.status(&request, fingerprint),
            OP_HISTORY_UNDO => self.move_history(&request, fingerprint, true),
            OP_HISTORY_REDO => self.move_history(&request, fingerprint, false),
            _ => self.retain_failure(&request, fingerprint, DispatchErrorCode::InvalidRequest),
        }
    }

    fn forward_query(
        &mut self,
        request: OpacityRequest,
        fingerprint: RequestFingerprint,
    ) -> ResponseEnvelope {
        let response = self.application.handle(request.clone());
        self.receipts.retain(
            request.request_id,
            fingerprint,
            RetainedDisposition::Forwarded,
        );
        response
    }

    fn apply_command(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
    ) -> ResponseEnvelope {
        if self.transactions.has_active() {
            return self.retain_failure(request, fingerprint, DispatchErrorCode::BusyConflict);
        }
        let parsed = serde_json::from_value::<ApplyOpacity>(request.payload.clone()).ok();
        let before_revision = self.content_revision();
        let response = self.application.handle(request.clone());
        if !response.is_ok() {
            return self.retain_inner_failure(request, fingerprint, response);
        }
        let applied = response
            .result()
            .and_then(|value| value["applied"].as_bool())
            == Some(true);
        let added = if applied {
            let payload = parsed.expect("N08 accepted only a valid opacity payload");
            self.undo.push(HistoryEntry {
                layer_uid: payload.stable_target.layer_uid,
                before_revision,
                after_revision: self.content_revision(),
            });
            self.redo.clear();
            1
        } else {
            0
        };
        self.retain_success(
            request,
            fingerprint,
            CompactResult::Command {
                applied,
                history_entries_added: added,
            },
        )
    }

    fn commit(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
    ) -> ResponseEnvelope {
        if request.expected_revision.is_some() {
            return self.retain_failure(request, fingerprint, DispatchErrorCode::InvalidRequest);
        }
        let payload = match transaction_id(request) {
            Some(payload) => payload,
            None => {
                return self.retain_failure(request, fingerprint, DispatchErrorCode::InvalidRequest)
            }
        };
        let Some(record) = self.transactions.get(&payload.transaction_id).cloned() else {
            return self.retain_failure(request, fingerprint, DispatchErrorCode::NotFound);
        };
        if self.transactions.active_id() != Some(record.id.as_str()) {
            return self.retain_failure(request, fingerprint, DispatchErrorCode::NotFound);
        }
        if record.base_revision != self.content_revision() {
            return self.retain_failure(request, fingerprint, DispatchErrorCode::StaleRevision);
        }
        if record.base_value == record.working_value {
            return self.retain_failure(request, fingerprint, DispatchErrorCode::InvalidRequest);
        }
        let command = self.internal_opacity_command(
            &request.request_id,
            record.base_revision,
            &record.layer_uid,
            record.working_value.clone(),
        );
        let response = self.application.handle(command);
        if !response.is_ok() {
            return self.retain_inner_failure(request, fingerprint, response);
        }
        let committed_revision = self.content_revision();
        self.undo.push(HistoryEntry {
            layer_uid: record.layer_uid.clone(),
            before_revision: record.base_revision,
            after_revision: committed_revision,
        });
        self.redo.clear();
        let finished = self
            .transactions
            .finish(
                &record.id,
                TerminalDisposition::Succeeded,
                Some(committed_revision),
            )
            .expect("the active transaction remains owned through commit");
        self.retain_success(request, fingerprint, CompactResult::Transaction(finished))
    }

    fn move_history(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
        undo: bool,
    ) -> ResponseEnvelope {
        if self.transactions.has_active() {
            return self.retain_failure(request, fingerprint, DispatchErrorCode::BusyConflict);
        }
        if let Some(response) = self.require_current_revision(request, &fingerprint) {
            return response;
        }
        if !request
            .payload
            .as_object()
            .is_some_and(|payload| payload.is_empty())
        {
            return self.retain_failure(request, fingerprint, DispatchErrorCode::InvalidRequest);
        }
        let entry = if undo {
            self.undo.last().cloned()
        } else {
            self.redo.last().cloned()
        };
        let Some(entry) = entry else {
            return self.retain_failure(request, fingerprint, DispatchErrorCode::Unavailable);
        };
        let source_revision = if undo {
            entry.before_revision
        } else {
            entry.after_revision
        };
        let value = self
            .opacity_at(source_revision, &entry.layer_uid)
            .expect("history revisions retain immutable opacity snapshots");
        let command = self.internal_opacity_command(
            &request.request_id,
            self.content_revision(),
            &entry.layer_uid,
            value,
        );
        let response = self.application.handle(command);
        if !response.is_ok() {
            return self.retain_inner_failure(request, fingerprint, response);
        }
        debug_assert_eq!(entry.after_revision, entry.before_revision + 1);
        if undo {
            self.undo.pop();
            self.redo.push(entry);
        } else {
            self.redo.pop();
            self.undo.push(entry);
        }
        self.retain_success(
            request,
            fingerprint,
            CompactResult::Command {
                applied: true,
                history_entries_added: 0,
            },
        )
    }

    pub(crate) fn require_current_revision(
        &mut self,
        request: &OpacityRequest,
        fingerprint: &RequestFingerprint,
    ) -> Option<ResponseEnvelope> {
        let code = match request.expected_revision {
            None => Some(DispatchErrorCode::InvalidRequest),
            Some(revision) if revision != self.content_revision() => {
                Some(DispatchErrorCode::StaleRevision)
            }
            Some(_) => None,
        };
        code.map(|code| self.retain_failure(request, fingerprint.clone(), code))
    }

    pub(crate) fn opacity_at(
        &self,
        revision: u64,
        layer_uid: &str,
    ) -> Result<Number, DispatchErrorCode> {
        let snapshot = self
            .acquire_snapshot(revision)
            .ok_or(DispatchErrorCode::NotFound)?;
        let layer = snapshot
            .document()
            .layers()
            .iter()
            .find(|layer| layer.layer_uid() == layer_uid)
            .ok_or(DispatchErrorCode::NotFound)?;
        layer
            .static_opacity()
            .cloned()
            .ok_or(DispatchErrorCode::Unavailable)
    }

    fn internal_opacity_command(
        &self,
        request_id: &str,
        expected_revision: u64,
        layer_uid: &str,
        value: Number,
    ) -> OpacityRequest {
        OpacityRequest::command(
            request_id,
            self.instance_id(),
            self.document_id(),
            expected_revision,
            json!({
                "command": "layer.opacity.set",
                "stableTarget": { "layerUid": layer_uid },
                "value": value
            }),
        )
    }

    pub(crate) fn retain_success(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
        result: CompactResult,
    ) -> ResponseEnvelope {
        self.retain(
            request,
            fingerprint,
            RetainedDisposition::Exact {
                content_revision: self.content_revision(),
                outcome: Ok(result),
            },
        )
    }

    fn retain_inner_failure(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
        response: ResponseEnvelope,
    ) -> ResponseEnvelope {
        let error = response
            .error
            .expect("an unsuccessful N08 response carries an error");
        self.retain(
            request,
            fingerprint,
            RetainedDisposition::Exact {
                content_revision: response.content_revision,
                outcome: Err(CompactFailure {
                    code: error.code,
                    message: error.message,
                    details: error.details,
                }),
            },
        )
    }

    pub(crate) fn retain_failure(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
        code: DispatchErrorCode,
    ) -> ResponseEnvelope {
        self.retain(
            request,
            fingerprint,
            RetainedDisposition::Exact {
                content_revision: self.content_revision(),
                outcome: Err(CompactFailure {
                    code,
                    message: error_message(code).to_owned(),
                    details: None,
                }),
            },
        )
    }

    fn retain(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
        disposition: RetainedDisposition,
    ) -> ResponseEnvelope {
        let response = disposition
            .response(request, self.instance_id(), self.document_id())
            .expect("only native N09 dispositions are retained here");
        self.receipts
            .retain(request.request_id.clone(), fingerprint, disposition);
        response
    }

    pub(crate) fn failure(
        &self,
        request: &OpacityRequest,
        code: DispatchErrorCode,
        message: &str,
        details: Option<Value>,
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
                message: message.to_owned(),
                details,
            }),
        }
    }
}

fn error_message(code: DispatchErrorCode) -> &'static str {
    match code {
        DispatchErrorCode::InvalidRequest => "Invalid native opacity history stage.",
        DispatchErrorCode::StaleRevision => "Read the current content revision before writing.",
        DispatchErrorCode::BusyConflict => "An opacity transaction owns the document.",
        DispatchErrorCode::Unavailable => "No native opacity history entry is available.",
        DispatchErrorCode::NotFound => "The transaction was not found or is no longer active.",
        DispatchErrorCode::Internal => "The native opacity history stage failed.",
        _ => "The native opacity history request failed.",
    }
}
