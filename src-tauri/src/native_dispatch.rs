//! Shared boxed native-application seam used by the UI and bundled MCP host.

use native_engine::{
    application::{ExportResourceResolver, NativeApplication},
    commands::{OpacityRequest, ResponseEnvelope},
    document::OpacityDocument,
    export_job::{ExportCompositor, JobReceipt, JobStatus, PendingFrame, StagedArtifactPort},
};
use std::{
    any::Any,
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex},
};

pub(crate) type NativeState = Arc<Mutex<NativeAuthority>>;

pub(crate) struct NativeAuthority {
    generation: u64,
    retained_releases: BTreeMap<String, ReleaseTombstone>,
    pub(crate) phase: NativePhase,
}

pub(crate) enum NativePhase {
    Vacant,
    Installing {
        generation: u64,
        origin: InstallOrigin,
    },
    Active {
        generation: u64,
        application: Box<dyn NativeDispatch>,
    },
    Releasing {
        generation: u64,
        request_id: String,
        fingerprint: Vec<u8>,
        application: Option<Box<dyn NativeDispatch>>,
    },
    Released(ReleaseTombstone),
}

pub(crate) enum InstallOrigin {
    Vacant,
    Released(ReleaseTombstone),
}

#[derive(Clone)]
pub(crate) struct ReleaseTombstone {
    pub(crate) generation: u64,
    pub(crate) request_id: String,
    pub(crate) fingerprint: Vec<u8>,
    pub(crate) receipt: serde_json::Value,
    pub(crate) succeeded: bool,
}

#[derive(Debug)]
pub(crate) enum ReleaseAdmission {
    Execute {
        generation: u64,
    },
    Retry {
        generation: u64,
        receipt: serde_json::Value,
    },
}

impl Default for NativeAuthority {
    fn default() -> Self {
        Self {
            generation: 0,
            retained_releases: BTreeMap::new(),
            phase: NativePhase::Vacant,
        }
    }
}

