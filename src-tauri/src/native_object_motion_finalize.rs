//! Exact owner/revision/batch callback after the original read guards return.
use crate::application_mcp::revision_sync::{RevisionEvent, RevisionState};
use crate::application_mcp::ApplicationMcp;
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{object_document::FrameScopeKind, object_frame_packet::ObjectFrameSelector};
use std::time::Duration;
use tauri::{Emitter, Manager};
use tokio::sync::oneshot;

#[derive(Clone)]
pub(super) struct MotionReservation {
    batch_id: String,
    generation: u64,
    instance: String,
    document: String,
    revision: u64,
    snapshot: String,
    context: String,
    scope: FrameScopeKind,
    frames: Vec<u32>,
    requires_revision: bool,
    waiting_on: Option<RevisionEvent>,
    scheduled: bool,
    blocked: Option<String>,
}
impl MotionReservation {
    pub(super) fn matches_owner(
        &self,
        generation: u64,
        instance: &str,
        document: &str,
        revision: u64,
    ) -> bool {
        self.generation == generation
            && self.instance == instance
            && self.document == document
            && self.revision == revision
    }
    pub(super) fn retains_request(&self, request_id: &str) -> bool {
        self.waiting_on
            .as_ref()
            .is_some_and(|event| event.request_id == request_id)
    }
    fn matches_source(&self, generation: u64, expected: &ObjectFrameSelector<'_>) -> bool {
        self.matches_owner(
            generation,
            expected.instance_id,
            expected.document_id,
            expected.content_revision,
        ) && self.snapshot == expected.document_snapshot_id
            && self.context == expected.context_id
            && self.scope == expected.scope_kind
    }
    fn selector(&self) -> ObjectFrameSelector<'_> {
        ObjectFrameSelector {
            instance_id: &self.instance,
            document_id: &self.document,
            content_revision: self.revision,
            document_snapshot_id: &self.snapshot,
            context_id: &self.context,
            scope_kind: self.scope.clone(),
            frame: self.frames[0],
        }
    }
}
impl RevisionState {
    pub(super) fn admit_motion_read(
        &self,
        generation: u64,
        expected: &ObjectFrameSelector<'_>,
        proof: bool,
    ) -> Result<(), String> {
        let Some(batch) = &self.motion_reservation else {
            return Ok(());
        };
        // The previous synchronous operation already closed its actual frame
        // union. Another frame cannot merge a timeout into diagnostic sealing.
        // Only the original exact proof may revisit a retained timeout frame.
        if proof
            && batch.requires_revision
            && batch.matches_source(generation, expected)
            && batch.frames.contains(&expected.frame)
        {
            return Ok(());
        }
        Err("native retained Motion batch awaits its post-read finalizer".into())
    }
    pub(super) fn reserve_motion(
        &mut self,
        generation: u64,
        expected: &ObjectFrameSelector<'_>,
        mut frames: Vec<u32>,
        requires_revision: bool,
    ) -> Result<(), String> {
        frames.sort_unstable();
        frames.dedup();
        if frames.is_empty() || frames.len() > native_engine::export_job::MAX_OBJECT_EXPORT_FRAMES {
            return Err("native retained Motion batch is invalid".into());
        }
        if let Some(reserved) = self.motion_reservation.as_mut() {
            if !reserved.matches_source(generation, expected) {
                return Err("native retained Motion completion source is occupied".into());
            }
            let mut joined = reserved.frames.clone();
            joined.extend(frames);
            joined.sort_unstable();
            joined.dedup();
            if joined.len() > native_engine::export_job::MAX_OBJECT_EXPORT_FRAMES {
                return Err("native retained Motion batch exceeds original frame limit".into());
            }
            reserved.frames = joined;
            reserved.requires_revision |= requires_revision;
            if requires_revision && reserved.waiting_on.is_none() {
                reserved.waiting_on = self.pending.as_ref().map(|p| p.event.clone());
            }
            return Ok(());
        }
        self.motion_reservation = Some(MotionReservation {
            batch_id: uuid::Uuid::new_v4().to_string(),
            generation,
            instance: expected.instance_id.into(),
            document: expected.document_id.into(),
            revision: expected.content_revision,
            snapshot: expected.document_snapshot_id.into(),
            context: expected.context_id.into(),
            scope: expected.scope_kind.clone(),
            frames,
            requires_revision,
            waiting_on: if requires_revision {
                self.pending.as_ref().map(|p| p.event.clone())
            } else {
                None
            },
            scheduled: false,
            blocked: None,
        });
        Ok(())
    }
}

