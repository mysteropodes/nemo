//! Original application authority plus real desktop destination controls.
use crate::application_mcp::ApplicationMcp;
use crate::native_application_contract::host_error;
use crate::native_object_desktop_export::{
    admit_request, export_with_target, port::LazyDesktopArtifactPort, MAX_PNG_BYTES,
};
use native_engine::{
    compositor::Compositor,
    export_job::{ExportArtifact, StagedArtifactPort},
    object_codec::decode_storage_project,
};
use serde_json::{json, Value};
use std::{cell::Cell, fs, path::PathBuf};
#[path = "native_object_desktop_export_extent_tests.rs"]
mod extent_tests;

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("nemo-object-route-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn destination(&self) -> PathBuf {
        self.0.join("object-png-sequence")
    }
    fn stage(&self) -> PathBuf {
        self.0.join("stage")
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn object_document() -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut record = cases["records"][0].clone();
    record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":0},"layerUid":"L0","strokeId":"a"});
    record["geometry"]["segments"] =
        json!([[4,4],[28,4],[28,28],[4,28]].map(|[x,y]|
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    record["fill"] = json!({"kind":"solid","r":0,"g":0,"b":1,"a":1});
    json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":1,
        "layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],"objects":[record],"compositionOrder":{
            "format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
            "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
            "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[{"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}}]}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}})
}

fn fixture(scratch: &Scratch) -> (ApplicationMcp, Value) {
    fixture_document(scratch, object_document())
}
fn fixture_document(scratch: &Scratch, document: Value) -> (ApplicationMcp, Value) {
    let state = ApplicationMcp::default();
    let (document_id, revision) = state
        .install_native_object(
            decode_storage_project(&serde_json::to_vec(&document).unwrap()).unwrap(),
        )
        .unwrap();
    let generation = state.native_state().lock().unwrap().active().unwrap().0;
    let query = json!({"apiVersion":2,"requestId":"object-export-pins","instanceId":state.instance_id(),
        "documentId":document_id,"operation":"query.document.object.serialize","payload":{"atRevision":revision}});
    let response = state
        .serialize_object_file_snapshot(&query.to_string())
        .unwrap();
    assert!(response.ok);
    let raw = json!({"apiVersion":2,"requestId":"original-png-export","instanceId":state.instance_id(),
        "documentId":document_id,"contentRevision":revision,
        "documentSnapshotId":response.result.unwrap()["documentSnapshotId"],
        "lifecycleGeneration":generation,"contextId":"scene-root","scopeKind":"authored","frame":0,
        "outputSpec":{"kind":"frame","format":"rgba8","width":320,"height":180,
            "colorInterpretation":"srgb","alphaMode":"straight"},
        "viewport":{"cssBounds":{"x":0,"y":0,"width":320,"height":180},
            "physicalExtent":{"width":320,"height":180},"compositionExtent":{"width":320,"height":180},
            "reportedDpr":1},"destinationDirectory":scratch.destination(),"maximumEncodedBytes":65536});
    (state, raw)
}

#[test]
fn raw_admission_refuses_unscoped_collision_symlink_and_invalid_pins() {
    let scratch = Scratch::new();
    let (state, raw) = fixture(&scratch);
    assert!(admit_request(&raw.to_string(), state.instance_id(), |_| false).is_err());
    for (field, value) in [
        ("instanceId", json!("foreign")),
        ("lifecycleGeneration", json!(0)),
        ("scopeKind", json!("reference")),
        ("maximumEncodedBytes", json!(0)),
        ("maximumEncodedBytes", json!(MAX_PNG_BYTES + 1)),
        ("cancelledBeforeDispatch", json!(true)),
    ] {
        let mut changed = raw.clone();
        changed[field] = value;
        assert!(
            admit_request(&changed.to_string(), state.instance_id(), |_| true).is_err(),
            "{field}"
        );
    }
    let mut changed = raw.clone();
    changed["destinationDirectory"] = json!("relative-output");
    assert!(admit_request(&changed.to_string(), state.instance_id(), |_| true).is_err());
    changed = raw.clone();
    changed["outputSpec"]["width"] = json!(64);
    assert!(admit_request(&changed.to_string(), state.instance_id(), |_| true).is_err());
    let duplicate = raw.to_string().replacen('{', "{\"apiVersion\":2,", 1);
    assert!(admit_request(&duplicate, state.instance_id(), |_| true).is_err());
    fs::create_dir(scratch.destination()).unwrap();
    fs::write(scratch.destination().join("original"), b"preserve").unwrap();
    assert!(admit_request(&raw.to_string(), state.instance_id(), |_| true).is_err());
    assert_eq!(
        fs::read(scratch.destination().join("original")).unwrap(),
        b"preserve"
    );
    fs::remove_dir_all(scratch.destination()).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&scratch.0, scratch.destination()).unwrap();
        assert!(admit_request(&raw.to_string(), state.instance_id(), |_| true).is_err());
    }
}

