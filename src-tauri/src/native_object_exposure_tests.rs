//! Actual strict host preview/export with fixed authored key/sample/blank pixels.
//! The inherited surface records real GPU bytes; this is not installed/AppKit proof.
use crate::native_application_ports::DesktopArtifactPort;
use crate::native_dispatch::object_host::preview::{
    reference_tests::{attach, pixel, viewport},
    tests::{output, selector, snapshot},
};
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{
    compositor::Compositor, desktop_viewport::ViewportStatus, export_job::JobStatus,
};
use serde_json::{json, Value};
use std::{fs, path::PathBuf};

const WHITE: [u8; 4] = [255; 4];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const GRAY: [u8; 4] = [128, 128, 128, 255];
const ORANGE: [u8; 4] = [255, 128, 0, 255];
// Independent expected full/key/sample/held/blank matrix. Wrong exact-frame and
// sample-as-key selection differ at2/4; blank-as-missing differs at5/7.
const A_PIXELS: [[u8; 4]; 8] = [WHITE, BLUE, BLUE, RED, BLUE, WHITE, GREEN, WHITE];
const B_PIXELS: [[u8; 4]; 8] = [GRAY, GRAY, ORANGE, ORANGE, ORANGE, ORANGE, WHITE, WHITE];
fn timing(keys: &[u32], samples: Value) -> Value {
    json!({"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing","contextId":"scene-root",
        "keys":keys.iter().map(|frame|json!({"frame":frame})).collect::<Vec<_>>(),"samples":samples})
}
fn document() -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut objects = Vec::new();
    let mut scopes = Vec::new();
    let authored = [
        ("A", "A", 1, [0., 0., 1.], 4),
        ("A", "IGNORED", 2, [1., 1., 0.], 4),
        ("A", "C", 3, [1., 0., 0.], 4),
        ("A", "D", 6, [0., 1., 0.], 4),
        ("B", "B0", 0, [0.5, 0.5, 0.5], 36),
        ("B", "B2", 2, [1., 0.5, 0.], 36),
    ];
    for (layer, id, frame, color, start) in authored {
        let mut record = cases["records"][0].clone();
        record["target"] = json!({"contextId":"scene-root","frameScope":{"kind":"authored","frame":frame},"layerUid":layer,"strokeId":id});
        record["geometry"]["segments"] =
            json!([[start,start],[start+24,start],[start+24,start+24],[start,start+24]].map(|[x,y]|
            json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
        record["fill"] = json!({"kind":"solid","r":color[0],"g":color[1],"b":color[2],"a":1});
        objects.push(record);
    }
    for frame in [0, 1, 2, 3, 6] {
        let mut positions = std::collections::HashMap::<String, usize>::new();
        let elements:Vec<_>=objects.iter().filter(|r|r["target"]["frameScope"]["frame"]==frame).map(|r|{
            let layer=r["target"]["layerUid"].as_str().unwrap();let position=positions.entry(layer.into()).or_default();
            let value=json!({"layerUid":layer,"strokeId":r["target"]["strokeId"],"naturalPosition":*position,"order":{}});*position+=1;value
        }).collect();
        scopes.push(json!({"frameScope":{"kind":"authored","frame":frame},"elements":elements}));
    }
    json!({"format":"nemo.native-project-document","formatVersion":1,"project":{"canvasW":64,"canvasH":64,"fps":24},
        "viewportReference":null,"assets":[],"drawing":{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":8,
        "layers":[{"layerUid":"A","timing":timing(&[1,5],json!([{"frame":3},{"frame":6,"manualEdit":true}]))},
                  {"layerUid":"B","timing":timing(&[0,2,6],json!([]))}],"objects":objects,
        "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
        "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
        "layers":[{"layerUid":"A","naturalPosition":0,"order":{}},{"layerUid":"B","naturalPosition":1,"order":{}}],"scopes":scopes}]}}})
}
fn host(value: &Value) -> NativeObjectHost {
    NativeObjectHost::from_project_file("held-host".into(), value.to_string().as_bytes()).unwrap()
}
pub(super) struct Scratch(pub(super) PathBuf);
impl Scratch {
    pub(super) fn new() -> Self {
        let p = std::env::temp_dir().join(format!("nemo-held-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&p).unwrap();
        Self(p.canonicalize().unwrap())
    }
    pub(super) fn port(&self) -> DesktopArtifactPort {
        DesktopArtifactPort::new(
            self.0.join("stage"),
            [("held-output".into(), self.0.join("published"))],
        )
        .unwrap()
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[cfg(target_os = "macos")]
pub(super) fn decode_png(bytes: &[u8]) -> Vec<u8> {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};
    unsafe {
        let data: *mut Object = msg_send![class!(NSData), alloc];
        let data: *mut Object = msg_send![data,initWithBytes:bytes.as_ptr() length:bytes.len()];
        let image: *mut Object = msg_send![class!(NSBitmapImageRep), alloc];
        let image: *mut Object = msg_send![image,initWithData:data];
        assert!(!image.is_null());
        let width: usize = msg_send![image, pixelsWide];
        let height: usize = msg_send![image, pixelsHigh];
        let stride: usize = msg_send![image, bytesPerRow];
        let pixels: *const u8 = msg_send![image, bitmapData];
        assert_eq!((width, height), (64, 64));
        assert!(!pixels.is_null() && stride >= 256);
        let mut result = Vec::with_capacity(64 * 64 * 4);
        for row in 0..64 {
            result.extend_from_slice(std::slice::from_raw_parts(pixels.add(row * stride), 256));
        }
        let _: () = msg_send![image, release];
        let _: () = msg_send![data, release];
        result
    }
}
#[test]
fn cpu_missing_mixed_and_reference_object_temporal_preview_refuse_before_borrowing() {
    for missing in [1, 2] {
        let mut value = document();
        for layer in value["drawing"]["layers"]
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .take(missing)
        {
            layer.as_object_mut().unwrap().remove("timing");
            layer["visible"] = json!(false);
        }
        let mut owner = host(&value);
        let s = snapshot(&owner);
        let mut called = false;
        let error = owner
            .with_prepared_object_preview(&selector(&s, 4), output(), |_| {
                called = true;
                Ok(())
            })
            .unwrap_err();
        assert!(error.contains("Unavailable"));
        assert!(!called);
        assert!(owner.preview.prepared.is_none());
        assert!(owner.preview.pending.is_none());
    }
    let mut owner = host(&document());
    let s = snapshot(&owner);
    let mut reference = selector(&s, 1);
    reference.scope_kind = native_engine::object_document::FrameScopeKind::Reference;
    assert!(owner
        .with_prepared_object_preview(&reference, output(), |_| Ok(()))
        .unwrap_err()
        .contains("Unavailable"));
}
#[cfg(target_os = "macos")]
#[test]
fn gpu_actual_host_preview_and_png_use_independent_layer_sources_and_exclude_reference_media() {
    let mut owner = host(&document());
    attach(&mut owner);
    let s = snapshot(&owner);
    let saved = owner
        .project_file_snapshot(s.content_revision(), s.snapshot_id())
        .unwrap()
        .expect("native project fixture");
    let mut gpu = Compositor::new().unwrap();
    let (mut view, trace) = viewport();
    for frame in 0..8 {
        let shown = owner
            .present_object_preview(&selector(&s, frame), output(), &mut gpu, &mut view)
            .unwrap();
        assert_eq!(shown.status(), &ViewportStatus::Presented);
        assert_eq!(
            shown.identity().evaluation_key().frame(),
            frame,
            "requested output identity, never held source"
        );
        pixel(&trace, 8, 8, A_PIXELS[frame as usize]);
        pixel(&trace, 40, 56, B_PIXELS[frame as usize]);
        pixel(&trace, 8, 32, [128, 128, 255, 255]); // Existing blue/white reference at its native 0.5 opacity.
    }
    let scratch = Scratch::new();
    let mut port = scratch.port();
    let outcome = owner
        .export_pinned_object_png_frames(
            &selector(&s, 0),
            &[0, 1, 2, 3, 4, 5, 6, 7],
            "held-range".into(),
            "held-output".into(),
            output(),
            65536,
            &mut port,
            &mut gpu,
        )
        .unwrap();
    let receipt = outcome.receipt.unwrap();
    assert_eq!(receipt.status, JobStatus::Succeeded);
    assert_eq!(receipt.pinned_revision, s.content_revision());
    assert_eq!(receipt.document_snapshot_id, s.snapshot_id());
    assert_eq!(receipt.artifact.as_ref().unwrap().files.len(), 8);
    assert!(outcome.release.cleanup_complete);
    for frame in 0..8 {
        let bytes = fs::read(
            scratch
                .0
                .join(format!("published/frame_{:04}.png", frame + 1)),
        )
        .unwrap();
        let rgba = decode_png(&bytes);
        for (x, y, expected) in [
            (8, 8, A_PIXELS[frame]),
            (40, 56, B_PIXELS[frame]),
            (8, 32, WHITE),
        ] {
            let actual = &rgba[(y * 64 + x) * 4..(y * 64 + x) * 4 + 4];
            assert!(
                actual.iter().zip(expected).all(|(a, b)| a.abs_diff(b) <= 1),
                "frame{frame}/{x},{y}: {actual:?} != {expected:?}"
            );
        }
    }
    let current = snapshot(&owner);
    assert_eq!(current.content_revision(), s.content_revision());
    assert_eq!(current.snapshot_id(), s.snapshot_id());
    assert_eq!(
        owner
            .project_file_snapshot(current.content_revision(), current.snapshot_id())
            .unwrap()
            .expect("native project fixture"),
        saved,
        "render/export cannot change source"
    );
    assert!(owner.release_project().unwrap().reconciliation_complete());
    let mut missing = document();
    missing["drawing"]["layers"][0]
        .as_object_mut()
        .unwrap()
        .remove("timing");
    let mut refused = host(&missing);
    let empty = Scratch::new();
    let mut port = empty.port();
    let s = snapshot(&refused);
    assert!(refused
        .export_pinned_object_png(
            &selector(&s, 4),
            "missing".into(),
            "held-output".into(),
            output(),
            65536,
            &mut port,
            &mut gpu
        )
        .unwrap_err()
        .contains("Unavailable"));
    assert!(!empty.0.join("published").exists());
    assert!(
        refused.object_export.is_none(),
        "refusal before staging/session"
    );
}

#[test]
fn cpu_canonical_p38_default_preview_selects_authored_sources_at_0_6_7_12_20() {
    let fixtures = [
        (
            "A",
            include_bytes!("../../tests/fixtures/p38/native-project/A.json").as_slice(),
        ),
        (
            "B",
            include_bytes!("../../tests/fixtures/p38/native-project/B.json").as_slice(),
        ),
    ];
    for (label, bytes) in fixtures {
        let mut owner =
            NativeObjectHost::from_project_file(format!("p38-held-{label}"), bytes).unwrap();
        let source = snapshot(&owner);
        let info = owner.project_file_info().unwrap();
        let output = native_engine::scheduler::OutputSpec::new(
            "frame",
            "rgba8",
            info["canvasW"].as_u64().unwrap() as u32,
            info["canvasH"].as_u64().unwrap() as u32,
            "srgb",
            "straight",
        )
        .unwrap();
        let saved = owner
            .project_file_snapshot(source.content_revision(), source.snapshot_id())
            .unwrap()
            .unwrap();
        assert_eq!(
            saved.as_slice(),
            bytes,
            "canonical fixture bytes remain exact"
        );
        assert_eq!(source.document().objects().len(), 3);
        let alpha = source
            .document()
            .layers()
            .iter()
            .position(|layer| layer.layer_uid() == "legacy layer:alpha")
            .unwrap();
        let reference = source
            .document()
            .layers()
            .iter()
            .position(|layer| layer.layer_uid() == "0")
            .unwrap();
        assert!(source.document().objects().iter().any(|record| {
            record.target().layer_uid() == "0"
                && record.target().frame() == 12
                && record.target().scope_kind()
                    == &native_engine::object_document::FrameScopeKind::Reference
        }));
        for (requested, authored, stroke) in [
            (0, 0, "ink/path_A"),
            (6, 0, "ink/path_A"),
            (7, 7, "ink/path_B"),
            (12, 7, "ink/path_B"),
            (20, 7, "ink/path_B"),
        ] {
            owner
                .with_prepared_object_preview(
                    &selector(&source, requested),
                    output.clone(),
                    |host| {
                        let prepared = host.preview.prepared.as_ref().unwrap();
                        let packet = prepared.admission.packet();
                        assert_eq!(prepared.output, output);
                        assert_eq!(packet.frame(), requested);
                        assert_eq!(packet.content_revision(), source.content_revision());
                        assert_eq!(packet.document_snapshot_id(), source.snapshot_id());
                        assert_eq!(packet.source_frame_for_layer(alpha), Some(authored));
                        assert_eq!(packet.source_frame_for_layer(reference), None);
                        assert_eq!(packet.records().len(), 1);
                        let record = packet.records().next().unwrap();
                        assert_eq!(record.source_layer_index(), alpha);
                        assert_eq!(record.record().target().layer_uid(), "legacy layer:alpha");
                        assert_eq!(record.record().target().stroke_id(), stroke);
                        assert_eq!(record.record().target().frame(), authored);
                        assert_eq!(
                            record.record().target().scope_kind(),
                            &native_engine::object_document::FrameScopeKind::Authored
                        );
                        assert!(std::ptr::eq(
                            record.record(),
                            &source.document().objects()[record.source_object_index()]
                        ));
                        assert_eq!(prepared.admission.contour_count(), 1);
                        Ok(())
                    },
                )
                .unwrap();
        }
        let current = snapshot(&owner);
        assert_eq!(current.content_revision(), source.content_revision());
        assert_eq!(current.snapshot_id(), source.snapshot_id());
        assert_eq!(
            owner
                .project_file_snapshot(current.content_revision(), current.snapshot_id())
                .unwrap()
                .unwrap(),
            saved
        );
    }
}
