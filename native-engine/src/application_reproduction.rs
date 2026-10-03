//! Opt-in capture at the native dispatch boundary, separate from terminal metadata.
use crate::application::{ExportResourceResolver, NativeApplication, ReproductionEligibility};
use crate::commands::{OpacityRequest, ResponseEnvelope};
use crate::export_job::{ExportCompositor, StagedArtifactPort};
use crate::protocol;
use crate::request_receipts::{ApplyOpacity, RequestFingerprint};
use serde::Serialize;
use serde_json::Number;

#[path = "reproduction_bundle.rs"]
mod bundle;
use bundle::{CapturedCommand, MAX_COMMANDS, TARGET};

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
            | "transaction.status"
    )
}

impl<P: StagedArtifactPort, C: ExportCompositor, R: ExportResourceResolver>
    NativeApplication<P, C, R>
{
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

    pub(super) fn prepare_reproduction(
        &mut self,
        request: &OpacityRequest,
    ) -> Option<CaptureCandidate> {
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

#[cfg(test)]
#[path = "../tests/application_reproduction.rs"]
mod tests;