#[test]
fn original_owner_and_revision_barriers_precede_any_target_callback() {
    let scratch = Scratch::new();
    let (state, raw) = fixture(&scratch);
    let invoked = Cell::new(0);
    for (field, value) in [
        ("documentId", json!("foreign")),
        ("contentRevision", json!(1)),
        ("lifecycleGeneration", json!(999)),
    ] {
        let mut changed = raw.clone();
        changed[field] = value;
        let admitted = admit_request(&changed.to_string(), state.instance_id(), |_| true).unwrap();
        assert!(export_with_target(&state, &admitted, scratch.stage(), |_| {
            invoked.set(invoked.get() + 1);
            Err(host_error("internal", "unexpected target"))
        })
        .is_err());
    }
    assert_eq!(invoked.get(), 0);
    assert!(!scratch.stage().exists());
    let admitted = admit_request(&raw.to_string(), state.instance_id(), |_| true).unwrap();
    assert!(export_with_target(&state, &admitted, scratch.stage(), |_| {
        invoked.set(invoked.get() + 1);
        Err(host_error("unavailable", "controlled target refusal"))
    })
    .is_err());
    assert_eq!(invoked.get(), 1);
    assert!(!scratch.stage().exists());
}

#[test]
fn lazy_artifact_port_refuses_pre_begin_operations_without_filesystem_effects() {
    let scratch = Scratch::new();
    let mut port = LazyDesktopArtifactPort::new(
        scratch.stage(),
        "object-output".into(),
        scratch.destination(),
    );
    assert!(port
        .write_frame("job", "frame_0001.png", b"invalid")
        .is_err());
    assert!(port.publish("job", "object-output", &[]).is_err());
    assert!(port.begin_staging("job", "foreign").is_err());
    assert!(port.cleanup("job").is_ok());
    assert!(!scratch.stage().exists());
    assert!(!scratch.destination().exists());
}

#[cfg(target_os = "macos")]
fn decoded_pixel(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let data: *mut Object = msg_send![class!(NSData), alloc];
        let data: *mut Object = msg_send![data, initWithBytes:bytes.as_ptr() length:bytes.len()];
        let image: *mut Object = msg_send![class!(NSBitmapImageRep), alloc];
        let image: *mut Object = msg_send![image, initWithData:data];
        assert!(!image.is_null());
        let width: usize = msg_send![image, pixelsWide];
        let height: usize = msg_send![image, pixelsHigh];
        assert_eq!((width, height), (320, 180));
        let stride: usize = msg_send![image, bytesPerRow];
        let pixels: *const u8 = msg_send![image, bitmapData];
        assert!(!pixels.is_null() && stride >= 320 * 4);
        let pixel = std::slice::from_raw_parts(pixels.add(y * stride + x * 4), 4);
        let result = [pixel[0], pixel[1], pixel[2], pixel[3]];
        let _: () = msg_send![image, release];
        let _: () = msg_send![data, release];
        result
    }
}

