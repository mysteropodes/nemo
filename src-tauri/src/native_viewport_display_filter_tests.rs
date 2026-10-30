//! Executes the actual production shader/final-pass dispatcher on the retained GPU context.
use super::{DisplayFilterBlitter, ViewportDisplayFilter};
use native_engine::compositor::Compositor;

const PIXELS: [u8; 32] = [
    255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 17, 34, 51, 128, 255, 255, 255, 255, 0, 0, 0,
    0, 120, 140, 160, 200, 80, 100, 120, 255,
];
const EXTENT: wgpu::Extent3d = wgpu::Extent3d {
    width: 4,
    height: 2,
    depth_or_array_layers: 1,
};

fn texture(
    gpu: &Compositor,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
) -> wgpu::Texture {
    gpu.device().create_texture(&wgpu::TextureDescriptor {
        label: Some("display-filter-test-texture"),
        size: EXTENT,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    })
}

fn read(gpu: &Compositor, texture: &wgpu::Texture, format: wgpu::TextureFormat) -> Vec<u8> {
    const ROW: u32 = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = gpu.device().create_buffer(&wgpu::BufferDescriptor {
        label: Some("display-filter-test-readback"),
        size: u64::from(ROW * 2),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device().create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(ROW),
                rows_per_image: Some(2),
            },
        },
        EXTENT,
    );
    gpu.queue().submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    gpu.device()
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(std::time::Duration::from_secs(10)),
        })
        .unwrap();
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let mapped = slice.get_mapped_range();
    let mut bytes = Vec::with_capacity(32);
    for row in 0..2 {
        let offset = (row * ROW) as usize;
        bytes.extend_from_slice(&mapped[offset..offset + 16]);
    }
    drop(mapped);
    buffer.unmap();
    if format == wgpu::TextureFormat::Bgra8Unorm {
        for pixel in bytes.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
    }
    bytes
}

#[test]
fn gpu_display_filter_presets_preserve_source_alpha_and_restore_color() {
    use wgpu::TextureFormat::{Bgra8Unorm, Rgba8Unorm};
    use ViewportDisplayFilter::{Contrast, Dim, None as Off, Values};
    let gpu = Compositor::new().unwrap();
    let errors = gpu.device().push_error_scope(wgpu::ErrorFilter::Validation);
    let source = texture(
        &gpu,
        Rgba8Unorm,
        wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
    );
    gpu.queue().write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &source,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &PIXELS,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(16),
            rows_per_image: Some(2),
        },
        EXTENT,
    );
    let source_view = source.create_view(&Default::default());
    // Independent CSS reference samples include primaries, clipped contrast,
    // non-gray midtones, partial alpha, transparent black, and orientation.
    let gray: [u8; 8] = [54, 182, 18, 32, 255, 0, 137, 97];
    let contrast: [[u8; 3]; 8] = [
        [255, 0, 0],
        [0, 255, 0],
        [0, 0, 255],
        [0, 0, 6],
        [255, 255, 255],
        [0, 0, 0],
        [113, 148, 183],
        [49, 84, 119],
    ];
    let dim: [[u8; 3]; 8] = [
        [140, 0, 0],
        [0, 140, 0],
        [0, 0, 140],
        [9, 19, 28],
        [140, 140, 140],
        [0, 0, 0],
        [66, 77, 88],
        [44, 55, 66],
    ];
    for format in [Rgba8Unorm, Bgra8Unorm] {
        let target = texture(
            &gpu,
            format,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        );
        let target_view = target.create_view(&Default::default());
        let blitter = DisplayFilterBlitter::new(gpu.device(), format);
        // Reuse target/pipelines, returning to off after every mode. This catches
        // stale filter state and accidental feedback into the compositor source.
        for filter in [Off, Values, Off, Contrast, Off, Dim, Off, Values, Off] {
            let mut encoder = gpu.device().create_command_encoder(&Default::default());
            blitter.copy(
                gpu.device(),
                &mut encoder,
                &source_view,
                &target_view,
                filter,
            );
            gpu.queue().submit(Some(encoder.finish()));
            let actual = read(&gpu, &target, format);
            if filter == Off {
                assert_eq!(actual, PIXELS, "off/color restoration {format:?}");
            } else {
                for (index, pixel) in actual.chunks_exact(4).enumerate() {
                    let expected = match filter {
                        Values => [gray[index]; 3],
                        Contrast => contrast[index],
                        Dim => dim[index],
                        _ => unreachable!("mirrored variants: see the mirror test"),
                    };
                    for channel in 0..3 {
                        assert!((i16::from(pixel[channel]) - i16::from(expected[channel])).abs() <= 1,
                            "{format:?} {filter:?} pixel {index} channel {channel}: {pixel:?} expected {expected:?}");
                    }
                    assert_eq!(
                        pixel[3],
                        PIXELS[index * 4 + 3],
                        "alpha {format:?} {filter:?}"
                    );
                    if filter == Values {
                        assert_eq!(pixel[0], pixel[1]);
                        assert_eq!(pixel[1], pixel[2]);
                    }
                }
            }
            assert_eq!(
                read(&gpu, &source, Rgba8Unorm),
                PIXELS,
                "retained source changed {filter:?}"
            );
        }
    }
    assert!(
        pollster::block_on(errors.pop()).is_none(),
        "production display-filter GPU validation"
    );
}

