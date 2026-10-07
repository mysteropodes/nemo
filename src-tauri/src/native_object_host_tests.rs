//! Independent object data and real shared host installation/lifecycle controls.
use crate::application_mcp::ApplicationMcp;
use crate::{
    native_application::{admit_release_request, complete_release},
    native_application_contract::{host_error, NativeReleaseRequest, HOST_API_VERSION},
    native_dispatch::{NativeDispatch, NativeObjectHost, NativePhase, ReleaseAdmission},
};
use native_engine::{
    commands::{NativeOpacityApplication, OpacityRequest, ResponseEnvelope},
    document::OpacityDocument,
    export_job::{JobReceipt, PendingFrame},
    object_codec::decode_project,
};
use serde_json::{json, Value};

fn fixture() -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    json!({"format":"nemo.native-object-document","formatVersion":1,"totalFrames":21,
        "layers":[{"layerUid":"legacy layer:alpha"},{"layerUid":"0"}],"objects":cases["records"]})
}

#[test]
fn production_installer_owns_the_host_slot_at_fresh_revision_zero() {
    let state = ApplicationMcp::default();
    let document = decode_project(&serde_json::to_vec(&fixture()).unwrap()).unwrap();
    state.install_native_object(document).unwrap();
    let native = state.native_state();
    let authority = native.lock().unwrap();
    let (_, owner) = authority.active().unwrap();
    assert_eq!(owner.instance_id(), state.instance_id());
    assert!(owner.document_id().starts_with("native-object-document-"));
    assert_eq!(owner.content_revision(), 0);
    assert!(uuid::Uuid::parse_str(owner.instance_id()).is_ok());
    assert!(owner.document_id().len() <= 128);
    assert!(owner
        .document_id()
        .strip_prefix("native-object-document-")
        .unwrap()
        .parse::<u64>()
        .is_ok());
}

fn install(state: &ApplicationMcp) {
    state
        .install_native_object(decode_project(&serde_json::to_vec(&fixture()).unwrap()).unwrap())
        .unwrap();
}
fn identity(state: &ApplicationMcp) -> (u64, String, u64) {
    let native = state.native_state();
    let guard = native.lock().unwrap();
    let (generation, owner) = guard.active().unwrap();
    (
        generation,
        owner.document_id().into(),
        owner.content_revision(),
    )
}
fn release(state: &ApplicationMcp, id: &str) -> NativeReleaseRequest {
    NativeReleaseRequest {
        api_version: HOST_API_VERSION,
        request_id: id.into(),
        instance_id: state.instance_id().into(),
        document_id: identity(state).1,
        expected_revision: identity(state).2,
        cancelled_before_dispatch: false,
    }
}
fn admit(state: &ApplicationMcp, request: &NativeReleaseRequest) -> u64 {
    match admit_release_request(&state.native_state(), request).unwrap() {
        ReleaseAdmission::Execute { generation } => generation,
        ReleaseAdmission::Retry { .. } => panic!("fresh release unexpectedly replayed"),
    }
}

// Real production opacity document authority in a test-only host adapter. This
// does not claim DesktopNativeApplication/GPU/installed-desktop acceptance.
struct OpacityFixture(NativeOpacityApplication);
impl NativeDispatch for OpacityFixture {
    fn instance_id(&self) -> &str {
        self.0.instance_id()
    }
    fn document_id(&self) -> &str {
        self.0.document_id()
    }
    fn content_revision(&self) -> u64 {
        self.0.content_revision()
    }
    fn dispatch(&mut self, request: OpacityRequest) -> Result<ResponseEnvelope, String> {
        Ok(self.0.handle(request))
    }
    fn replace_document(&mut self, _: OpacityDocument) -> Result<Vec<JobReceipt>, String> {
        Err("unused".into())
    }
    fn start_next_export_frame(&mut self, _: &str) -> Result<Option<PendingFrame>, String> {
        Err("unused".into())
    }
    fn finish_export_frame(&mut self, _: PendingFrame) -> Result<JobReceipt, String> {
        Err("unused".into())
    }
    fn release_project(&mut self) -> Result<crate::native_dispatch::NativeReleaseProgress, String> {
        Err("unused".into())
    }
    fn release_progress(&self) -> Option<crate::native_dispatch::NativeReleaseProgress> {
        None
    }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
        self
    }
}
fn opacity(state: &ApplicationMcp) -> OpacityFixture {
    let document = native_engine::codec::decode_project(include_bytes!(
        "../../native-engine/tests/fixtures/opacity-v2/project.json"
    ))
    .unwrap();
    OpacityFixture(NativeOpacityApplication::new(state.instance_id(), document).unwrap())
}

