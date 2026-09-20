//! Immutable frame scheduling and terminal lifetime ownership.
//! This local N11 contract activates no compositor, GPU, host, or dispatch.

pub use crate::resource_leases::{
    FrameFailure, FrameFailureKind, FrameTerminalStatus, PublicationDisposition, ScheduleReceipt,
    ViewGeneration, WorkId,
};
use crate::resource_leases::{LeaseCounters, LeaseError, LeaseId, ResourceLeaseManager};
use crate::revision::DocumentSnapshot;
use std::collections::BTreeMap;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct OutputSpec {
    kind: String,
    format: String,
    width: u32,
    height: u32,
    color_interpretation: String,
    alpha_mode: String,
}

impl OutputSpec {
    pub fn new(
        kind: impl Into<String>,
        format: impl Into<String>,
        width: u32,
        height: u32,
        color_interpretation: impl Into<String>,
        alpha_mode: impl Into<String>,
    ) -> Result<Self, ScheduleError> {
        let output = Self {
            kind: kind.into(),
            format: format.into(),
            width,
            height,
            color_interpretation: color_interpretation.into(),
            alpha_mode: alpha_mode.into(),
        };
        if output.kind.is_empty()
            || output.format.is_empty()
            || output.color_interpretation.is_empty()
            || output.alpha_mode.is_empty()
        {
            return Err(ScheduleError::InvalidKey(
                "output kind, format, color interpretation, and alpha mode must be non-empty",
            ));
        }
        if output.width == 0 || output.height == 0 {
            return Err(ScheduleError::InvalidKey(
                "output dimensions must be greater than zero",
            ));
        }
        Ok(output)
    }

