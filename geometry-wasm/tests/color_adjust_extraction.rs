// Fixed native WebGPU oracle for the existing adjustment-layer pass.
// Kept as a small integration fixture so its expected RGBA8 bytes can be
// recorded before and after the production extraction.

use std::sync::mpsc;
use vello::wgpu;

const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const WIDTH: u32 = 4;
const HEIGHT: u32 = 1;
const INPUT: [[u8; 4]; WIDTH as usize] = [
    [0, 0, 0, 0],
    [50, 20, 5, 64],
    [60, 70, 90, 128],
    [20, 210, 250, 255],
];
const BLEND_SCRATCH_FORMAT: wgpu::TextureFormat = FORMAT;

#[path = "../src/engine/color_adjust.rs"]
mod color_adjust;

struct Fixture {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniform: wgpu::Buffer,
    adapter_info: wgpu::AdapterInfo,
}

fn setup() -> Fixture {
    pollster::block_on(async {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions::default())
            .await
            .expect("no native GPU adapter available for color adjustment fixture");
        let adapter_info = adapter.get_info();
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                required_limits: adapter.limits(),
                ..Default::default()
            })
            .await
            .expect("request_device failed");
        let (pipeline, layout, sampler, uniform) =
            color_adjust::create_color_adjust_pipeline(&device);
        Fixture {
            device,
            queue,
            pipeline,
            layout,
            sampler,
            uniform,
            adapter_info,
        }
    })
}

fn render(f: &Fixture, brightness: f32, contrast: f32) -> Vec<[u8; 4]> {
    let input = f.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("color-adjust-input"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let mut input_bytes = [0u8; (WIDTH * HEIGHT * 4) as usize];
    for (i, channel) in INPUT.iter().flatten().enumerate() {
        input_bytes[i] = *channel;
    }
    f.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &input,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &input_bytes,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(WIDTH * 4),
            rows_per_image: Some(HEIGHT),
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    let output = f.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("color-adjust-output"),
        size: wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let input_view = input.create_view(&Default::default());
    let output_view = output.create_view(&Default::default());
    let row_bytes = WIDTH * 4;
    let padded_row_bytes =
        row_bytes.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let readback = f.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("color-adjust-readback"),
        size: (padded_row_bytes * HEIGHT) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    color_adjust::color_adjust_pass(
        &f.device,
        &f.queue,
        &f.pipeline,
        &f.layout,
        &f.sampler,
        &f.uniform,
        &input_view,
        brightness,
        contrast,
        &output_view,
    );
    let mut encoder = f
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("color-adjust-readback"),
        });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &output,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row_bytes),
                rows_per_image: Some(HEIGHT),
            },
        },
        wgpu::Extent3d {
            width: WIDTH,
            height: HEIGHT,
            depth_or_array_layers: 1,
        },
    );
    f.queue.submit(Some(encoder.finish()));
    let slice = readback.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        tx.send(result).unwrap();
    });
    f.device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("GPU poll failed");
    rx.recv().unwrap().expect("readback map failed");
    let mapped = slice.get_mapped_range();
    let pixels = mapped
        .chunks_exact(padded_row_bytes as usize)
        .next()
        .unwrap()
        .chunks_exact(4)
        .take(WIDTH as usize)
        .map(|px| [px[0], px[1], px[2], px[3]])
        .collect();
    drop(mapped);
    readback.unmap();
    pixels
}

fn assert_pixels_close(actual: &[[u8; 4]], expected: &[[u8; 4]]) {
    assert_eq!(actual.len(), expected.len());
    for (pixel_index, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        for channel in 0..4 {
            assert!(
                actual[channel].abs_diff(expected[channel]) <= 1,
                "pixel {pixel_index} channel {channel}: actual {}, expected {} ±1",
                actual[channel],
                expected[channel]
            );
        }
        assert_eq!(
            actual[3], expected[3],
            "alpha changed at pixel {pixel_index}"
        );
    }
}

#[test]
fn color_adjust_fixed_rgba8_oracle() {
    let fixture = setup();
    let adjusted = render(&fixture, 0.125, 0.5);
    let neutral = render(&fixture, 0.0, 0.0);
    println!("adapter={:?}", fixture.adapter_info);
    println!("adjusted={adjusted:?}");
    println!("neutral={neutral:?}");
    assert_pixels_close(
        &adjusted,
        &[
            [0, 0, 0, 0],
            [64, 22, 0, 64],
            [74, 89, 119, 128],
            [0, 255, 255, 255],
        ],
    );
    assert_pixels_close(
        &neutral,
        &[
            [0, 0, 0, 0],
            [50, 20, 5, 64],
            [60, 70, 90, 128],
            [20, 210, 250, 255],
        ],
    );
}