#[test]
fn object_and_real_opacity_owners_cannot_coexist_or_evict_each_other() {
    let state = ApplicationMcp::default();
    let owner = opacity(&state);
    let old_document =
        serde_json::to_value(owner.0.acquire_snapshot(0).unwrap().document()).unwrap();
    let reservation = state.reserve_native_install().unwrap();
    state
        .install_dispatch(reservation.generation(), Box::new(owner))
        .unwrap();
    let before = identity(&state);
    assert!(state
        .install_native_object(decode_project(&serde_json::to_vec(&fixture()).unwrap()).unwrap())
        .is_err());
    assert_eq!(identity(&state), before);
    let native = state.native_state();
    let mut guard = native.lock().unwrap();
    let active = guard
        .active_mut(before.0)
        .unwrap()
        .as_any_mut()
        .downcast_mut::<OpacityFixture>()
        .unwrap();
    assert_eq!(
        serde_json::to_value(active.0.acquire_snapshot(0).unwrap().document()).unwrap(),
        old_document
    );
    drop(guard);
    let object_state = ApplicationMcp::default();
    install(&object_state);
    let before = identity(&object_state);
    assert!(object_state.reserve_native_install().is_err());
    assert!(object_state
        .install_dispatch(before.0, Box::new(opacity(&object_state)))
        .is_err());
    assert_eq!(identity(&object_state), before);
}

#[test]
fn typed_invalid_documents_and_bad_constructor_instances_are_rejected_before_reservation() {
    let state = ApplicationMcp::default();
    for (path, invalid) in [
        ("/totalFrames", json!(0)),
        ("/objects/0/fill/r", json!(2)),
        ("/objects/0/target/layerUid", json!("absent")),
    ] {
        let mut value = fixture();
        *value.pointer_mut(path).unwrap() = invalid;
        assert!(state
            .install_native_object(serde_json::from_value(value).unwrap())
            .is_err());
        assert!(matches!(
            state.native_state().lock().unwrap().phase,
            NativePhase::Vacant
        ));
    }
    for instance in ["", "bad instance", "🟠", ".bad", &"x".repeat(129)] {
        assert!(NativeObjectHost::new(
            instance.into(),
            decode_project(&serde_json::to_vec(&fixture()).unwrap()).unwrap()
        )
        .is_err());
    }
    install(&state);
    let before = identity(&state);
    let binding = state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    let mut invalid = fixture();
    invalid["totalFrames"] = json!(0);
    assert!(state
        .install_native_object(serde_json::from_value(invalid).unwrap())
        .is_err());
    assert_eq!(identity(&state), before);
    let after = state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    assert_eq!(binding["lifecycleGeneration"], after["lifecycleGeneration"]);
    assert_eq!(binding["documentId"], after["documentId"]);
}

#[test]
fn original_reservation_path_rolls_back_duplicates_and_stale_install_without_losing_origin() {
    let state = ApplicationMcp::default();
    let reservation: crate::application_mcp::NativeInstallReservation =
        state.reserve_native_install().unwrap();
    let generation = reservation.generation();
    assert!(state.reserve_native_install().is_err());
    assert!(state
        .install_native_object(decode_project(&serde_json::to_vec(&fixture()).unwrap()).unwrap())
        .is_err());
    let owner = NativeObjectHost::new(
        state.instance_id().into(),
        decode_project(&serde_json::to_vec(&fixture()).unwrap()).unwrap(),
    )
    .unwrap();
    assert!(state
        .install_dispatch(generation + 1, Box::new(owner))
        .is_err());
    assert!(
        matches!(state.native_state().lock().unwrap().phase, NativePhase::Installing { generation: current, .. } if current == generation)
    );
    drop(reservation);
    assert!(matches!(
        state.native_state().lock().unwrap().phase,
        NativePhase::Vacant
    ));
    install(&state);
    assert!(identity(&state).0 > generation);
}