#[test]
#[cfg(target_os = "macos")]
fn exact_application_owner_repeats_literal_png_through_real_port() {
    let scratch = Scratch::new();
    let (state, raw) = fixture(&scratch);
    let mut gpu = Compositor::new().unwrap();
    let mut stale = raw.clone();
    stale["documentSnapshotId"] = json!("foreign-snapshot");
    let stale = admit_request(&stale.to_string(), state.instance_id(), |_| true).unwrap();
    assert!(
        export_with_target(&state, &stale, scratch.stage(), |consumer| {
            consumer
                .run(&mut gpu)
                .map_err(|message| host_error("unavailable", message))
        })
        .is_err()
    );
    assert!(!scratch.stage().exists());
    assert!(!scratch.destination().exists());

    let admitted = admit_request(&raw.to_string(), state.instance_id(), |_| true).unwrap();
    let result = export_with_target(&state, &admitted, scratch.stage(), |consumer| {
        consumer
            .run(&mut gpu)
            .map_err(|message| host_error("unavailable", message))
    })
    .unwrap();
    assert_eq!(result.status, "succeeded");
    assert_eq!(result.external_effect_disposition, "committed");
    assert!(result.published && result.cleanup_complete);
    assert_eq!(result.artifact_files, ["frame_0001.png"]);
    let bytes = fs::read(scratch.destination().join("frame_0001.png")).unwrap();
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(decoded_pixel(&bytes, 8, 8), [0, 0, 255, 255]);
    assert_eq!(decoded_pixel(&bytes, 40, 40), [255; 4]);
    assert_eq!(fs::read_dir(scratch.stage()).unwrap().count(), 0);
    assert!(
        export_with_target(&state, &admitted, scratch.stage(), |consumer| {
            consumer
                .run(&mut gpu)
                .map_err(|message| host_error("unavailable", message))
        })
        .is_err()
    );
    let second = Scratch::new();
    let mut next = raw.clone();
    next["destinationDirectory"] = json!(second.destination());
    // The same ID cannot acquire a new destination after its committed job.
    let replay = admit_request(&next.to_string(), state.instance_id(), |_| true).unwrap();
    assert!(
        export_with_target(&state, &replay, second.stage(), |consumer| {
            consumer
                .run(&mut gpu)
                .map_err(|message| host_error("unavailable", message))
        })
        .is_err()
    );
    assert!(!second.stage().exists());
    assert!(!second.destination().exists());
    next["requestId"] = json!("second-png-export");
    let invoked = Cell::new(0);
    for (field, value) in [
        ("documentId", json!("foreign")),
        ("contentRevision", json!(1)),
        ("lifecycleGeneration", json!(999)),
    ] {
        let mut changed = next.clone();
        changed[field] = value;
        let bad = admit_request(&changed.to_string(), state.instance_id(), |_| true).unwrap();
        assert!(export_with_target(&state, &bad, second.stage(), |_| {
            invoked.set(invoked.get() + 1);
            Err(host_error("internal", "unexpected target"))
        })
        .is_err());
    }
    assert_eq!(invoked.get(), 0);
    assert!(!second.stage().exists());
    let mut cancelled = next.clone();
    cancelled["cancelledBeforeDispatch"] = json!(true);
    assert!(admit_request(&cancelled.to_string(), state.instance_id(), |_| true).is_err());
    assert!(admit_request(&raw.to_string(), state.instance_id(), |_| true).is_err());
    assert!(!second.stage().exists());
    let admitted = admit_request(&next.to_string(), state.instance_id(), |_| true).unwrap();
    let repeated = export_with_target(&state, &admitted, second.stage(), |consumer| {
        consumer
            .run(&mut gpu)
            .map_err(|message| host_error("unavailable", message))
    })
    .unwrap();
    assert_eq!(repeated.request_id, "second-png-export");
    assert_eq!(repeated.document_id, result.document_id);
    assert_eq!(repeated.document_snapshot_id, result.document_snapshot_id);
    assert_eq!(repeated.content_revision, result.content_revision);
    assert_eq!(repeated.status, "succeeded");
    assert_eq!(repeated.external_effect_disposition, "committed");
    assert!(repeated.published && repeated.cleanup_complete);
    assert_eq!(repeated.artifact_files, ["frame_0001.png"]);
    let second_bytes = fs::read(second.destination().join("frame_0001.png")).unwrap();
    assert_eq!(decoded_pixel(&second_bytes, 8, 8), [0, 0, 255, 255]);
    assert_eq!(decoded_pixel(&second_bytes, 40, 40), [255; 4]);
    assert_eq!(fs::read_dir(second.stage()).unwrap().count(), 0);
    assert_eq!(fs::read_dir(second.destination()).unwrap().count(), 1);
    assert_eq!(
        fs::read(scratch.destination().join("frame_0001.png")).unwrap(),
        bytes
    );
}

