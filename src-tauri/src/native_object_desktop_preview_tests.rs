use crate::native_application_contract::host_error;
use crate::native_application_viewport::TestObjectSlot;
use crate::native_object_desktop_preview::admit_request;
use crate::native_object_desktop_preview::tests::support::{document, fill, Fixture};
use native_engine::{
    compositor::Compositor, desktop_viewport::SurfacePresentation,
    object_codec::decode_storage_project,
};
use serde_json::json;
#[path = "native_object_desktop_preview_extent_tests.rs"]
mod extent_tests;
#[path = "native_object_desktop_preview_test_support.rs"]
mod support;

pub(super) fn pixel(source: &SurfacePresentation<'_>, expected: [u8; 4]) {
    let buffer = source.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("object-desktop-pixel"),
        size: 1280 * 180,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = source
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: source.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(1280),
                rows_per_image: Some(180),
            },
        },
        wgpu::Extent3d {
            width: 320,
            height: 180,
            depth_or_array_layers: 1,
        },
    );
    source.queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |value| tx.send(value).unwrap());
    source
        .device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let bytes = slice.get_mapped_range();
    assert_eq!(&bytes[8 * 1280 + 8 * 4..8 * 1280 + 8 * 4 + 4], &expected);
    assert_eq!(
        &bytes[40 * 1280 + 40 * 4..40 * 1280 + 40 * 4 + 4],
        &[255; 4]
    );
    drop(bytes);
    buffer.unmap();
}

#[test]
fn every_invalid_pin_and_original_byte_shape_refuses_before_first_gpu_factory() {
    let f = Fixture::new();
    for field in [
        "instanceId",
        "documentId",
        "documentSnapshotId",
        "contextId",
    ] {
        let mut raw = f.raw.clone();
        raw[field] = json!("foreign");
        assert!(f.run(&raw.to_string()).is_err(), "{field}");
    }
    for (field, value) in [
        ("contentRevision", json!(1)),
        ("lifecycleGeneration", json!(999)),
        ("scopeKind", json!("reference")),
        ("frame", json!(1)),
        ("cancelledBeforeDispatch", json!(true)),
    ] {
        let mut raw = f.raw.clone();
        raw[field] = value;
        assert!(f.run(&raw.to_string()).is_err(), "{field}");
    }
    let mut raw = f.raw.clone();
    raw["outputSpec"]["width"] = json!(64);
    assert!(f.run(&raw.to_string()).is_err());
    let duplicate = f.raw.to_string().replacen('{', "{\"apiVersion\":2,", 1);
    assert!(f.run(&duplicate).is_err());
    assert!(f.run(&" ".repeat(8193)).is_err());
    assert_eq!(f.creations.get(), 0);
    assert_eq!(f.trace.borrow().acquisitions, 0);
}

#[test]
fn exact_pending_subscriber_proof_presents_on_original_gpu_without_ack_or_barrier_release() {
    let mut f = Fixture::new();
    let binding = f
        .state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    f.state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding.clone()}))
        .unwrap();
    let (response, notification) = f
        .state
        .external_object_preview_for_test(&f.request("command.document.object.fill.set", fill(1)))
        .unwrap();
    assert!(response.ok);
    let (event, mut receiver) = notification.unwrap();
    f.refresh();
    f.trace.borrow_mut().color = [255, 0, 0, 255];
    assert!(f.run(&f.raw.to_string()).is_err());
    assert_eq!(f.creations.get(), 0);

    let mut tagged = f.raw.clone();
    tagged["revisionSync"] =
        json!({"subscriptionId":binding["subscriptionId"],"event":event.clone()});
    let mut array_proof = tagged.clone();
    array_proof["revisionSync"] = json!([]);
    assert!(admit_request(&array_proof.to_string(), f.state.instance_id()).is_err());
    let mut array_event = tagged.clone();
    array_event["revisionSync"]["event"] = json!([]);
    assert!(admit_request(&array_event.to_string(), f.state.instance_id()).is_err());
    let mut null_proof = tagged.clone();
    null_proof["revisionSync"] = serde_json::Value::Null;
    assert!(admit_request(&null_proof.to_string(), f.state.instance_id()).is_err());
    let mut malformed = tagged.clone();
    malformed["revisionSync"]["extra"] = json!(true);
    assert!(admit_request(&malformed.to_string(), f.state.instance_id()).is_err());
    let mut wrong = tagged.clone();
    wrong["revisionSync"]["event"]["requestId"] = json!("foreign-request");
    assert!(f.run(&wrong.to_string()).is_err());
    assert_eq!(f.creations.get(), 0);

    assert_eq!(f.run(&tagged.to_string()).unwrap().status, "presented");
    assert_eq!(f.creations.get(), 1);
    assert_eq!(f.trace.borrow().acquisitions, 1);
    assert!(matches!(
        receiver.try_recv(),
        Err(tokio::sync::oneshot::error::TryRecvError::Empty)
    ));
    f.state
        .control_revision_for_test(json!({"action":"acknowledge","subscriptionId":binding["subscriptionId"],"event":event.clone()}))
        .unwrap();
    assert_eq!(receiver.try_recv(), Ok(()));
    assert!(
        f.run(&tagged.to_string()).is_err(),
        "an already acknowledged proof must not present again"
    );
    assert!(f.state.settle_object_preview_for_test(event, true));
    assert!(
        f.run(&tagged.to_string()).is_err(),
        "a settled proof must not be reusable"
    );
    assert_eq!(f.run(&f.raw.to_string()).unwrap().status, "presented");
}

