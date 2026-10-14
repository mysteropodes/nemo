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

#[test]
fn gpu_public_export_retains_one_pixel_owner_through_last_clone() {
    // Global-domain accounting needs a fresh process, including parallel libtest.
    const KEY: &str = "NEMO_EXPORT_PIXEL_OWNER_TEST";
    let name = concat!(
        module_path!(),
        "::gpu_public_export_retains_one_pixel_owner_through_last_clone"
    )
    .split_once("::")
    .unwrap()
    .1;
    if std::env::var(KEY).as_deref() != Ok(name) {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--nocapture", "--test-threads=1"])
            .env(KEY, name)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
        return;
    }
    use crate::codec::decode_project;
    use crate::commands::NativeOpacityApplication;
    use crate::png_output::{validate_readback, ExportCompositor};
    use crate::render_scene::{prepare, GeometryPaintInput, LayerGeometry, OpaqueSrgbPaint};
    use crate::revision::shared_image_resource_budget;
    use crate::scheduler::{EvaluationKey, FrameScheduler, OutputSpec};

    let app = NativeOpacityApplication::new(
        "export-pixel-owner",
        decode_project(include_bytes!("fixtures/opacity-v2/project.json")).unwrap(),
    )
    .unwrap();
    let snapshot = app.acquire_snapshot(0).unwrap();
    let key = EvaluationKey::new(
        snapshot.id(),
        "scene-root",
        10,
        "final",
        OutputSpec::new("frame", "rgba8", 320, 180, "srgb", "straight").unwrap(),
        [("geometry/r08".to_owned(), "v1".to_owned())],
    )
    .unwrap();
    let mut scheduler = FrameScheduler::new();
    let frame = scheduler.schedule(snapshot, key).unwrap();
    let geometry = GeometryPaintInput::new(
        "geometry/r08",
        "v1",
        vec![LayerGeometry::new(
            "r08_curve_layer",
            [20., 60., 40., 80.],
            [1., 0., 0., 1., 0., 0.],
            OpaqueSrgbPaint::new(255, 0, 0),
        )
        .unwrap()],
    )
    .unwrap();
    let scene = prepare(&scheduler, &frame, &geometry).unwrap();
    let budget = shared_image_resource_budget();
    let mut gpu = Compositor::new().unwrap();
    let cost = 320 * 180 * 4;
    assert_eq!(budget.used().unwrap(), 0);
    let mut injected = Vec::with_capacity(9);
    injected.extend_from_slice(&[1, 2, 3]);
    let capacity = injected.capacity();
    let pointer = injected.as_ptr();
    let admitted = ReadbackBytes::try_from(injected).unwrap();
    assert_eq!(admitted.as_ptr(), pointer);
    assert_eq!(budget.used().unwrap(), capacity);
    drop(admitted);
    assert_eq!(budget.used().unwrap(), 0);
    let composed = ExportCompositor::compose(&mut gpu, &scene).unwrap();
    assert_eq!(budget.used().unwrap(), cost);
    let exported = ExportCompositor::readback_rgba8(&gpu, &composed).unwrap();
    assert_eq!(budget.used().unwrap(), 2 * cost);
    validate_readback(&exported, &scene).unwrap();
    assert_eq!(&exported.bytes[..4], &[255; 4]);
    let clone = exported.clone();
    assert_eq!(exported.bytes.as_ptr(), clone.bytes.as_ptr());
    assert_eq!(budget.used().unwrap(), 2 * cost);
    // Scalar pressure only; no large fixture allocation. A refused second
    // readback must preserve the first export and undo its partial reservation.
    let pressure = budget.reserve(384 * 1024 * 1024 - 4 * cost + 1).unwrap();
    let held = budget.used().unwrap();
    assert!(ExportCompositor::readback_rgba8(&gpu, &composed).is_err());
    assert_eq!(budget.used().unwrap(), held);
    drop(pressure);
    drop(composed);
    complete(&gpu);
    assert_eq!(budget.used().unwrap(), cost);
    drop(exported);
    assert_eq!(budget.used().unwrap(), cost);
    drop(gpu);
    validate_readback(&clone, &scene).unwrap();
    assert_eq!(&clone.bytes[..4], &[255; 4]);
    drop(clone);
    assert_eq!(budget.used().unwrap(), 0);
}

#[test]
fn adopted_pixels_charge_capacity_share_storage_and_release_after_last_owner() {
    let mut bytes = Vec::with_capacity(11);
    bytes.extend_from_slice(&[3, 5, 7]);
    let capacity = bytes.capacity();
    let pointer = bytes.as_ptr();
    let budget = ReadbackBudget::new(capacity);
    let pixels = ReadbackBytes::admit_with(bytes, |n| budget.reserve(n)).unwrap();
    assert_eq!(pixels.as_ptr(), pointer);
    assert_eq!(&*pixels, &[3, 5, 7]);
    assert_eq!(budget.used(), capacity);
    let clone = pixels.clone();
    assert_eq!(clone.as_ptr(), pointer);
    drop(pixels);
    assert_eq!(budget.used(), capacity);
    assert_eq!(&*clone, &[3, 5, 7]);
    drop(clone);
    assert_eq!(budget.used(), 0);
}

#[test]
fn adopted_pixels_refuse_capacity_pressure_without_publishing_or_leaking() {
    let mut bytes = Vec::with_capacity(11);
    bytes.push(3);
    let capacity = bytes.capacity();
    let budget = ReadbackBudget::new(capacity - 1);
    let result = ReadbackBytes::admit_with(bytes, |n| {
        assert_eq!(n, capacity, "spare capacity is retained payload");
        budget.reserve(n)
    });
    assert_eq!(
        result.unwrap_err().to_string(),
        "readback payload budget refused"
    );
    assert_eq!(budget.used(), 0);
    let result = catch_unwind(AssertUnwindSafe(|| {
        ReadbackBytes::admit_with(vec![1], |n| {
            let _held = budget.reserve(n).unwrap();
            panic!("injected admission unwind");
        })
    }));
    assert!(result.is_err());
    assert_eq!(budget.used(), 0);
}

#[test]
fn adopted_empty_pixels_skip_only_zero_capacity_admission() {
    let empty = ReadbackBytes::admit_with(Vec::new(), |_| {
        panic!("zero-capacity input has no payload allocation")
    })
    .unwrap();
    assert!(empty.is_empty());
    let bytes = Vec::with_capacity(9);
    let capacity = bytes.capacity();
    let budget = ReadbackBudget::new(capacity);
    let empty_with_storage = ReadbackBytes::admit_with(bytes, |n| budget.reserve(n)).unwrap();
    assert!(empty_with_storage.is_empty());
    assert_eq!(empty, empty_with_storage);
    assert_eq!(budget.used(), capacity);
    drop(empty_with_storage);
    assert_eq!(budget.used(), 0);
}
