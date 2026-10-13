//! Actual Metal allocation/queue lifetime, with independently counted payloads.
use super::*;
use crate::render_scene::ReadbackBudget;
use std::{
    cell::Cell,
    panic::{catch_unwind, AssertUnwindSafe},
};

fn complete(gpu: &Compositor) {
    gpu.device
        .poll(wgpu::PollType::wait_indefinitely())
        .unwrap();
}
fn textures(gpu: &Compositor) -> usize {
    gpu.instance
        .generate_report()
        .unwrap()
        .hub
        .textures
        .num_allocated
}
fn target(gpu: &Compositor, budget: &ReadbackBudget) -> ObjectTarget {
    gpu.object_target_with(3, 2, |n| budget.reserve(n), |_, _| Ok(()))
        .unwrap()
}

#[test]
fn gpu_target_admission_precedes_creation_and_outputs_add_to_decoded_pixels() {
    let gpu = Compositor::new().unwrap();
    let baseline = textures(&gpu);
    for limit in [1, 23] {
        let budget = ReadbackBudget::new(limit);
        let reached = Cell::new(false);
        assert!(gpu
            .object_target_with(
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
        assert_eq!(textures(&gpu), baseline);
    }
    let budget = ReadbackBudget::new(60);
    let reference = budget.retained_reference();
    assert_eq!(
        reference.rgba8(),
        [17, 34, 51, 255, 68, 85, 102, 128, 119, 136, 153, 0]
    );
    let mut stages = Vec::new();
    let first = gpu
        .object_target_with(
            3,
            2,
            |n| budget.reserve(n),
            |stage, texture| {
                stages.push(stage);
                assert_eq!(budget.used(), 36);
                assert_eq!(textures(&gpu), baseline + usize::from(stage != "reserved"));
                if let Some(texture) = texture {
                    assert_eq!((texture.width(), texture.height()), (3, 2));
                }
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(stages, ["reserved", "texture-created", "view-created"]);
    let second = target(&gpu, &budget);
    assert_eq!(budget.used(), 60);
    assert!(gpu
        .object_target_with(1, 1, |n| budget.reserve(n), |_, _| panic!("over quota"))
        .is_err());
    assert_eq!(textures(&gpu), baseline + 2);
    drop(first);
    assert_eq!(budget.used(), 36);
    drop(second);
    assert_eq!(budget.used(), 12);
    drop(reference);
    assert_eq!(budget.used(), 0);
    assert_eq!(textures(&gpu), baseline);
    for (width, height) in [(0, 2), (3, 0), (u32::MAX, 1), (1, u32::MAX)] {
        assert!(gpu
            .object_target_with(
                width,
                height,
                |_| panic!("invalid dimensions reserved"),
                |_, _| Ok(())
            )
            .is_err());
    }
    assert!(target_bytes(u32::MAX, u32::MAX, u32::MAX).is_err());
}

#[test]
fn gpu_target_creation_failure_and_unwind_retire_before_releasing_charge() {
    let gpu = Compositor::new().unwrap();
    let baseline = textures(&gpu);
    for fail_at in ["reserved", "texture-created", "view-created"] {
        for unwind in [false, true] {
            let budget = ReadbackBudget::new(24);
            let result = catch_unwind(AssertUnwindSafe(|| {
                gpu.object_target_with(
                    3,
                    2,
                    |n| budget.reserve(n),
                    |stage, _| {
                        if stage == fail_at {
                            if unwind {
                                panic!("injected target creation unwind");
                            }
                            return Err(CompositorError::new("injected creation refusal"));
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
            assert_eq!(budget.used(), 0);
            assert_eq!(textures(&gpu), baseline);
        }
    }
}

#[test]
fn gpu_target_render_failure_unwind_and_success_wait_for_queue_completion() {
    let mut gpu = Compositor::new().unwrap();
    for stage in ["before-submit", "after-submit", "success"] {
        for unwind in [false, true] {
            let budget = ReadbackBudget::new(24);
            let result = catch_unwind(AssertUnwindSafe(
                || -> Result<ObjectTarget, CompositorError> {
                    let mut output = target(&gpu, &budget);
                    output.render(|view| {
                        if stage != "before-submit" {
                            gpu.render_target(view, 3, 2, vello::peniko::Color::WHITE)?;
                        }
                        if stage != "success" {
                            if unwind {
                                panic!("injected render unwind");
                            }
                            return Err(CompositorError::new("injected render failure"));
                        }
                        Ok(())
                    })?;
                    Ok(output)
                },
            ));
            if stage == "success" {
                drop(result.unwrap().unwrap());
            } else if unwind {
                assert!(result.is_err());
            } else {
                assert!(result.unwrap().is_err());
            }
            assert_eq!(
                budget.used(),
                24,
                "even failed rendering can have submitted work"
            );
            assert!(budget.reserve(1).is_err());
            complete(&gpu);
            assert_eq!(budget.used(), 0);
        }
    }
}

#[test]
fn gpu_target_retirement_invalidates_escaped_private_texture_and_view() {
    let gpu = Compositor::new().unwrap();
    let budget = ReadbackBudget::new(24);
    let output = target(&gpu, &budget);
    let escaped = output.texture().clone();
    let view = output.view().clone();
    drop(output);
    assert_eq!(budget.used(), 0);
    let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("destroyed-target-control"),
        size: 512,
        usage: wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let scope = gpu.device.push_error_scope(wgpu::ErrorFilter::Validation);
    let mut encoder = gpu
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &escaped,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: Some(2),
            },
        },
        wgpu::Extent3d {
            width: 3,
            height: 2,
            depth_or_array_layers: 1,
        },
    );
    gpu.queue.submit([encoder.finish()]);
    complete(&gpu);
    let error = pollster::block_on(scope.pop()).expect("escaped GPU source must be destroyed");
    assert!(error.to_string().contains("destroyed"), "{error}");
    drop(view);
    drop(escaped);
    assert_eq!(budget.used(), 0);
}

// A child runs just this test so the production shared domain is genuinely
// isolated even when an external caller uses Cargo's default parallel harness.
fn isolated_shared_test(name: &str) -> bool {
    const KEY: &str = "NEMO_TARGET_BUDGET_TEST";
    let name = name.split_once("::").unwrap().1; // libtest omits the crate name.
    if std::env::var(KEY).as_deref() == Ok(name) {
        return true;
    }
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
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("1 passed"),
        "child selected no test"
    );
    false
}

#[test]
fn gpu_completed_target_callbacks_are_serviced_before_reusing_full_quota() {
    let gpu = Compositor::new().unwrap();
    let budget = ReadbackBudget::new(24);
    let mut output = target(&gpu, &budget);
    output.render(|_| Ok(())).unwrap();
    drop(output);
    assert_eq!(budget.used(), 24);
    let next = target(&gpu, &budget);
    assert_eq!(
        budget.used(),
        24,
        "completed old charge retires before admission"
    );
    drop(next);
    assert_eq!(budget.used(), 0);
}

#[test]
fn gpu_actual_destroyed_target_render_failure_retains_charge_until_completion() {
    let mut gpu = Compositor::new().unwrap();
    let budget = ReadbackBudget::new(24);
    let result = catch_unwind(AssertUnwindSafe(|| {
        let mut output = target(&gpu, &budget);
        output.texture().destroy();
        output.render(|view| gpu.render_target(view, 3, 2, vello::peniko::Color::WHITE))
    }));
    assert!(
        result.is_err() || result.unwrap().is_err(),
        "actual invalid render must fail"
    );
    assert_eq!(budget.used(), 24);
    complete(&gpu);
    assert_eq!(budget.used(), 0);
}

#[test]
fn gpu_target_real_readback_and_decoded_pixels_share_one_production_domain() {
    if !isolated_shared_test(concat!(
        module_path!(),
        "::gpu_target_real_readback_and_decoded_pixels_share_one_production_domain"
    )) {
        return;
    }
    use crate::{
        object_codec::ImageAssetLimits, read_queries::decode_snapshot_reference,
        revision::shared_image_resource_budget,
    };
    let mut gpu = Compositor::new().unwrap();
    let fixture_budget = ReadbackBudget::new(12);
    let fixture = fixture_budget.retained_reference();
    let reference = decode_snapshot_reference(
        fixture.snapshot(),
        ImageAssetLimits::new(1024, 64, 4096, 4, 4).unwrap(),
    )
    .unwrap();
    assert_eq!(reference.rgba8(), fixture.rgba8());
    let budget = shared_image_resource_budget();
    let base = budget.used().unwrap();
    let mut output = gpu.make_object_target(3, 2).unwrap();
    assert_eq!(budget.used().unwrap(), base + 24);
    output
        .render(|view| gpu.render_target(view, 3, 2, vello::peniko::Color::WHITE))
        .unwrap();
    complete(&gpu);
    let buffers = gpu
        .instance
        .generate_report()
        .unwrap()
        .hub
        .buffers
        .num_allocated;
    // Logical reservations force tiny remaining headroom; no 384 MiB payload
    // allocation or capacity/performance acceptance is claimed by this control.
    let fill = budget
        .reserve(384 * 1024 * 1024 - budget.used().unwrap() - 535)
        .unwrap();
    assert!(gpu.readback_texture(output.texture(), 3, 2).is_err());
    assert_eq!(
        gpu.instance
            .generate_report()
            .unwrap()
            .hub
            .buffers
            .num_allocated,
        buffers
    );
    drop(fill);
    let fill = budget
        .reserve(384 * 1024 * 1024 - budget.used().unwrap() - 536)
        .unwrap();
    let pixels = gpu.readback_texture(output.texture(), 3, 2).unwrap();
    assert_eq!(&*pixels, &[255; 24]);
    drop(fill);
    assert_eq!(
        budget.used().unwrap(),
        base + 48,
        "CPU pixels and real output overlap"
    );
    drop(output);
    assert_eq!(budget.used().unwrap(), base + 48);
    complete(&gpu);
    assert_eq!(budget.used().unwrap(), base + 24);
    drop(pixels);
    assert_eq!(budget.used().unwrap(), base);
    drop(reference);
    assert_eq!(budget.used().unwrap(), base - 12);
}

#[test]
fn gpu_scheduled_target_cancellation_keeps_charge_until_result_and_queue_retire() {
    if !isolated_shared_test(concat!(
        module_path!(),
        "::gpu_scheduled_target_cancellation_keeps_charge_until_result_and_queue_retire"
    )) {
        return;
    }
    use crate::{
        compositor::{
            object_test_owner, object_test_plan, object_test_selector, schedule_test_object,
        },
        object_render_scene::prepare_scheduled_object_composition,
        revision::shared_image_resource_budget,
        scheduler::FrameScheduler,
    };
    let mut gpu = Compositor::new().unwrap();
    let owner = object_test_owner("target-charge-cancel");
    let snapshot = owner.acquire_snapshot(0).unwrap();
    let plan = object_test_plan(&snapshot, 7);
    let mut scheduler = FrameScheduler::new();
    let work = schedule_test_object(&mut scheduler, &snapshot, 7);
    let live = prepare_scheduled_object_composition(
        &scheduler,
        &work,
        &plan,
        &object_test_selector(&snapshot, 7),
    )
    .unwrap();
    let budget = shared_image_resource_budget();
    let base = budget.used().unwrap();
    let result = gpu.compose_scheduled_object(&live).unwrap();
    assert_eq!(budget.used().unwrap(), base + 64 * 64 * 4);
    let bytes = gpu.readback_scheduled_object_rgba8(&result, &live).unwrap();
    assert_eq!(bytes.bytes(&live).unwrap().len(), 64 * 64 * 4);
    assert_eq!(budget.used().unwrap(), base + 2 * 64 * 64 * 4);
    drop(live);
    scheduler.cancel(work.work_id()).unwrap();
    assert!(prepare_scheduled_object_composition(
        &scheduler,
        &work,
        &plan,
        &object_test_selector(&snapshot, 7)
    )
    .is_err());
    assert_eq!(
        budget.used().unwrap(),
        base + 2 * 64 * 64 * 4,
        "cancellation cannot revoke retained bytes"
    );
    drop(result);
    assert_eq!(budget.used().unwrap(), base + 2 * 64 * 64 * 4);
    complete(&gpu);
    assert_eq!(budget.used().unwrap(), base + 64 * 64 * 4);
    drop(bytes);
    assert_eq!(budget.used().unwrap(), base);
}
