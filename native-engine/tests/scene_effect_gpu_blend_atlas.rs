//! Actual Metal atlas pressure and retained-image lifetime controls.
use super::*;

fn pressure_image(gpu: &mut crate::compositor::Compositor, red: u8) -> vello::peniko::WeakBlob<u8> {
    use std::sync::Arc;
    use vello::kurbo::Affine;
    use vello::peniko::{Blob, Color, ImageAlphaType, ImageBrush, ImageData, ImageFormat};
    let mut bytes = vec![0; 1024 * 1024 * 4];
    for pixel in bytes.chunks_exact_mut(4) {
        pixel.copy_from_slice(&[red, 0, 0, 255]);
    }
    let image = ImageData {
        data: Blob::new(Arc::new(bytes)),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::Alpha,
        width: 1024,
        height: 1024,
    };
    let weak = image.data.downgrade();
    gpu.scene.reset();
    gpu.scene
        .draw_image(&ImageBrush::new(image.clone()), Affine::scale(1.0 / 128.0));
    let mut target = gpu.make_object_target(8, 8).unwrap();
    target
        .render(|view| gpu.render_target(view, 8, 8, Color::TRANSPARENT))
        .unwrap();
    let pixels = gpu.readback_texture(target.texture(), 8, 8).unwrap();
    for pixel in pixels.chunks_exact(4) {
        assert_eq!(
            pixel,
            &[red, 0, 0, 255],
            "pressure image itself must be present"
        );
    }
    gpu.scene.reset();
    drop(image);
    weak
}

#[test]
fn gpu_unique_rgb_pixels_survive_real_atlas_pressure_and_old_blobs_retire() {
    use super::super::support::{complete, textures};
    use vello::kurbo::{Rect, Shape};
    let mut gpu = super::super::support::headless();
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    let paths: Vec<_> = (0..SIDE)
        .flat_map(|y| {
            (0..SIDE).map(move |x| {
                Rect::new(x as f64, y as f64, (x + 1) as f64, (y + 1) as f64).to_path(0.1)
            })
        })
        .collect();
    let first = pressure_image(&mut gpu, 211);
    let mut warm_textures = None;
    for pass in 0..20_u8 {
        let pressure = pressure_image(&mut gpu, pass + 1);
        let colors: Vec<_> = (0..SIDE)
            .flat_map(|y| {
                (0..SIDE).map(move |x| {
                    [
                        (31 * x) as f32 / 255.0,
                        (29 * y) as f32 / 255.0,
                        pass as f32 / 255.0,
                        1.0,
                    ]
                })
            })
            .collect();
        let actual = isolated_paint(&mut gpu, &paths, &colors);
        for y in 0..SIDE {
            for x in 0..SIDE {
                assert_pixel(&actual, x, y, [(31 * x) as u8, (29 * y) as u8, pass, 255]);
            }
        }
        complete(&gpu);
        assert!(
            pressure.upgrade().is_some(),
            "current atlas image must remain retained"
        );
        // Four full 1024-square generations plus swatches force pressure in
        // Vello's initial1024/grown2048 atlas. Observe, do not simulate eviction.
        if pass == 5 {
            warm_textures = Some(textures(&gpu));
        }
        if pass > 5 {
            assert_eq!(
                textures(&gpu),
                warm_textures.unwrap(),
                "texture allocation count must plateau"
            );
            assert!(
                first.upgrade().is_none(),
                "old Blob must actually retire under pressure"
            );
        }
    }
    // This measures actual Blob reachability and texture count, not a claim
    // about private atlas byte residency or native project admission.
}

#[test]
fn gpu_renderer_drop_releases_actual_retained_swatch_blob_after_completed_work() {
    use vello::kurbo::{Rect, Shape};
    use vello::peniko::Color;
    let mut gpu = super::super::support::headless();
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    let path = Rect::new(0.0, 0.0, 8.0, 8.0).to_path(0.1);
    let paint = super::super::GpuLayerPaint {
        fills: vec![super::super::GpuFill {
            path: &path,
            rgba: [0.2, 0.6, 0.4, 1.0],
            prepared: None,
        }],
    };
    let swatches = super::super::encode_straight_paints(&mut gpu.scene, &[&paint]);
    let weak = swatches.values().next().unwrap().data.downgrade();
    let mut target = gpu.make_object_target(8, 8).unwrap();
    target
        .render(|view| gpu.render_target(view, 8, 8, Color::TRANSPARENT))
        .unwrap();
    let pixels = gpu.readback_texture(target.texture(), 8, 8).unwrap();
    for y in 0..SIDE {
        for x in 0..SIDE {
            assert_pixel(&pixels, x, y, [51, 153, 102, 255]);
        }
    }
    gpu.scene.reset();
    drop(swatches);
    drop(target);
    super::super::support::complete(&gpu);
    assert!(
        weak.upgrade().is_some(),
        "renderer atlas, not our map/scene, retains the actual Blob"
    );
    drop(gpu);
    assert!(
        weak.upgrade().is_none(),
        "renderer disposal must release its actual CPU Blob"
    );
}

