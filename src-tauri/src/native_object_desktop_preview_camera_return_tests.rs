//! Production desktop admission -> retained owner -> materialization -> common
//! compositor -> viewport acquisition. No fixed preview replies or copied kernel.
//! This does not exercise the installed WebView or replace its original oracle.
use crate::native_object_desktop_preview::tests::support::Fixture;
use native_engine::desktop_viewport::{ObjectPreviewView, SurfacePresentation};
use serde_json::{json, Value};

const CAMERA: &str = include_str!("../../tests/fixtures/p38/native-project/Camera-view.json");
const INK: [u8; 4] = [17, 34, 51, 255];
const WHITE: [u8; 4] = [255; 4];

// Fixed interior samples in composition coordinates, not values computed from
// the returned matrix. At frame 4 / 1.1 the triangle is (28,24)-(72,24)-(28,68).
// At frame 0 / 1.1 it is (94,57)-(116,57)-(94,79). Each sample changes colour.
fn pixels(source: &SurfacePresentation<'_>, expected: [[u8; 4]; 2]) {
    assert_eq!((source.texture.width(), source.texture.height()), (320, 180));
    let buffer = source.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("camera-return-original-fixture"),
        size: 1280 * 180,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = source.device.create_command_encoder(&Default::default());
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
        wgpu::Extent3d { width: 320, height: 180, depth_or_array_layers: 1 },
    );
    source.queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (send, receive) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| send.send(result).unwrap());
    source.device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receive.recv_timeout(std::time::Duration::from_secs(10)).unwrap().unwrap();
    let bytes = slice.get_mapped_range();
    for ((x, y), colour) in [(35, 30), (98, 61)].into_iter().zip(expected) {
        let offset = y * 1280 + x * 4;
        assert_eq!(&bytes[offset..offset + 4], &colour, "fixed pixel ({x},{y})");
    }
    drop(bytes);
    buffer.unmap();
}
fn four(source: &SurfacePresentation<'_>) {
    assert_eq!(source.identity.evaluation_key().frame(), 4);
    assert_eq!(source.identity.evaluation_key().context_id(), "scene-root");
    pixels(source, [INK, WHITE]);
}
fn first(source: &SurfacePresentation<'_>) {
    assert_eq!(source.identity.evaluation_key().frame(), 0);
    assert_eq!(source.identity.evaluation_key().context_id(), "scene-root");
    pixels(source, [WHITE, INK]);
}
fn affine(actual: [f64; 6], expected: [f64; 6]) {
    for (a, e) in actual.into_iter().zip(expected) {
        assert!((a - e).abs() < 1e-12, "matrix component {a} != {e}");
    }
}

#[test]
fn gpu_production_camera_first_after_zoom_replaces_frame_four_pixels_on_the_same_owner() {
    assert_eq!(CAMERA.len(), 4269, "retained original fixture bytes");
    let fixture: Value = serde_json::from_str(CAMERA).unwrap();
    let f = Fixture::with_document(fixture);
    let retained = f.retained_project();
    let initial_pins = f.raw.clone();
    let mut raw = f.raw.clone();
    raw["viewMode"] = json!("stored-camera");
    raw["navigation"] = json!({"zoom":1.1,"pan":[0,0]});
    raw["frame"] = json!(4);
    f.trace.borrow_mut().pixel_check = Some(four);
    let zoomed = f.run(&raw.to_string()).unwrap();
    assert_eq!(zoomed.status, "presented");
    assert!(zoomed.view.camera_available);
    assert_eq!(zoomed.view.mode, ObjectPreviewView::StoredCamera);
    affine(zoomed.view.document_to_composition, [2.2, 0., 0., 2.2, -192., -108.]);
    assert_eq!(zoomed.view.navigation.zoom(), 1.1);
    assert_eq!(zoomed.view.navigation.pan(), [0., 0.]);
    assert_eq!(f.trace.borrow().acquisitions, 1);

    // Keep the same host, compositor, target and navigation; only the ordinary
    // preview frame/request identity changes. A foreign selector cannot acquire.
    let mut stale = raw.clone();
    stale["frame"] = json!(0);
    stale["documentSnapshotId"] = json!("foreign");
    assert!(f.run(&stale.to_string()).is_err());
    assert_eq!(f.trace.borrow().acquisitions, 1);
    raw["frame"] = json!(0);
    raw["requestId"] = json!("camera-first-after-zoom");
    f.trace.borrow_mut().pixel_check = Some(first);
    let returned = f.run(&raw.to_string()).unwrap();
    assert_eq!(returned.status, "presented");
    assert!(returned.view.camera_available);
    assert_eq!(returned.view.mode, ObjectPreviewView::StoredCamera);
    affine(returned.view.document_to_composition, [1.1, 0., 0., 1.1, -16., -9.]);
    affine(returned.view.composition_to_document,
        [10. / 11., 0., 0., 10. / 11., 160. / 11., 90. / 11.]);
    assert_eq!(returned.view.navigation.zoom(), 1.1);
    assert_eq!(returned.view.navigation.pan(), [0., 0.]);
    assert_ne!(returned.work_id, zoomed.work_id);
    assert_eq!(f.creations.get(), 1, "no fresh device or target hides a stale cache");
    assert_eq!(f.trace.borrow().acquisitions, 2);
    assert_eq!(f.retained_project(), retained, "preview changes no stored camera/history/bytes");
    assert_eq!(f.raw, initial_pins, "same revision/snapshot/lifecycle owner");

    // A later legitimate frame-4 request still reaches the common painter.
    raw["frame"] = json!(4);
    raw["requestId"] = json!("camera-next-after-first");
    f.trace.borrow_mut().pixel_check = Some(four);
    let later = f.run(&raw.to_string()).unwrap();
    assert_eq!(later.status, "presented");
    assert_eq!(later.view.mode, ObjectPreviewView::StoredCamera);
    affine(later.view.document_to_composition, [2.2, 0., 0., 2.2, -192., -108.]);
    assert_eq!(f.trace.borrow().acquisitions, 3);
    assert_eq!(f.creations.get(), 1);
    assert_eq!(f.retained_project(), retained);
    let released = f.release();
    assert!(released.reentry_available, "normal owner/target cleanup must complete");
    assert!(released.error.is_none());
    assert_eq!(f.trace.borrow().dispositions, 1);
}
