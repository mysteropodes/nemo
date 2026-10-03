//! Independent capture expectations through the real native command boundary.
use super::*;
use crate::application::{ResourceResolutionError, REPRODUCTION_FIXTURE};
use crate::codec::{decode_project, encode_project};
use crate::commands::DispatchErrorCode;
use crate::export_job::{ExportArtifact, ExportReadback};
use crate::protocol::OpaqueResourceHandle;
use crate::render_scene::{GeometryPaintInput, LayerGeometry, OpaqueSrgbPaint, RenderScene};
use serde_json::{json, Value};

const PROJECT: &[u8] = include_bytes!("fixtures/opacity-v2/project.json");
#[derive(Default)]
struct Port {
    fail_cleanup: bool,
    panic_cleanup: bool,
}
impl StagedArtifactPort for Port {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn write_frame(&mut self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
        panic!("capture cannot write")
    }
    fn publish(&mut self, _: &str, _: &str, _: &[String]) -> Result<ExportArtifact, String> {
        panic!("capture cannot publish")
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        assert!(!self.panic_cleanup, "private cleanup panic");
        if self.fail_cleanup {
            Err("private cleanup detail".into())
        } else {
            Ok(())
        }
    }
}
struct Compositor;
impl ExportCompositor for Compositor {
    type Composition = ();
    fn compose(&mut self, _: &RenderScene) -> Result<(), String> {
        panic!("capture cannot render")
    }
    fn readback_rgba8(&self, _: &()) -> Result<ExportReadback, String> {
        panic!("capture cannot read pixels")
    }
}
struct Resolver;
impl ExportResourceResolver for Resolver {
    fn resolve_geometry(
        &mut self,
        handle: &OpaqueResourceHandle,
    ) -> Result<GeometryPaintInput, ResourceResolutionError> {
        Ok(GeometryPaintInput::new(
            handle.resource_id(),
            handle.resource_version(),
            vec![LayerGeometry::new(
                TARGET,
                [20.0, 60.0, 40.0, 80.0],
                [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
                OpaqueSrgbPaint::new(255, 0, 0),
            )
            .unwrap()],
        )
        .unwrap())
    }
}
type App = NativeApplication<Port, Compositor, Resolver>;
fn app_with(port: Port) -> App {
    App::from_reproduction_fixture(
        "private-instance",
        REPRODUCTION_FIXTURE,
        port,
        Compositor,
        Resolver,
    )
    .unwrap()
}
fn app() -> App {
    app_with(Port::default())
}
fn armed() -> App {
    let mut app = app();
    app.opt_in_reproduction().unwrap();
    app
}
fn request(app: &App, id: &str, operation: &str, payload: Value) -> OpacityRequest {
    OpacityRequest::query(id, app.instance_id(), app.document_id(), operation, payload)
}
fn set(app: &App, id: &str, value: Value) -> OpacityRequest {
    OpacityRequest::command(
        id,
        app.instance_id(),
        app.document_id(),
        app.content_revision(),
        json!({"command":"layer.opacity.set","stableTarget":{"layerUid":TARGET},"value":value}),
    )
}
fn exported(app: &App) -> Value {
    serde_json::from_slice(&app.export_reproduction_bundle().unwrap()).unwrap()
}
fn document(app: &App) -> Value {
    serde_json::from_slice(
        &encode_project(
            app.history
                .acquire_snapshot(app.content_revision())
                .unwrap()
                .document(),
        )
        .unwrap(),
    )
    .unwrap()
}
fn stores(app: &App) -> String {
    format!(
        "{:?}",
        (
            document(app),
            format!("{:?}", app.history),
            format!("{:?}", app.requests),
            format!("{:?}", app.reproduction),
            app.diagnostics.retained_len(),
            app.exports.release_snapshot(),
            app.release_progress(),
            app.replacement_progress()
        )
    )
}
fn assert_invalid(app: &App, reason: ReproductionReason) {
    assert_eq!(
        app.reproduction_status(),
        ReproductionStatus {
            state: ReproductionState::Invalid,
            reason: Some(reason),
            command_count: 0,
            exportable: false,
        }
    );
    assert_eq!(app.export_reproduction_bundle(), Err(reason));
}

#[test]
fn opt_in_requires_pristine_provenance_and_never_resets_a_capture() {
    let mut normal = App::new(
        "normal",
        decode_project(PROJECT).unwrap(),
        Port::default(),
        Compositor,
        Resolver,
    )
    .unwrap();
    assert_eq!(
        normal.opt_in_reproduction(),
        Err(ReproductionReason::NotCatalog)
    );
    assert_eq!(
        normal.export_reproduction_bundle(),
        Err(ReproductionReason::NotOptedIn)
    );
    assert!(normal
        .dispatch(set(&normal, "normal-40", json!(40)))
        .is_ok());
    assert_eq!(
        normal.reproduction_status().state,
        ReproductionState::Disabled
    );
    let mut touched = app();
    assert!(touched.dispatch(set(&touched, "noop", json!(25))).is_ok());
    assert_eq!(
        touched.opt_in_reproduction(),
        Err(ReproductionReason::NotPristine)
    );
    let mut app = app();
    let before = document(&app);
    assert_eq!(
        app.opt_in_reproduction().unwrap(),
        ReproductionStatus {
            state: ReproductionState::Recording,
            reason: None,
            command_count: 0,
            exportable: false,
        }
    );
    assert_eq!(document(&app), before);
    assert!(app.requests.is_empty());
    assert_eq!(
        app.export_reproduction_bundle(),
        Err(ReproductionReason::EmptyJournal)
    );
    assert!(app.dispatch(set(&app, "first", json!(40))).is_ok());
    let before = stores(&app);
    assert_eq!(
        app.opt_in_reproduction(),
        Err(ReproductionReason::AlreadyOptedIn)
    );
    assert_eq!(stores(&app), before);
}

#[test]
fn complete_prefix_matches_independent_intermediate_state_history_and_keyed_oracle() {
    let mut app = armed();
    let mut expected: Value = serde_json::from_slice(PROJECT).unwrap();
    assert_eq!(document(&app), expected);
    let first = set(&app, "private-first/path", json!(40));
    let mut first_response = None;
    for (index, (value, revision, undo, applied)) in
        [(40, 1, 1, true), (60, 2, 2, true), (60, 2, 2, false)]
            .into_iter()
            .enumerate()
    {
        let command = if index == 0 {
            first.clone()
        } else {
            set(&app, &format!("private-{index}"), json!(value))
        };
        let response = app.dispatch(command);
        assert!(response.is_ok());
        assert_eq!(response.content_revision(), revision);
        assert_eq!(
            response.result().unwrap(),
            &json!({"applied":applied,"historyEntriesAdded":u8::from(applied)})
        );
        if index == 0 {
            first_response = Some(response);
        }
        expected["layers"][0]["motionStatic"]["opacity"] = json!([value]);
        assert_eq!(document(&app), expected);
        assert_eq!(app.history.history_depths(), (undo, 0));
        assert_eq!(app.reproduction_status().command_count, index + 1);
    }
    let expected_bundle = json!({
        "format":"nemo.native-opacity-reproduction","formatVersion":1,"apiVersion":2,
        "fixture":{"id":"native-opacity-static","version":1,"sha256":"895ca05a43295104301c472287149b1c01aa6ef681db80dcbb79e58a55b36050"},
        "command":"layer.opacity.set","stableTarget":{"layerUid":"r08_curve_layer"},
        "clock":null,"seed":null,"versions":{"nativeEngine":"0.1.0"},
        "commands":[
            {"id":1,"expectedRevision":0,"value":40,"revision":1,"applied":true},
            {"id":2,"expectedRevision":1,"value":60,"revision":2,"applied":true},
            {"id":3,"expectedRevision":2,"value":60,"revision":2,"applied":false}
        ]
    });
    assert_eq!(exported(&app), expected_bundle);
    assert_eq!(app.dispatch(first), first_response.unwrap());
    assert_eq!(exported(&app), expected_bundle);
    assert_eq!(app.content_revision(), 2);
    assert_eq!(app.history.history_depths(), (2, 0));
    for (frame, value) in [(0, 20.0), (10, 50.0), (20, 80.0)] {
        let query = request(
            &app,
            &format!("evaluate-{frame}"),
            "query.document.evaluate",
            json!({"atRevision":2,"contextId":"scene-root","frame":frame}),
        );
        let response = app.dispatch(query);
        assert!(response.is_ok(), "{response:?}");
        assert_eq!(
            response.result().unwrap()["layers"][0]["value"].as_f64(),
            Some(value)
        );
    }
    assert_eq!(exported(&app), expected_bundle);
    let bytes = String::from_utf8(app.export_reproduction_bundle().unwrap()).unwrap();
    for secret in ["private-first/path", "private-instance", app.document_id()] {
        assert!(!bytes.contains(secret));
    }
}

#[test]
fn inspection_never_changes_any_store_and_ordinary_query_receipts_keep_their_behavior() {
    let mut app = armed();
    assert!(app.dispatch(set(&app, "first", json!(40))).is_ok());
    let before = stores(&app);
    let bytes = app.export_reproduction_bundle().unwrap();
    for _ in 0..3 {
        assert_eq!(app.reproduction_status().command_count, 1);
        assert_eq!(app.export_reproduction_bundle().unwrap(), bytes);
        let query = request(&app, "inspect", "query.diagnostics.recent", json!({}));
        assert!(app.dispatch(query).is_ok());
    }
    assert_eq!(stores(&app), before);
    let count = app.requests.len();
    let query = request(&app, "retained-read", "query.document.revision", json!({}));
    assert!(app.dispatch(query).is_ok());
    assert_eq!(app.requests.len(), count + 1);
    assert_eq!(app.export_reproduction_bundle().unwrap(), bytes);
}

#[test]
fn an_unrelated_private_user_authority_is_untouched_and_never_enters_export() {
    let mut private: Value = serde_json::from_slice(PROJECT).unwrap();
    private["layers"][0]["layerUid"] = json!("private-user-document-sentinel");
    let mut user = App::new(
        "private-user-instance",
        decode_project(&serde_json::to_vec(&private).unwrap()).unwrap(),
        Port::default(),
        Compositor,
        Resolver,
    )
    .unwrap();
    let before = stores(&user);
    assert_eq!(
        user.opt_in_reproduction(),
        Err(ReproductionReason::NotCatalog)
    );
    let mut synthetic = armed();
    assert!(synthetic
        .dispatch(set(&synthetic, "synthetic-write", json!(40)))
        .is_ok());
    let bytes = synthetic.export_reproduction_bundle().unwrap();
    assert!(!String::from_utf8(bytes).unwrap().contains("private-user"));
    assert_eq!(stores(&user), before);
    assert_eq!(
        user.export_reproduction_bundle(),
        Err(ReproductionReason::NotOptedIn)
    );
}

#[test]
fn thirty_two_entries_fit_but_the_thirty_third_invalidates_without_failing_edit() {
    let mut app = armed();
    for index in 0..32 {
        assert!(app
            .dispatch(set(&app, &format!("noop-{index}"), json!(25)))
            .is_ok());
    }
    let bundle = exported(&app);
    assert_eq!(bundle["commands"].as_array().unwrap().len(), 32);
    assert_eq!(
        bundle["commands"][31],
        json!({"id":32,"expectedRevision":0,"value":25,"revision":0,"applied":false})
    );
    assert!(app.export_reproduction_bundle().unwrap().len() <= 3072);
    let overflow = app.dispatch(set(&app, "overflow", json!(40)));
    assert!(overflow.is_ok());
    assert_eq!(app.content_revision(), 1);
    assert_invalid(&app, ReproductionReason::CommandLimit);
    assert!(app.dispatch(set(&app, "after-limit", json!(60))).is_ok());
    assert_eq!(app.history.history_depths(), (2, 0));
}

#[test]
fn encoded_byte_limit_is_independent_of_command_count() {
    let mut app = armed();
    let mut invalidated_at = None;
    for index in 0..32 {
        let value = if index % 2 == 0 {
            1.2345678901234567e-300
        } else {
            2.3456789012345678e-300
        };
        assert!(app
            .dispatch(set(&app, &format!("long-{index}"), json!(value)))
            .is_ok());
        if app.reproduction_status().state == ReproductionState::Invalid {
            invalidated_at = Some(index + 1);
            break;
        }
        assert!(app.export_reproduction_bundle().unwrap().len() <= 3072);
    }
    assert!(invalidated_at.is_some_and(|count| count < 32));
    assert_invalid(&app, ReproductionReason::ByteLimit);
    assert!(app.dispatch(set(&app, "after-bytes", json!(60))).is_ok());
}

#[test]
fn portable_object_fits_the_complete_future_v2_envelopes_with_maximum_identities() {
    let mut app = armed();
    let mut largest = exported_after_first(&mut app);
    for index in 1..32 {
        let value = if index % 2 == 0 {
            1.2345678901234567e-300
        } else {
            2.3456789012345678e-300
        };
        assert!(app
            .dispatch(set(&app, &format!("sizing-{index}"), json!(value)))
            .is_ok());
        if let Ok(bytes) = app.export_reproduction_bundle() {
            largest = serde_json::from_slice(&bytes).unwrap();
        } else {
            break;
        }
    }
    let bundle_bytes = serde_json::to_vec(&largest).unwrap().len();
    assert!(bundle_bytes > 2900);
    let identity = "x".repeat(128);
    // A4 must embed this as an object, never as an escaped JSON string.
    let response = ResponseEnvelope {
        api_version: 2,
        request_id: identity.clone(),
        instance_id: identity.clone(),
        document_id: identity.clone(),
        content_revision: 9_007_199_254_740_991,
        ok: true,
        result: Some(json!({"bundle":largest})),
        error: None,
    };
    let response_bytes = serde_json::to_vec(&response).unwrap().len();
    assert!(response_bytes <= 4096);
    assert!(response_bytes - bundle_bytes + 3072 <= 4096);
    let replay = OpacityRequest::query(
        identity.clone(),
        identity.clone(),
        identity,
        "diagnostics.reproduction.replay",
        json!({"bundle":largest}),
    );
    let request_bytes = serde_json::to_vec(&replay).unwrap().len();
    assert!(request_bytes <= 4096);
    assert!(request_bytes - bundle_bytes + 3072 <= 4096);
    assert!(response.result().unwrap()["bundle"].is_object());
}
fn exported_after_first(app: &mut App) -> Value {
    assert!(app
        .dispatch(set(app, "sizing-first", json!(1.2345678901234567e-300)))
        .is_ok());
    exported(app)
}

#[path = "reproduction_lifecycle.rs"]
mod lifecycle;
