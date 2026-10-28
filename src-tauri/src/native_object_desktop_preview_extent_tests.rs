//! Object extent and owned-project correspondence; controlled targets only.
use crate::native_object_desktop_preview::tests::*;

// Scalar New is the actual project authority; no fabricated metadata/owner.
fn extent_project(
    width: u32,
    height: u32,
) -> (crate::application_mcp::ApplicationMcp, serde_json::Value) {
    let state = crate::application_mcp::ApplicationMcp::default();
    let new = json!({"apiVersion":2,"requestId":"extent-new","instanceId":state.instance_id(),
        "newProject":{"canvasW":width,"canvasH":height,"fps":24.0}});
    let receipt = crate::native_object_bootstrap::bootstrap_raw(&state, &new.to_string()).unwrap();
    let receipt = serde_json::to_value(receipt).unwrap();
    let info = state
        .native_project_info(receipt["documentId"].as_str().unwrap())
        .unwrap();
    let raw = json!({"apiVersion":2,"requestId":"extent-preview","instanceId":state.instance_id(),
        "documentId":info["documentId"],"contentRevision":info["contentRevision"],
        "documentSnapshotId":info["documentSnapshotId"],"lifecycleGeneration":info["lifecycleGeneration"],
        "contextId":"scene-root","scopeKind":"authored","frame":0,
        "outputSpec":{"kind":"frame","format":"rgba8","width":width,"height":height,"colorInterpretation":"srgb","alphaMode":"straight"},
        "viewport":{"cssBounds":{"x":0,"y":0,"width":640,"height":360},
            "physicalExtent":{"width":1280,"height":720},"compositionExtent":{"width":width,"height":height},"reportedDpr":2}});
    (state, raw)
}

#[test]
fn object_extent_bounds_preserve_the_fixed_opacity_contract_without_gpu() {
    use crate::native_application_contract::{OutputSpecInput, ViewportInput};
    for (width, height) in [(1, 1), (320, 180), (1920, 1080), (4096, 4096)] {
        let (_, raw) = extent_project(width, height);
        let output: OutputSpecInput = serde_json::from_value(raw["outputSpec"].clone()).unwrap();
        let admitted = output.admit_object().unwrap();
        assert_eq!(admitted.dimensions(), (width, height));
        let viewport: ViewportInput = serde_json::from_value(raw["viewport"].clone()).unwrap();
        assert_eq!(
            viewport
                .admit_object(&admitted)
                .unwrap()
                .composition_extent()
                .width,
            width
        );
        assert_eq!(output.admit().is_ok(), (width, height) == (320, 180));
        assert_eq!(viewport.admit().is_ok(), (width, height) == (320, 180));
    }
    let f = Fixture::new();
    for (width, height) in [
        (0, 180),
        (320, 0),
        (4097, 1),
        (1, 4097),
        (u32::MAX, u32::MAX),
    ] {
        let mut raw = f.raw.clone();
        raw["outputSpec"]["width"] = json!(width);
        raw["outputSpec"]["height"] = json!(height);
        raw["viewport"]["compositionExtent"] = json!({"width":width,"height":height});
        assert_eq!(
            admit_request(&raw.to_string(), f.state.instance_id())
                .err()
                .unwrap()
                .code,
            "invalid_request"
        );
    }
    for (field, value) in [
        ("kind", "image"),
        ("format", "rgba16f"),
        ("colorInterpretation", "linear"),
        ("alphaMode", "premultiplied"),
    ] {
        let mut raw = f.raw.clone();
        raw["outputSpec"][field] = json!(value);
        assert_eq!(
            admit_request(&raw.to_string(), f.state.instance_id())
                .err()
                .unwrap()
                .code,
            "invalid_request"
        );
    }
    let mut mismatch = f.raw.clone();
    mismatch["viewport"]["compositionExtent"] = json!({"width":1920,"height":1080});
    assert_eq!(
        admit_request(&mismatch.to_string(), f.state.instance_id())
            .err()
            .unwrap()
            .code,
        "invalid_request"
    );
    assert_eq!(f.creations.get(), 0);
    assert_eq!(f.trace.borrow().acquisitions, 0);
}

