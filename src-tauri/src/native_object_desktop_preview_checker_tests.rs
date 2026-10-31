//! #2373: the "Show transparency" request of a desktop preview is a strict, viewport-only view flag; the renderer paints the baseline
//! checker behind the artwork and the receipt echoes it only when asked for.
use super::*;

/// The presented 320 x 180 viewport texture, read back as RGBA8.
fn presented(source: &SurfacePresentation<'_>) -> Vec<u8> {
    let buffer = source.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("object-desktop-checker-pixels"),
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
/// The document (white background, blue square at 4..28) under the 8 px baseline checker: the 222 tile is (column + row) even.
fn checkered(source: &SurfacePresentation<'_>) {
    let bytes = presented(source);
    assert_eq!(
        at(&bytes, 8, 8),
        [0, 0, 255, 255],
        "the artwork is exactly what it was"
    );
    assert_eq!(at(&bytes, 40, 40), [222, 222, 222, 255]);
    assert_eq!(at(&bytes, 48, 40), [255; 4]);
    assert_eq!(at(&bytes, 300, 170), [222, 222, 222, 255]);
}

#[test]
fn the_checker_request_is_strict_and_the_renderer_paints_it_behind_the_artwork() {
    let f = Fixture::new();
    for bad in [json!("yes"), json!(1), json!(null), json!([true])] {
        let mut raw = f.raw.clone();
        raw["alphaChecker"] = bad;
        assert!(f.run(&raw.to_string()).is_err());
    }
    assert_eq!(
        f.creations.get(),
        0,
        "a malformed checker request never reaches the GPU"
    );
    let plain = f.run(&f.raw.to_string()).unwrap();
    assert!(!plain.alpha_checker);
    assert!(serde_json::to_value(&plain)
        .unwrap()
        .get("alphaChecker")
        .is_none());
    let mut raw = f.raw.clone();
    raw["alphaChecker"] = json!(true);
    f.trace.borrow_mut().pixel_check = Some(checkered);
    let shown = f.run(&raw.to_string()).unwrap();
    assert_eq!(shown.status, "presented");
    assert_eq!(
        serde_json::to_value(&shown).unwrap()["alphaChecker"],
        json!(true)
    );
    // The checker is part of the presentation identity: no cached plain frame is reused for it, nor the other way round.
    assert_ne!(shown.work_id, plain.work_id);
    // Asking for it twice in a row repeats the picture; switching it off restores the plain pixels.
    raw["requestId"] = json!("desktop-preview-again");
    assert_eq!(f.run(&raw.to_string()).unwrap().status, "presented");
    f.trace.borrow_mut().pixel_check = None;
    let mut off = f.raw.clone();
    off["requestId"] = json!("desktop-preview-off");
    let restored = f.run(&off.to_string()).unwrap();
    assert!(!restored.alpha_checker);
    assert_eq!(f.trace.borrow().acquisitions, 4);
}
