use crate::application_mcp::*;
use crate::{
    native_application::{admit_release_request, NativePhase, ReleaseAdmission},
    native_application_contract::{NativeReleaseRequest, HOST_API_VERSION},
};
use native_engine::{
    application::{ResourceResolutionError, ResourceResolutionErrorKind},
    codec::decode_project,
    export_job::{ExportArtifact, ExportReadback},
    protocol::OpaqueResourceHandle,
    render_scene::{GeometryPaintInput, RenderScene},
};
use serde_json::json;
use std::sync::{mpsc, Arc, Barrier};

#[path = "application_mcp_diagnostics_tests.rs"]
mod diagnostics;

const PROJECT: &[u8] = include_bytes!("../../native-engine/tests/fixtures/opacity-v2/project.json");

struct Port;

impl StagedArtifactPort for Port {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn write_frame(&mut self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
        Ok(())
    }
    fn publish(
        &mut self,
        _: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String> {
        Ok(ExportArtifact {
            target: target.into(),
            files: files.to_vec(),
        })
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
}

struct Compositor;

impl ExportCompositor for Compositor {
    type Composition = ExportReadback;
    fn compose(&mut self, _: &RenderScene) -> Result<Self::Composition, String> {
        Err("unused compositor".into())
    }
    fn readback_rgba8(&self, result: &Self::Composition) -> Result<ExportReadback, String> {
        Ok(result.clone())
    }
}

struct Resolver;

impl ExportResourceResolver for Resolver {
    fn resolve_geometry(
        &mut self,
        _: &OpaqueResourceHandle,
    ) -> Result<GeometryPaintInput, ResourceResolutionError> {
        Err(ResourceResolutionError::new(
            ResourceResolutionErrorKind::Unavailable,
            "unused resolver",
        ))
    }
}

fn document() -> OpacityDocument {
    decode_project(PROJECT).unwrap()
}

fn fixture() -> (ApplicationMcp, String) {
    let state = ApplicationMcp {
        instance_id: "native-host-fixture".into(),
        ..ApplicationMcp::default()
    };
    let application = NativeApplication::new(
        state.instance_id.clone(),
        document(),
        Port,
        Compositor,
        Resolver,
    )
    .unwrap();
    let document_id = application.document_id().to_owned();
    state.install_native(application).unwrap();
    (state, document_id)
}

fn request(document_id: &str) -> NativeApplicationRequest {
    NativeApplicationRequest {
        api_version: NATIVE_API_VERSION,
        request_id: "shared-query".into(),
        instance_id: "native-host-fixture".into(),
        document_id: document_id.into(),
        expected_revision: None,
        operation: "query.document.revision".into(),
        payload: json!({}),
        cancelled_before_dispatch: false,
    }
}

fn status_request() -> NativeStatusRequest {
    NativeStatusRequest {
        api_version: NATIVE_API_VERSION,
        request_id: "status-query".into(),
        instance_id: "native-host-fixture".into(),
    }
}

fn release_request(document_id: &str, request_id: &str) -> NativeReleaseRequest {
    NativeReleaseRequest {
        api_version: HOST_API_VERSION,
        request_id: request_id.into(),
        instance_id: "native-host-fixture".into(),
        document_id: document_id.into(),
        expected_revision: 0,
        cancelled_before_dispatch: false,
    }
}

#[test]
fn ui_and_mcp_paths_share_one_native_application_and_retained_receipt() {
    let (state, document_id) = fixture();
    let direct = state.dispatch_native(request(&document_id)).unwrap();
    let wire = dispatch_native(&state.instance_id, &state.native, request(&document_id)).unwrap();
    assert!(direct.ok);
    assert_eq!(wire, direct);
    let status = state.native_status(status_request()).unwrap();
    assert!(status.available);
    assert_eq!(status.document_id.as_deref(), Some(document_id.as_str()));
    assert_eq!(status.content_revision, Some(0));
}

#[test]
fn inactive_status_and_explicit_replacement_never_replay_an_old_request() {
    let empty = ApplicationMcp {
        instance_id: "native-host-fixture".into(),
        ..ApplicationMcp::default()
    };
    let unavailable = empty.native_status(status_request()).unwrap();
    assert!(!unavailable.available);
    assert_eq!(
        unavailable.reason.as_deref(),
        Some("native application is staged but not active")
    );

    let (state, old_document) = fixture();
    let duplicate = NativeApplication::new(
        state.instance_id.clone(),
        document(),
        Port,
        Compositor,
        Resolver,
    )
    .unwrap();
    assert_eq!(
        state.install_native(duplicate),
        Err("native application is already installed; replace its document explicitly".into())
    );
    state.replace_native_document(document()).unwrap();
    let current = state.native_status(status_request()).unwrap();
    assert_ne!(current.document_id.as_deref(), Some(old_document.as_str()));
    let rejected = state.dispatch_native(request(&old_document)).unwrap();
    assert!(!rejected.ok);
    assert_eq!(rejected.error.unwrap().code, "wrong_document");
}

#[test]
fn legacy_capabilities_advertise_v2_without_activating_a_document() {
    let response = advertise_native(ApplicationResponse {
        api_version: 1,
        request_id: "capabilities".into(),
        instance_id: "native-host-fixture".into(),
        document_id: "legacy-document".into(),
        revision: 0,
        ok: true,
        result: Some(json!({"capabilities": []})),
        error: None,
    });
    assert_eq!(response.result.unwrap()["nativeApiVersion"], 2);
}

#[test]
fn concurrent_bootstrap_is_exclusive_and_failed_install_releases_reservation() {
    let state = Arc::new(ApplicationMcp::default());
    let start = Arc::new(Barrier::new(3));
    let hold_winner = Arc::new(Barrier::new(2));
    let (sender, receiver) = mpsc::channel();
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let (state, start, hold, sender) = (
                Arc::clone(&state),
                Arc::clone(&start),
                Arc::clone(&hold_winner),
                sender.clone(),
            );
            std::thread::spawn(move || {
                start.wait();
                let reservation = state.reserve_native_install();
                sender.send(reservation.is_ok()).unwrap();
                if reservation.is_ok() {
                    hold.wait();
                }
            })
        })
        .collect();
    start.wait();
    let mut outcomes = [receiver.recv().unwrap(), receiver.recv().unwrap()];
    outcomes.sort();
    assert_eq!(outcomes, [false, true]);
    hold_winner.wait();
    for thread in threads {
        thread.join().unwrap();
    }