#[test]
fn ordinary_dispatch_and_external_invalid_identities_refuse_without_content_change() {
    let state = ApplicationMcp::default();
    install(&state);
    let before = identity(&state);
    let request = json!({"apiVersion":2,"requestId":"ordinary","instanceId":state.instance_id(),
        "documentId":before.1,"operation":"query.document.opacity","payload":{"stableTarget":{"layerUid":"layer"}}});
    assert_eq!(
        state
            .dispatch_native(serde_json::from_value(request.clone()).unwrap())
            .unwrap_err(),
        "native object host dispatch is unavailable"
    );
    for field in ["requestId", "instanceId", "documentId"] {
        let mut invalid = request.clone();
        invalid[field] = json!("bad id");
        assert!(state
            .dispatch_native(serde_json::from_value(invalid).unwrap())
            .is_err());
    }
    assert_eq!(identity(&state), before);
    let mut owner = NativeObjectHost::new(
        state.instance_id().into(),
        decode_project(&serde_json::to_vec(&fixture()).unwrap()).unwrap(),
    )
    .unwrap();
    assert!(owner
        .replace_document(
            native_engine::codec::decode_project(include_bytes!(
                "../../native-engine/tests/fixtures/opacity-v2/project.json"
            ))
            .unwrap()
        )
        .is_err());
    assert!(owner.start_next_export_frame("unused").is_err());
    assert_eq!(owner.content_revision(), 0);
    assert_eq!(
        serde_json::to_value(owner.history.acquire_snapshot(0).unwrap().document()).unwrap(),
        fixture()
    );
}

#[test]
fn cleanup_retains_real_history_depths_and_closes_the_real_owner_without_resources() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut owner = NativeObjectHost::new(
        "history-host".into(),
        decode_project(&serde_json::to_vec(&fixture()).unwrap()).unwrap(),
    )
    .unwrap();
    let zero = owner.history.acquire_snapshot(0).unwrap();
    let candidate = owner
        .history
        .prepare_fill_json(&serde_json::to_vec(&cases["command"]["request"]["payload"]).unwrap())
        .unwrap();
    assert_eq!(owner.history.commit_fill(candidate), Ok(1));
    assert!(owner.release_progress().is_none());
    let progress = owner.release_project().unwrap();
    assert!(progress.reconciliation_complete());
    assert_eq!(progress.application.content_revision, 1);
    assert_eq!(
        (
            progress.application.undo_depth,
            progress.application.redo_depth
        ),
        (1, 0)
    );
    assert!(progress.application.cancelled_transaction_id.is_none());
    assert!(progress.application.exports.receipts.is_empty());
    assert!(progress.cancelled_preview.is_empty() && progress.unresolved_preview.is_empty());
    let retained = owner.release_progress().unwrap();
    assert_eq!(retained.application, progress.application);
    assert_eq!(
        owner.release_project().unwrap().application,
        progress.application
    );
    assert_eq!(
        owner
            .dispatch(OpacityRequest::query(
                "closed",
                owner.instance_id(),
                owner.document_id(),
                "query.document.revision",
                json!({})
            ))
            .unwrap_err(),
        "native object host was released"
    );
    assert_eq!(serde_json::to_value(zero.document()).unwrap(), fixture());
}

