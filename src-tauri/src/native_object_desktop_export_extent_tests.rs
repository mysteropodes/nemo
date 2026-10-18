//! Extent admission through the real owner; no compositor or artifact port runs.
use crate::native_object_desktop_export::tests::*;
#[path = "native_object_desktop_export_text_tests.rs"]
mod text_publication;

fn project_fixture(scratch: &Scratch, width: u32, height: u32) -> (ApplicationMcp, Value) {
    let state = ApplicationMcp::default();
    let request = json!({"apiVersion":2,"requestId":"export-extent-new","instanceId":state.instance_id(),
        "newProject":{"canvasW":width,"canvasH":height,"fps":24.0}});
    let receipt =
        crate::native_object_bootstrap::bootstrap_raw(&state, &request.to_string()).unwrap();
    let receipt = serde_json::to_value(receipt).unwrap();
    let info = state
        .native_project_info(receipt["documentId"].as_str().unwrap())
        .unwrap();
    let raw = json!({"apiVersion":2,"requestId":"export-project-extent","instanceId":state.instance_id(),
        "documentId":info["documentId"],"contentRevision":info["contentRevision"],
        "documentSnapshotId":info["documentSnapshotId"],"lifecycleGeneration":info["lifecycleGeneration"],
        "contextId":"scene-root","scopeKind":"authored","frame":0,
        "outputSpec":{"kind":"frame","format":"rgba8","width":width,"height":height,
            "colorInterpretation":"srgb","alphaMode":"straight"},
        "viewport":{"cssBounds":{"x":0,"y":0,"width":640,"height":360},
            "physicalExtent":{"width":1280,"height":720},"compositionExtent":{"width":width,"height":height},"reportedDpr":2},
        "destinationDirectory":scratch.destination(),"maximumEncodedBytes":MAX_PNG_BYTES});
    (state, raw)
}

#[test]
fn full_hd_project_export_reaches_target_without_staging_or_destination_effects() {
    let scratch = Scratch::new();
    let (state, raw) = project_fixture(&scratch, 1920, 1080);
    let document = raw["documentId"].as_str().unwrap();
    let before = state.native_project_info(document).unwrap();
    let admitted = admit_request(&raw.to_string(), state.instance_id(), |_| true).unwrap();
    assert_eq!(admitted.output.dimensions(), (1920, 1080));
    assert_eq!(admitted.mapping.composition_extent().width, 1920);
    let called = Cell::new(0);
    let error = export_with_target(&state, &admitted, scratch.stage(), |_| {
        called.set(called.get() + 1);
        Err(host_error("unavailable", "extent-controlled-export-target"))
    })
    .err()
    .unwrap();
    assert_eq!(error.message, "extent-controlled-export-target");
    assert_eq!(called.get(), 1);
    assert!(!scratch.stage().exists());
    assert!(!scratch.destination().exists());
    assert_eq!(state.native_project_info(document).unwrap(), before);
}

#[test]
fn invalid_export_extents_refuse_before_destination_scope_or_target_admission() {
    let scratch = Scratch::new();
    let (state, raw) = project_fixture(&scratch, 1920, 1080);
    let scope_calls = Cell::new(0);
    for (width, height) in [
        (0, 1080),
        (1920, 0),
        (4097, 1),
        (1, 4097),
        (u32::MAX, u32::MAX),
    ] {
        let mut changed = raw.clone();
        changed["outputSpec"]["width"] = json!(width);
        changed["outputSpec"]["height"] = json!(height);
        changed["viewport"]["compositionExtent"] = json!({"width":width,"height":height});
        let error = admit_request(&changed.to_string(), state.instance_id(), |_| {
            scope_calls.set(scope_calls.get() + 1);
            true
        })
        .err()
        .unwrap();
        assert_eq!(error.code, "invalid_request");
    }
    let mut mismatch = raw.clone();
    mismatch["viewport"]["compositionExtent"] = json!({"width":320,"height":180});
    assert_eq!(
        admit_request(&mismatch.to_string(), state.instance_id(), |_| {
            scope_calls.set(scope_calls.get() + 1);
            true
        })
        .err()
        .unwrap()
        .code,
        "invalid_request"
    );
    assert_eq!(scope_calls.get(), 0);
    assert!(!scratch.stage().exists());
    assert!(!scratch.destination().exists());
}

#[test]
fn project_and_bare_drawing_export_dimension_mismatches_never_enter_target() {
    let scratch = Scratch::new();
    for (width, height, output_width, output_height) in [
        (64, 48, 320, 180),
        (320, 180, 1920, 1080),
        (1920, 1080, 320, 180),
    ] {
        let (state, mut raw) = project_fixture(&scratch, width, height);
        let document = raw["documentId"].as_str().unwrap().to_owned();
        let before = state.native_project_info(&document).unwrap();
        raw["outputSpec"]["width"] = json!(output_width);
        raw["outputSpec"]["height"] = json!(output_height);
        raw["viewport"]["compositionExtent"] = json!({"width":output_width,"height":output_height});
        let admitted = admit_request(&raw.to_string(), state.instance_id(), |_| true).unwrap();
        let called = Cell::new(0);
        assert!(export_with_target(&state, &admitted, scratch.stage(), |_| {
            called.set(called.get() + 1);
            Err(host_error(
                "internal",
                "project mismatch reached export target",
            ))
        })
        .is_err());
        assert_eq!(called.get(), 0);
        assert_eq!(state.native_project_info(&document).unwrap(), before);
    }
    let (state, mut raw) = fixture(&scratch);
    raw["outputSpec"]["width"] = json!(1920);
    raw["outputSpec"]["height"] = json!(1080);
    raw["viewport"]["compositionExtent"] = json!({"width":1920,"height":1080});
    let admitted = admit_request(&raw.to_string(), state.instance_id(), |_| true).unwrap();
    assert!(
        export_with_target(&state, &admitted, scratch.stage(), |_| panic!(
            "bare drawing admits only original 320x180 export"
        ))
        .is_err()
    );
    assert!(!scratch.stage().exists());
    assert!(!scratch.destination().exists());
}