#[test]
fn ordinary_default_new_reaches_preview_target_with_its_owned_canvas_without_gpu() {
    let (state, raw) = extent_project(1920, 1080);
    let document = raw["documentId"].as_str().unwrap();
    let before = state.native_project_info(document).unwrap();
    assert_eq!(
        (before["canvasW"].as_u64(), before["canvasH"].as_u64()),
        (Some(1920), Some(1080))
    );
    assert_eq!(before["fps"], json!(24.0));
    let request = admit_request(&raw.to_string(), state.instance_id()).unwrap();
    let called = std::cell::Cell::new(0);
    let error = crate::native_object_desktop_preview::preview_with_target(&state, &request, |_| {
        called.set(called.get() + 1);
        Err(host_error("unavailable", "extent-controlled-target"))
    })
    .err()
    .unwrap();
    assert_eq!(error.message, "extent-controlled-target");
    assert_eq!(called.get(), 1);
    assert_eq!(state.native_project_info(document).unwrap(), before);
}

#[test]
fn preview_project_size_and_bare_drawing_compatibility_refuse_before_target() {
    for (width, height, output_width, output_height) in [
        (64, 48, 320, 180),
        (320, 180, 1920, 1080),
        (1920, 1080, 320, 180),
    ] {
        let (state, mut raw) = extent_project(width, height);
        let document = raw["documentId"].as_str().unwrap().to_owned();
        let before = state.native_project_info(&document).unwrap();
        raw["outputSpec"]["width"] = json!(output_width);
        raw["outputSpec"]["height"] = json!(output_height);
        raw["viewport"]["compositionExtent"] = json!({"width":output_width,"height":output_height});
        let request = admit_request(&raw.to_string(), state.instance_id()).unwrap();
        let called = std::cell::Cell::new(0);
        assert!(crate::native_object_desktop_preview::preview_with_target(
            &state,
            &request,
            |_| {
                called.set(called.get() + 1);
                Err(host_error("internal", "mismatched project reached target"))
            }
        )
        .is_err());
        assert_eq!(called.get(), 0);
        assert_eq!(state.native_project_info(&document).unwrap(), before);
    }
    let f = Fixture::new();
    let mut raw = f.raw.clone();
    raw["outputSpec"]["width"] = json!(1920);
    raw["outputSpec"]["height"] = json!(1080);
    raw["viewport"]["compositionExtent"] = json!({"width":1920,"height":1080});
    let request = admit_request(&raw.to_string(), f.state.instance_id()).unwrap();
    assert!(crate::native_object_desktop_preview::preview_with_target(
        &f.state,
        &request,
        |_| panic!("bare drawing admits only the original 320x180 output")
    )
    .is_err());
    assert_eq!(f.creations.get(), 0);
    assert_eq!(f.trace.borrow().acquisitions, 0);
}

#[path = "native_object_desktop_preview_camera_return_tests.rs"]
mod camera_return_tests;

// The object path enters the occlusion diagnostics' preview scope per request, closed with the
// receipt's status (before this, every object acquisition recorded preview=null).
#[cfg(target_os = "macos")]
#[test]
fn gpu_object_preview_correlates_occlusion_diagnostics_per_request() {
    let diagnostics = crate::native_application_viewport::test_observations();
    let f = Fixture::new();
    f.trace.borrow_mut().mode = 1;
    assert_eq!(f.run(&f.raw.to_string()).unwrap().status, "deferred-timeout");
    diagnostics.validate_last("deferred-timeout");
    f.trace.borrow_mut().mode = 0;
    assert_eq!(f.run(&f.raw.to_string()).unwrap().status, "presented");
    diagnostics.validate_last("presented");
}