#[test]
fn real_terminal_release_replays_without_touching_a_fresh_object_or_opacity_owner() {
    let state = ApplicationMcp::default();
    install(&state);
    let before = identity(&state);
    let request = release(&state, "release-object");
    let generation = admit(&state, &request);
    assert!(state.reserve_native_install().is_err());
    let receipt = complete_release(&state.native_state(), generation, &request, || {
        Ok((vec![], "already_absent"))
    })
    .unwrap();
    assert_eq!(receipt.status, "succeeded");
    assert!(receipt.authority_removal_completed);
    assert_eq!(receipt.document_id, before.1);
    assert!(receipt.reconciled_exports.is_empty() && receipt.cancelled_preview_work_ids.is_empty());
    install(&state);
    let fresh = identity(&state);
    assert!(fresh.0 > generation);
    assert_ne!(fresh.1, before.1);
    let replay = admit_release_request(&state.native_state(), &request).unwrap();
    let ReleaseAdmission::Retry {
        receipt: replay, ..
    } = replay
    else {
        panic!("retained release did not replay")
    };
    assert_eq!(replay["status"], "succeeded");
    assert_eq!(replay["documentId"], before.1);
    let mut normalized = replay;
    assert_eq!(normalized["retrieved"], true);
    normalized["retrieved"] = json!(false);
    assert_eq!(normalized, serde_json::to_value(&receipt).unwrap());
    assert_eq!(identity(&state), fresh);
    assert!(
        complete_release(&state.native_state(), generation, &request, || panic!(
            "stale callback ran"
        ))
        .is_err()
    );
    let mut changed = request.clone();
    changed.expected_revision = 1;
    assert!(admit_release_request(&state.native_state(), &changed).is_err());
    let next = release(&state, "release-fresh");
    let generation = admit(&state, &next);
    complete_release(&state.native_state(), generation, &next, || {
        Ok((vec![], "already_absent"))
    })
    .unwrap();
    let reservation = state.reserve_native_install().unwrap();
    state
        .install_dispatch(reservation.generation(), Box::new(opacity(&state)))
        .unwrap();
    assert!(identity(&state).1.starts_with("native-document-"));
}

#[test]
fn invalid_or_cancelled_release_preserves_owner_and_failed_cleanup_fences_reentry() {
    let state = ApplicationMcp::default();
    install(&state);
    let before = identity(&state);
    let request = release(&state, "failing-release");
    for (field, invalid) in [
        ("instanceId", json!("other")),
        ("documentId", json!("other")),
        ("expectedRevision", json!(1)),
        ("cancelledBeforeDispatch", json!(true)),
    ] {
        let mut value = serde_json::to_value(&request).unwrap();
        value[field] = invalid;
        assert!(admit_release_request(
            &state.native_state(),
            &serde_json::from_value(value).unwrap()
        )
        .is_err());
        assert_eq!(identity(&state), before);
    }
    let generation = admit(&state, &request);
    let receipt = complete_release(&state.native_state(), generation, &request, || {
        Err(host_error("cleanup_failed", "fixture viewport failure"))
    })
    .unwrap();
    assert_eq!(receipt.status, "indeterminate");
    assert!(state.reserve_native_install().is_err());
    assert!(state
        .install_native_object(decode_project(&serde_json::to_vec(&fixture()).unwrap()).unwrap())
        .is_err());
}

#[test]
fn pinned_complete_opaque_data_and_fallible_library_reads_survive_real_owner_drop() {
    let state = ApplicationMcp::default();
    let mut source = fixture();
    source["layers"][0]["layerUid"] = json!("opaque layer / ".repeat(20));
    source["objects"][0]["target"]["layerUid"] = source["layers"][0]["layerUid"].clone();
    state
        .install_native_object(decode_project(&serde_json::to_vec(&source).unwrap()).unwrap())
        .unwrap();
    let snapshot = {
        let native = state.native_state();
        let mut authority = native.lock().unwrap();
        let generation = authority.active_generation().unwrap();
        let owner = authority
            .active_mut(generation)
            .unwrap()
            .as_any_mut()
            .downcast_mut::<NativeObjectHost>()
            .unwrap();
        owner.history.acquire_snapshot(0).unwrap()
    };
    let request = release(&state, "release-pinned");
    let generation = admit(&state, &request);
    complete_release(&state.native_state(), generation, &request, || {
        Ok((vec![], "already_absent"))
    })
    .unwrap();
    assert_eq!(serde_json::to_value(snapshot.document()).unwrap(), source);
    let read = OpacityRequest::query(
        "pinned-read",
        snapshot.instance_id(),
        snapshot.document_id(),
        "query.document.object",
        json!({"atRevision":0,"stableTarget":source["objects"][0]["target"]}),
    );
    let response = serde_json::to_value(snapshot.dispatch(read).unwrap()).unwrap();
    assert_eq!(response["result"]["object"], source["objects"][0]);
    assert!(snapshot
        .dispatch(OpacityRequest::query(
            "bad id",
            snapshot.instance_id(),
            snapshot.document_id(),
            "query.document.object",
            json!({})
        ))
        .is_err());
}
