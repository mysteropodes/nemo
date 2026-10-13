//! Real filesystem publication through the original object owner and port.
use crate::native_application_ports::DesktopArtifactPort;
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{
    compositor::Compositor,
    export_job::{ExternalEffectDisposition, JobStatus},
    object_codec::decode_storage_project,
    object_document::FrameScopeKind,
    object_frame_packet::ObjectFrameSelector,
    scheduler::OutputSpec,
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!("nemo-object-port-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        Self(root.canonicalize().unwrap())
    }
    fn port(&self) -> DesktopArtifactPort {
        DesktopArtifactPort::new(
            self.0.join("stage"),
            [("object-output".into(), self.0.join("published"))],
        )
        .unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn host() -> NativeObjectHost {
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
    let document = json!({
        "format":"nemo.native-object-document","formatVersion":2,"totalFrames":1,
        "layers":[{"layerUid":"L0"}],"objects":[record],
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,
            "documentFormatVersion":2,"contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],
                "scopes":[{"frameScope":{"kind":"authored","frame":0},
                    "elements":[{"layerUid":"L0","strokeId":"a","naturalPosition":0,"order":{}}]}]}],
            "migration":"explicit-plan-required","referencePolicy":"unresolved"}
    });
    NativeObjectHost::new(
        "desktop-object-export-owner".into(),
        decode_storage_project(&serde_json::to_vec(&document).unwrap()).unwrap(),
    )
    .unwrap()
}
fn output() -> OutputSpec {
    OutputSpec::new("frame", "rgba8", 64, 64, "srgb", "straight").unwrap()
}
fn selector<'a>(
    snapshot: &'a native_engine::object_snapshot::ObjectSnapshot,
) -> ObjectFrameSelector<'a> {
    ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    }
}

#[cfg(target_os = "macos")]
fn decode_png(bytes: &[u8]) -> Vec<u8> {
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
        let stride: usize = msg_send![image, bytesPerRow];
        let pixels: *const u8 = msg_send![image, bitmapData];
        assert_eq!((width, height), (64, 64));
        assert!(!pixels.is_null() && stride >= width * 4);
        let mut result = Vec::with_capacity(width * height * 4);
        for row in 0..height {
            result.extend_from_slice(std::slice::from_raw_parts(
                pixels.add(row * stride),
                width * 4,
            ));
        }
        let _: () = msg_send![image, release];
        let _: () = msg_send![data, release];
        result
    }
}

#[test]
#[cfg(target_os = "macos")]
fn original_owner_exports_one_literal_png_through_real_desktop_port() {
    let scratch = Scratch::new();
    let mut owner = host();
    let snapshot = owner.history.acquire_snapshot(0).unwrap();
    let expected = selector(&snapshot);
    let mut port = scratch.port();
    let mut gpu = Compositor::new().unwrap();
    let outcome = owner
        .export_pinned_object_png(
            &expected,
            "first".into(),
            "object-output".into(),
            output(),
            65536,
            &mut port,
            &mut gpu,
        )
        .unwrap();
    let receipt = outcome.receipt.unwrap();
    assert_eq!(receipt.status, JobStatus::Succeeded);
    assert_eq!(receipt.pinned_revision, 0);
    assert_eq!(receipt.document_snapshot_id, snapshot.snapshot_id());
    assert_eq!(
        receipt.external_effect_disposition,
        ExternalEffectDisposition::Committed
    );
    assert_eq!(receipt.artifact.unwrap().files, ["frame_0001.png"]);
    assert!(outcome.release.cleanup_complete);
    assert_eq!(fs::read_dir(scratch.0.join("stage")).unwrap().count(), 0);
    let png = fs::read(scratch.0.join("published/frame_0001.png")).unwrap();
    assert!(png.starts_with(b"\x89PNG\r\n\x1a\n"));
    let rgba = decode_png(&png);
    assert_eq!(
        &rgba[(8 * 64 + 8) * 4..(8 * 64 + 8) * 4 + 4],
        &[0, 0, 255, 255]
    );
    assert_eq!(&rgba[(40 * 64 + 40) * 4..(40 * 64 + 40) * 4 + 4], &[255; 4]);
    assert!(owner
        .export_pinned_object_png(
            &expected,
            "second".into(),
            "object-output".into(),
            output(),
            65536,
            &mut port,
            &mut gpu,
        )
        .is_err());
    let release = owner.release_project().unwrap();
    assert!(release.application.exports.cleanup_complete);
    assert_eq!(release.application.exports.receipts.len(), 1);
}

#[test]
#[cfg(target_os = "macos")]
fn collision_and_stale_owner_never_overwrite_existing_destination() {
    let scratch = Scratch::new();
    let mut owner = host();
    let snapshot = owner.history.acquire_snapshot(0).unwrap();
    let mut expected = selector(&snapshot);
    let mut port = scratch.port();
    let mut gpu = Compositor::new().unwrap();
    expected.content_revision = 1;
    assert!(owner
        .export_pinned_object_png(
            &expected,
            "stale".into(),
            "object-output".into(),
            output(),
            65536,
            &mut port,
            &mut gpu,
        )
        .is_err());
    assert!(!scratch.0.join("published").exists());
    expected.content_revision = 0;
    fs::create_dir(scratch.0.join("published")).unwrap();
    fs::write(scratch.0.join("published/original"), b"preserve").unwrap();
    let outcome = owner
        .export_pinned_object_png(
            &expected,
            "collision".into(),
            "object-output".into(),
            output(),
            65536,
            &mut port,
            &mut gpu,
        )
        .unwrap();
    let receipt = outcome.receipt.unwrap();
    assert_eq!(receipt.status, JobStatus::Failed);
    assert!(outcome.release.cleanup_complete);
    assert_eq!(
        fs::read(scratch.0.join("published/original")).unwrap(),
        b"preserve"
    );
    assert_eq!(fs::read_dir(scratch.0.join("stage")).unwrap().count(), 0);
    let release = owner.release_project().unwrap();
    assert_eq!(
        release.application.exports.receipts[0].status,
        JobStatus::Failed
    );
    assert!(owner
        .export_pinned_object_png(
            &expected,
            "late".into(),
            "object-output".into(),
            output(),
            65536,
            &mut port,
            &mut gpu,
        )
        .is_err());
}

#[test]
#[cfg(target_os = "macos")]
fn symlink_destination_cannot_redirect_original_owner_export() {
    use std::os::unix::fs::symlink;

    let scratch = Scratch::new();
    let mut owner = host();
    let snapshot = owner.history.acquire_snapshot(0).unwrap();
    let expected = selector(&snapshot);
    let mut port = scratch.port();
    let mut gpu = Compositor::new().unwrap();
    let outside = scratch.0.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("original"), b"preserve").unwrap();
    symlink(&outside, scratch.0.join("published")).unwrap();

    let outcome = owner
        .export_pinned_object_png(
            &expected,
            "symlink".into(),
            "object-output".into(),
            output(),
            65536,
            &mut port,
            &mut gpu,
        )
        .unwrap();
    assert_eq!(outcome.receipt.unwrap().status, JobStatus::Failed);
    assert!(outcome.release.cleanup_complete);
    assert_eq!(fs::read(outside.join("original")).unwrap(), b"preserve");
    assert_eq!(fs::read_dir(&outside).unwrap().count(), 1);
    assert_eq!(fs::read_dir(scratch.0.join("stage")).unwrap().count(), 0);
}