    let reservation = state.reserve_native_install().unwrap();
    let wrong =
        NativeApplication::new("wrong-instance", document(), Port, Compositor, Resolver).unwrap();
    assert_eq!(
        state.install_dispatch(reservation.generation(), Box::new(wrong)),
        Err("native application instance mismatch".into())
    );
    drop(reservation);
    assert!(state.reserve_native_install().is_ok());
}

#[test]
fn replacement_and_release_admission_are_serialized_in_both_orders() {
    let (state, old_document) = fixture();
    state.replace_native_document(document()).unwrap();
    let error = admit_release_request(
        &state.native,
        &release_request(&old_document, "replace-first"),
    )
    .unwrap_err();
    assert_eq!(error.code, "wrong_document");
    assert!(matches!(
        state.native.lock().unwrap().phase,
        NativePhase::Active { .. }
    ));

    let (state, document_id) = fixture();
    assert!(matches!(
        admit_release_request(
            &state.native,
            &release_request(&document_id, "release-first")
        ),
        Ok(ReleaseAdmission::Execute { .. })
    ));
    assert!(state.replace_native_document(document()).is_err());
    assert!(matches!(
        state.native.lock().unwrap().phase,
        NativePhase::Releasing { .. }
    ));
}

#[test]
fn release_identity_and_schema_negatives_leave_active_authority_untouched() {
    let (state, document_id) = fixture();
    let generation = state.native.lock().unwrap().active_generation().unwrap();
    let rejects = |request: &NativeReleaseRequest, code: &str| {
        assert_eq!(
            admit_release_request(&state.native, request)
                .unwrap_err()
                .code,
            code
        );
        assert_eq!(
            state.native.lock().unwrap().active_generation(),
            Ok(generation)
        );
    };
    let mut request = release_request(&document_id, "release-stale");
    request.expected_revision = 1;
    rejects(&request, "stale_revision");
    request = release_request(&document_id, "release-wrong-instance");
    request.instance_id = "wrong-instance".into();
    rejects(&request, "wrong_instance");
    request = release_request("wrong-document", "release-wrong-document");
    rejects(&request, "wrong_document");
    request = release_request(&document_id, "release-wrong-version");
    request.api_version += 1;
    rejects(&request, "invalid_request");
    request = release_request(&document_id, " bad");
    rejects(&request, "invalid_request");
    let mut unknown = serde_json::to_value(release_request(&document_id, "unknown")).unwrap();
    unknown["extra"] = json!(true);
    assert!(serde_json::from_value::<NativeReleaseRequest>(unknown).is_err());
    assert_eq!(
        state.native.lock().unwrap().active_generation(),
        Ok(generation)
    );
}

#[test]
fn ui_and_bundled_mcp_share_the_released_unavailable_state() {
    let (state, document_id) = fixture();
    state.retire_native_for_test(json!({"status":"succeeded"}));

    let status = state.native_status(status_request()).unwrap();
    assert!(!status.available);
    assert_eq!(
        status.reason.as_deref(),
        Some("native application authority was released")
    );
    let ui = state.dispatch_native(request(&document_id)).unwrap();
    let mcp = dispatch_native(&state.instance_id, &state.native, request(&document_id)).unwrap();
    assert_eq!(ui, mcp);
    assert_eq!(ui.error.unwrap().code, "unavailable");
}

#[path = "native_audio_track_public_tests.rs"]
mod audio_track_public_tests;
#[path = "native_layer_playhead_cut_public_tests.rs"]
mod layer_playhead_cut_public_tests;
#[path = "native_project_placement_public_tests.rs"]
mod project_placement_public_tests;