impl NativeAuthority {
    pub(crate) fn next_generation(&mut self) -> Result<u64, String> {
        self.generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| "native lifecycle generation exhausted".to_string())?;
        Ok(self.generation)
    }

    pub(crate) fn active(&self) -> Option<(u64, &dyn NativeDispatch)> {
        match &self.phase {
            NativePhase::Active {
                generation,
                application,
                ..
            } => Some((*generation, application.as_ref())),
            _ => None,
        }
    }

    pub(crate) fn active_mut(
        &mut self,
        expected_generation: u64,
    ) -> Result<&mut dyn NativeDispatch, String> {
        match &mut self.phase {
            NativePhase::Active {
                generation,
                application,
                ..
            } if *generation == expected_generation => Ok(application.as_mut()),
            _ => Err("native lifecycle generation is stale or unavailable".into()),
        }
    }

    pub(crate) fn active_generation(&self) -> Result<u64, String> {
        self.active()
            .map(|(generation, _)| generation)
            .ok_or_else(|| "native application is unavailable".to_string())
    }

    pub(crate) fn require_installing_generation(&self, expected: u64) -> Result<(), String> {
        match self.phase {
            NativePhase::Installing { generation, .. } if generation == expected => Ok(()),
            _ => Err("native bootstrap generation is stale".into()),
        }
    }

    pub(crate) fn unavailable_reason(&self) -> &'static str {
        match self.phase {
            NativePhase::Vacant => "native application is staged but not active",
            NativePhase::Installing { .. } => "native application installation is in progress",
            NativePhase::Active { .. } => "native application is active",
            NativePhase::Releasing { .. } => "native application release is in progress",
            NativePhase::Released(ref tombstone) if tombstone.succeeded => {
                "native application authority was released"
            }
            NativePhase::Released(_) => "native application cleanup is indeterminate",
        }
    }

    pub(crate) fn reserve_install(&mut self) -> Result<u64, String> {
        match &self.phase {
            NativePhase::Vacant => {}
            NativePhase::Released(tombstone) if tombstone.succeeded => {}
            NativePhase::Installing { .. } => {
                return Err("native application bootstrap is already in progress".into());
            }
            NativePhase::Releasing { .. } => {
                return Err("native application release is in progress".into());
            }
            NativePhase::Active { .. } => {
                return Err(
                    "native application is already installed; replace its document explicitly"
                        .into(),
                );
            }
            NativePhase::Released(_) => {
                return Err("native application release blocks re-entry".into());
            }
        }
        let generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| "native lifecycle generation exhausted".to_string())?;
        let phase = std::mem::replace(&mut self.phase, NativePhase::Vacant);
        let origin = match phase {
            NativePhase::Vacant => InstallOrigin::Vacant,
            NativePhase::Released(tombstone) if tombstone.succeeded => {
                InstallOrigin::Released(tombstone)
            }
            _ => unreachable!("install eligibility was validated before phase extraction"),
        };
        self.generation = generation;
        self.phase = NativePhase::Installing { generation, origin };
        Ok(generation)
    }

    pub(crate) fn rollback_install(&mut self, generation: u64) {
        let phase = std::mem::replace(&mut self.phase, NativePhase::Vacant);
        match phase {
            NativePhase::Installing {
                generation: active,
                origin,
            } if active == generation => {
                let _ = self.next_generation();
                self.phase = match origin {
                    InstallOrigin::Vacant => NativePhase::Vacant,
                    InstallOrigin::Released(tombstone) => NativePhase::Released(tombstone),
                };
            }
            other => self.phase = other,
        }
    }

    pub(crate) fn install(
        &mut self,
        generation: u64,
        application: Box<dyn NativeDispatch>,
    ) -> Result<(), String> {
        let phase = std::mem::replace(&mut self.phase, NativePhase::Vacant);
        match phase {
            NativePhase::Installing {
                generation: active,
                origin: _,
            } if active == generation => {
                self.phase = NativePhase::Active {
                    generation,
                    application,
                };
                Ok(())
            }
            other => {
                self.phase = other;
                Err("native bootstrap generation is stale".into())
            }
        }
    }

    pub(crate) fn admit_release(
        &mut self,
        request_id: &str,
        instance_id: &str,
        document_id: &str,
        expected_revision: u64,
        fingerprint: &[u8],
        cancelled_before_dispatch: bool,
    ) -> Result<ReleaseAdmission, String> {
        if let Some(tombstone) = self.retained_releases.get(request_id) {
            if tombstone.fingerprint != fingerprint {
                return Err(
                    "invalid_request:requestId was reused with a changed release body".into(),
                );
            }
            let mut receipt = tombstone.receipt.clone();
            receipt["retrieved"] = serde_json::Value::Bool(true);
            return Ok(ReleaseAdmission::Retry {
                generation: tombstone.generation,
                receipt,
            });
        }
        if cancelled_before_dispatch {
            return Err(
                "cancelled_before_dispatch:native release was cancelled before admission".into(),
            );
        }
        match &self.phase {
            NativePhase::Active { application, .. } => {
                let error = if application.instance_id() != instance_id {
                    Some("wrong_instance:native application instance mismatch")
                } else if application.document_id() != document_id {
                    Some("wrong_document:native document was replaced")
                } else if application.content_revision() != expected_revision {
                    Some("stale_revision:native content revision is stale")
                } else {
                    None
                };
                if let Some(error) = error {
                    return Err(error.into());
                }
            }
            NativePhase::Installing { .. } => {
                return Err("unavailable:native application installation is in progress".into());
            }
            NativePhase::Releasing { .. } => {
                return Err("unavailable:native application release is in progress".into());
            }
            NativePhase::Released(_) => {
                return Err("unavailable:native application authority was already released".into());
            }
            NativePhase::Vacant => {
                return Err("unavailable:native application is unavailable".into());
            }
        }
        let next_generation = self
            .generation
            .checked_add(1)
            .ok_or_else(|| "native lifecycle generation exhausted".to_string())?;
        let phase = std::mem::replace(&mut self.phase, NativePhase::Vacant);
        match phase {
            NativePhase::Active { application, .. } => {
                self.generation = next_generation;
                self.phase = NativePhase::Releasing {
                    generation: next_generation,
                    request_id: request_id.into(),
                    fingerprint: fingerprint.to_vec(),
                    application: Some(application),
                };
                Ok(ReleaseAdmission::Execute {
                    generation: next_generation,
                })
            }
            _ => unreachable!("release eligibility was validated before phase extraction"),
        }
    }

    pub(crate) fn finish_release(&mut self, tombstone: ReleaseTombstone) {
        self.retained_releases
            .insert(tombstone.request_id.clone(), tombstone.clone());
        self.phase = NativePhase::Released(tombstone);
    }
}

