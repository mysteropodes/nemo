//! Real Metal copies, original allocation admission and completion-owned cleanup.
use super::*;
use crate::render_scene::ReadbackBudget;
use std::{
    cell::Cell,
    panic::{catch_unwind, AssertUnwindSafe},
};

const PIXELS: [u8; 24] = [
    255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 0, 3, 7, 11, 19, 23, 31, 47, 63, 127, 191, 223, 251,
];
fn source(gpu: &Compositor) -> wgpu::Texture {
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("literal-padded-readback-source"),
        size: wgpu::Extent3d {
            width: 3,
            height: 2,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    gpu.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &PIXELS,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(12),
            rows_per_image: Some(2),
        },
        wgpu::Extent3d {
            width: 3,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([]);
    complete(gpu);
    texture
}
fn complete(gpu: &Compositor) {
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
}
fn buffers(gpu: &Compositor) -> usize {
    gpu.instance
        .generate_report()
        .unwrap()
        .hub
        .buffers
        .num_allocated
}

#[test]
fn gpu_padded_readback_holds_overlap_and_shares_exact_pixels_until_last_clone() {
    let gpu = Compositor::new().unwrap();
    let texture = source(&gpu);
    let baseline = buffers(&gpu);
    let budget = ReadbackBudget::new(548);
    let held = budget.retained_reference();
    assert_eq!(
        held.rgba8(),
        [17, 34, 51, 255, 68, 85, 102, 128, 119, 136, 153, 0]
    );
    let mut stages = Vec::new();
    let result = gpu
        .readback_with(
            &texture,
            3,
            2,
            |n| budget.reserve(n),
            |stage, buffer| {
                stages.push(stage);
                assert_eq!(
                    budget.used(),
                    548,
                    "both payloads must overlap existing CPU bytes"
                );
                if stage == "reserved" {
                    assert_eq!(
                        buffers(&gpu),
                        baseline,
                        "reservation must precede actual GPU creation"
                    );
                } else {
                    assert_eq!(buffer.unwrap().size(), 512);
                    assert_eq!(buffers(&gpu), baseline + 1);
                }
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(
        stages,
        [
            "reserved",
            "gpu-created",
            "submitted",
            "mapped",
            "packed-created",
            "packed-copied"
        ]
    );
    assert_eq!(&*result, &PIXELS);
    assert_eq!(result.0.bytes.capacity(), 24);
    assert_eq!(
        buffers(&gpu),
        baseline,
        "owned GPU handle must actually retire"
    );
    assert_eq!(budget.used(), 36);
    let a = result.clone();
    let b = a.clone();
    assert_eq!(result.as_ptr(), a.as_ptr());
    assert_eq!(result.as_ptr(), b.as_ptr());
    assert_eq!(result, a);
    drop(result);
    drop(a);
    assert_eq!(budget.used(), 36);
    assert_eq!(&*b, &PIXELS);
    drop(b);
    assert_eq!(budget.used(), 12);
    drop(held);
    assert_eq!(budget.used(), 0);
    // Exercise the production shared-domain entry as well as the isolated oracle.
    assert_eq!(&*gpu.readback_texture(&texture, 3, 2).unwrap(), &PIXELS);
}

#[test]
fn gpu_readback_quota_and_layout_refuse_before_allocating_or_publishing() {
    let gpu = Compositor::new().unwrap();
    let texture = source(&gpu);
    let baseline = buffers(&gpu);
    for limit in [511, 512, 535] {
        let budget = ReadbackBudget::new(limit);
        let reached = Cell::new(false);
        assert!(gpu
            .readback_with(
                &texture,
                3,
                2,
                |n| budget.reserve(n),
                |_, _| {
                    reached.set(true);
                    Ok(())
                }
            )
            .is_err());
        assert!(!reached.get());
        assert_eq!(budget.used(), 0);
        assert_eq!(buffers(&gpu), baseline);
    }
    for (width, height) in [(0, 2), (3, 0), (u32::MAX, 1), (u32::MAX / 4, 1)] {
        let reached = Cell::new(false);
        let budget = ReadbackBudget::new(536);
        assert!(gpu
            .readback_with(
                &texture,
                width,
                height,
                |n| {
                    reached.set(true);
                    budget.reserve(n)
                },
                |_, _| Ok(())
            )
            .is_err());
        assert!(!reached.get(), "overflow must refuse before reservations");
        assert_eq!(buffers(&gpu), baseline);
    }
}

#[test]
fn gpu_readback_error_and_unwind_keep_submitted_buffer_until_queue_completion() {
    let gpu = Compositor::new().unwrap();
    let texture = source(&gpu);
    let baseline = buffers(&gpu);
    for stage in [
        "reserved",
        "gpu-created",
        "submitted",
        "mapped",
        "packed-created",
        "packed-copied",
    ] {
        for unwind in [false, true] {
            let budget = ReadbackBudget::new(536);
            let result = catch_unwind(AssertUnwindSafe(|| {
                gpu.readback_with(
                    &texture,
                    3,
                    2,
                    |n| budget.reserve(n),
                    |current, _| {
                        if current == stage {
                            if unwind {
                                panic!("injected readback unwind");
                            }
                            return Err(CompositorError::new("injected readback refusal"));
                        }
                        Ok(())
                    },
                )
            }));
            if unwind {
                assert!(result.is_err());
            } else {
                assert!(result.unwrap().is_err());
            }
            if stage == "submitted" {
                // The caller and packed reservation are gone. A queued cleanup
                // closure still owns the real buffer and its original charge.
                assert_eq!(budget.used(), 512);
                assert_eq!(buffers(&gpu), baseline + 1);
            } else {
                assert_eq!(budget.used(), 0);
            }
            complete(&gpu);
            assert_eq!(budget.used(), 0);
            assert_eq!(buffers(&gpu), baseline);
        }
    }
}

#[test]
fn gpu_readback_actual_map_failure_keeps_no_pixels_or_reusable_early_charge() {
    let gpu = Compositor::new().unwrap();
    let texture = source(&gpu);
    let baseline = buffers(&gpu);
    let budget = ReadbackBudget::new(536);
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let result = gpu.readback_with(
        &texture,
        3,
        2,
        |n| budget.reserve(n),
        |stage, buffer| {
            if stage == "submitted" {
                buffer.unwrap().destroy();
            }
            Ok(())
        },
    );
    assert!(result.is_err());
    complete(&gpu);
    assert!(pollster::block_on(scope.pop()).is_some());
    assert_eq!(budget.used(), 0);
    assert_eq!(buffers(&gpu), baseline);
}

#[test]
fn gpu_readback_actual_copy_failure_cleans_up_without_publishing() {
    let gpu = Compositor::new().unwrap();
    let texture = source(&gpu);
    let baseline = buffers(&gpu);
    let budget = ReadbackBudget::new(536);
    texture.destroy();
    // The real copy/submit validation path reports a destroyed source through
    // wgpu's uncaptured-error panic, rather than a synthetic checkpoint error.
    let result = catch_unwind(AssertUnwindSafe(|| {
        gpu.readback_with(&texture, 3, 2, |n| budget.reserve(n), |_, _| Ok(()))
    }));
    assert!(result.is_err(), "a failed GPU copy cannot publish pixels");
    complete(&gpu);
    assert_eq!(budget.used(), 0);
    assert_eq!(buffers(&gpu), baseline);
}
