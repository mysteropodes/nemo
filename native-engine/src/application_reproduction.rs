//! Opt-in capture at the native dispatch boundary, separate from terminal metadata.
use crate::application::{ExportResourceResolver, NativeApplication, ReproductionEligibility};
use crate::commands::{DispatchErrorCode, OpacityRequest, ResponseEnvelope};
use crate::export_job::{ExportCompositor, StagedArtifactPort};
use crate::protocol;
use crate::request_receipts::{ApplyOpacity, RequestFingerprint};
use serde::Serialize;
use serde_json::{json, Number, Value};

#[path = "reproduction_bundle.rs"]
mod bundle;
use bundle::{CapturedCommand, MAX_COMMANDS, TARGET};
#[path = "reproduction_replay.rs"]
mod replay;
pub use replay::{
    replay_reproduction_bundle, ReproductionReplayReport, ReproductionReplayState,
    ReproductionReplayStep,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReproductionReplayError {
    ByteLimit,
    InvalidBundle,
    IncompatibleBundle,
    InvalidSequence,
    CatalogUnavailable,
    ReplayMismatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReproductionReason {
    NotCatalog,
    NotPristine,
    AlreadyOptedIn,
    NotOptedIn,
    EmptyJournal,
    FailedMutation,
    ChangedRetry,
    UnsupportedTransition,
    Released,
    Replaced,
    CommandLimit,
    ByteLimit,
    InvalidProjection,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReproductionState {
    Disabled,
    Recording,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReproductionStatus {
    pub state: ReproductionState,
    pub reason: Option<ReproductionReason>,
    pub command_count: usize,
    pub exportable: bool,
}

#[derive(Debug, Default)]
pub(super) enum ReproductionJournal {
    #[default]
    Disabled,
    Recording(Vec<CapturedCommand>),
    Invalid(ReproductionReason),
}

impl ReproductionJournal {
    pub(super) fn invalidate(&mut self, reason: ReproductionReason) {
        if matches!(self, Self::Recording(_)) {
            *self = Self::Invalid(reason);
        }
    }

    fn commands(&self) -> Result<&[CapturedCommand], ReproductionReason> {
        match self {
            Self::Disabled => Err(ReproductionReason::NotOptedIn),
            Self::Invalid(reason) => Err(*reason),
            Self::Recording(commands) if commands.is_empty() => {
                Err(ReproductionReason::EmptyJournal)
            }
            Self::Recording(commands) => Ok(commands),
        }
    }

    fn append(&mut self, candidate: CaptureCandidate, response: &ResponseEnvelope) {
        let Self::Recording(commands) = self else {
            return;
        };
        if !response.is_ok() {
            self.invalidate(ReproductionReason::FailedMutation);
            return;
        }
        let applied = response
            .result()
            .and_then(|value| value["applied"].as_bool());
        let expected = commands.last().map_or(0, |command| command.revision);
        if applied.is_none()
            || candidate.expected_revision != expected
            || response.content_revision() != expected + u64::from(applied.unwrap_or(false))
            || response
                .result()
                .and_then(|value| value["historyEntriesAdded"].as_u64())
                != Some(u64::from(applied.unwrap_or(false)))
        {
            self.invalidate(ReproductionReason::InvalidProjection);
            return;
        }
        if commands.len() == MAX_COMMANDS {
            self.invalidate(ReproductionReason::CommandLimit);
            return;
        }
        // The temporary projection is bounded to 32 typed commands. Check its
        // full encoding before retaining; never evict the reproducible prefix.
        let mut next = commands.clone();
        next.push(CapturedCommand {
            id: next.len() as u8 + 1,
            expected_revision: candidate.expected_revision,
            value: candidate.value,
            revision: response.content_revision(),
            applied: applied.unwrap(),
        });
        match bundle::encode(&next) {
            Ok(_) => *commands = next,
            Err(reason) => self.invalidate(reason),
        }
    }
}

pub(super) struct CaptureCandidate {
    expected_revision: u64,
    value: Number,
}

fn read_only(operation: &str) -> bool {
    matches!(
        operation,
        "query.document.opacity"
            | "query.document.revision"
            | "query.document.snapshot.acquire"
            | "query.document.serialize"
            | "query.document.evaluate"
            | "query.diagnostics.recent"
            | "command.reproduction.opt_in"
            | "query.reproduction.status"
            | "query.reproduction.export"
            | "query.reproduction.replay"
            | "transaction.status"
    )
}

impl<P: StagedArtifactPort, C: ExportCompositor, R: ExportResourceResolver>
    NativeApplication<P, C, R>
{
    /// Control reads have no dispatch receipt and never mutate the document.
    /// The one opt-in transition only changes the bounded journal.
    pub(super) fn dispatch_reproduction(&mut self, request: &OpacityRequest) -> ResponseEnvelope {
        let empty = request
            .payload
            .as_object()
            .is_some_and(|object| object.is_empty());
        if request.expected_revision.is_some() {
            return self.failure(
                request,
                DispatchErrorCode::InvalidRequest,
                "Reproduction operations forbid expectedRevision.",
            );
        }
        let result = match request.operation.as_str() {
            "command.reproduction.opt_in" if empty => self
                .opt_in_reproduction()
                .map(|status| serde_json::to_value(status).expect("status is serializable"))
                .map_err(reason_code),
            "query.reproduction.status" if empty => {
                Ok(serde_json::to_value(self.reproduction_status())
                    .expect("status is serializable"))
            }
            "query.reproduction.export" if empty => self
                .export_reproduction_bundle()
                .and_then(|bytes| {
                    serde_json::from_slice::<Value>(&bytes)
                        .map_err(|_| ReproductionReason::InvalidProjection)
                })
                .map(|bundle| json!({"bundle": bundle}))
                .map_err(reason_code),
            "query.reproduction.report" => self.reproduction_report(&request.payload),
            "query.reproduction.replay" => {
                let bundle = request
                    .payload
                    .as_object()
                    .filter(|object| object.len() == 1)
                    .and_then(|object| object.get("bundle"))
                    .filter(|bundle| bundle.is_object());
                match bundle.and_then(|bundle| serde_json::to_vec(bundle).ok()) {
                    Some(bytes) => replay_reproduction_bundle(&bytes)
                        .map(|report| serde_json::to_value(report).expect("report is serializable"))
                        .map_err(replay_code),
                    None => Err(DispatchErrorCode::InvalidRequest),
                }
            }
            _ => Err(DispatchErrorCode::InvalidRequest),
        };
        let result = match result {
            Ok(result) => result,
            Err(code) => return self.failure(request, code, "Reproduction request was rejected."),
        };
        let response = ResponseEnvelope {
            api_version: 2,
            request_id: request.request_id.clone(),
            instance_id: self.instance_id().into(),
            document_id: self.document_id().into(),
            content_revision: self.content_revision(),
            ok: true,
            result: Some(result),
            error: None,
        };
        if serde_json::to_vec(&response).is_ok_and(|bytes| bytes.len() <= 4096) {
            response
        } else {
            self.failure(
                request,
                DispatchErrorCode::InvalidRequest,
                "Reproduction response exceeds the 4096-byte transport limit.",
            )
        }
    }

    pub fn opt_in_reproduction(&mut self) -> Result<ReproductionStatus, ReproductionReason> {
        if !matches!(self.reproduction, ReproductionJournal::Disabled) {
            return Err(ReproductionReason::AlreadyOptedIn);
        }
        match self.reproduction_eligibility() {
            ReproductionEligibility::Eligible => {}
            ReproductionEligibility::NotCatalog => return Err(ReproductionReason::NotCatalog),
            ReproductionEligibility::Released => return Err(ReproductionReason::Released),
            ReproductionEligibility::Replacing => return Err(ReproductionReason::Replaced),
            ReproductionEligibility::NotPristine => return Err(ReproductionReason::NotPristine),
        }
        self.reproduction = ReproductionJournal::Recording(Vec::new());
        Ok(self.reproduction_status())
    }

    pub fn reproduction_status(&self) -> ReproductionStatus {
        let (state, reason, count) = match &self.reproduction {
            ReproductionJournal::Disabled => (
                ReproductionState::Disabled,
                Some(ReproductionReason::NotOptedIn),
                0,
            ),
            ReproductionJournal::Invalid(reason) => (ReproductionState::Invalid, Some(*reason), 0),
            ReproductionJournal::Recording(commands) => {
                (ReproductionState::Recording, None, commands.len())
            }
        };
        ReproductionStatus {
            state,
            reason,
            command_count: count,
            exportable: count > 0,
        }
    }

    /// Encoded portable bytes only; this read does not retain a dispatch receipt.
    pub fn export_reproduction_bundle(&self) -> Result<Vec<u8>, ReproductionReason> {
        bundle::encode(self.reproduction.commands()?)
    }

    /// The host holds its single authority lock throughout dispatch. Revision,
    /// attempt sequence, provenance and journal are read in this one borrow;
    /// no await, retained receipt or second sampled authority participates.
    fn reproduction_report(&self, payload: &Value) -> Result<Value, DispatchErrorCode> {
        let object = payload
            .as_object()
            .filter(|object| object.len() == 2)
            .ok_or(DispatchErrorCode::InvalidRequest)?;
        let token = |key| {
            object
                .get(key)
                .and_then(Value::as_u64)
                .filter(|value| *value <= 9_007_199_254_740_991)
                .ok_or(DispatchErrorCode::InvalidRequest)
        };
        let expected_revision = token("expectedContentRevision")?;
        let expected_sequence = token("expectedSequence")?;
        if expected_revision != self.content_revision() {
            return Err(DispatchErrorCode::StaleRevision);
        }
        let sequence = self
            .diagnostics
            .report_sequence()
            .ok_or(DispatchErrorCode::Unavailable)?;
        if expected_sequence != sequence {
            return Err(DispatchErrorCode::BusyConflict);
        }
        if self.reproduction_origin.is_none() {
            return Err(DispatchErrorCode::Unavailable);
        }
        let bytes = self.export_reproduction_bundle().map_err(reason_code)?;
        let bundle: Value =
            serde_json::from_slice(&bytes).map_err(|_| DispatchErrorCode::Internal)?;
        Ok(
            json!({"bundle": bundle, "verifiedContentRevision": expected_revision,
            "verifiedSequence": sequence}),
        )
    }

    pub(super) fn prepare_reproduction(
        &mut self,
        request: &OpacityRequest,
    ) -> Option<CaptureCandidate> {
        // Even a malformed report or collision with a retained write is a read.
        // Normal dispatch still rejects it, without invalidating the journal.
        if request.operation == "query.reproduction.report" {
            return None;
        }
        if !matches!(self.reproduction, ReproductionJournal::Recording(_))
            || request.instance_id != self.instance_id()
            || request.document_id != self.document_id()
        {
            return None;
        }
        let retained = self.requests.get(&request.request_id);
        if protocol::preflight_identity(
            request,
            self.instance_id(),
            self.document_id(),
            self.content_revision(),
        )
        .is_some()
        {
            if retained.is_some() {
                self.reproduction
                    .invalidate(ReproductionReason::ChangedRetry);
            } else if !read_only(&request.operation) {
                self.reproduction
                    .invalidate(ReproductionReason::FailedMutation);
            }
            return None;
        }
        if let Some(retained) = retained {
            if RequestFingerprint::new(request).as_ref() != Some(retained.fingerprint()) {
                self.reproduction
                    .invalidate(ReproductionReason::ChangedRetry);
            }
            return None;
        }
        if read_only(&request.operation) {
            return None;
        }
        if request.operation != "command.document.apply" {
            self.reproduction
                .invalidate(ReproductionReason::UnsupportedTransition);
            return None;
        }
        let parsed = serde_json::from_value::<ApplyOpacity>(request.payload.clone()).ok();
        let parsed = parsed.filter(|payload| {
            payload.command == "layer.opacity.set"
                && payload.stable_target.layer_uid == TARGET
                && payload
                    .value
                    .as_f64()
                    .is_some_and(|value| value.is_finite() && (0.0..=100.0).contains(&value))
        });
        if request.cancelled_before_dispatch
            || request.expected_revision.is_none()
            || parsed.is_none()
        {
            self.reproduction
                .invalidate(ReproductionReason::FailedMutation);
            return None;
        }
        Some(CaptureCandidate {
            expected_revision: request.expected_revision.unwrap(),
            value: parsed.unwrap().value,
        })
    }

    pub(super) fn finish_reproduction(
        &mut self,
        candidate: Option<CaptureCandidate>,
        response: &ResponseEnvelope,
    ) {
        if let Some(candidate) = candidate {
            self.reproduction.append(candidate, response);
        }
    }
}

fn reason_code(reason: ReproductionReason) -> DispatchErrorCode {
    match reason {
        ReproductionReason::InvalidProjection => DispatchErrorCode::Internal,
        ReproductionReason::NotCatalog
        | ReproductionReason::NotPristine
        | ReproductionReason::AlreadyOptedIn => DispatchErrorCode::InvalidRequest,
        _ => DispatchErrorCode::Unavailable,
    }
}

fn replay_code(error: ReproductionReplayError) -> DispatchErrorCode {
    match error {
        ReproductionReplayError::CatalogUnavailable | ReproductionReplayError::ReplayMismatch => {
            DispatchErrorCode::Internal
        }
        _ => DispatchErrorCode::InvalidRequest,
    }
}

#[cfg(test)]
#[path = "../tests/application_reproduction.rs"]
mod tests;
