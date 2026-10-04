//! Fixed-origin and lifecycle oracles; no capture, replay, filesystem or GPU work.
use super::*;
use crate::application::{ResourceResolutionError, REPRODUCTION_FIXTURE};
use crate::codec::encode_project;
use crate::commands::OpacityRequest;
use crate::export_job::{ExportArtifact, ExportReadback};
use crate::protocol::{OpaqueResourceHandle, OP_JOB_EXPORT_PNG_BEGIN};
use crate::render_scene::{GeometryPaintInput, LayerGeometry, OpaqueSrgbPaint, RenderScene};
use serde_json::{json, Value};

const ORIGINAL: &[u8] = include_bytes!("fixtures/opacity-v2/project.json");
const LAYER: &str = "r08_curve_layer";
#[derive(Default)]
struct Port {
    fail_cleanup: bool,
}
impl StagedArtifactPort for Port {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn write_frame(&mut self, _: &str, _: &str, _: &[u8]) -> Result<(), String> {
        panic!("catalog inspection must not write")
    }
    fn publish(&mut self, _: &str, _: &str, _: &[String]) -> Result<ExportArtifact, String> {
        panic!("catalog inspection must not publish")
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        if self.fail_cleanup {
            Err("private injected cleanup detail".into())
        } else {
            Ok(())
        }
    }
}
struct Compositor;
impl ExportCompositor for Compositor {
    type Composition = ();
    fn compose(&mut self, _: &RenderScene) -> Result<(), String> {
        panic!("catalog inspection must not render")
    }
    fn readback_rgba8(&self, _: &()) -> Result<ExportReadback, String> {
        panic!("catalog inspection must not read pixels")
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
                LAYER,
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
fn app() -> App {
    App::from_reproduction_fixture(
        "catalog-test",
        REPRODUCTION_FIXTURE,
        Port::default(),
        Compositor,
        Resolver,
    )
    .unwrap()
}
fn request(app: &App, id: &str, operation: &str, payload: Value) -> OpacityRequest {
    OpacityRequest::query(id, app.instance_id(), app.document_id(), operation, payload)
}
fn set(app: &App, id: &str, value: u32) -> OpacityRequest {
    OpacityRequest::command(
        id,
        app.instance_id(),
        app.document_id(),
        app.content_revision(),
        json!({"command":"layer.opacity.set", "stableTarget":{"layerUid":LAYER}, "value":value}),
    )
}
fn state(app: &App) -> String {
    format!(
        "{:?}",
        (
            encode_project(
                app.history
                    .acquire_snapshot(app.content_revision())
                    .unwrap()
                    .document()
            )
            .unwrap(),
            format!("{:?}", app.history),
            format!("{:?}", app.requests),
            app.diagnostics.retained_len(),
            app.exports.release_snapshot(),
            app.release_progress(),
            app.replacement_progress(),
        )
    )
}

#[test]
fn catalog_preserves_exact_shipped_bytes_and_independent_static_keyed_oracle() {
    assert_eq!(BYTES, ORIGINAL);
    assert_eq!(REPRODUCTION_FIXTURE.id, "native-opacity-static");
    assert_eq!(REPRODUCTION_FIXTURE.version, 1);
    assert_eq!(
        REPRODUCTION_FIXTURE.sha256,
        "895ca05a43295104301c472287149b1c01aa6ef681db80dcbb79e58a55b36050"
    );
    let app = app();
    assert_eq!(
        app.reproduction_eligibility(),
        ReproductionEligibility::Eligible
    );
    let snapshot = app.acquire_snapshot(0).unwrap();
    assert_eq!(snapshot.static_opacity(LAYER), Some(25.0));
    let encoded: Value =
        serde_json::from_slice(&encode_project(snapshot.document()).unwrap()).unwrap();
    let original: Value = serde_json::from_slice(ORIGINAL).unwrap();
    assert_eq!(encoded, original);
    let expected: Value =
        serde_json::from_slice(include_bytes!("fixtures/opacity-v2/expected.json")).unwrap();
    let keys = encoded["layers"][0]["motion"]["opacity"]["keys"]
        .as_array()
        .unwrap();
    assert_eq!(keys.len(), 2);
    for (index, key) in keys.iter().enumerate() {
        assert_eq!(key["frame"], expected["document"]["keys"][index]["frame"]);
        assert_eq!(key["v"][0], expected["document"]["keys"][index]["value"]);
        assert_eq!(key["curvePoints"], expected["document"]["curvePoints"]);
        assert_eq!(key["hOut"], json!([0, 0]));
        assert_eq!(key["hIn"], json!([0, 0]));
    }
}

#[test]
fn unknown_selectors_missing_bytes_and_even_semantically_identical_tampering_fail_closed() {
    for fixture in [
        ReproductionFixture {
            id: "private-fake-label",
            ..REPRODUCTION_FIXTURE
        },
        ReproductionFixture {
            version: 2,
            ..REPRODUCTION_FIXTURE
        },
        ReproductionFixture {
            sha256: "private-fake-hash",
            ..REPRODUCTION_FIXTURE
        },
    ] {
        let result = App::from_reproduction_fixture(
            "catalog-test",
            fixture,
            Port::default(),
            Compositor,
            Resolver,
        );
        assert!(matches!(
            result,
            Err(ReproductionCatalogError::UnknownFixture)
        ));
    }
    let mut whitespace_changed = ORIGINAL.to_vec();
    whitespace_changed.push(b'\n');
    assert!(decode_project(&whitespace_changed).is_ok());
    let mut opacity_changed: Value = serde_json::from_slice(ORIGINAL).unwrap();
    opacity_changed["layers"][0]["motionStatic"]["opacity"] = json!([26]);
    for bytes in [
        Vec::new(),
        whitespace_changed,
        serde_json::to_vec(&opacity_changed).unwrap(),
    ] {
        assert!(matches!(
            verified_document(REPRODUCTION_FIXTURE, &bytes),
            Err(ReproductionCatalogError::InvalidFixtureBytes)
        ));
    }
    assert!(matches!(
        App::from_reproduction_fixture(
            "",
            REPRODUCTION_FIXTURE,
            Port::default(),
            Compositor,
            Resolver
        ),
        Err(ReproductionCatalogError::InvalidInstance)
    ));
}

#[test]
fn ordinary_identical_document_or_matching_label_never_grants_provenance() {
    let user = App::new(
        REPRODUCTION_FIXTURE.id,
        decode_project(ORIGINAL).unwrap(),
        Port::default(),
        Compositor,
        Resolver,
    )
    .unwrap();
    let before = state(&user);
    for _ in 0..3 {
        assert_eq!(
            user.reproduction_eligibility(),
            ReproductionEligibility::NotCatalog
        );
    }
    assert_eq!(state(&user), before);
    let synthetic = app();
    assert_ne!(user.document_id(), synthetic.document_id());
    assert_eq!(
        state(&user),
        before,
        "synthetic construction cannot replace a user authority"
    );
}

#[test]
fn eligibility_and_metadata_inspection_leave_all_authority_stores_unchanged() {
    let mut app = app();
    let before = state(&app);
    for _ in 0..3 {
        assert_eq!(
            app.reproduction_eligibility(),
            ReproductionEligibility::Eligible
        );
        let query = request(&app, "inspect", "query.diagnostics.recent", json!({}));
        assert!(app.dispatch(query).is_ok());
    }
    assert_eq!(state(&app), before);
}

#[test]
fn edit_noop_failed_command_and_retained_query_each_end_pristine_eligibility() {
    for (id, operation, payload, revision, ok) in [
        (
            "edit",
            "command.document.apply",
            json!({"command":"layer.opacity.set", "stableTarget":{"layerUid":LAYER}, "value":40}),
            Some(0),
            true,
        ),
        (
            "noop",
            "command.document.apply",
            json!({"command":"layer.opacity.set", "stableTarget":{"layerUid":LAYER}, "value":25}),
            Some(0),
            true,
        ),
        (
            "failed",
            "command.document.apply",
            json!({"command":"layer.opacity.set", "stableTarget":{"layerUid":LAYER}, "value":101}),
            Some(0),
            false,
        ),
        ("read", "query.document.revision", json!({}), None, true),
    ] {
        let mut app = app();
        let mut query = request(&app, id, operation, payload);
        query.expected_revision = revision;
        assert_eq!(app.dispatch(query).is_ok(), ok);
        assert_eq!(app.requests.len(), 1);
        let before = state(&app);
        assert_eq!(
            app.reproduction_eligibility(),
            ReproductionEligibility::NotPristine,
            "{id}"
        );
        assert_eq!(state(&app), before);
    }
}

#[test]
fn transaction_cancellation_and_history_roundtrips_cannot_restore_eligibility() {
    let mut transaction = app();
    let mut begin = request(
        &transaction,
        "begin",
        "transaction.begin",
        json!({"stableTarget":{"layerUid":LAYER}}),
    );
    begin.expected_revision = Some(0);
    let begun = transaction.dispatch(begin);
    assert!(begun.is_ok());
    assert_eq!(
        transaction.reproduction_eligibility(),
        ReproductionEligibility::NotPristine
    );
    let id = begun.result().unwrap()["transactionId"].clone();
    let cancel = request(
        &transaction,
        "cancel",
        "transaction.cancel",
        json!({"transactionId":id}),
    );
    assert!(transaction.dispatch(cancel).is_ok());
    assert_eq!(transaction.content_revision(), 0);
    assert_eq!(transaction.history.history_depths(), (0, 0));
    assert!(!transaction.history.transactions.has_active());
    assert_eq!(
        transaction.reproduction_eligibility(),
        ReproductionEligibility::NotPristine
    );

    let mut history = app();
    assert!(history.dispatch(set(&history, "edit", 40)).is_ok());
    for operation in ["history.undo", "history.redo"] {
        let mut command = request(&history, operation, operation, json!({}));
        command.expected_revision = Some(history.content_revision());
        assert!(history.dispatch(command).is_ok());
        assert_eq!(
            history.reproduction_eligibility(),
            ReproductionEligibility::NotPristine
        );
    }
}

#[test]
fn release_and_identical_replacements_cannot_reuse_origin() {
    let mut released = app();
    released.release_transaction_stage();
    assert_eq!(
        released.reproduction_eligibility(),
        ReproductionEligibility::Released
    );
    released.release_export_stage();
    assert!(released.requests.is_empty());
    assert_eq!(
        released.reproduction_eligibility(),
        ReproductionEligibility::Released
    );
    let mut replaced = app();
    for _ in 0..2 {
        let old_id = replaced.document_id().to_owned();
        replaced
            .replace_document(decode_project(ORIGINAL).unwrap())
            .unwrap();
        assert_ne!(replaced.document_id(), old_id);
        assert_eq!(replaced.content_revision(), 0);
        assert_eq!(replaced.history.history_depths(), (0, 0));
        assert!(replaced.requests.is_empty());
        assert!(replaced.reproduction_origin.is_none());
        assert_eq!(
            replaced.reproduction_eligibility(),
            ReproductionEligibility::NotCatalog
        );
    }
}

#[test]
fn failed_fenced_replacement_retires_provenance_before_reconciliation() {
    let mut app = App::from_reproduction_fixture(
        "catalog-test",
        REPRODUCTION_FIXTURE,
        Port { fail_cleanup: true },
        Compositor,
        Resolver,
    )
    .unwrap();
    let mut begin = request(
        &app,
        "export",
        OP_JOB_EXPORT_PNG_BEGIN,
        json!({"contextId":"scene-root", "quality":"final", "outputHandle":"synthetic-output",
            "frames":[{"sourceFrame":0,"geometryHandle":{"resourceId":"geometry-r08","resourceVersion":"v1"}}]}),
    );
    begin.expected_revision = Some(0);
    assert!(app.dispatch(begin).is_ok());
    let old_id = app.document_id().to_owned();
    assert!(app
        .replace_document(decode_project(ORIGINAL).unwrap())
        .is_err());
    assert_eq!(app.document_id(), old_id);
    assert!(app.replacement_progress().is_some());
    assert!(app.reproduction_origin.is_none());
    assert_eq!(
        app.reproduction_eligibility(),
        ReproductionEligibility::NotCatalog
    );
    assert!(app
        .replace_document(decode_project(ORIGINAL).unwrap())
        .is_err());
    assert!(app.reproduction_origin.is_none());
}
