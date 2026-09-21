use crate::{
    native_application::*,
    native_application_contract::{
        admit_project, require_api_instance, GeometryResourceInput, NativeBootstrapRequest,
        OutputSpecInput, ViewportInput,
    },
    native_application_ports::{DesktopArtifactPort, SharedCompositor},
};
use native_engine::compositor::Compositor;
use nemo_mcp::contract::NATIVE_API_VERSION;
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

const PROJECT: &[u8] = include_bytes!("../../native-engine/tests/fixtures/opacity-v2/project.json");
const LAYER: &str = "r08_curve_layer";

fn project() -> Value {
    serde_json::from_slice(PROJECT).unwrap()
}

fn resource(id: &str, version: &str) -> Value {
    json!({
        "resourceId": id,
        "resourceVersion": version,
        "layers": [{
            "layerUid": LAYER,
            "bounds": [0.0, 0.0, 320.0, 180.0],
            "transform": [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            "paint": { "red": 12, "green": 34, "blue": 56 }
        }]
    })
}

fn resources(values: Vec<Value>) -> Vec<GeometryResourceInput> {
    serde_json::from_value(Value::Array(values)).unwrap()
}

fn bootstrap_value() -> Value {
    json!({
        "apiVersion": NATIVE_API_VERSION,
        "instanceId": "native-fixture",
        "projection": project(),
        "resources": [resource("geometry", "v1")],
        "outputBindings": [],
        "viewport": null
    })
}

fn error_code<T>(result: Result<T, nemo_mcp::contract::NativeApplicationError>) -> String {
    result.err().expect("request must fail").code
}

#[test]
fn bootstrap_contract_rejects_unknown_fields_and_wrong_version() {
    let mut unknown = bootstrap_value();
    unknown["unexpected"] = json!(true);
    assert!(serde_json::from_value::<NativeBootstrapRequest>(unknown).is_err());

    assert_eq!(
        error_code(require_api_instance(
            NATIVE_API_VERSION + 1,
            "native-fixture",
            "native-fixture"
        )),
        "invalid_request"
    );
    assert_eq!(
        error_code(require_api_instance(
            NATIVE_API_VERSION,
            "other-fixture",
            "native-fixture"
        )),
        "wrong_instance"
    );
}

#[test]
fn admission_requires_exact_projection_geometry_join() {
    let admitted = admit_project(&project(), &resources(vec![resource("geometry", "v1")]))
        .expect("exact layer join must be admitted");
    assert_eq!(admitted.document.layers().len(), 1);
    assert_eq!(admitted.resources.len(), 1);

    let mut missing = resource("geometry", "v1");
    missing["layers"] = json!([]);
    assert_eq!(
        error_code(admit_project(&project(), &resources(vec![missing]))),
        "invalid_request"
    );

    let mut extra = resource("geometry", "v1");
    extra["layers"].as_array_mut().unwrap().push(json!({
        "layerUid": "extra-layer",
        "bounds": [0.0, 0.0, 1.0, 1.0],
        "transform": [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        "paint": { "red": 0, "green": 0, "blue": 0 }
    }));
    assert_eq!(
        error_code(admit_project(&project(), &resources(vec![extra]))),
        "invalid_request"
    );

    let mut duplicate_layer = resource("geometry", "v1");
    let layer = duplicate_layer["layers"][0].clone();
    duplicate_layer["layers"]
        .as_array_mut()
        .unwrap()
        .push(layer);
    assert_eq!(
        error_code(admit_project(&project(), &resources(vec![duplicate_layer]))),
        "invalid_request"
    );
}

#[test]
fn admission_rejects_duplicate_resource_identity_and_invalid_geometry() {
    assert_eq!(
        error_code(admit_project(
            &project(),
            &resources(vec![resource("geometry", "v1"), resource("geometry", "v1")])
        )),
        "invalid_request"
    );

    let mut invalid = resource("geometry", "v1");
    invalid["layers"][0]["bounds"] = json!([0.0, 0.0, 0.0, 180.0]);
    assert_eq!(
        error_code(admit_project(&project(), &resources(vec![invalid]))),
        "invalid_request"
    );
}

#[test]
fn admission_rejects_unsupported_projection_before_host_mutation() {
    let mut unsupported = project();
    unsupported["formatVersion"] = json!(99);
    assert_eq!(
        error_code(admit_project(
            &unsupported,
            &resources(vec![resource("geometry", "v1")])
        )),
        "invalid_request"
    );
}

#[test]
fn preview_and_viewport_contracts_are_fixed_to_the_admitted_surface() {
    let output: OutputSpecInput = serde_json::from_value(json!({
        "kind": "frame",
        "format": "rgba8",
        "width": 320,
        "height": 180,
        "colorInterpretation": "srgb",
        "alphaMode": "straight"
    }))
    .unwrap();
    assert_eq!(output.admit().unwrap().dimensions(), (320, 180));

    let wrong_output: OutputSpecInput = serde_json::from_value(json!({
        "kind": "frame",
        "format": "rgba8",
        "width": 640,
        "height": 360,
        "colorInterpretation": "srgb",
        "alphaMode": "straight"
    }))
    .unwrap();
    assert_eq!(error_code(wrong_output.admit()), "invalid_request");

    let viewport: ViewportInput = serde_json::from_value(json!({
        "cssBounds": { "x": 0.0, "y": 0.0, "width": 640.0, "height": 360.0 },
        "physicalExtent": { "width": 1280, "height": 720 },
        "compositionExtent": { "width": 320, "height": 180 },
        "reportedDpr": 2.0
    }))
    .unwrap();
    assert_eq!(viewport.admit().unwrap().composition_extent().width, 320);

    let wrong_viewport: ViewportInput = serde_json::from_value(json!({
        "cssBounds": { "x": 0.0, "y": 0.0, "width": 640.0, "height": 360.0 },
        "physicalExtent": { "width": 1280, "height": 720 },
        "compositionExtent": { "width": 640, "height": 360 },
        "reportedDpr": 2.0
    }))
    .unwrap();
    assert_eq!(error_code(wrong_viewport.admit()), "invalid_request");
}

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "nemo-native-application-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn application_identity_rejects_wrong_instance_document_and_revision() {
    let scratch = Scratch::new();
    let admitted = admit_project(&project(), &resources(vec![resource("geometry", "v1")]))
        .expect("fixture project");
    let artifacts = DesktopArtifactPort::new(
        scratch.0.join("staging"),
        std::iter::empty::<(String, PathBuf)>(),
    )
    .unwrap();
    let compositor = SharedCompositor::new(Compositor::new().expect("native GPU context"));
    let application =
        DesktopNativeApplication::new("native-fixture".into(), admitted, artifacts, compositor)
            .unwrap();
    let document = application.document_id().to_owned();

    assert!(application
        .require_identity("native-fixture", &document, 0)
        .is_ok());
    assert_eq!(
        error_code(application.require_identity("other", &document, 0)),
        "wrong_instance"
    );
    assert_eq!(
        error_code(application.require_identity("native-fixture", "old-document", 0)),
        "wrong_document"
    );
    assert_eq!(
        error_code(application.require_identity("native-fixture", &document, 1)),
        "stale_revision"
    );
}