enum Step {
    Done(bool),
    Wait,
    Notification(RevisionEvent, oneshot::Receiver<()>),
}
impl ApplicationMcp {
    // Only this trusted callback consumes the opaque owner receipts. No request
    // payload, public operation, preview proof or getter supplies mutation data.
    fn motion_completion_step(&self, batch_id: &str, allow_revision: bool) -> Result<Step, String> {
        let mut authority = self
            .native
            .lock()
            .map_err(|_| "native application lock unavailable")?;
        let mut revisions = self
            .revisions
            .0
            .lock()
            .map_err(|_| "native revision lock unavailable")?;
        let Some(plan) = revisions.motion_reservation.clone() else {
            return Ok(Step::Done(false));
        };
        if plan.batch_id != batch_id {
            return Err("native Motion completion batch changed".into());
        }
        let generation = authority.active_generation()?;
        let application = authority.active_mut(generation)?;
        if !plan.matches_owner(
            generation,
            application.instance_id(),
            application.document_id(),
            application.content_revision(),
        ) {
            // A different active document/lifecycle never inherits this fence.
            revisions.reconcile(generation, application.document_id());
            return Err("native Motion completion owner or revision changed".into());
        }
        if plan.requires_revision && !allow_revision {
            return Ok(Step::Wait);
        }
        if plan.requires_revision {
            if let Some(pending) = revisions.pending.as_ref() {
                if pending.failed {
                    return Err(
                        "native revision synchronization is indeterminate; effects retained".into(),
                    );
                }
                if plan.waiting_on.as_ref() != Some(&pending.event)
                    || pending.event.to_revision != plan.revision
                {
                    return Err(
                        "native Motion completion cannot borrow another pending revision".into(),
                    );
                }
                return Ok(Step::Wait); // ACK alone is insufficient; original settle must clear it.
            }
        }
        if plan.requires_revision
            && revisions.subscriber.as_ref().is_none_or(|subscriber| {
                subscriber.instance_id != plan.instance
                    || subscriber.document_id != plan.document
                    || subscriber.lifecycle_generation != plan.generation
            })
        {
            return Ok(Step::Wait); // No silent revision advance without a live notification consumer.
        }
        let owner = application
            .as_any_mut()
            .downcast_mut::<NativeObjectHost>()
            .ok_or("native Motion completion requires the original object owner")?;
        // No other read can extend this closed owner/source batch. Check the
        // receipt's real mutation requirement before entering the atomic seal.
        if owner.object_motion_requires_revision() != plan.requires_revision {
            return Err("native Motion completion receipt requirement changed".into());
        }
        let commit = owner.finalize_object_motion_effects(&plan.selector(), &plan.frames)?;
        let Some(commit) = commit else {
            revisions.motion_reservation = None;
            return Ok(Step::Done(false));
        };
        // Core returns an actual committed receipt even if reconstruction or
        // old-work cleanup cannot succeed. Never lose that revision notification.
        if commit.changed() {
            let event = RevisionEvent {
                instance_id: plan.instance,
                document_id: plan.document,
                lifecycle_generation: plan.generation,
                from_revision: commit.before_revision(),
                to_revision: commit.content_revision(),
                request_id: format!("motion-effects/{}", uuid::Uuid::new_v4()),
            };
            revisions.motion_reservation = None;
            let (event, receiver) = revisions.begin_event(event);
            Ok(Step::Notification(event, receiver))
        } else {
            revisions.motion_reservation = None;
            Ok(Step::Done(commit.frame_ready()))
        }
    }

