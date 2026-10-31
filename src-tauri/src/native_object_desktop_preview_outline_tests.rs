//! #2372: the "Current frame outline" request of a desktop preview is a strict, viewport-only view flag; the renderer draws the items
//! of the presented frame as outlines and the receipt echoes it only when asked for.
use super::*;

/// The presented 320 x 180 viewport texture, read back as RGBA8.
fn presented(source: &SurfacePresentation<'_>) -> Vec<u8> {
    let buffer = source.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("object-desktop-outline-pixels"),
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
    let bytes = slice.get_mapped_range().to_vec();
    buffer.unmap();
    bytes
}
fn at(bytes: &[u8], x: usize, y: usize) -> [u8; 4] {
    bytes[y * 1280 + x * 4..][..4].try_into().unwrap()
}
/// The document (white background, blue square at 4..28) as an outline: the fill is gone and the edge carries the 235,235,240
/// stroke (half covered on each side of the edge at this scale), nothing is added elsewhere.
fn outlined(source: &SurfacePresentation<'_>) {
    let bytes = presented(source);
    assert_eq!(at(&bytes, 16, 16), [255; 4], "the fill is dropped");
    assert_eq!(at(&bytes, 100, 100), [255; 4], "nothing is added outside the shape");
    let edge = at(&bytes, 4, 16);
    assert!(
        (225..250).contains(&edge[0]) && (225..250).contains(&edge[1]) && edge[2] >= edge[0] && edge[3] == 255,
        "{edge:?}"
    );
}

#[test]
fn the_outline_request_is_strict_and_the_renderer_draws_outlines_only_when_asked() {
    let f = Fixture::new();
    for bad in [json!("yes"), json!(1), json!(null), json!([true])] {
        let mut raw = f.raw.clone();
        raw["currentOutline"] = bad;
        assert!(f.run(&raw.to_string()).is_err());
    }
    assert_eq!(f.creations.get(), 0, "a malformed outline request never reaches the GPU");
    let plain = f.run(&f.raw.to_string()).unwrap();
    assert!(!plain.current_outline);
    assert!(serde_json::to_value(&plain).unwrap().get("currentOutline").is_none());
    let mut raw = f.raw.clone();
    raw["currentOutline"] = json!(true);
    f.trace.borrow_mut().pixel_check = Some(outlined);
    let shown = f.run(&raw.to_string()).unwrap();
    assert_eq!(shown.status, "presented");
    assert_eq!(serde_json::to_value(&shown).unwrap()["currentOutline"], json!(true));
    // The outline is part of the presentation identity: no cached plain frame is reused for it, nor the other way round.
    assert_ne!(shown.work_id, plain.work_id);
    f.trace.borrow_mut().pixel_check = None;
    let mut off = f.raw.clone();
    off["requestId"] = json!("desktop-preview-off");
    let restored = f.run(&off.to_string()).unwrap();
    assert!(!restored.current_outline);
    assert_eq!(f.trace.borrow().acquisitions, 3);
}

/// Pixels are checked by the other test; this one is about the request.
fn anything(_: &SurfacePresentation<'_>) {}

#[test]
fn the_outline_composes_with_the_checker_and_leaves_exports_untouched() {
    let f = Fixture::new();
    f.trace.borrow_mut().pixel_check = Some(anything);
    let mut raw = f.raw.clone();
    raw["currentOutline"] = json!(true);
    raw["alphaChecker"] = json!(true);
    let both = f.run(&raw.to_string()).unwrap();
    assert!(both.current_outline && both.alpha_checker);
    let mut only_outline = f.raw.clone();
    only_outline["currentOutline"] = json!(true);
    only_outline["requestId"] = json!("desktop-preview-outline-only");
    let alone = f.run(&only_outline.to_string()).unwrap();
    assert_ne!(both.work_id, alone.work_id);
    // The view flags are properties of the preview request only: the export request type has no such field.
    assert!(!include_str!("native_object_desktop_export.rs").contains("current_outline"));
}