    pub fn kind(&self) -> &str {
        &self.kind
    }
    pub fn format(&self) -> &str {
        &self.format
    }
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    pub fn color_interpretation(&self) -> &str {
        &self.color_interpretation
    }
    pub fn alpha_mode(&self) -> &str {
        &self.alpha_mode
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EvaluationKey {
    document_snapshot_id: String,
    context_id: String,
    frame: u32,
    quality: String,
    output_spec: OutputSpec,
    declared_resource_versions: BTreeMap<String, String>,
}

impl EvaluationKey {
    pub fn new(
        document_snapshot_id: impl Into<String>,
        context_id: impl Into<String>,
        frame: u32,
        quality: impl Into<String>,
        output_spec: OutputSpec,
        declared_resource_versions: impl IntoIterator<Item = (String, String)>,
    ) -> Result<Self, ScheduleError> {
        let mut versions = BTreeMap::new();
        for (resource_id, version) in declared_resource_versions {
            if resource_id.is_empty() || version.is_empty() {
                return Err(ScheduleError::InvalidKey(
                    "declared resource IDs and versions must be non-empty",
                ));
            }
            if versions.insert(resource_id, version).is_some() {
                return Err(ScheduleError::InvalidKey(
                    "declared resource IDs must be unique",
                ));
            }
        }
        let key = Self {
            document_snapshot_id: document_snapshot_id.into(),
            context_id: context_id.into(),
            frame,
            quality: quality.into(),
            output_spec,
            declared_resource_versions: versions,
        };
        key.validate()?;
        Ok(key)
    }

    pub fn document_snapshot_id(&self) -> &str {
        &self.document_snapshot_id
    }

    pub fn context_id(&self) -> &str {
        &self.context_id
    }
    pub fn frame(&self) -> u32 {
        self.frame
    }
    pub fn quality(&self) -> &str {
        &self.quality
    }
    pub fn output_spec(&self) -> &OutputSpec {
        &self.output_spec
    }

    pub fn declared_resource_versions(&self) -> &BTreeMap<String, String> {
        &self.declared_resource_versions
    }

    fn validate(&self) -> Result<(), ScheduleError> {
        if self.document_snapshot_id.is_empty()
            || self.context_id.is_empty()
            || self.quality.is_empty()
        {
            return Err(ScheduleError::InvalidKey(
                "snapshot ID, context ID, and quality must be non-empty",
            ));
        }
        for (resource, version) in &self.declared_resource_versions {
            if resource.is_empty() || version.is_empty() {
                return Err(ScheduleError::InvalidKey(
                    "declared resource IDs and versions must be non-empty",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledFrame {
    work_id: WorkId,
    view_generation: ViewGeneration,
    key: EvaluationKey,
    admission: ScheduleAdmission,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScheduleAdmission {
    Scheduled,
    Deduplicated,
}

impl ScheduledFrame {
    pub fn work_id(&self) -> WorkId {
        self.work_id
    }

    pub fn view_generation(&self) -> ViewGeneration {
        self.view_generation
    }

    pub fn key(&self) -> &EvaluationKey {
        &self.key
    }

    pub fn admission(&self) -> ScheduleAdmission {
        self.admission
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScheduleError {
    InvalidKey(&'static str),
    SnapshotKeyMismatch {
        key_snapshot_id: String,
        actual_snapshot_id: String,
    },
    WrongDocument {
        requested_document_id: String,
        current_document_id: String,
    },
    GenerationExhausted,
    WorkIdExhausted,
    Lease(LeaseError),
    UnknownWork(WorkId),
}

impl Display for ScheduleError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidKey(message) => formatter.write_str(message),
            Self::SnapshotKeyMismatch {
                key_snapshot_id,
                actual_snapshot_id,
            } => write!(
                formatter,
                "key snapshot {key_snapshot_id} does not match {actual_snapshot_id}"
            ),
            Self::WrongDocument {
                requested_document_id,
                current_document_id,
            } => write!(
                formatter,
                "document {requested_document_id} is not current {current_document_id}"
            ),
            Self::GenerationExhausted => formatter.write_str("view generation sequence exhausted"),
            Self::WorkIdExhausted => {
                formatter.write_str("scheduled work identifier sequence exhausted")
            }
            Self::Lease(error) => Display::fmt(error, formatter),
            Self::UnknownWork(work_id) => {
                write!(formatter, "scheduled work {} is unknown", work_id.value())
            }
        }
    }
}

impl std::error::Error for ScheduleError {}

#[derive(Debug)]
struct ActiveFrame {
    key: EvaluationKey,
    view_generation: ViewGeneration,
    document_id: String,
    leases: Option<crate::resource_leases::LeaseBundle>,
    receipt: Option<ScheduleReceipt>,
}

/// Local owner for immutable frame work; no compositor or host is activated here.
#[derive(Debug, Default)]
pub struct FrameScheduler {
    current_document_id: Option<String>,
    next_generation: u64,
    next_work_id: u64,
    active_by_key: BTreeMap<EvaluationKey, WorkId>,
    frames: BTreeMap<WorkId, ActiveFrame>,
    leases: ResourceLeaseManager,
}

impl FrameScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(crate) fn with_sequences(
        next_generation: u64,
        next_work_id: u64,
        next_lease_id: u64,
    ) -> Self {
        Self {
            next_generation,
            next_work_id,
            leases: ResourceLeaseManager::with_next_id(next_lease_id),
            ..Self::new()
        }
    }

    pub fn schedule(
        &mut self,
        snapshot: DocumentSnapshot,
        key: EvaluationKey,
    ) -> Result<ScheduledFrame, ScheduleError> {
        key.validate()?;
        if key.document_snapshot_id() != snapshot.id() {
            return Err(ScheduleError::SnapshotKeyMismatch {
                key_snapshot_id: key.document_snapshot_id().to_owned(),
                actual_snapshot_id: snapshot.id().to_owned(),
            });
        }
        self.validate_document(snapshot.document_id())?;

        if let Some(work_id) = self.active_by_key.get(&key).copied() {
            let active = self
                .frames
                .get(&work_id)
                .expect("active key always has work");
            return Ok(ScheduledFrame {
                work_id,
                view_generation: active.view_generation,
                key,
                admission: ScheduleAdmission::Deduplicated,
            });
        }

        let resource_ids = key.declared_resource_versions().keys().cloned();
        let bundle = self
            .leases
            .acquire(snapshot, resource_ids)
            .map_err(ScheduleError::Lease)?;
        let generation = match self.next_generation.checked_add(1) {
            Some(value) => value,
            None => {
                self.leases
                    .release(bundle)
                    .expect("fresh bundle releases once");
                return Err(ScheduleError::GenerationExhausted);
            }
        };
        let work = match self.next_work_id.checked_add(1) {
            Some(value) => value,
            None => {
                self.leases
                    .release(bundle)
                    .expect("fresh bundle releases once");
                return Err(ScheduleError::WorkIdExhausted);
            }
        };
        self.next_generation = generation;
        self.next_work_id = work;
        if self.current_document_id.is_none() {
            self.current_document_id = Some(bundle.snapshot().document_id().to_owned());
        }
        let work_id = WorkId(work);
        let view_generation = ViewGeneration(generation);
        self.active_by_key.insert(key.clone(), work_id);
        self.frames.insert(
            work_id,
            ActiveFrame {
                key: key.clone(),
                view_generation,
                document_id: bundle.snapshot().document_id().to_owned(),
                leases: Some(bundle),
                receipt: None,
            },
        );
        Ok(ScheduledFrame {
            work_id,
            view_generation,
            key,
            admission: ScheduleAdmission::Scheduled,
        })
    }

    pub fn succeed(&mut self, work_id: WorkId) -> Result<ScheduleReceipt, ScheduleError> {
        let newest_generation = ViewGeneration(self.next_generation);
        self.terminalize(work_id, |active| {
            if active.view_generation == newest_generation {
                (
                    FrameTerminalStatus::Succeeded,
                    PublicationDisposition::Published,
                )
            } else {
                (
                    FrameTerminalStatus::Succeeded,
                    PublicationDisposition::SuppressedStale { newest_generation },
                )
            }
        })
    }

    pub fn fail(
        &mut self,
        work_id: WorkId,
        error: FrameFailure,
    ) -> Result<ScheduleReceipt, ScheduleError> {
        self.terminalize(work_id, |_| {
            (
                FrameTerminalStatus::Failed { error },
                PublicationDisposition::NotPublished,
            )
        })
    }

    pub fn cancel(&mut self, work_id: WorkId) -> Result<ScheduleReceipt, ScheduleError> {
        self.terminalize(work_id, |_| {
            (
                FrameTerminalStatus::Cancelled,
                PublicationDisposition::NotPublished,
            )
        })
    }

    /// Cancels every old-document frame and records the identity transition.
    /// Generations intentionally continue across replacement, preventing a
    /// late old result from ever becoming newer than the replacement's work.
    pub fn replace_document(
        &mut self,
        new_document_id: impl Into<String>,
    ) -> Result<Vec<ScheduleReceipt>, ScheduleError> {
        let new_document_id = new_document_id.into();
        if new_document_id.is_empty() {
            return Err(ScheduleError::InvalidKey(
                "replacement document ID must be non-empty",
            ));
        }
        if self.current_document_id.as_deref() == Some(new_document_id.as_str()) {
            return Err(ScheduleError::InvalidKey(
                "replacement document ID must differ from the current document",
            ));
        }
        let old_document_id = self.current_document_id.replace(new_document_id.clone());
        let active: Vec<WorkId> = self
            .frames
            .iter()
            .filter_map(|(id, frame)| frame.receipt.is_none().then_some(*id))
            .collect();
        let mut receipts = Vec::with_capacity(active.len());
        for work_id in active {
            let old = self.frames[&work_id].document_id.clone();
            receipts.push(self.terminalize(work_id, |_| {
                (
                    FrameTerminalStatus::Replaced {
                        old_document_id: old,
                        new_document_id: new_document_id.clone(),
                    },
                    PublicationDisposition::NotPublished,
                )
            })?);
        }
        if old_document_id.is_none() {
            return Ok(Vec::new());
        }
        Ok(receipts)
    }

    pub fn receipt(&self, work_id: WorkId) -> Option<&ScheduleReceipt> {
        self.frames
            .get(&work_id)
            .and_then(|frame| frame.receipt.as_ref())
    }

    pub fn pinned_snapshot(&self, work_id: WorkId) -> Option<&DocumentSnapshot> {
        self.frames
            .get(&work_id)
            .and_then(|frame| frame.leases.as_ref())
            .map(|leases| leases.snapshot())
    }

    pub fn lease_ids(&self, work_id: WorkId) -> Option<(LeaseId, Vec<LeaseId>)> {
        self.frames.get(&work_id).and_then(|frame| {
            frame
                .leases
                .as_ref()
                .map(|leases| (leases.snapshot_lease_id(), leases.resource_lease_ids()))
        })
    }

    pub fn lease_counters(&self) -> LeaseCounters {
        self.leases.counters()
    }

    #[cfg(test)]
    pub(crate) fn inject_resource_failure_after(&mut self, count: Option<usize>) {
        self.leases.inject_failure_after_resources(count);
    }

    fn validate_document(&self, document_id: &str) -> Result<(), ScheduleError> {
        match &self.current_document_id {
            Some(current) if current != document_id => Err(ScheduleError::WrongDocument {
                requested_document_id: document_id.to_owned(),
                current_document_id: current.clone(),
            }),
            Some(_) | None => Ok(()),
        }
    }

    fn terminalize(
        &mut self,
        work_id: WorkId,
        disposition: impl FnOnce(&ActiveFrame) -> (FrameTerminalStatus, PublicationDisposition),
    ) -> Result<ScheduleReceipt, ScheduleError> {
        let (key, leases, receipt) = {
            let frame = self
                .frames
                .get_mut(&work_id)
                .ok_or(ScheduleError::UnknownWork(work_id))?;
            if let Some(receipt) = &frame.receipt {
                return Ok(receipt.clone());
            }
            let (terminal_status, publication) = disposition(frame);
            let receipt = ScheduleReceipt {
                work_id,
                view_generation: frame.view_generation,
                document_snapshot_id: frame.key.document_snapshot_id().to_owned(),
                document_id: frame.document_id.clone(),
                terminal_status,
                publication,
            };
            (
                frame.key.clone(),
                frame.leases.take().expect("active frame owns one bundle"),
                receipt,
            )
        };
        self.leases.release(leases).map_err(ScheduleError::Lease)?;
        self.active_by_key.remove(&key);
        self.frames
            .get_mut(&work_id)
            .expect("scheduled frame remains retained for idempotent receipts")
            .receipt = Some(receipt.clone());
        Ok(receipt)
    }
}