    async fn complete_object_motion_effects(
        &self,
        batch_id: &str,
        emit: impl FnOnce(RevisionEvent) -> Result<(), String>,
        timeout: Duration,
    ) -> Result<bool, String> {
        let deadline = tokio::time::Instant::now() + timeout;
        loop {
            // Register before checking state so original settle/disconnect cannot
            // wake between the check and registration. No mutex survives await.
            let changed = self.revisions.1.notified();
            tokio::pin!(changed);
            changed.as_mut().enable();
            match self.motion_completion_step(batch_id, true)? {
                Step::Done(ready) => return Ok(ready),
                Step::Wait => {
                    tokio::time::timeout_at(deadline,changed).await
                        .map_err(|_|"native Motion completion awaits subscriber/settlement; effects retained".to_string())?;
                }
                Step::Notification(event, receiver) => {
                    let acknowledged = emit(event.clone()).is_ok()
                        && tokio::time::timeout(timeout, receiver)
                            .await
                            .is_ok_and(|result| result.is_ok());
                    if !self.revisions.settle(&self.native, &event, acknowledged) {
                        return Err("native Motion disable committed but synchronization is indeterminate; do not execute again; release before re-entry".into());
                    }
                    return Ok(false); // Current request pins remain stale; the event consumer refreshes.
                }
            }
        }
    }

    /// Called synchronously after read guards return, before the main-thread
    /// adapter resolves. Pure diagnostic publication never waits for R1 ACK.
    pub(crate) fn seal_object_motion_diagnostics(&self) -> Result<Option<bool>, String> {
        let id = self
            .revisions
            .0
            .lock()
            .map_err(|_| "native revision lock unavailable")?
            .motion_reservation
            .as_ref()
            .map(|batch| batch.batch_id.clone());
        let Some(id) = id else {
            return Ok(None);
        };
        match self.motion_completion_step(&id, false)? {
            Step::Done(ready) => Ok(Some(ready)),
            Step::Wait => Ok(None),
            Step::Notification(_, _) => Err("diagnostic-only seal cannot advance revision".into()),
        }
    }

    fn block_motion_completion(&self, batch_id: &str, error: String) {
        if let Ok(mut revisions) = self.revisions.0.lock() {
            if let Some(reserved) = revisions
                .motion_reservation
                .as_mut()
                .filter(|r| r.batch_id == batch_id)
            {
                reserved.scheduled = false;
                reserved.blocked = Some(error);
            }
        }
    }

    /// The worker is scheduled inside the main-thread wrapper AFTER its read
    /// returned, so cancellation of the awaiting command cannot cancel sealing.
    /// It never holds native/revision/GPU locks while emitting or awaiting ACK.
    pub(crate) fn spawn_object_motion_completion(
        &self,
        app: &tauri::AppHandle,
    ) -> Option<(oneshot::Receiver<Result<bool, String>>, bool)> {
        let mut revisions = self.revisions.0.lock().ok()?;
        let reserved = revisions.motion_reservation.as_mut()?;
        if reserved.scheduled || !reserved.requires_revision {
            return None;
        }
        reserved.scheduled = true;
        reserved.blocked = None;
        let batch_id = reserved.batch_id.clone();
        let waits_for_prior = reserved.requires_revision && reserved.waiting_on.is_some();
        drop(revisions);
        let app = app.clone();
        let (sender, receiver) = oneshot::channel();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<ApplicationMcp>();
            let result = state
                .complete_object_motion_effects(
                    &batch_id,
                    |event| {
                        app.emit_to("main", "nemo-native-revision", event)
                            .map_err(|e| e.to_string())
                    },
                    Duration::from_secs(5),
                )
                .await;
            if let Err(error) = &result {
                state.block_motion_completion(&batch_id, error.clone());
            }
            let _ = sender.send(result); // Dropped client receiver never discards owner receipts.
        });
        Some((receiver, waits_for_prior))
    }
}

#[cfg(test)]
#[path = "native_object_motion_finalize_tests.rs"]
mod tests;
