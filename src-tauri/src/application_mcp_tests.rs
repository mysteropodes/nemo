use crate::application_mcp::*;
use native_engine::{
    application::{ResourceResolutionError, ResourceResolutionErrorKind},
    codec::decode_project,
    export_job::{ExportArtifact, ExportReadback},
    protocol::OpaqueResourceHandle,
    render_scene::{GeometryPaintInput, RenderScene},
};
use serde_json::json;

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
fn bootstrap_reservation_is_exclusive_and_released_by_drop() {
    let state = ApplicationMcp::default();
    let reservation = state.reserve_native_install().unwrap();
    assert_eq!(
        state.reserve_native_install().err().unwrap(),
        "native application bootstrap is already in progress"
    );
    drop(reservation);
    assert!(state.reserve_native_install().is_ok());
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