pub(crate) trait NativeDispatch: Send {
    fn instance_id(&self) -> &str;
    fn document_id(&self) -> &str;
    fn content_revision(&self) -> u64;
    fn dispatch(&mut self, request: OpacityRequest) -> ResponseEnvelope;
    fn replace_document(&mut self, document: OpacityDocument) -> Result<Vec<JobReceipt>, String>;
    fn start_next_export_frame(&mut self, job_id: &str) -> Result<Option<PendingFrame>, String>;
    fn finish_export_frame(&mut self, pending: PendingFrame) -> Result<JobReceipt, String>;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

impl<P, C, R> NativeDispatch for NativeApplication<P, C, R>
where
    P: StagedArtifactPort + Send + 'static,
    C: ExportCompositor + Send + 'static,
    R: ExportResourceResolver + Send + 'static,
{
    fn instance_id(&self) -> &str {
        NativeApplication::instance_id(self)
    }
    fn document_id(&self) -> &str {
        NativeApplication::document_id(self)
    }
    fn content_revision(&self) -> u64 {
        NativeApplication::content_revision(self)
    }
    fn dispatch(&mut self, request: OpacityRequest) -> ResponseEnvelope {
        NativeApplication::dispatch(self, request)
    }
    fn replace_document(&mut self, document: OpacityDocument) -> Result<Vec<JobReceipt>, String> {
        NativeApplication::replace_document(self, document)
    }
    fn start_next_export_frame(&mut self, job_id: &str) -> Result<Option<PendingFrame>, String> {
        NativeApplication::start_next_export_frame(self, job_id).map_err(|error| error.message)
    }
    fn finish_export_frame(&mut self, pending: PendingFrame) -> Result<JobReceipt, String> {
        NativeApplication::finish_export_frame(self, pending).map_err(|error| error.message)
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Advance at most one frame per lock interval so cancellation and document
/// replacement can observe and terminalize a running job between frames.
pub(crate) fn spawn_export_pump(native: NativeState, generation: u64, job_id: String) {
    let _ = spawn_export_pump_with(native, generation, job_id, |task| {
        std::thread::Builder::new()
            .name("nemo-native-export".into())
            .spawn(task)
            .map(|_| ())
    });
}

type PumpTask = Box<dyn FnOnce() + Send + 'static>;

/// A failed OS thread allocation falls back to the calling transport worker,
/// so an already-accepted export cannot remain running without an owner.
fn spawn_export_pump_with(
    native: NativeState,
    generation: u64,
    job_id: String,
    spawn: impl FnOnce(PumpTask) -> io::Result<()>,
) -> bool {
    let thread_native = Arc::clone(&native);
    let thread_job = job_id.clone();
    match spawn(Box::new(move || {
        run_export_pump(thread_native, generation, thread_job)
    })) {
        Ok(()) => true,
        Err(_) => {
            run_export_pump(native, generation, job_id);
            false
        }
    }
}

fn run_export_pump(native: NativeState, generation: u64, job_id: String) {
    run_export_pump_with(native, generation, job_id, std::thread::yield_now);
}

fn run_export_pump_with(
    native: NativeState,
    generation: u64,
    job_id: String,
    mut between_locks: impl FnMut(),
) {
    loop {
        let pending = match native.lock() {
            Ok(mut guard) => guard
                .active_mut(generation)
                .and_then(|application| application.start_next_export_frame(&job_id)),
            Err(_) => return,
        };
        let Ok(Some(pending)) = pending else {
            return;
        };
        between_locks();
        let receipt = match native.lock() {
            Ok(mut guard) => guard
                .active_mut(generation)
                .and_then(|application| application.finish_export_frame(pending)),
            Err(_) => return,
        };
        if !matches!(receipt, Ok(receipt) if receipt.status == JobStatus::Running) {
            return;
        }
    }
}

#[cfg(test)]
pub(crate) fn run_export_pump_interleaved(
    native: NativeState,
    generation: u64,
    job_id: String,
    between_locks: impl FnMut(),
) {
    run_export_pump_with(native, generation, job_id, between_locks);
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    pub(crate) struct TerminalPump(pub(crate) Arc<AtomicUsize>);

    impl NativeDispatch for TerminalPump {
        fn instance_id(&self) -> &str {
            "pump-fixture"
        }
        fn document_id(&self) -> &str {
            "document-fixture"
        }
        fn content_revision(&self) -> u64 {
            0
        }
        fn dispatch(&mut self, _: OpacityRequest) -> ResponseEnvelope {
            unreachable!("pump test does not dispatch transport requests")
        }
        fn replace_document(&mut self, _: OpacityDocument) -> Result<Vec<JobReceipt>, String> {
            unreachable!("pump test does not replace documents")
        }
        fn start_next_export_frame(&mut self, _: &str) -> Result<Option<PendingFrame>, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }
        fn finish_export_frame(&mut self, _: PendingFrame) -> Result<JobReceipt, String> {
            unreachable!("terminal pump has no pending frame")
        }
        fn as_any_mut(&mut self) -> &mut dyn Any {
            self
        }
    }

    #[test]
    fn thread_allocation_failure_runs_the_accepted_job_synchronously() {
        let advances = Arc::new(AtomicUsize::new(0));
        let native: NativeState = Arc::new(Mutex::new(NativeAuthority {
            generation: 1,
            retained_releases: BTreeMap::new(),
            phase: NativePhase::Active {
                generation: 1,
                application: Box::new(TerminalPump(Arc::clone(&advances))),
            },
        }));
        let asynchronous = spawn_export_pump_with(native, 1, "job-1".into(), |_| {
            Err(io::Error::other("simulated thread exhaustion"))
        });
        assert!(!asynchronous);
        assert_eq!(advances.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn generation_exhaustion_preserves_active_authority_during_release_admission() {
        let mut authority = NativeAuthority {
            generation: u64::MAX,
            retained_releases: BTreeMap::new(),
            phase: NativePhase::Active {
                generation: u64::MAX,
                application: Box::new(TerminalPump(Arc::new(AtomicUsize::new(0)))),
            },
        };
        assert!(authority
            .admit_release(
                "release",
                "pump-fixture",
                "document-fixture",
                0,
                b"body",
                false,
            )
            .unwrap_err()
            .contains("generation exhausted"));
        assert_eq!(authority.active_generation().unwrap(), u64::MAX);
    }

    #[test]
    fn generation_exhaustion_preserves_released_reentry_tombstone() {
        let tombstone = ReleaseTombstone {
            generation: u64::MAX,
            request_id: "release".into(),
            fingerprint: b"body".to_vec(),
            receipt: serde_json::json!({"status":"succeeded"}),
            succeeded: true,
        };
        let mut authority = NativeAuthority {
            generation: u64::MAX,
            retained_releases: BTreeMap::new(),
            phase: NativePhase::Released(tombstone),
        };
        assert!(authority
            .reserve_install()
            .unwrap_err()
            .contains("generation exhausted"));
        assert!(matches!(authority.phase, NativePhase::Released(_)));
    }
}