#[test]
fn gpu_one_original_target_retries_and_shared_edits_history_update_literal_pixels() {
    let mut f = Fixture::new();
    f.trace.borrow_mut().mode = 1;
    let first = f.run(&f.raw.to_string()).unwrap();
    assert_eq!(first.status, "deferred-timeout");
    f.trace.borrow_mut().mode = 0;
    let next = f.run(&f.raw.to_string()).unwrap();
    assert_eq!(next.status, "presented");
    assert_eq!(first.work_id, next.work_id);
    for (op, payload, color) in [
        (
            "command.document.object.fill.set",
            fill(1),
            [255, 0, 0, 255],
        ),
        (
            "command.document.object.undo",
            json!({"command":"object.undo"}),
            [0, 0, 255, 255],
        ),
        (
            "command.document.object.redo",
            json!({"command":"object.redo"}),
            [255, 0, 0, 255],
        ),
    ] {
        let response = f
            .state
            .serialize_object_file_snapshot(&f.request(op, payload))
            .unwrap();
        assert!(response.ok);
        assert!(f.run(&f.raw.to_string()).is_err());
        f.refresh();
        f.trace.borrow_mut().color = color;
        assert_eq!(f.run(&f.raw.to_string()).unwrap().status, "presented");
    }
    assert_eq!(f.creations.get(), 1);
    let released = f.release();
    assert!(released.reentry_available);
    assert_eq!(f.trace.borrow().dispositions, 1);
    assert!(f.run(&f.raw.to_string()).is_err());
    f.state
        .install_native_object(
            decode_storage_project(&serde_json::to_vec(&document()).unwrap()).unwrap(),
        )
        .unwrap();
    let old = f.raw.clone();
    f.refresh();
    assert!(f.run(&old.to_string()).is_err());
    f.trace.borrow_mut().color = [0, 0, 255, 255];
    f.run(&f.raw.to_string()).unwrap();
    assert_eq!(f.creations.get(), 2);
    f.release();
    assert_eq!(f.trace.borrow().dispositions, 2);
}

#[test]
fn gpu_callback_unwind_retains_owner_debt_and_target_until_common_release() {
    let f = Fixture::new();
    f.trace.borrow_mut().mode = 2;
    assert!(f.run(&f.raw.to_string()).is_err());
    assert_eq!(f.creations.get(), 1);
    assert!(f.state.native_state().lock().is_ok());
    assert!(f.run(&f.raw.to_string()).is_err());
    assert_eq!(f.trace.borrow().acquisitions, 1);
    let receipt = f.release();
    assert!(receipt.reentry_available);
    assert_eq!(receipt.cancelled_preview_work_ids.len(), 1);
    assert_eq!(receipt.disposed_viewport_work_ids.len(), 1);
    assert_eq!(f.trace.borrow().dispositions, 1);
}

#[test]
fn gpu_actual_pending_ack_unsettled_and_failed_revision_barriers_never_allocate() {
    let mut f = Fixture::new();
    let binding = f
        .state
        .control_revision_for_test(json!({"action":"binding"}))
        .unwrap();
    f.state
        .control_revision_for_test(json!({"action":"subscribe","binding":binding}))
        .unwrap();
    let (response, notification) = f
        .state
        .external_object_preview_for_test(&f.request("command.document.object.fill.set", fill(1)))
        .unwrap();
    assert!(response.ok);
    let (event, mut receiver) = notification.unwrap();
    f.refresh();
    assert!(f.run(&f.raw.to_string()).is_err());
    assert_eq!(f.creations.get(), 0);
    f.state.control_revision_for_test(json!({"action":"acknowledge","subscriptionId":binding["subscriptionId"],"event":event})).unwrap();
    assert_eq!(receiver.try_recv(), Ok(()));
    assert!(f.run(&f.raw.to_string()).is_err());
    assert_eq!(f.creations.get(), 0);
    assert!(f.state.settle_object_preview_for_test(event, true));
    f.trace.borrow_mut().color = [255, 0, 0, 255];
    f.run(&f.raw.to_string()).unwrap();
    let (response, notification) = f
        .state
        .external_object_preview_for_test(&f.request("command.document.object.fill.set", fill(0)))
        .unwrap();
    assert!(response.ok);
    assert!(notification.is_some());
    f.refresh();
    f.state
        .control_revision_for_test(
            json!({"action":"disconnect","subscriptionId":binding["subscriptionId"]}),
        )
        .unwrap();
    assert!(f.run(&f.raw.to_string()).is_err());
    assert_eq!(f.creations.get(), 1);
    assert_eq!(f.trace.borrow().acquisitions, 1);
    f.release();
}

