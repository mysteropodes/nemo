//! Actual host + engine + borrowed real GPU surface. Not installed/AppKit proof.
#[cfg(target_os = "macos")]
use super::exposure_tests::decode_png;
use super::exposure_tests::Scratch;
use super::{pixel, viewport};
use crate::native_dispatch::object_host::preview::tests::{output, selector, snapshot};
use crate::native_dispatch::{NativeDispatch, NativeObjectHost};
use native_engine::{
    compositor::Compositor, desktop_viewport::ViewportStatus, export_job::JobStatus,
};
use serde_json::{json, Value};

fn project() -> Value {
    let cases: Value = serde_json::from_str(include_str!(
        "../../engineering/application/examples/native-object-v1/cases.json"
    ))
    .unwrap();
    let mut anchor = cases["records"][0].clone();
    anchor["family"] = json!("editable-vector-path");
    anchor["target"] = json!({"contextId":"scene-root","layerUid":"L0","strokeId":"anchor","frameScope":{"kind":"authored","frame":0}});
    anchor["geometry"]["segments"] =
        json!([[4,4],[40,4],[4,40]].map(|[x,y]|
        json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
    anchor["fill"] = json!({"kind":"solid","r":0,"g":1,"b":0,"a":1});
    anchor["vector"] = json!({"stroke":null,"hasRealStroke":false,"strokeWidth":8,"strokeCap":"round","strokeJoin":"round",
        "miterLimit":10,"dashOffset":0,"opacity":1});
    anchor["vectorExt"] = json!({"bitmapBrush":{"bitmapBrushSpec":{"tip":"hard","size":8,"spacing":15,"scatter":0,
        "opacity":1,"color":"#0000ffff","pressure":false,"seed":17},"brushGroupId":"host brush","preTextureStroke":null}});
    json!({"format":"nemo.native-project-document","formatVersion":2,"project":{"canvasW":64,"canvasH":64,"fps":24},
        "viewportReference":null,"assets":[{"id":"ink","name":"Authored ink","kind":"image","source":{"kind":"embedded",
            "mediaType":"image/png","encoding":"base64","data":"iVBORw0KGgoAAAANSUhEUgAAAAIAAAABCAYAAAD0In+KAAAADUlEQVR4nGNgYPgPBgAU9AX7uVYDfQAAAABJRU5ErkJggg=="}}],
        "assetInventory":{"assetIds":["ink"],"placements":[]},"drawing":{"format":"nemo.native-object-document","formatVersion":2,"totalFrames":12,
            "layers":[{"layerUid":"L0","timing":{"format":"nemo.native-layer-timing","formatVersion":1,"family":"drawing",
                "contextId":"scene-root","keys":[{"frame":0}],"samples":[]}}],"objects":[anchor],
            "items":{"raster":[{"target":{"contextId":"scene-root","layerUid":"L0","strokeId":"ink","frameScope":{"kind":"authored","frame":0}},
                "item":{"assetId":"ink","x":16,"y":16,"width":8,"height":8,"opacity":1,
                    "isBitmapBrush":true,"isBrushTextureCopy":true,"brushGroupId":"host brush"}}]},
            "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
                "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                    "layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],"scopes":[{"frameScope":{"kind":"authored","frame":0},
                        "elements":[{"layerUid":"L0","strokeId":"anchor","naturalPosition":0,"order":{}},
                            {"layerUid":"L0","strokeId":"ink","naturalPosition":1,"order":{}}]}]}]}}})
}
fn host(wire: &Value) -> NativeObjectHost {
    NativeObjectHost::from_project_file("host-native-brush".into(), wire.to_string().as_bytes())
        .unwrap()
}
#[test]
fn cpu_real_brush_host_refuses_bad_pins_or_companion_before_target_factory() {
    for corrupt in [false, true] {
        let mut wire = project();
        if corrupt {
            wire["drawing"]["items"]["raster"][0]["item"]["brushGroupId"] = json!("foreign");
        }
        let mut owner = host(&wire);
        let pin = snapshot(&owner);
        let before = owner
            .project_file_snapshot(0, pin.snapshot_id())
            .unwrap()
            .unwrap();
        let mut selected = selector(&pin, 0);
        if !corrupt {
            selected.document_snapshot_id = "foreign";
        }
        let mut called = false;
        assert!(owner
            .with_prepared_object_preview(&selected, output(), |_| {
                called = true;
                Ok(())
            })
            .is_err());
        assert!(!called);
        assert!(owner.preview.prepared.is_none());
        assert!(owner.preview.pending.is_none());
        assert!(owner.preview.reference_cache.is_none());
        assert_eq!(owner.preview.scheduler.lease_counters().live(), 0);
        assert_eq!(
            owner
                .project_file_snapshot(0, pin.snapshot_id())
                .unwrap()
                .unwrap(),
            before
        );
    }
}
#[cfg(target_os = "macos")]
#[test]
fn gpu_real_host_brush_preview_invalidate_and_export_share_native_runtime_and_literal_pixels() {
    let mut owner = host(&project());
    let source = snapshot(&owner);
    let saved = owner
        .project_file_snapshot(0, source.snapshot_id())
        .unwrap()
        .unwrap();
    let runtime_address = &owner.preview.brush_runtime as *const _;
    let mut gpu = Compositor::new().expect("real Metal required");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    let device = gpu.device().clone();
    let (mut view, trace) = viewport();
    trace.borrow_mut().defer = true;
    let deferred = owner
        .present_object_preview(&selector(&source, 0), output(), &mut gpu, &mut view)
        .unwrap();
    assert!(matches!(deferred.status(), ViewportStatus::Deferred(_)));
    let pending = owner.preview.pending.as_ref().unwrap();
    let work_id = pending.work_id();
    assert!(owner.preview.scheduler.is_current(pending));
    assert_eq!(pending.key().declared_resource_versions().len(), 1);
    let lease_ids = owner.preview.scheduler.lease_ids(work_id).unwrap();
    assert_eq!(lease_ids.1.len(), 1, "one admitted Brush resource lease");
    assert_ne!(
        lease_ids.0, lease_ids.1[0],
        "snapshot and resource leases differ"
    );
    let leased = owner.preview.scheduler.lease_counters();
    assert_eq!(
        (leased.acquired(), leased.released(), leased.live()),
        (2, 0, 2)
    );
    let retried = owner
        .present_object_preview(&selector(&source, 0), output(), &mut gpu, &mut view)
        .unwrap();
    assert!(matches!(retried.status(), ViewportStatus::Deferred(_)));
    assert_eq!(retried.identity(), deferred.identity());
    assert_eq!(owner.preview.pending.as_ref().unwrap().work_id(), work_id);
    assert_eq!(
        owner.preview.scheduler.lease_ids(work_id).unwrap(),
        lease_ids
    );
    assert_eq!(owner.preview.scheduler.lease_counters(), leased);
    trace.borrow_mut().defer = false;
    let shown = owner
        .present_object_preview(&selector(&source, 0), output(), &mut gpu, &mut view)
        .unwrap();
    assert_eq!(shown.status(), &ViewportStatus::Presented);
    assert_eq!(shown.identity(), deferred.identity());
    assert!(owner.preview.pending.is_none());
    assert!(owner.preview.scheduler.lease_ids(work_id).is_none());
    let settled = owner.preview.scheduler.lease_counters();
    assert_eq!(
        (settled.acquired(), settled.released(), settled.live()),
        (2, 2, 0)
    );
    pixel(&trace, 8, 8, [0, 255, 0, 255]);
    pixel(&trace, 13, 15, [0, 0, 255, 255]);
    pixel(&trace, 40, 40, [255; 4]);
    let old = owner.preview.reference_cache.as_ref().unwrap().clone();
    let versions = old.plan().scene_resource_versions().unwrap();
    assert_eq!(versions.len(), 1);
    owner.preview.invalidate();
    assert_eq!(runtime_address, &owner.preview.brush_runtime as *const _);
    let shown = owner
        .present_object_preview(&selector(&source, 1), output(), &mut gpu, &mut view)
        .unwrap();
    assert_eq!(shown.identity().evaluation_key().frame(), 1);
    pixel(&trace, 13, 15, [0, 0, 255, 255]);
    assert_eq!(
        owner
            .preview
            .reference_cache
            .as_ref()
            .unwrap()
            .plan()
            .scene_resource_versions()
            .unwrap(),
        versions
    );
    let scratch = Scratch::new();
    let mut port = scratch.port();
    // Scratch's existing DesktopArtifactPort owns this real target and normal staging/rename.
    let outcome = owner
        .export_pinned_object_png_frames(
            &selector(&source, 0),
            &[0, 1],
            "host-brush-export".into(),
            "held-output".into(),
            output(),
            65536,
            &mut port,
            &mut gpu,
        )
        .unwrap();
    assert_eq!(outcome.receipt.unwrap().status, JobStatus::Succeeded);
    assert!(outcome.release.cleanup_complete);
    assert_eq!(runtime_address, &owner.preview.brush_runtime as *const _);
    assert_eq!(gpu.device(), &device);
    for name in ["frame_0001.png", "frame_0002.png"] {
        let pixels = decode_png(&std::fs::read(scratch.0.join("published").join(name)).unwrap());
        for (x, y, color) in [
            (8, 8, [0, 255, 0, 255]),
            (13, 15, [0, 0, 255, 255]),
            (40, 40, [255; 4]),
        ] {
            assert_eq!(&pixels[(y * 64 + x) * 4..(y * 64 + x + 1) * 4], &color);
        }
    }
    assert_eq!(owner.preview.scheduler.lease_counters().live(), 0);
    let complete = owner.preview.scheduler.lease_counters();
    assert_eq!(complete.acquired(), complete.released());
    assert_eq!(
        owner
            .project_file_snapshot(0, source.snapshot_id())
            .unwrap()
            .unwrap(),
        saved
    );
    assert!(owner.release_project().unwrap().reconciliation_complete());
    assert!(owner.preview.reference_cache.is_none());
    assert!(owner
        .with_prepared_object_preview::<()>(&selector(&source, 0), output(), |_| panic!(
            "released host cannot borrow target"
        ))
        .is_err());
}
#[test]
fn gpu_actual_native_fill_undo_keeps_authored_brush_ink_and_restores_original_anchor_pixels() {
    let mut owner = host(&project());
    let original = snapshot(&owner);
    let before = owner
        .project_file_snapshot(0, original.snapshot_id())
        .unwrap()
        .unwrap();
    let mut gpu = Compositor::new().expect("real Metal required");
    let device = gpu.device().clone();
    let (mut view, trace) = viewport();
    owner
        .present_object_preview(&selector(&original, 0), output(), &mut gpu, &mut view)
        .unwrap();
    pixel(&trace, 8, 8, [0, 255, 0, 255]);
    pixel(&trace, 13, 15, [0, 0, 255, 255]);
    let fill = json!({"command":"object.fill.set","stableTarget":{"contextId":"scene-root","layerUid":"L0","strokeId":"anchor",
        "frameScope":{"kind":"authored","frame":0}},"fill":{"kind":"solid","r":1,"g":0,"b":0,"a":1}});
    let prepared = owner
        .history
        .prepare_fill_json(fill.to_string().as_bytes())
        .unwrap();
    assert_eq!(owner.history.commit_fill(prepared), Ok(1));
    owner.preview.invalidate();
    let edited = snapshot(&owner);
    owner
        .present_object_preview(&selector(&edited, 0), output(), &mut gpu, &mut view)
        .unwrap();
    pixel(&trace, 8, 8, [255, 0, 0, 255]);
    pixel(&trace, 13, 15, [0, 0, 255, 255]);
    pixel(&trace, 40, 40, [255; 4]);
    assert_eq!(owner.history.undo(1), Ok(2));
    owner.preview.invalidate();
    let undone = snapshot(&owner);
    owner
        .present_object_preview(&selector(&undone, 0), output(), &mut gpu, &mut view)
        .unwrap();
    pixel(&trace, 8, 8, [0, 255, 0, 255]);
    pixel(&trace, 13, 15, [0, 0, 255, 255]);
    assert_eq!(gpu.device(), &device);
    assert_eq!(
        owner
            .project_file_snapshot(0, original.snapshot_id())
            .unwrap()
            .unwrap(),
        before
    );
    assert!(owner.release_project().unwrap().reconciliation_complete());
    assert_eq!(owner.preview.scheduler.lease_counters().live(), 0);
}
