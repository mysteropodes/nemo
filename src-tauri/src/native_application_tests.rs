use crate::{
    native_application::*,
    native_application_contract::{
        admit_project, require_api_instance, GeometryResourceInput, NativeBootstrapRequest,
        OutputSpecInput, ViewportInput,
    },
    native_application_ports::{DesktopArtifactPort, SharedCompositor},
};
use native_engine::application::ExportResourceResolver;
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

fn cap_resource(version: &str, offset: f64) -> Value {
    let mut value = resource("cap", version);
    let layer = &mut value["layers"][0];
    layer["bounds"] = json!([40, 40, 80, 80]);
    layer["transform"] = json!([1, 0, 0, 1, offset, 0]);
    layer["paint"] = json!({"red":255,"green":0,"blue":0});
    layer["path"] = json!({"version":1,"closed":true,"segments":[
        {"point":[40,40],"handleIn":[0,0],"handleOut":[0,40]},
        {"point":[80,40],"handleIn":[0,40],"handleOut":[0,0]}
    ]});
    value
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

fn replacement_value(application: &DesktopNativeApplication, resource: Value) -> Value {
    json!({"apiVersion":2,"requestId":"replace-cap","instanceId":application.instance_id(),
        "documentId":application.document_id(),"expectedRevision":application.content_revision(),
        "projection":project(),"resources":[resource]})
}

fn apply_replacement(application: &mut DesktopNativeApplication, value: Value) -> HostResult<()> {
    let request: NativeReplacementRequest = serde_json::from_value(value)
        .map_err(|error| host_error("invalid_request", error.to_string()))?;
    replacement_fingerprint(&request)?;
    application.require_identity(
        &request.instance_id,
        &request.document_id,
        request.expected_revision,
    )?;
    let admitted = admit_project(&request.projection, &request.resources)?;
    let prepared = DesktopNativeApplication::prepare_replacement(admitted)?;
    application.replace_project_prepared(prepared, &mut Default::default(), |_| Ok(()))?;
    Ok(())
}

fn assert_rejections_preserve_host(application: &mut DesktopNativeApplication) {
    let original = application
        .preview_resources
        .resolve_identity("cap", "v1")
        .unwrap();
    let identity = (
        application.instance_id().to_owned(),
        application.document_id().to_owned(),
        application.content_revision(),
    );
    let snapshot = application
        .core
        .acquire_snapshot(identity.2)
        .unwrap()
        .id()
        .to_owned();
    let base = replacement_value(application, cap_resource("v2", 64.0));
    let segment = base["resources"][0]["layers"][0]["path"]["segments"][0].clone();
    let mut invalid = Vec::new();
    for (field, value) in [
        ("path", Value::Null),
        ("bounds", json!([40, 40, 80, 79])),
        ("layerUid", json!("missing")),
        ("stroke", json!({})),
    ] {
        let mut changed = base.clone();
        changed["resources"][0]["layers"][0][field] = value;
        invalid.push(changed);
    }
    for (field, value) in [
        ("version", json!(2)),
        ("closed", json!(false)),
        ("segments", json!([])),
        ("segments", json!(vec![segment; 257])),
    ] {
        let mut changed = base.clone();
        changed["resources"][0]["layers"][0]["path"][field] = value;
        invalid.push(changed);
    }
    let mut duplicate = base.clone();
    duplicate["resources"] = json!([cap_resource("v2", 64.0), cap_resource("v2", 64.0)]);
    invalid.push(duplicate);
    let mut extra = base.clone();
    extra["resources"][0]["layers"]
        .as_array_mut()
        .unwrap()
        .push(resource("extra", "v1")["layers"][0].clone());
    invalid.push(extra);
    let mut missing = base.clone();
    missing["resources"][0]["layers"] = json!([]);
    invalid.push(missing);
    let mut malformed = base;
    malformed["unexpected"] = json!(true);
    invalid.push(malformed);
    for value in invalid {
        assert_eq!(
            error_code(apply_replacement(application, value)),
            "invalid_request"
        );
        assert_eq!(
            (
                application.instance_id(),
                application.document_id(),
                application.content_revision()
            ),
            (identity.0.as_str(), identity.1.as_str(), identity.2)
        );
        assert_eq!(
            application.core.acquire_snapshot(identity.2).unwrap().id(),
            snapshot
        );
        let resolver = &application.preview_resources;
        assert_eq!(resolver.resolve_identity("cap", "v1").unwrap(), original);
        assert!(resolver.resolve_identity("cap", "v2").is_err());
        let handle =
            serde_json::from_value(json!({"resourceId":"cap","resourceVersion":"v1"})).unwrap();
        assert_eq!(
            application
                .core
                .resource_resolver_mut()
                .resolve_geometry(&handle)
                .unwrap(),
            original
        );
        assert!(application.preview_work.is_empty());
    }
}

#[cfg(target_os = "macos")]
fn decode_png(bytes: &[u8]) -> Vec<u8> {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    // Independent platform decoder, using the host's existing AppKit linkage.
    // Retained objects own the copied PNG and bitmap until all rows are copied.
    unsafe {
        let data: *mut Object = msg_send![class!(NSData), alloc];
        let data: *mut Object = msg_send![data, initWithBytes:bytes.as_ptr() length:bytes.len()];
        let bitmap: *mut Object = msg_send![class!(NSBitmapImageRep), alloc];
        let bitmap: *mut Object = msg_send![bitmap, initWithData:data];
        assert!(!bitmap.is_null());
        let width: usize = msg_send![bitmap, pixelsWide];
        let height: usize = msg_send![bitmap, pixelsHigh];
        let bits: usize = msg_send![bitmap, bitsPerSample];
        let samples: usize = msg_send![bitmap, samplesPerPixel];
        let format: usize = msg_send![bitmap, bitmapFormat];
        let stride: usize = msg_send![bitmap, bytesPerRow];
        let pixels: *const u8 = msg_send![bitmap, bitmapData];
        assert_eq!(
            (width, height, bits, samples, format & 1),
            (320, 180, 8, 4, 0)
        );
        assert!(!pixels.is_null() && stride >= width * 4);
        let mut rgba = Vec::with_capacity(width * height * 4);
        for y in 0..height {
            rgba.extend_from_slice(std::slice::from_raw_parts(
                pixels.add(y * stride),
                width * 4,
            ));
        }
        let _: () = msg_send![bitmap, release];
        let _: () = msg_send![data, release];
        rgba
    }
}

#[test]
#[cfg(target_os = "macos")]
fn cubic_host_admission_resolver_preview_and_export_share_immutable_resources() {
    let scratch = Scratch::new();
    let mut bootstrap = bootstrap_value();
    bootstrap["resources"] = json!([cap_resource("v1", 0.0)]);
    let bootstrap: NativeBootstrapRequest = serde_json::from_value(bootstrap).unwrap();
    let admitted = admit_project(&bootstrap.projection, &bootstrap.resources).unwrap();
    let gpu = Compositor::new().expect("native Metal GPU context");
    eprintln!("N24B host GPU: {:?}", gpu.adapter().get_info());
    let mut application = DesktopNativeApplication::new(
        bootstrap.instance_id,
        admitted,
        DesktopArtifactPort::new(scratch.0.join("stage"), []).unwrap(),
        SharedCompositor::new(gpu),
    )
    .unwrap();
    assert_rejections_preserve_host(&mut application);
    for (version, offset) in [("v1", 0), ("v2", 64)] {
        if version == "v2" {
            let replacement = replacement_value(&application, cap_resource(version, offset as f64));
            apply_replacement(&mut application, replacement).unwrap();
            assert!(application
                .preview_resources
                .resolve_identity("cap", "v1")
                .is_err());
        }
        let geometry = application
            .preview_resources
            .resolve_identity("cap", version)
            .unwrap();
        assert_eq!(
            (geometry.resource_id(), geometry.resource_version()),
            ("cap", version)
        );
        let shared = application.preview_compositor().inner();
        let snapshot = application
            .core
            .acquire_snapshot(application.content_revision())
            .unwrap();
        let mut previews = Vec::new();
        for (frame, opacity) in [(0, 20), (10, 50), (20, 80)] {
            let mut request: NativePreviewRequest = serde_json::from_value(json!({
                "apiVersion":2,"instanceId":application.instance_id(),"documentId":application.document_id(),
                "contentRevision":application.content_revision(),"documentSnapshotId":snapshot.id(),
                "contextId":"scene-root","frame":frame,"quality":"final",
                "outputSpec":{"kind":"frame","format":"rgba8","width":320,"height":180,"colorInterpretation":"srgb","alphaMode":"straight"},
                "geometryHandle":{"resourceId":"cap","resourceVersion":"missing"}
            })).unwrap();
            assert_eq!(
                error_code(application.prepare_preview(&request)),
                "not_found"
            );
            assert!(application.preview_work.is_empty());
            request.geometry_handle.resource_version = version.into();
            let prepared = application.prepare_preview(&request).unwrap();
            let pixels = shared
                .lock()
                .unwrap()
                .readback_rgba8(&prepared.result)
                .unwrap();
            assert_eq!(
                (
                    pixels.document_snapshot_id(),
                    pixels.content_revision(),
                    pixels.frame()
                ),
                (snapshot.id(), application.content_revision(), frame)
            );
            let pixel = |x: usize, y: usize| {
                &pixels.bytes()[((y * 320 + x + offset) * 4)..((y * 320 + x + offset) * 4 + 4)]
            };
            for (x, y) in [(60, 50), (60, 65)] {
                let p = pixel(x, y);
                assert_eq!((p[0], p[3]), (255, 255));
                assert!(
                    (f64::from(p[1]) - 255.0 * (1.0 - f64::from(opacity) / 100.0)).abs() <= 1.0
                );
                assert_eq!(p[1], p[2]);
            }
            for (x, y) in [(45, 65), (60, 75), (30, 50)] {
                assert_eq!(pixel(x, y), [255; 4]);
            }
            previews.push(pixels.bytes().to_vec());
            application
                .finish_preview(prepared.identity.work_id(), "presented")
                .unwrap();
        }
        let destination = scratch.0.join(version);
        application
            .bind_output(version.into(), destination.clone())
            .unwrap();
        let payload = |resource_version: &str| {
            json!({"contextId":"scene-root","quality":"final","outputHandle":version,
            "frames":([0,10,20].map(|frame| json!({"sourceFrame":frame,"geometryHandle":{"resourceId":"cap","resourceVersion":resource_version}})))})
        };
        let failed = crate::native_application::release_tests::dispatch_history(
            &mut application,
            &format!("missing-{version}"),
            "job.export.png.begin",
            payload("missing"),
        );
        assert!(!failed["error"].is_null());
        assert!(!destination.exists());
        let begun = crate::native_application::release_tests::dispatch_history(
            &mut application,
            &format!("export-{version}"),
            "job.export.png.begin",
            payload(version),
        );
        let job = begun["result"]["jobId"].as_str().expect("export admitted");
        let receipt = application.core.run_export_to_completion(job).unwrap();
        assert_eq!(
            receipt.status,
            native_engine::export_job::JobStatus::Succeeded
        );
        for (index, expected) in previews.iter().enumerate() {
            let bytes = fs::read(destination.join(format!("frame_{:04}.png", index + 1))).unwrap();
            assert_eq!(
                &decode_png(&bytes),
                expected,
                "decoded export equals preview at {index}"
            );
        }
    }
}
