//! Typed scheduling admission; both snapshot kinds share one work/lease owner.
use crate::resource_leases::{
    FrameTerminalStatus, LeaseBundle, LeaseError, LeaseId, PublicationDisposition,
    ResourceLeaseManager, ScheduleReceipt, ViewGeneration, WorkId,
};
use crate::revision::{DocumentSnapshot, ObjectSnapshot};
use crate::scheduler::{
    ActiveFrame, EvaluationKey, FrameScheduler, ScheduleAdmission, ScheduleError, ScheduledFrame,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectScheduleScope {
    Authored,
    Unsupported,
}

/// Read-only request pins, checked against the retained revision before admission.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ObjectSchedulePins {
    pub instance_id: String,
    pub document_id: String,
    pub content_revision: u64,
    pub document_snapshot_id: String,
    pub context_id: String,
    pub frame: u32,
    pub scope: ObjectScheduleScope,
}

impl ObjectSchedulePins {
    fn validate(&self, snapshot: &ObjectSnapshot) -> Result<FrameOwner, ScheduleError> {
        if self.instance_id.is_empty()
            || self.document_id.is_empty()
            || self.document_snapshot_id.is_empty()
            || self.instance_id != snapshot.instance_id()
            || self.document_id != snapshot.document_id()
            || self.content_revision != snapshot.content_revision()
            || self.document_snapshot_id != snapshot.snapshot_id()
        {
            return Err(ScheduleError::InvalidKey(
                "object pins do not match the retained owner revision",
            ));
        }
        if self.scope != ObjectScheduleScope::Authored {
            return Err(ScheduleError::InvalidKey(
                "object scheduling requires authored scope",
            ));
        }
        // #2256: scene-root or a component the snapshot declares (an entered component is presented by the viewport);
        // the frame is bounded by that context's own frame count. Any other id is not a context.
        let Some(frames) = snapshot.declared_context_frames(&self.context_id) else {
            return Err(ScheduleError::InvalidKey(
                "object scheduling requires a declared context",
            ));
        };
        if self.frame >= frames {
            return Err(ScheduleError::InvalidKey(
                "object frame is outside the retained document range",
            ));
        }
        Ok(FrameOwner::Object {
            instance: self.instance_id.clone(),
            document: self.document_id.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum FrameOwner {
    Opacity(String),
    Object { instance: String, document: String },
}

impl FrameOwner {
    fn document(&self) -> &str {
        match self {
            Self::Opacity(document) | Self::Object { document, .. } => document,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum FrameKey {
    Opacity(EvaluationKey),
    Object(ObjectSchedulePins, EvaluationKey),
}

impl FrameKey {
    pub(super) fn evaluation(&self) -> &EvaluationKey {
        match self {
            Self::Opacity(key) | Self::Object(_, key) => key,
        }
    }
}

#[derive(Debug)]
pub(super) enum FrameLeases {
    Opacity(LeaseBundle<DocumentSnapshot>),
    Object(LeaseBundle<ObjectSnapshot>),
}

impl FrameLeases {
    pub(super) fn opacity(&self) -> Option<&DocumentSnapshot> {
        match self {
            Self::Opacity(bundle) => Some(bundle.snapshot()),
            Self::Object(_) => None,
        }
    }
    pub(super) fn object(&self) -> Option<&ObjectSnapshot> {
        match self {
            Self::Object(bundle) => Some(bundle.snapshot()),
            Self::Opacity(_) => None,
        }
    }
    pub(super) fn ids(&self) -> (LeaseId, Vec<LeaseId>) {
        match self {
            Self::Opacity(bundle) => (bundle.snapshot_lease_id(), bundle.resource_lease_ids()),
            Self::Object(bundle) => (bundle.snapshot_lease_id(), bundle.resource_lease_ids()),
        }
    }
    pub(super) fn release(self, manager: &mut ResourceLeaseManager) -> Result<(), LeaseError> {
        match self {
            Self::Opacity(bundle) => manager.release(bundle),
            Self::Object(bundle) => manager.release(bundle),
        }
    }
}

impl FrameScheduler {
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
        let owner = FrameOwner::Opacity(snapshot.document_id().to_owned());
        self.admit(owner, FrameKey::Opacity(key), |manager, key| {
            manager
                .acquire(snapshot, key.declared_resource_versions().keys().cloned())
                .map(FrameLeases::Opacity)
        })
    }

    pub fn schedule_object(
        &mut self,
        snapshot: ObjectSnapshot,
        pins: ObjectSchedulePins,
        key: EvaluationKey,
    ) -> Result<ScheduledFrame, ScheduleError> {
        key.validate()?;
        let owner = pins.validate(&snapshot)?;
        if key.document_snapshot_id() != pins.document_snapshot_id
            || key.context_id() != pins.context_id
            || key.frame() != pins.frame
        {
            return Err(ScheduleError::InvalidKey(
                "evaluation key does not match object scheduling pins",
            ));
        }
        self.admit(owner, FrameKey::Object(pins, key), |manager, key| {
            manager
                .acquire_object(snapshot, key.declared_resource_versions().keys().cloned())
                .map(FrameLeases::Object)
        })
    }

    fn admit(
        &mut self,
        owner: FrameOwner,
        key: FrameKey,
        acquire: impl FnOnce(
            &mut ResourceLeaseManager,
            &EvaluationKey,
        ) -> Result<FrameLeases, LeaseError>,
    ) -> Result<ScheduledFrame, ScheduleError> {
        if let Some(current) = &self.current_owner {
            if current != &owner {
                return Err(if current.document() != owner.document() {
                    ScheduleError::WrongDocument {
                        requested_document_id: owner.document().to_owned(),
                        current_document_id: current.document().to_owned(),
                    }
                } else {
                    ScheduleError::InvalidKey("snapshot type or instance is not the current owner")
                });
            }
        }
        if let Some(work_id) = self.active_by_key.get(&key).copied() {
            return Ok(ScheduledFrame {
                provenance: self.provenance.clone(),
                work_id,
                view_generation: self.frames[&work_id].view_generation,
                key: key.evaluation().clone(),
                admission: ScheduleAdmission::Deduplicated,
            });
        }
        let bundle = acquire(&mut self.leases, key.evaluation()).map_err(ScheduleError::Lease)?;
        let sequences = self
            .next_generation
            .checked_add(1)
            .ok_or(ScheduleError::GenerationExhausted)
            .and_then(|generation| {
                self.next_work_id
                    .checked_add(1)
                    .map(|work| (generation, work))
                    .ok_or(ScheduleError::WorkIdExhausted)
            });
        let (generation, work) = match sequences {
            Ok(value) => value,
            Err(error) => {
                bundle
                    .release(&mut self.leases)
                    .expect("fresh bundle releases once");
                return Err(error);
            }
        };
        self.next_generation = generation;
        self.next_work_id = work;
        let document_id = owner.document().to_owned();
        self.current_owner = Some(owner);
        let work_id = WorkId(work);
        let view_generation = ViewGeneration(generation);
        let evaluation = key.evaluation().clone();
        self.active_by_key.insert(key.clone(), work_id);
        self.frames.insert(
            work_id,
            ActiveFrame {
                key,
                view_generation,
                document_id,
                leases: Some(bundle),
                receipt: None,
            },
        );
        Ok(ScheduledFrame {
            provenance: self.provenance.clone(),
            work_id,
            view_generation,
            key: evaluation,
            admission: ScheduleAdmission::Scheduled,
        })
    }

    /// Validate the new immutable owner before terminalizing any existing work.
    pub fn replace_object_document(
        &mut self,
        snapshot: &ObjectSnapshot,
        pins: &ObjectSchedulePins,
    ) -> Result<Vec<ScheduleReceipt>, ScheduleError> {
        self.replace_owner(pins.validate(snapshot)?)
    }

    /// Preserve opacity replacement while keeping generations monotonic across types.
    pub fn replace_document(
        &mut self,
        new_document_id: impl Into<String>,
    ) -> Result<Vec<ScheduleReceipt>, ScheduleError> {
        let document = new_document_id.into();
        if document.is_empty() {
            return Err(ScheduleError::InvalidKey(
                "replacement document ID must be non-empty",
            ));
        }
        self.replace_owner(FrameOwner::Opacity(document))
    }

    fn replace_owner(&mut self, owner: FrameOwner) -> Result<Vec<ScheduleReceipt>, ScheduleError> {
        if self.current_owner.as_ref() == Some(&owner) {
            return Err(ScheduleError::InvalidKey(
                "replacement document ID must differ from the current document",
            ));
        }
        let new_document_id = owner.document().to_owned();
        let active: Vec<_> = self
            .frames
            .iter()
            .filter_map(|(id, frame)| frame.receipt.is_none().then_some(*id))
            .collect();
        let mut receipts = Vec::with_capacity(active.len());
        for work_id in active {
            receipts.push(self.terminalize(work_id, |frame| {
                (
                    FrameTerminalStatus::Replaced {
                        old_document_id: frame.document_id.clone(),
                        new_document_id: new_document_id.clone(),
                    },
                    PublicationDisposition::NotPublished,
                )
            })?);
        }
        self.current_owner = Some(owner);
        Ok(receipts)
    }
}

#[cfg(all(test, feature = "history"))]
#[path = "../tests/object_scheduler.rs"]
mod tests;
