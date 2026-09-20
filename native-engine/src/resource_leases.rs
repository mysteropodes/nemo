//! Opaque, counted leases for immutable snapshots and declared resources.
//!
//! This module deliberately models lifetime only. It neither opens media nor
//! submits GPU work; later leaves provide those effects while using these
//! handles to keep their inputs pinned through their own completion boundary.

use crate::revision::DocumentSnapshot;
use std::collections::BTreeSet;
use std::fmt::{Display, Formatter};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LeaseId(u64);

impl LeaseId {
    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorkId(pub(crate) u64);

impl WorkId {
    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ViewGeneration(pub(crate) u64);

impl ViewGeneration {
    pub fn value(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeaseCounters {
    acquired: u64,
    released: u64,
    live: u64,
}

impl LeaseCounters {
    pub fn acquired(self) -> u64 {
        self.acquired
    }

    pub fn released(self) -> u64 {
        self.released
    }

    pub fn live(self) -> u64 {
        self.live
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeaseError {
    IdExhausted,
    CounterOverflow,
    InjectedResourceFailure { resource_id: String },
    UnknownLease(LeaseId),
}

impl Display for LeaseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::IdExhausted => formatter.write_str("lease identifier sequence exhausted"),
            Self::CounterOverflow => formatter.write_str("lease counter overflow"),
            Self::InjectedResourceFailure { resource_id } => {
                write!(
                    formatter,
                    "injected acquisition failure for resource {resource_id}"
                )
            }
            Self::UnknownLease(id) => write!(formatter, "lease {} is not live", id.value()),
        }
    }
}

impl std::error::Error for LeaseError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameFailureKind {
    Evaluation,
    Worker,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FrameFailure {
    kind: FrameFailureKind,
    message: String,
}

impl FrameFailure {
    pub fn new(kind: FrameFailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn kind(&self) -> &FrameFailureKind {
        &self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FrameTerminalStatus {
    Succeeded,
    Failed {
        error: FrameFailure,
    },
    Cancelled,
    Replaced {
        old_document_id: String,
        new_document_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublicationDisposition {
    Published,
    SuppressedStale { newest_generation: ViewGeneration },
    NotPublished,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleReceipt {
    pub(crate) work_id: WorkId,
    pub(crate) view_generation: ViewGeneration,
    pub(crate) document_snapshot_id: String,
    pub(crate) document_id: String,
    pub(crate) terminal_status: FrameTerminalStatus,
    pub(crate) publication: PublicationDisposition,
}

impl ScheduleReceipt {
    pub fn work_id(&self) -> WorkId {
        self.work_id
    }
    pub fn view_generation(&self) -> ViewGeneration {
        self.view_generation
    }
    pub fn document_snapshot_id(&self) -> &str {
        &self.document_snapshot_id
    }
    pub fn document_id(&self) -> &str {
        &self.document_id
    }
    pub fn terminal_status(&self) -> &FrameTerminalStatus {
        &self.terminal_status
    }
    pub fn publication(&self) -> &PublicationDisposition {
        &self.publication
    }
}

#[derive(Debug)]
struct SnapshotLease {
    id: LeaseId,
    snapshot: DocumentSnapshot,
}

#[derive(Debug)]
struct ResourceLease {
    id: LeaseId,
}

/// The concrete pins acquired together for one scheduled frame.
///
/// The snapshot is intentionally stored, rather than its ID alone, so a later
/// document revision cannot turn this into a mutable re-read.
#[derive(Debug)]
pub struct LeaseBundle {
    snapshot: SnapshotLease,
    resources: Vec<ResourceLease>,
}

impl LeaseBundle {
    pub fn snapshot(&self) -> &DocumentSnapshot {
        &self.snapshot.snapshot
    }

    pub fn snapshot_lease_id(&self) -> LeaseId {
        self.snapshot.id
    }

    pub fn resource_lease_ids(&self) -> Vec<LeaseId> {
        self.resources.iter().map(|lease| lease.id).collect()
    }

    fn release(self, manager: &mut ResourceLeaseManager) -> Result<(), LeaseError> {
        manager.release_one(self.snapshot.id)?;
        for resource in self.resources {
            manager.release_one(resource.id)?;
        }
        Ok(())
    }
}

/// Single owner for opaque lease IDs and lifetime counters.
#[derive(Debug)]
pub struct ResourceLeaseManager {
    next_id: u64,
    live: BTreeSet<LeaseId>,
    counters: LeaseCounters,
    fail_after_resources: Option<usize>,
}

impl Default for ResourceLeaseManager {
    fn default() -> Self {
        Self::new()
    }
}

impl ResourceLeaseManager {
    pub fn new() -> Self {
        Self {
            next_id: 0,
            live: BTreeSet::new(),
            counters: LeaseCounters {
                acquired: 0,
                released: 0,
                live: 0,
            },
            fail_after_resources: None,
        }
    }

    pub fn counters(&self) -> LeaseCounters {
        self.counters
    }

    /// Causes acquisition to fail before the resource at this zero-based index.
    /// It is an injected negative control, not a runtime resource policy.
    #[cfg(test)]
    pub(crate) fn inject_failure_after_resources(&mut self, count: Option<usize>) {
        self.fail_after_resources = count;
    }

    #[cfg(test)]
    pub(crate) fn with_next_id(next_id: u64) -> Self {
        Self {
            next_id,
            ..Self::new()
        }
    }

    pub(crate) fn acquire(
        &mut self,
        snapshot: DocumentSnapshot,
        resource_ids: impl IntoIterator<Item = String>,
    ) -> Result<LeaseBundle, LeaseError> {
        let snapshot_lease = self.grant()?;
        let mut resources = Vec::new();
        for resource_id in resource_ids {
            if self.fail_after_resources == Some(resources.len()) {
                self.unwind(snapshot_lease, &resources);
                return Err(LeaseError::InjectedResourceFailure { resource_id });
            }
            match self.grant() {
                Ok(id) => resources.push(ResourceLease { id }),
                Err(error) => {
                    self.unwind(snapshot_lease, &resources);
                    return Err(error);
                }
            }
        }
        Ok(LeaseBundle {
            snapshot: SnapshotLease {
                id: snapshot_lease,
                snapshot,
            },
            resources,
        })
    }

    pub(crate) fn release(&mut self, bundle: LeaseBundle) -> Result<(), LeaseError> {
        bundle.release(self)
    }

    #[cfg(test)]
    pub(crate) fn release_id_for_test(&mut self, lease: LeaseId) -> Result<(), LeaseError> {
        self.release_one(lease)
    }

    fn grant(&mut self) -> Result<LeaseId, LeaseError> {
        let id = self.next_id.checked_add(1).ok_or(LeaseError::IdExhausted)?;
        let acquired = self
            .counters
            .acquired
            .checked_add(1)
            .ok_or(LeaseError::CounterOverflow)?;
        let live = self
            .counters
            .live
            .checked_add(1)
            .ok_or(LeaseError::CounterOverflow)?;
        self.next_id = id;
        let lease = LeaseId(id);
        self.live.insert(lease);
        self.counters.acquired = acquired;
        self.counters.live = live;
        Ok(lease)
    }

    fn release_one(&mut self, lease: LeaseId) -> Result<(), LeaseError> {
        if !self.live.remove(&lease) {
            return Err(LeaseError::UnknownLease(lease));
        }
        self.counters.released = self
            .counters
            .released
            .checked_add(1)
            .ok_or(LeaseError::CounterOverflow)?;
        self.counters.live = self
            .counters
            .live
            .checked_sub(1)
            .expect("a live lease is counted before release");
        Ok(())
    }

    fn unwind(&mut self, snapshot: LeaseId, resources: &[ResourceLease]) {
        for resource in resources.iter().rev() {
            self.release_one(resource.id)
                .expect("freshly acquired resource lease is live during unwind");
        }
        self.release_one(snapshot)
            .expect("freshly acquired snapshot lease is live during unwind");
    }
}