#[cfg(all(feature = "application", feature = "project_media_codec"))]
#[test]
fn gpu_max_native_reference_atlas_then_layered_colors_retire_without_missing_pixels() {
    use crate::compositor::object_compositor::ObjectOutputSpec;
    use crate::compositor::{object_test_owner, object_test_plan, object_test_selector};
    use crate::history::NativeObjectHistory;
    use crate::object_document::FrameScopeKind;
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
    use crate::object_render_scene::scene_effect_kernels::scene_effect_order::{
        plan_composition, LayerComposition,
    };
    use crate::object_render_scene::{
        prepare_object_composition, prepare_object_paint_plan, prepare_object_render_scene,
        prepare_object_viewport_composition,
    };
    use crate::request_receipts::OpacityRequest;
    use serde_json::json;
    use std::io::Write;
    let mut encoded = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut encoded, 8192, 8192);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_compression(png::Compression::Balanced);
        encoder.set_filter(png::Filter::NoFilter);
        let mut writer = encoder.write_header().unwrap();
        let mut stream = writer.stream_writer().unwrap();
        let row = [0, 0, 255, 255].repeat(8192);
        for _ in 0..8192 {
            stream.write_all(&row).unwrap();
        }
        stream.finish().unwrap();
        writer.finish().unwrap();
    }
    assert!(encoded.len() <= 16 * 1024 * 1024);
    let mut owner =
        NativeObjectHistory::new_blank_project("max-reference-atlas", 64, 64, 24.0).unwrap();
    let request = OpacityRequest::history_stage(
        "atlas-ref",
        owner.instance_id(),
        owner.document_id(),
        Some(0),
        "command.document.reference.attach",
        json!({"id":"atlas-max","name":"blue.png"}),
    );
    assert!(owner
        .dispatch_reference_command(&request, Some(&encoded))
        .unwrap()
        .is_ok());
    assert_eq!(owner.content_revision(), 1);
    // Native attachment defaults to 50%; this pressure fixture requires opaque blue.
    let settings = OpacityRequest::history_stage(
        "atlas-ref-opaque",
        owner.instance_id(),
        owner.document_id(),
        Some(owner.content_revision()),
        "command.document.reference.settings",
        json!({"name":"blue.png","visible":true,"opacity":1.0,"offsetFrames":0}),
    );
    let response = owner.dispatch_reference_command(&settings, None).unwrap();
    assert!(response.is_ok());
    assert_eq!(response.content_revision(), 2);
    assert_eq!(owner.content_revision(), 2);
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let pins = ObjectFrameSelector {
        instance_id: snapshot.instance_id(),
        document_id: snapshot.document_id(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id(),
        context_id: "scene-root",
        scope_kind: FrameScopeKind::Authored,
        frame: 0,
    };
    let packet = prepare_object_frame(&snapshot, &pins).unwrap();
    let scene = prepare_object_render_scene(&packet, &pins).unwrap();
    let plan = prepare_object_paint_plan(&scene, &pins).unwrap();
    let reference = prepare_object_viewport_composition(&plan, &pins, None).unwrap();
    let (image, _, opacity) = reference.reference_image().unwrap();
    assert_eq!(opacity, 1.0);
    assert_eq!((image.width, image.height), (8192, 8192));
    let weak = image.data.downgrade();
    let output = || ObjectOutputSpec::new(64, 64, "white-srgb-normal-rgba8-area").unwrap();
    let mut gpu = super::super::support::headless();
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    let old = gpu.compose_object(&reference, output()).unwrap();
    let old_pixels = gpu.readback_object_rgba8(&old).unwrap();
    for pixel in old_pixels.bytes().chunks_exact(4) {
        assert_eq!(pixel, &[0, 0, 255, 255]);
    }
    let next_owner = object_test_owner("after-max-reference");
    let next_snapshot = next_owner.acquire_snapshot(0).unwrap();
    let next_plan = object_test_plan(&next_snapshot, 7);
    let witness =
        prepare_object_composition(&next_plan, &object_test_selector(&next_snapshot, 7)).unwrap();
    let runs = super::super::layer_runs_from_plan(&next_plan).unwrap();
    assert_eq!(runs.len(), 3);
    let mut compositions = vec![LayerComposition::default(); runs.len()];
    compositions.push(LayerComposition {
        is_effect_layer: Some(true),
        ..Default::default()
    });
    let mut inputs: Vec<_> = runs
        .iter()
        .map(|run| super::super::GpuLayerInput {
            paint: run.paint.clone(),
            ..Default::default()
        })
        .collect();
    inputs.push(super::super::GpuLayerInput::default());
    let stack =
        super::super::GpuLayerStack::from_plan(&plan_composition(&compositions), inputs).unwrap();
    let fresh = gpu
        .compose_object_layered(&witness, output(), &stack)
        .unwrap();
    let pixels = gpu.readback_object_rgba8(&fresh).unwrap();
    // Original D2 source-over literals, not a rendering-derived expected image.
    let at = (8 * 64 + 8) * 4;
    for (actual, expected) in pixels.bytes()[at..at + 4].iter().zip([64, 96, 159, 255]) {
        assert!(
            actual.abs_diff(expected) <= 1,
            "a missing isolated layer must not pass"
        );
    }
    assert_eq!(
        &pixels.bytes()[(40 * 64 + 40) * 4..(40 * 64 + 40) * 4 + 4],
        &[255; 4]
    );
    assert_eq!(
        gpu.readback_object_rgba8(&old).unwrap().bytes(),
        old_pixels.bytes(),
        "old target stays usable"
    );
    assert!(
        weak.upgrade().is_some(),
        "old witness still legitimately retains the reference"
    );
    drop(old_pixels);
    drop(old);
    drop(reference);
    drop(plan);
    drop(scene);
    drop(packet);
    drop(snapshot);
    drop(owner);
    super::super::support::complete(&gpu);
    assert!(
        weak.upgrade().is_none(),
        "retired atlas must no longer pin an otherwise dropped native image"
    );
}