fn sequence_document() -> Value {
    let mut document = object_document();
    document["totalFrames"] = json!(2);
    document["layers"][0]["timing"]["keys"] = json!([{"frame": 0}, {"frame": 1}]);
    let mut next = document["objects"][0].clone();
    next["target"]["frameScope"]["frame"] = json!(1);
    next["fill"] = json!({"kind":"solid","r":0,"g":1,"b":0,"a":1});
    document["objects"].as_array_mut().unwrap().push(next);
    let mut scope = document["compositionOrder"]["contexts"][0]["scopes"][0].clone();
    scope["frameScope"]["frame"] = json!(1);
    let scopes = document["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .unwrap();
    scopes.push(scope);
    document
}

#[test]
fn malformed_sequences_are_refused_before_private_staging() {
    let scratch = Scratch::new();
    let (state, raw) = fixture_document(&scratch, sequence_document());
    for frames in [
        json!(null),
        json!([]),
        json!([0, 0]),
        json!([1, 0]),
        json!([1]),
        json!([-1]),
        json!([0, 0.5]),
        json!("0,1"),
        json!((0..33).collect::<Vec<_>>()),
    ] {
        let mut changed = raw.clone();
        changed["frames"] = frames;
        assert!(admit_request(&changed.to_string(), state.instance_id(), |_| true).is_err());
    }
    assert!(!scratch.stage().exists());
    assert!(!scratch.destination().exists());
}

#[test]
#[cfg(target_os = "macos")]
fn authored_sequence_publishes_two_decoded_frames_once_and_repeats_without_history_change() {
    let scratch = Scratch::new();
    let (state, mut raw) = fixture_document(&scratch, sequence_document());
    raw["frames"] = json!([0, 1]);
    let query = json!({"apiVersion":2,"requestId":"sequence-invariance","instanceId":state.instance_id(),
        "documentId":raw["documentId"],"operation":"query.document.object.serialize","payload":{"atRevision":0}});
    let query = query.to_string();
    let read = || {
        let snapshot = state.serialize_object_file_snapshot(&query).unwrap();
        serde_json::to_value(snapshot).unwrap()
    };
    let before = read();
    let mut gpu = Compositor::new().unwrap();
    for (index, frames) in [json!([0, 2]), json!([0, 1]), json!([0, 1])]
        .into_iter()
        .enumerate()
    {
        let output = Scratch::new();
        let mut next = raw.clone();
        next["frames"] = frames;
        next["requestId"] = json!(format!("sequence-{index}"));
        next["destinationDirectory"] = json!(output.destination());
        let admitted = admit_request(&next.to_string(), state.instance_id(), |_| true).unwrap();
        let result = export_with_target(&state, &admitted, output.stage(), |consumer| {
            consumer
                .run(&mut gpu)
                .map_err(|e| host_error("unavailable", e))
        });
        if index == 0 {
            assert!(result.is_err());
            assert!(!output.stage().exists());
            assert!(!output.destination().exists());
            continue;
        }
        let receipt = result.unwrap();
        let live = state
            .with_object_preview_owner(
                admitted.request.lifecycle_generation,
                &admitted.request.selector(),
                crate::native_dispatch::NativeObjectHost::export_test_state,
            )
            .unwrap();
        assert_eq!(live, (0, (0, 0)));
        assert_eq!(receipt.artifact_files, ["frame_0001.png", "frame_0002.png"]);
        assert_eq!(receipt.status, "succeeded");
        assert_eq!(receipt.external_effect_disposition, "committed");
        assert!(receipt.published && receipt.cleanup_complete);
        for (name, expected) in [
            ("frame_0001.png", [0, 0, 255, 255]),
            ("frame_0002.png", [0, 255, 0, 255]),
        ] {
            let bytes = fs::read(output.destination().join(name)).unwrap();
            assert_eq!(decoded_pixel(&bytes, 8, 8), expected);
            assert_eq!(decoded_pixel(&bytes, 40, 40), [255; 4]);
        }
        assert_eq!(fs::read_dir(output.destination()).unwrap().count(), 2);
        assert_eq!(fs::read_dir(output.stage()).unwrap().count(), 0);
        assert_eq!(
            receipt.document_snapshot_id,
            raw["documentSnapshotId"].as_str().unwrap()
        );
    }
    assert_eq!(read(), before);
}

#[path = "native_object_desktop_sequence_tests.rs"]
mod sequence_tests;

#[test]
fn contiguous_png_range_admission_stays_lazy_and_preserves_the_explicit_frame_bound() {
    let scratch = Scratch::new();
    let (state, raw) = fixture(&scratch);
    for format in [None, Some("png")] {
        let mut ranged = raw.clone();
        ranged["frameRange"] = json!({"start":0,"end":119});
        if let Some(format) = format {
            ranged["format"] = json!(format);
        }
        let admitted = admit_request(&ranged.to_string(), state.instance_id(), |_| true).unwrap();
        assert!(
            admitted.request.frames.is_none(),
            "range admission must not expand an explicit list"
        );
        assert_eq!(admitted.request.frame, 0);
    }
    // Raw admission has no owner document; source-length validation remains owner preflight.
    let mut boundary = raw.clone();
    boundary["frameRange"] = json!({"start":0,"end":9998});
    assert!(admit_request(&boundary.to_string(), state.instance_id(), |_| true).is_ok());
    let mut explicit = raw.clone();
    explicit["frames"] = json!((0..32).collect::<Vec<u32>>());
    assert!(admit_request(&explicit.to_string(), state.instance_id(), |_| true).is_ok());
    explicit["frames"] = json!((0..33).collect::<Vec<u32>>());
    assert!(admit_request(&explicit.to_string(), state.instance_id(), |_| true).is_err());
    assert!(!scratch.stage().exists());
    assert!(!scratch.destination().exists());
}

#[test]
fn malformed_or_conflicting_ranges_refuse_before_scope_or_artifact_effects() {
    let scratch = Scratch::new();
    let (state, raw) = fixture(&scratch);
    let scope_calls = Cell::new(0);
    for range in [
        json!(null),
        json!([]),
        json!({"start":0}),
        json!({"end":119}),
        json!({"start":0,"end":119,"extra":true}),
        json!({"start":1,"end":0}),
        json!({"start":0,"end":9999}),
        json!({"start":-1,"end":1}),
        json!({"start":0,"end":0.5}),
        json!({"start":0,"end":4294967296u64}),
    ] {
        let mut changed = raw.clone();
        if range["start"] == json!(1) {
            changed["frame"] = json!(1);
        }
        changed["frameRange"] = range;
        let error = admit_request(&changed.to_string(), state.instance_id(), |_| {
            scope_calls.set(scope_calls.get() + 1);
            true
        })
        .err()
        .unwrap();
        assert_eq!(error.code, "invalid_request");
    }
    for (field, value) in [
        ("frame", json!(1)),
        ("frames", json!([0])),
        ("frames", json!([])),
        ("format", json!("svg")),
    ] {
        let mut changed = raw.clone();
        changed["frameRange"] = json!({"start":0,"end":119});
        changed[field] = value;
        assert_eq!(
            admit_request(&changed.to_string(), state.instance_id(), |_| {
                scope_calls.set(scope_calls.get() + 1);
                true
            })
            .err()
            .unwrap()
            .code,
            "invalid_request",
            "{field}"
        );
    }
    assert_eq!(scope_calls.get(), 0);
    assert!(!scratch.stage().exists());
    assert!(!scratch.destination().exists());
}
