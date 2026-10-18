//! Private desktop SVG routing: strict admission, original owner and real artifacts.
use super::*;
use native_engine::export_job::StagedArtifactPort;
use native_engine::object_codec::decode_storage_project;
use serde_json::{json, Value};
use std::cell::Cell;

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nemo-svg-route-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }
    fn destination(&self) -> PathBuf {
        self.0.join("sequence")
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

fn fixture(scratch: &Scratch) -> (ApplicationMcp, Value) {
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
    let document = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],
        "objects":[record],"compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "contexts":[{"contextId":"scene-root","layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
        "scopes":[{"frameScope":{"kind":"authored","frame":0},"elements":[{"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}}]}]}],
        "migration":"explicit-plan-required","referencePolicy":"unresolved"}});
    let state = ApplicationMcp::default();
    let (document_id, revision) = state
        .install_native_object(
            decode_storage_project(&serde_json::to_vec(&document).unwrap()).unwrap(),
        )
        .unwrap();
    let generation = state.native_state().lock().unwrap().active().unwrap().0;
    let query = json!({"apiVersion":2,"requestId":"svg-route-pins","instanceId":state.instance_id(),
        "documentId":document_id,"operation":"query.document.object.serialize","payload":{"atRevision":revision}});
    let response = state
        .serialize_object_file_snapshot(&query.to_string())
        .unwrap();
    assert!(response.ok);
    let raw = json!({"apiVersion":2,"requestId":"main-svg-export","instanceId":state.instance_id(),
        "documentId":document_id,"contentRevision":revision,"documentSnapshotId":response.result.unwrap()["documentSnapshotId"],
        "lifecycleGeneration":generation,"contextId":"scene-root","scopeKind":"authored","frame":0,"frames":[0,7],
        "outputSpec":{"kind":"frame","format":"rgba8","width":320,"height":180,"colorInterpretation":"srgb","alphaMode":"straight"},
        "viewport":{"cssBounds":{"x":0,"y":0,"width":320,"height":180},"physicalExtent":{"width":320,"height":180},
        "compositionExtent":{"width":320,"height":180},"reportedDpr":1},
        "destinationDirectory":scratch.destination(),"maximumEncodedBytes":65536,"format":"svg"});
    (state, raw)
}

#[test]
fn svg_format_is_closed_and_omission_preserves_the_existing_png_default() {
    let scratch = Scratch::new();
    let (state, raw) = fixture(&scratch);
    let admitted = admit_request(&raw.to_string(), state.instance_id(), |_| true).unwrap();
    assert_eq!(admitted.request.format, ObjectExportFormat::Svg);
    let mut png = raw.clone();
    png.as_object_mut().unwrap().remove("format");
    assert_eq!(
        admit_request(&png.to_string(), state.instance_id(), |_| true)
            .unwrap()
            .request
            .format,
        ObjectExportFormat::Png
    );
    png["format"] = json!("png");
    assert_eq!(
        admit_request(&png.to_string(), state.instance_id(), |_| true)
            .unwrap()
            .request
            .format,
        ObjectExportFormat::Png
    );
    for value in [
        json!(null),
        json!("SVG"),
        json!("jpeg"),
        json!(0),
        json!(true),
        json!({}),
        json!([]),
    ] {
        let mut invalid = raw.clone();
        invalid["format"] = value;
        assert!(admit_request(&invalid.to_string(), state.instance_id(), |_| true).is_err());
    }
    let duplicate = raw.to_string().replacen('{', "{\"format\":\"svg\",", 1);
    assert!(admit_request(&duplicate, state.instance_id(), |_| true).is_err());
    let mut unknown = raw.clone();
    unknown["encoder"] = json!("svg");
    assert!(admit_request(&unknown.to_string(), state.instance_id(), |_| true).is_err());
    assert!(!scratch.stage().exists() && !scratch.destination().exists());
}

#[test]
fn svg_range_scope_cancel_and_destination_admission_precede_any_filesystem_effect() {
    let scratch = Scratch::new();
    let (state, raw) = fixture(&scratch);
    assert!(admit_request(&raw.to_string(), state.instance_id(), |_| false).is_err());
    for (field, value) in [
        ("instanceId", json!("foreign")),
        ("scopeKind", json!("reference")),
        ("cancelledBeforeDispatch", json!(true)),
        ("maximumEncodedBytes", json!(0)),
        ("maximumEncodedBytes", json!(MAX_PNG_BYTES + 1)),
        ("frames", json!(null)),
        ("frames", json!([])),
        ("frames", json!([0, 0])),
        ("frames", json!([7, 0])),
        ("frames", json!([7])),
        ("frames", json!((0..33).collect::<Vec<_>>())),
        ("destinationDirectory", json!("relative-output")),
    ] {
        let mut changed = raw.clone();
        changed[field] = value;
        assert!(
            admit_request(&changed.to_string(), state.instance_id(), |_| true).is_err(),
            "{field}"
        );
    }
    let mut mismatched_extent = raw.clone();
    mismatched_extent["outputSpec"]["width"] = json!(64);
    assert!(
        admit_request(&mismatched_extent.to_string(), state.instance_id(), |_| {
            true
        })
        .is_err()
    );
    assert!(!scratch.stage().exists() && !scratch.destination().exists());
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
fn svg_foreign_owner_revision_and_lifecycle_never_reach_the_target() {
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
        assert!(
            export_with_target(&state, &admitted, scratch.stage(), |_| {
                invoked.set(invoked.get() + 1);
                Err(host_error("internal", "unexpected target"))
            })
            .is_err(),
            "{field}"
        );
    }
    assert_eq!(invoked.get(), 0);
    assert!(!scratch.stage().exists() && !scratch.destination().exists());
}

#[test]
#[cfg(target_os = "macos")]
fn svg_main_route_publishes_ordinal_sequence_through_the_same_owner_and_keeps_png_defaults() {
    let scratch = Scratch::new();
    let (state, raw) = fixture(&scratch);
    let query = json!({"apiVersion":2,"requestId":"svg-invariance","instanceId":state.instance_id(),
        "documentId":raw["documentId"],"operation":"query.document.object.serialize","payload":{"atRevision":0}}).to_string();
    let before =
        serde_json::to_value(state.serialize_object_file_snapshot(&query).unwrap()).unwrap();
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
    assert!(!scratch.stage().exists() && !scratch.destination().exists());
    let admitted = admit_request(&raw.to_string(), state.instance_id(), |_| true).unwrap();
    let result = export_with_target(&state, &admitted, scratch.stage(), |consumer| {
        assert_eq!(consumer.format, ObjectExportFormat::Svg);
        consumer
            .run(&mut gpu)
            .map_err(|message| host_error("unavailable", message))
    })
    .unwrap();
    assert_eq!(result.status, "succeeded");
    assert_eq!(result.external_effect_disposition, "committed");
    assert!(result.published && result.cleanup_complete);
    assert_eq!(result.artifact_files, ["frame_0001.svg", "frame_0002.svg"]);
    let bytes = fs::read(scratch.destination().join("frame_0001.svg")).unwrap();
    assert_eq!(
        bytes,
        fs::read(scratch.destination().join("frame_0002.svg")).unwrap()
    );
    let svg = String::from_utf8(bytes).unwrap();
    assert!(svg.contains("width=\"320\"") && svg.contains("height=\"180\""));
    assert_eq!(svg.matches("<path ").count(), 1);
    assert!(svg.contains("M4,4 C4,4 28,4 28,4 C28,4 28,28 28,28 C28,28 4,28 4,28 C4,28 4,4 4,4 Z"));
    assert!(svg.contains("fill=\"rgb(0%,0%,100%)\" fill-opacity=\"1\""));
    assert!(!svg.contains("data:image") && !svg.contains("<text"));
    assert_eq!(fs::read_dir(scratch.destination()).unwrap().count(), 2);
    assert_eq!(fs::read_dir(scratch.stage()).unwrap().count(), 0);
    assert_eq!(
        result.document_snapshot_id,
        raw["documentSnapshotId"].as_str().unwrap()
    );
    assert_eq!(result.content_revision, 0);
    assert_eq!(
        state
            .with_object_preview_owner(
                admitted.request.lifecycle_generation,
                &admitted.request.selector(),
                NativeObjectHost::export_test_state
            )
            .unwrap(),
        (0, (0, 0))
    );
    assert_eq!(
        serde_json::to_value(state.serialize_object_file_snapshot(&query).unwrap()).unwrap(),
        before
    );

    let second = Scratch::new();
    let mut next = raw.clone();
    next["destinationDirectory"] = json!(second.destination());
    next.as_object_mut().unwrap().remove("format");
    next.as_object_mut().unwrap().remove("frames");
    let replay = admit_request(&next.to_string(), state.instance_id(), |_| true).unwrap();
    assert!(
        export_with_target(&state, &replay, second.stage(), |consumer| {
            consumer
                .run(&mut gpu)
                .map_err(|message| host_error("unavailable", message))
        })
        .is_err(),
        "changing format cannot re-arm the committed request"
    );
    assert!(!second.stage().exists() && !second.destination().exists());
    next["requestId"] = json!("next-default-png");
    let png = admit_request(&next.to_string(), state.instance_id(), |_| true).unwrap();
    let result = export_with_target(&state, &png, second.stage(), |consumer| {
        assert_eq!(consumer.format, ObjectExportFormat::Png);
        consumer
            .run(&mut gpu)
            .map_err(|message| host_error("unavailable", message))
    })
    .unwrap();
    assert_eq!(result.artifact_files, ["frame_0001.png"]);
    assert!(result.published && result.cleanup_complete);
    assert!(fs::read(second.destination().join("frame_0001.png"))
        .unwrap()
        .starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_eq!(fs::read_dir(second.destination()).unwrap().count(), 1);
    assert_eq!(fs::read_dir(second.stage()).unwrap().count(), 0);
}

#[test]
fn svg_lazy_setup_refuses_early_calls_without_filesystem_effects_and_keeps_svg_format() {
    let scratch = Scratch::new();
    let mut port = LazyDesktopArtifactPort::new_svg(
        scratch.stage(),
        "svg-output".into(),
        scratch.destination(),
    );
    assert!(port
        .write_frame("svg", "frame_0001.svg", b"<svg/>")
        .is_err());
    assert!(port
        .publish("svg", "svg-output", &["frame_0001.svg".into()])
        .is_err());
    assert!(port.begin_staging("svg", "foreign-output").is_err());
    port.cleanup("svg").unwrap();
    assert!(!scratch.stage().exists() && !scratch.destination().exists());
    port.begin_staging("svg", "svg-output").unwrap();
    assert!(port
        .write_frame("svg", "frame_0001.png", b"refused")
        .is_err());
    port.write_frame("svg", "frame_0001.svg", b"<svg/>")
        .unwrap();
    assert!(port
        .publish("svg", "svg-output", &["frame_0001.png".into()])
        .is_err());
    assert!(!scratch.destination().exists());
    port.cleanup("svg").unwrap();
    assert!(scratch.stage().is_dir());
    assert_eq!(fs::read_dir(scratch.stage()).unwrap().count(), 0);
    assert!(!scratch.destination().exists());
}