// #2242 the generation published in host status is the actual active owner slot's, never a reserved or retired one
// (ported from the superseded #2324).
#[test]
fn status_generation_follows_the_actual_slot_through_release_and_reinstall() {
    let (state, old_document) = fixture();
    let original = state.native_status(status_request()).unwrap();
    let original_generation = state.native.lock().unwrap().active_generation().unwrap();
    assert_eq!(original.lifecycle_generation, Some(original_generation));
    state.retire_native_for_test(json!({"status":"succeeded"}));
    let released = state.native_status(status_request()).unwrap();
    released.validate().unwrap();
    assert!(!released.available);
    assert_eq!(released.lifecycle_generation, None);
    assert!(serde_json::to_value(&released)
        .unwrap()
        .get("lifecycleGeneration")
        .is_none());

    let application = NativeApplication::new(
        state.instance_id.clone(),
        document(),
        Port,
        Compositor,
        Resolver,
    )
    .unwrap();
    let next_document = application.document_id().to_owned();
    state.install_native(application).unwrap();
    let current = state.native_status(status_request()).unwrap();
    current.validate().unwrap();
    let expected = {
        let authority = state.native.lock().unwrap();
        let (generation, owner) = authority.active().unwrap();
        (
            generation,
            owner.document_id().to_owned(),
            owner.content_revision(),
        )
    };
    assert!(
        expected.0 > original_generation,
        "a constant generation cannot qualify"
    );
    assert_ne!(next_document, old_document);
    assert_eq!(current.lifecycle_generation, Some(expected.0));
    assert_eq!(current.document_id.as_deref(), Some(expected.1.as_str()));
    assert_eq!(current.content_revision, Some(expected.2));
    assert_eq!(expected.1, next_document);
    let mut wrong_instance = status_request();
    wrong_instance.instance_id = "foreign-instance".into();
    let refused = state.native_status(wrong_instance).unwrap();
    refused.validate().unwrap();
    assert!(!refused.available);
    assert_eq!(refused.lifecycle_generation, None);
    assert_eq!(
        refused.reason.as_deref(),
        Some("native application instance mismatch")
    );
    assert_eq!(state.native_status(status_request()).unwrap(), current);
}

#[test]
fn installing_status_never_publishes_the_reserved_generation() {
    let state = ApplicationMcp {
        instance_id: "native-host-fixture".into(),
        ..ApplicationMcp::default()
    };
    let reservation = state.reserve_native_install().unwrap();
    assert!(reservation.generation() > 0);
    let installing = state.native_status(status_request()).unwrap();
    installing.validate().unwrap();
    assert!(!installing.available);
    assert_eq!(installing.lifecycle_generation, None);
    assert_eq!(installing.document_id, None);
    assert_eq!(installing.content_revision, None);
    assert_eq!(
        installing.reason.as_deref(),
        Some("native application installation is in progress")
    );
    drop(reservation);
    let vacant = state.native_status(status_request()).unwrap();
    vacant.validate().unwrap();
    assert!(!vacant.available);
    assert_eq!(vacant.lifecycle_generation, None);
}

#[test]
fn host_status_generation_agrees_with_normal_project_metadata_without_mutation() {
    let state = ApplicationMcp {
        instance_id: "native-host-fixture".into(),
        ..ApplicationMcp::default()
    };
    let (document_id, revision, _) = state.install_native_blank_project(64, 64, 24.0).unwrap();
    let before = state.native_project_info(&document_id).unwrap();
    let status = state.native_status(status_request()).unwrap();
    status.validate().unwrap();
    assert!(status.available);
    assert_eq!(status.instance_id, before["instanceId"].as_str().unwrap());
    assert_eq!(status.document_id.as_deref(), Some(document_id.as_str()));
    assert_eq!(status.content_revision, Some(revision));
    assert_eq!(
        status.lifecycle_generation,
        before["lifecycleGeneration"].as_u64()
    );
    assert_eq!(state.native_project_info(&document_id).unwrap(), before);
}

#[test]
fn replacing_status_cannot_publish_old_or_reserved_owner_generation() {
    let (state, document_id) = fixture();
    let old_generation = state
        .native_status(status_request())
        .unwrap()
        .lifecycle_generation
        .unwrap();
    let reserved = state
        .native
        .lock()
        .unwrap()
        .admit_replace(&state.instance_id, &document_id, 0, |_| Ok(()))
        .unwrap();
    assert!(reserved > old_generation);
    let replacing = state.native_status(status_request()).unwrap();
    replacing.validate().unwrap();
    assert!(!replacing.available);
    assert_eq!(replacing.lifecycle_generation, None);
    assert_eq!(replacing.document_id, None);
    assert_eq!(replacing.content_revision, None);
    assert_eq!(
        replacing.reason.as_deref(),
        Some("native replacement is in progress or indeterminate")
    );
}