#[test]
fn gpu_mirror_check_flips_each_row_and_keeps_its_color_check() {
    use wgpu::TextureFormat::Rgba8Unorm;
    use ViewportDisplayFilter::{
        Contrast, ContrastMirrored, Dim, DimMirrored, Mirrored, None as Off, Values, ValuesMirrored,
    };
    let gpu = Compositor::new().unwrap();
    let errors = gpu.device().push_error_scope(wgpu::ErrorFilter::Validation);
    let source = texture(
        &gpu,
        Rgba8Unorm,
        wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC
            | wgpu::TextureUsages::COPY_DST,
    );
    gpu.queue().write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &source,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &PIXELS,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(16),
            rows_per_image: Some(2),
        },
        EXTENT,
    );
    let source_view = source.create_view(&Default::default());
    let target = texture(
        &gpu,
        Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
    );
    let target_view = target.create_view(&Default::default());
    let blitter = DisplayFilterBlitter::new(gpu.device(), Rgba8Unorm);
    let run = |filter| {
        let mut encoder = gpu.device().create_command_encoder(&Default::default());
        blitter.copy(gpu.device(), &mut encoder, &source_view, &target_view, filter);
        gpu.queue().submit(Some(encoder.finish()));
        read(&gpu, &target, Rgba8Unorm)
    };
    // Each 4-pixel row reversed, pixel by pixel: a horizontal flip about the centre.
    let flip = |bytes: Vec<u8>| -> Vec<u8> {
        bytes
            .chunks_exact(16)
            .flat_map(|row| row.chunks_exact(4).rev().flatten().copied().collect::<Vec<_>>())
            .collect()
    };
    assert_eq!(run(Mirrored), flip(PIXELS.to_vec()), "mirror alone keeps every byte");
    for (plain, mirrored) in [
        (Off, Mirrored),
        (Values, ValuesMirrored),
        (Contrast, ContrastMirrored),
        (Dim, DimMirrored),
    ] {
        let expected = flip(run(plain));
        assert_eq!(run(mirrored), expected, "{mirrored:?} is {plain:?} flipped");
    }
    assert_eq!(run(Off), PIXELS, "off restores the unflipped source");
    assert_eq!(read(&gpu, &source, Rgba8Unorm), PIXELS, "retained source unchanged");
    assert!(
        pollster::block_on(errors.pop()).is_none(),
        "production mirror GPU validation"
    );
}
