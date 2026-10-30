use crate::native_dispatch::{
    replacement, spawn_export_pump_with, NativeAuthority, NativeDispatch, NativePhase,
    NativeReleaseProgress, NativeState, ReleaseAdmission, ReleaseTombstone, ReplacementAdmission,
    ReplacementReplay, ReplacementStage,
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

#[test]
fn dispatch_refusal_is_fallible_without_document_revision_or_generation_change() {
    let observations = Arc::new(AtomicUsize::new(0));
    let mut authority = NativeAuthority::default();
    let generation = authority.reserve_install().unwrap();
    authority
        .install(generation, Box::new(TerminalPump(observations.clone())))
        .unwrap();
    let request = OpacityRequest::query(
        "refused",
        "pump-fixture",
        "document-fixture",
        "query.document.revision",
        serde_json::json!({}),
    );
    let result: Result<ResponseEnvelope, String> =
        authority.active_mut(generation).unwrap().dispatch(request);
    assert_eq!(
        result.unwrap_err(),
        "terminal pump has no document dispatcher"
    );
    assert_eq!(authority.active_generation().unwrap(), generation);
    assert_eq!(authority.active().unwrap().1.content_revision(), 0);
    assert_eq!(
        authority.active().unwrap().1.document_id(),
        "document-fixture"
    );
    assert_eq!(observations.load(Ordering::SeqCst), 1);
}

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
    fn dispatch(&mut self, _: OpacityRequest) -> Result<ResponseEnvelope, String> {
        self.0.fetch_add(1, Ordering::SeqCst); // Observation only; no document writer.
        Err("terminal pump has no document dispatcher".into())
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
    fn release_project(&mut self) -> Result<NativeReleaseProgress, String> {
        Err("terminal pump does not support host cleanup".into())
    }
    fn release_progress(&self) -> Option<NativeReleaseProgress> {
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
        inactive_project: None,
        retained_releases: BTreeMap::new(),
        retained_replacements: BTreeMap::new(),
        retired_replacement_ids: Default::default(),
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
        inactive_project: None,
        retained_releases: BTreeMap::new(),
        retained_replacements: BTreeMap::new(),
        retired_replacement_ids: Default::default(),
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
        inactive_project: None,
        retained_releases: BTreeMap::new(),
        retained_replacements: BTreeMap::new(),
        retired_replacement_ids: Default::default(),
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

fn complete_replacement(authority: &mut NativeAuthority, request_id: &str) -> u64 {
    let ReplacementAdmission::Execute(generation) = authority
        .admit_replace_request(
            request_id,
            b"same-body",
            "pump-fixture",
            "document-fixture",
            0,
            |_| Ok(()),
        )
        .unwrap()
    else {
        panic!("{request_id} must execute")
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
    generation
}

#[test]
fn replacement_capacity_retires_superseded_receipts_so_it_never_blocks_a_user() {
    let mut authority = installed_pump();
    let total = replacement::MAX_RETAINED_REPLACEMENTS * 3 + 1;
    let mut last = 0;
    for ordinal in 0..total {
        last = complete_replacement(&mut authority, &format!("replace-{ordinal}"));
    }
    assert_eq!(authority.active_generation().unwrap(), last);
    // A recent request still replays its receipt.
    assert!(matches!(
        authority
            .lookup_replace_replay(&format!("replace-{}", total - 1), b"same-body")
            .unwrap(),
        Some(ReplacementReplay::Succeeded(_))
    ));
    // A retired one replays Expired, never re-executing, whatever body the retry carries.
    for body in [&b"same-body"[..], b"other-body"] {
        let ReplacementAdmission::Replay(ReplacementReplay::Expired) = authority
            .admit_replace_request(
                "replace-0",
                body,
                "pump-fixture",
                "document-fixture",
                0,
                |_| panic!("a retired request cannot preflight or re-execute"),
            )
            .unwrap()
        else {
            panic!("a retired request must replay Expired")
        };
    }
    assert_eq!(authority.active_generation().unwrap(), last);
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

fn seed_replacement(
    authority: &mut NativeAuthority,
    id: &str,
    generation: u64,
    outcome: ReplacementReplay,
) {
    authority.retained_replacements.insert(
        id.into(),
        replacement::ReplacementRecord {
            generation,
            fingerprint: b"body".to_vec(),
            outcome,
            tombstone: false,
        },
    );
}

fn admit_new(authority: &mut NativeAuthority, id: &str) -> Result<ReplacementAdmission, String> {
    authority.admit_replace_request(id, b"body", "pump-fixture", "document-fixture", 0, |_| {
        Ok(())
    })
}

#[test]
fn pending_and_current_generation_receipts_are_never_evicted() {
    let mut authority = installed_pump();
    authority.generation = 50;
    for ordinal in 0..replacement::MAX_RETAINED_REPLACEMENTS as u64 {
        let pending = ReplacementReplay::Pending(replacement::ReplacementProgress::default());
        seed_replacement(&mut authority, &format!("p{ordinal}"), ordinal + 1, pending);
    }
    // All pending: capacity is exhausted and nothing changes.
    assert!(admit_new(&mut authority, "new")
        .unwrap_err()
        .contains("capacity exhausted"));
    assert_eq!(
        authority.retained_replacements.len(),
        replacement::MAX_RETAINED_REPLACEMENTS
    );
    // A settled receipt of the current generation is not superseded either.
    seed_replacement(&mut authority, "p3", 50, ReplacementReplay::Expired);
    assert!(admit_new(&mut authority, "new")
        .unwrap_err()
        .contains("capacity exhausted"));
    // A settled superseded one is retired; every pending record survives.
    seed_replacement(&mut authority, "p7", 8, ReplacementReplay::Expired);
    assert!(matches!(
        admit_new(&mut authority, "new"),
        Ok(ReplacementAdmission::Execute(_))
    ));
    assert!(authority.retained_replacements["p7"].tombstone);
    let pending = authority
        .retained_replacements
        .values()
        .filter(|record| matches!(record.outcome, ReplacementReplay::Pending(_)))
        .count();
    assert_eq!(pending, replacement::MAX_RETAINED_REPLACEMENTS - 2 + 1);
}

#[test]
fn tombstones_are_bounded_and_the_oldest_are_forgotten_first() {
    let mut authority = installed_pump();
    authority.generation = 10_000;
    for ordinal in 0..(replacement::MAX_REPLACEMENT_TOMBSTONES + 5) as u64 {
        seed_replacement(
            &mut authority,
            &format!("t{ordinal}"),
            ordinal + 1,
            ReplacementReplay::Expired,
        );
        authority
            .retained_replacements
            .get_mut(&format!("t{ordinal}"))
            .unwrap()
            .tombstone = true;
    }
    authority.retire_for_capacity();
    let kept = authority
        .retained_replacements
        .values()
        .filter(|r| r.tombstone)
        .count();
    assert_eq!(kept, replacement::MAX_REPLACEMENT_TOMBSTONES);
    assert!(!authority.retained_replacements.contains_key("t0"));
    assert!(authority.retained_replacements.contains_key("t260"));
}

#[test]
fn an_id_pruned_beyond_the_tombstone_bound_never_reexecutes() {
    let mut authority = installed_pump();
    authority.generation = 10_000;
    let total = replacement::MAX_REPLACEMENT_TOMBSTONES + 40;
    for ordinal in 0..total as u64 {
        seed_replacement(
            &mut authority,
            &format!("t{ordinal}"),
            ordinal + 1,
            ReplacementReplay::Expired,
        );
        authority
            .retained_replacements
            .get_mut(&format!("t{ordinal}"))
            .unwrap()
            .tombstone = true;
    }
    authority.retire_for_capacity();
    assert!(!authority.retained_replacements.contains_key("t0"));
    // The exact tombstone is gone, yet every pruned id still replays Expired.
    for ordinal in 0..40 {
        let id = format!("t{ordinal}");
        assert!(matches!(
            authority.admit_replace_request(&id, b"body", "pump-fixture", "document-fixture", 0, |_| {
                panic!("a retired id must not re-execute")
            }),
            Ok(ReplacementAdmission::Replay(ReplacementReplay::Expired))
        ));
    }
    // An unrelated new id is still admitted and executed.
    assert!(matches!(
        admit_new(&mut authority, "fresh-request"),
        Ok(ReplacementAdmission::Execute(_))
    ));
}