#[test]
fn gpu_existing_other_owner_wrong_instance_and_stale_slot_never_run_factory() {
    let slot = TestObjectSlot::external("main".into(), ());
    for (instance, generation) in [("main", 1), ("foreign", 1)] {
        assert!(slot
            .with_target(
                instance,
                generation,
                || panic!("existing target must not allocate"),
                |_, _| Ok(())
            )
            .is_err());
    }
    let slot = TestObjectSlot::new();
    slot.with_target(
        "main",
        1,
        || Ok(((), Compositor::new().unwrap())),
        |_, _| Ok(()),
    )
    .unwrap();
    assert!(slot
        .with_target::<()>(
            "main",
            2,
            || panic!("stale must not allocate"),
            |_, _| panic!("stale must not consume")
        )
        .is_err());
    assert!(slot
        .with_target::<()>(
            "foreign",
            1,
            || panic!("foreign must not allocate"),
            |_, _| panic!("foreign must not consume")
        )
        .is_err());
    slot.release("main", |_| vec![]).unwrap();
    assert!(slot
        .with_target::<()>(
            "main",
            2,
            || Err(host_error("unavailable", "factory failed")),
            |_, _| Ok(())
        )
        .is_err());
}

#[test]
fn failed_or_panicking_factory_callback_clears_cpu_preparation_without_gpu_or_lock_poison() {
    let f = Fixture::new();
    let request = crate::native_object_desktop_preview::admit_request(
        &f.raw.to_string(),
        f.state.instance_id(),
    )
    .unwrap();
    for _ in 0..2 {
        let failure =
            crate::native_object_desktop_preview::preview_with_target(&f.state, &request, |_| {
                Err(host_error("unavailable", "factory declined"))
            })
            .unwrap_err();
        assert_eq!(failure.message, "factory declined");
    }
    assert!(crate::native_object_desktop_preview::preview_with_target(
        &f.state,
        &request,
        |_| panic!("factory callback dropped")
    )
    .is_err());
    assert!(f.state.native_state().lock().is_ok());
    let called = std::cell::Cell::new(false);
    let failure =
        crate::native_object_desktop_preview::preview_with_target(&f.state, &request, |_| {
            called.set(true);
            Err(host_error("unavailable", "after unwind"))
        })
        .unwrap_err();
    assert!(called.get());
    assert_eq!(failure.message, "after unwind");
    assert_eq!(f.creations.get(), 0);
    assert_eq!(f.trace.borrow().acquisitions, 0);
}

#[test]
fn camera_view_is_strict_session_only_and_bad_modes_refuse_before_target() {
    use native_engine::desktop_viewport::ObjectPreviewView;
    let f = Fixture::new();
    let canvas = admit_request(&f.raw.to_string(), f.state.instance_id()).unwrap();
    assert_eq!(canvas.view_mode, ObjectPreviewView::Canvas);
    for invalid in [
        json!(null),
        json!(false),
        json!(1),
        json!("camera"),
        json!({"mode":"stored-camera"}),
    ] {
        let mut raw = f.raw.clone();
        raw["viewMode"] = invalid;
        assert!(f.run(&raw.to_string()).is_err());
    }
    let mut raw = f.raw.clone();
    raw["viewMode"] = json!("stored-camera");
    let admitted = admit_request(&raw.to_string(), f.state.instance_id()).unwrap();
    assert_eq!(admitted.view_mode, ObjectPreviewView::StoredCamera);
    raw["cameraMatrix"] = json!([1, 0, 0, 1, 0, 0]);
    assert!(f.run(&raw.to_string()).is_err());
    assert_eq!(f.creations.get(), 0);
}

