//! Transaction state and compact request receipts for the native opacity history facade.

use crate::commands::{
    DispatchError, DispatchErrorCode, OpacityRequest, ResponseEnvelope, APPLICATION_API_VERSION,
};
use crate::history::NativeOpacityHistory;
use crate::request_receipts::RequestFingerprint;
use serde::Deserialize;
use serde_json::{json, Number, Value};
use std::collections::HashMap;

pub const OP_TRANSACTION_BEGIN: &str = "transaction.begin";
pub const OP_TRANSACTION_UPDATE: &str = "transaction.update";
pub const OP_TRANSACTION_COMMIT: &str = "transaction.commit";
pub const OP_TRANSACTION_CANCEL: &str = "transaction.cancel";
pub const OP_TRANSACTION_STATUS: &str = "transaction.status";
pub const OP_HISTORY_UNDO: &str = "history.undo";
pub const OP_HISTORY_REDO: &str = "history.redo";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct BeginPayload {
    stable_target: crate::request_receipts::StableTarget,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct UpdatePayload {
    transaction_id: String,
    value: Number,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct TransactionIdPayload {
    pub(crate) transaction_id: String,
}

impl OpacityRequest {
    pub fn history_stage(
        request_id: impl Into<String>,
        instance_id: impl Into<String>,
        document_id: impl Into<String>,
        expected_revision: Option<u64>,
        operation: impl Into<String>,
        payload: Value,
    ) -> Self {
        Self {
            api_version: APPLICATION_API_VERSION,
            request_id: request_id.into(),
            instance_id: instance_id.into(),
            document_id: document_id.into(),
            expected_revision,
            operation: operation.into(),
            payload,
            cancelled_before_dispatch: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TerminalDisposition {
    Succeeded,
    Cancelled,
}

impl TerminalDisposition {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransactionRecord {
    pub(crate) id: String,
    pub(crate) base_revision: u64,
    pub(crate) layer_uid: String,
    pub(crate) base_value: Number,
    pub(crate) working_value: Number,
    pub(crate) working_generation: u64,
    pub(crate) committed_revision: Option<u64>,
    pub(crate) terminal: Option<TerminalDisposition>,
}

impl TransactionRecord {
    pub(crate) fn result(&self) -> Value {
        let history_entries_added =
            u64::from(self.terminal == Some(TerminalDisposition::Succeeded));
        json!({
            "transactionId": self.id,
            "baseRevision": self.base_revision,
            "workingGeneration": self.working_generation,
            "workingState": {
                "stableTarget": { "layerUid": self.layer_uid },
                "value": self.working_value
            },
            "committedRevision": self.committed_revision,
            "terminalDisposition": self.terminal.as_ref().map(TerminalDisposition::as_str),
            "historyEntriesAdded": history_entries_added
        })
    }
}

#[derive(Debug, Default)]
pub(crate) struct Transactions {
    next_id: u64,
    active_id: Option<String>,
    records: HashMap<String, TransactionRecord>,
}

impl Transactions {
    pub(crate) fn has_active(&self) -> bool {
        self.active_id.is_some()
    }

    pub(crate) fn active_id(&self) -> Option<&str> {
        self.active_id.as_deref()
    }

    pub(crate) fn begin(
        &mut self,
        base_revision: u64,
        layer_uid: String,
        base_value: Number,
    ) -> Result<TransactionRecord, &'static str> {
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or("transactionId sequence exhausted")?;
        let id = format!("native-transaction-{}", self.next_id);
        let record = TransactionRecord {
            id: id.clone(),
            base_revision,
            layer_uid,
            base_value: base_value.clone(),
            working_value: base_value,
            working_generation: 0,
            committed_revision: None,
            terminal: None,
        };
        self.active_id = Some(id.clone());
        self.records.insert(id, record.clone());
        Ok(record)
    }

    pub(crate) fn get(&self, id: &str) -> Option<&TransactionRecord> {
        self.records.get(id)
    }

    pub(crate) fn update(
        &mut self,
        id: &str,
        value: Number,
    ) -> Result<TransactionRecord, &'static str> {
        if self.active_id.as_deref() != Some(id) {
            return Err("transaction is not the active transaction");
        }
        let record = self
            .records
            .get_mut(id)
            .ok_or("active transaction is missing")?;
        record.working_generation = record
            .working_generation
            .checked_add(1)
            .ok_or("transaction working generation overflow")?;
        record.working_value = value;
        Ok(record.clone())
    }

    pub(crate) fn finish(
        &mut self,
        id: &str,
        disposition: TerminalDisposition,
        committed_revision: Option<u64>,
    ) -> Result<TransactionRecord, &'static str> {
        if self.active_id.as_deref() != Some(id) {
            return Err("transaction is not the active transaction");
        }
        let record = self
            .records
            .get_mut(id)
            .ok_or("active transaction is missing")?;
        record.terminal = Some(disposition);
        record.committed_revision = committed_revision;
        self.active_id = None;
        Ok(record.clone())
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RetainedDisposition {
    Forwarded,
    Exact {
        content_revision: u64,
        outcome: Result<CompactResult, CompactFailure>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CompactFailure {
    pub(crate) code: DispatchErrorCode,
    pub(crate) message: String,
    pub(crate) details: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum CompactResult {
    Command {
        applied: bool,
        history_entries_added: u64,
    },
    Transaction(TransactionRecord),
}

impl RetainedDisposition {
    pub(crate) fn response(
        &self,
        request: &OpacityRequest,
        instance_id: &str,
        document_id: &str,
    ) -> Option<ResponseEnvelope> {
        let (content_revision, outcome) = match self {
            Self::Forwarded => return None,
            Self::Exact {
                content_revision,
                outcome,
            } => (*content_revision, outcome),
        };
        let (ok, result, error) = match outcome {
            Ok(result) => {
                let result = match result {
                    CompactResult::Command {
                        applied,
                        history_entries_added,
                    } => json!({
                        "applied": applied,
                        "historyEntriesAdded": history_entries_added
                    }),
                    CompactResult::Transaction(record) => record.result(),
                };
                (true, Some(result), None)
            }
            Err(failure) => (
                false,
                None,
                Some(DispatchError {
                    code: failure.code,
                    message: failure.message.clone(),
                    details: failure.details.clone(),
                }),
            ),
        };
        Some(ResponseEnvelope {
            api_version: APPLICATION_API_VERSION,
            request_id: request.request_id.clone(),
            instance_id: instance_id.to_owned(),
            document_id: document_id.to_owned(),
            content_revision,
            ok,
            result,
            error,
        })
    }
}

#[derive(Debug)]
struct RetainedEntry(RequestFingerprint, RetainedDisposition);

#[derive(Debug, Default)]
pub(crate) struct StageReceipts {
    entries: HashMap<String, RetainedEntry>,
}

pub(crate) enum ReceiptLookup<'a> {
    Missing,
    Retained(&'a RetainedDisposition),
    ChangedBody,
}

impl StageReceipts {
    pub(crate) fn lookup(
        &self,
        request_id: &str,
        fingerprint: &RequestFingerprint,
    ) -> ReceiptLookup<'_> {
        match self.entries.get(request_id) {
            None => ReceiptLookup::Missing,
            Some(entry) if &entry.0 == fingerprint => ReceiptLookup::Retained(&entry.1),
            Some(_) => ReceiptLookup::ChangedBody,
        }
    }

    pub(crate) fn retain(
        &mut self,
        request_id: String,
        fingerprint: RequestFingerprint,
        disposition: RetainedDisposition,
    ) {
        self.entries
            .insert(request_id, RetainedEntry(fingerprint, disposition));
    }

    pub(crate) fn clear(&mut self) {
        self.entries.clear();
    }
}

impl NativeOpacityHistory {
    pub(crate) fn preflight(&self, request: &OpacityRequest) -> Option<ResponseEnvelope> {
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
        if !matches!(
            serde_json::to_vec(request).map(|bytes| bytes.len()),
            Ok(0..=4096)
        ) {
            return Some(self.failure(
                request,
                DispatchErrorCode::InvalidRequest,
                "Native opacity requests are limited to 4096 encoded bytes.",
                None,
            ));
        }
        if request.instance_id != self.instance_id() {
            return Some(self.failure(
                request,
                DispatchErrorCode::WrongInstance,
                "Request targets a different native engine instance.",
                None,
            ));
        }
        if request.document_id != self.document_id() {
            return Some(self.failure(
                request,
                DispatchErrorCode::WrongDocument,
                "Request targets a replaced document.",
                Some(json!({ "requestedDocumentId": request.document_id })),
            ));
        }
        None
    }

    pub(crate) fn begin(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
    ) -> ResponseEnvelope {
        if self.transactions.has_active() {
            return self.retain_failure(request, fingerprint, DispatchErrorCode::BusyConflict);
        }
        if let Some(response) = self.require_current_revision(request, &fingerprint) {
            return response;
        }
        let payload: BeginPayload = match serde_json::from_value(request.payload.clone()) {
            Ok(payload) => payload,
            Err(_) => {
                return self.retain_failure(request, fingerprint, DispatchErrorCode::InvalidRequest)
            }
        };
        let value = match self.opacity_at(self.content_revision(), &payload.stable_target.layer_uid)
        {
            Ok(value) => value,
            Err(code) => return self.retain_failure(request, fingerprint, code),
        };
        let record = match self.transactions.begin(
            self.content_revision(),
            payload.stable_target.layer_uid,
            value,
        ) {
            Ok(record) => record,
            Err(_) => {
                return self.retain_failure(request, fingerprint, DispatchErrorCode::Internal)
            }
        };
        self.retain_success(request, fingerprint, CompactResult::Transaction(record))
    }

    pub(crate) fn update(
        &mut self,
        request: &OpacityRequest,
        fingerprint: RequestFingerprint,
    ) -> ResponseEnvelope {
        if request.expected_revision.is_some() {
            return self.retain_failure(request, fingerprint, DispatchErrorCode::InvalidRequest);
        }
        let payload: UpdatePayload =
            match serde_json::from_value::<UpdatePayload>(request.payload.clone()) {
                Ok(payload) if valid_opacity(&payload.value) => payload,
                _ => {
                    return self.retain_failure(
                        request,
                        fingerprint,
                        DispatchErrorCode::InvalidRequest,
                    )
                }
            };
        if self.transactions.active_id() != Some(payload.transaction_id.as_str()) {
            return self.retain_failure(request, fingerprint, DispatchErrorCode::NotFound);
        }
        let record = match self
            .transactions
            .update(&payload.transaction_id, payload.value)
        {
            Ok(record) => record,
            Err(_) => {
                return self.retain_failure(request, fingerprint, DispatchErrorCode::Internal)
            }
        };
        self.retain_success(request, fingerprint, CompactResult::Transaction(record))
    }

    pub(crate) fn cancel(
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
        let record = match self.transactions.get(&payload.transaction_id).cloned() {
            Some(record) if self.transactions.active_id() == Some(record.id.as_str()) => self
                .transactions
                .finish(&record.id, TerminalDisposition::Cancelled, None)
                .expect("the selected transaction is active"),
            _ => return self.retain_failure(request, fingerprint, DispatchErrorCode::NotFound),
        };
        self.retain_success(request, fingerprint, CompactResult::Transaction(record))
    }

    pub(crate) fn status(
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
        self.retain_success(request, fingerprint, CompactResult::Transaction(record))
    }
}

pub(crate) fn transaction_id(request: &OpacityRequest) -> Option<TransactionIdPayload> {
    serde_json::from_value(request.payload.clone()).ok()
}

fn valid_opacity(value: &Number) -> bool {
    value
        .as_f64()
        .is_some_and(|value| (0.0..=100.0).contains(&value))
}
