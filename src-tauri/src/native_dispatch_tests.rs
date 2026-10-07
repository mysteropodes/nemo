use crate::native_dispatch::{
    replacement, spawn_export_pump_with, NativeAuthority, NativeDispatch, NativePhase, NativeState,
    ReleaseAdmission, ReleaseTombstone, ReplacementAdmission, ReplacementReplay, ReplacementStage,
};
use native_engine::{
    commands::{OpacityRequest, ResponseEnvelope},
    document::OpacityDocument,
    export_job::{JobReceipt, PendingFrame},
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{
    any::Any,
    collections::BTreeMap,
    io,
    sync::{Arc, Mutex},
};

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
    fn release_project(&mut self) -> Result<super::NativeReleaseProgress, String> {
        Err("terminal pump does not support host cleanup".into())
    }
    fn release_progress(&self) -> Option<super::NativeReleaseProgress> {
        None
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
        retained_replacements: BTreeMap::new(),
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
        retained_replacements: BTreeMap::new(),
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
            false
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
        retained_replacements: BTreeMap::new(),
        phase: NativePhase::Released(tombstone),
    };
    assert!(authority
        .reserve_install()
        .unwrap_err()
        .contains("generation exhausted"));
    assert!(matches!(authority.phase, NativePhase::Released(_)));
}

fn installed_pump() -> NativeAuthority {
    let mut authority = NativeAuthority::default();
    let generation = authority.reserve_install().unwrap();
    authority
        .install(
            generation,
            Box::new(TerminalPump(Arc::new(AtomicUsize::new(0)))),
        )
        .unwrap();
    authority
}

#[test]
fn replacement_identity_replays_pending_and_fenced_without_second_preflight() {
    let mut authority = installed_pump();
    let old = authority.active_generation().unwrap();
    let calls = AtomicUsize::new(0);
    let first = authority
        .admit_replace_request(
            "replace",
            b"entire-body-a",
            "pump-fixture",
            "document-fixture",
            0,
            |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .unwrap();
    let ReplacementAdmission::Execute(fresh) = first else {
        panic!("first request must execute")
    };
    assert!(fresh > old);
    assert!(authority.active_mut(old).is_err());
    assert!(matches!(
        authority.admit_replace_request(
            "replace",
            b"entire-body-a",
            "pump-fixture",
            "document-fixture",
            0,
            |_| panic!("pending replay cannot preflight")
        ),
        Ok(ReplacementAdmission::Replay(ReplacementReplay::Pending(_)))
    ));
    assert!(authority
        .admit_replace_request(
            "replace",
            b"changed-body",
            "pump-fixture",
            "document-fixture",
            0,
            |_| panic!("changed body cannot preflight")
        )
        .unwrap_err()
        .contains("changed replacement body"));
    assert!(authority
        .admit_replace_request(
            "other-id",
            b"entire-body-a",
            "pump-fixture",
            "document-fixture",
            0,
            |_| panic!("old generation cannot preflight")
        )
        .is_err());
    authority
        .fence_replace(fresh, "callback_dropped", "lost callback")
        .unwrap();
    let ReplacementAdmission::Replay(ReplacementReplay::Fenced(progress)) = authority
        .admit_replace_request(
            "replace",
            b"entire-body-a",
            "pump-fixture",
            "document-fixture",
            0,
            |_| panic!("fenced replay cannot preflight"),
        )
        .unwrap()
    else {
        panic!("fenced replay must be retained")
    };
    assert_eq!(progress.failure.unwrap().kind, "callback_dropped");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn replacement_capacity_denies_new_ids_without_eviction_or_generation_change() {
    let mut authority = installed_pump();
    for ordinal in 0..replacement::MAX_RETAINED_REPLACEMENTS {
        let request_id = format!("replace-{ordinal}");
        let ReplacementAdmission::Execute(generation) = authority
            .admit_replace_request(
                &request_id,
                b"same-body",
                "pump-fixture",
                "document-fixture",
                0,
                |_| Ok(()),
            )
            .unwrap()
        else {
            panic!("within-capacity request must execute")
        };
        let (_, progress) = authority.replacing_mut(generation).unwrap();
        for stage in [
            &mut progress.callback,
            &mut progress.core,
            &mut progress.preview,
            &mut progress.resources,
            &mut progress.viewport,
        ] {
            *stage = ReplacementStage::Complete;
        }
        authority.activate_replace(generation).unwrap();
        authority.record_replace_success(generation, serde_json::json!({"requestId":request_id}));
    }
    let current = authority.active_generation().unwrap();
    assert!(authority
        .admit_replace_request(
            "overflow",
            b"same-body",
            "pump-fixture",
            "document-fixture",
            0,
            |_| panic!("capacity failure cannot preflight")
        )
        .unwrap_err()
        .contains("capacity exhausted"));
    assert_eq!(authority.active_generation().unwrap(), current);
    assert!(matches!(
        authority
            .lookup_replace_replay("replace-0", b"same-body")
            .unwrap(),
        Some(ReplacementReplay::Succeeded(_))
    ));
}

#[test]
fn release_and_reentry_expire_old_success_without_reexecuting_its_id() {
    let mut authority = installed_pump();
    let ReplacementAdmission::Execute(generation) = authority
        .admit_replace_request(
            "replace",
            b"body",
            "pump-fixture",
            "document-fixture",
            0,
            |_| Ok(()),
        )
        .unwrap()
    else {
        panic!("new request must execute")
    };
    let (_, progress) = authority.replacing_mut(generation).unwrap();
    for stage in [
        &mut progress.callback,
        &mut progress.core,
        &mut progress.preview,
        &mut progress.resources,
        &mut progress.viewport,
    ] {
        *stage = ReplacementStage::Complete;
    }
    authority.activate_replace(generation).unwrap();
    authority.record_replace_success(
        generation,
        serde_json::json!({"documentId":"document-fixture"}),
    );
    let ReleaseAdmission::Execute {
        generation: release_generation,
    } = authority
        .admit_release(
            "release",
            "pump-fixture",
            "document-fixture",
            0,
            b"release-body",
            false,
        )
        .unwrap()
    else {
        panic!("release must execute")
    };
    authority.finish_release(ReleaseTombstone {
        generation: release_generation,
        request_id: "release".into(),
        fingerprint: b"release-body".to_vec(),
        receipt: serde_json::json!({"status":"succeeded"}),
        succeeded: true,
    });
    let new_generation = authority.reserve_install().unwrap();
    authority
        .install(
            new_generation,
            Box::new(TerminalPump(Arc::new(AtomicUsize::new(0)))),
        )
        .unwrap();
    assert!(matches!(
        authority.lookup_replace_replay("replace", b"body").unwrap(),
        Some(ReplacementReplay::Expired)
    ));
    assert!(matches!(
        authority.admit_replace_request(
            "replace",
            b"body",
            "pump-fixture",
            "document-fixture",
            0,
            |_| panic!("old id cannot reexecute after reentry")
        ),
        Ok(ReplacementAdmission::Replay(ReplacementReplay::Expired))
    ));
    assert_eq!(authority.active_generation().unwrap(), new_generation);
}