#[test]
fn gpu_actual_camera_view_changes_painter_cache_and_returns_the_painted_inverse() {
    let drawing = document();
    let source = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24,"cameraLayerOn":true,
            "cameraKeys":[{"frame":0,"x":160,"y":90,"w":160}]},
        "drawing":drawing,"viewportReference":null,"assets":[]});
    let f = Fixture::with_document(source);
    let before = f.raw.clone();
    let canvas = f.run(&f.raw.to_string()).unwrap();
    assert_eq!(
        canvas.view.document_to_composition,
        [1., 0., 0., 1., 0., 0.]
    );
    let mut locked = f.raw.clone();
    locked["viewMode"] = json!("stored-camera");
    f.trace.borrow_mut().color = [255; 4]; // Authored blue square is outside the camera crop.
    let camera = f.run(&locked.to_string()).unwrap();
    assert_eq!(camera.status, "presented");
    assert!(camera.view.camera_available);
    assert!(canvas.view.camera_available);
    assert_ne!(canvas.work_id, camera.work_id);
    assert_eq!(
        camera.view.document_to_composition,
        [2., 0., 0., 2., -160., -90.]
    );
    assert_eq!(
        camera.view.composition_to_document,
        [0.5, 0., 0., 0.5, 80., 45.]
    );
    f.trace.borrow_mut().color = [0, 0, 255, 255];
    let unlocked = f.run(&f.raw.to_string()).unwrap();
    assert_eq!(
        unlocked.view.document_to_composition,
        [1., 0., 0., 1., 0., 0.]
    );
    assert_ne!(unlocked.work_id, camera.work_id);
    assert_eq!(f.raw, before);
    assert_eq!(f.raw["contentRevision"].as_u64(), Some(0));
    assert_eq!(f.trace.borrow().acquisitions, 3);
}

#[test]
fn viewport_navigation_refuses_non_closed_or_nonfinite_requests_before_target_creation() {
    let f = Fixture::new();
    for invalid in [
        json!(null),
        json!([]),
        json!({}),
        json!({"zoom":0,"pan":[0,0]}),
        json!({"zoom":-1,"pan":[0,0]}),
        json!({"zoom":1,"pan":[0]}),
        json!({"zoom":1,"pan":[0,null]}),
        json!({"zoom":1,"pan":[0,0],"matrix":[1,0,0,1,0,0]}),
    ] {
        let mut raw = f.raw.clone();
        raw["navigation"] = invalid;
        assert!(f.run(&raw.to_string()).is_err());
    }
    let raw = f.raw.to_string().replacen(
        "\"viewport\":",
        "\"navigation\":{\"zoom\":1e999,\"pan\":[0,0]},\"viewport\":",
        1,
    );
    assert!(f.run(&raw).is_err());
    let duplicate = f.raw.to_string().replacen(
        "\"viewport\":",
        "\"navigation\":{\"zoom\":1,\"zoom\":2,\"pan\":[0,0]},\"viewport\":",
        1,
    );
    assert!(f.run(&duplicate).is_err());
    assert_eq!(f.creations.get(), 0);
}

#[test]
fn gpu_camera_navigation_changes_real_pixels_cache_and_inverse_without_history_or_export_authority()
{
    let source = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":320,"canvasH":180,"fps":24,"cameraLayerOn":true,
            "cameraKeys":[{"frame":0,"x":160,"y":90,"w":160}]},
        "drawing":document(),"viewportReference":null,"assets":[]});
    let f = Fixture::with_document(source);
    let retained = || {
        let native = f.state.native_state();
        let guard = native.lock().unwrap();
        let (_, owner) = guard.active().unwrap();
        (
            owner.project_file_info().unwrap(),
            owner
                .project_file_snapshot(0, f.raw["documentSnapshotId"].as_str().unwrap())
                .unwrap(),
        )
    };
    let before = (f.raw.clone(), retained());
    let mut camera = f.raw.clone();
    camera["viewMode"] = json!("stored-camera");
    f.trace.borrow_mut().color = [255; 4];
    let locked = f.run(&camera.to_string()).unwrap();
    let mut manual = camera.clone();
    manual["navigation"] = json!({"zoom":0.5,"pan":[0,0]});
    // Half of the 2x camera is identity: blue [4,4]-[28,28], white [40,40].
    f.trace.borrow_mut().color = [0, 0, 255, 255];
    let navigated = f.run(&manual.to_string()).unwrap();
    assert_eq!(
        navigated.view.document_to_composition,
        [1., 0., 0., 1., 0., 0.]
    );
    assert_eq!(
        navigated.view.composition_to_document,
        [1., 0., 0., 1., 0., 0.]
    );
    assert_eq!(navigated.view.navigation.zoom(), 0.5);
    assert_eq!(navigated.view.navigation.pan(), [0., 0.]);
    assert_ne!(navigated.work_id, locked.work_id);
    let mut wrong = manual.clone();
    wrong["documentSnapshotId"] = json!("foreign");
    assert!(f.run(&wrong.to_string()).is_err());
    assert_eq!(f.trace.borrow().acquisitions, 2);
    assert_eq!((f.raw.clone(), retained()), before);
    assert_eq!(f.raw["contentRevision"].as_u64(), Some(0));
}
