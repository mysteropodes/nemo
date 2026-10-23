//! The raster branch of the existing object compositor, preserving pinned paint order and cache ownership.
use crate::compositor::object_compositor::prepare_command;
use crate::compositor::{Compositor, CompositorError};
use crate::object_render_scene::{ordered_entries, ObjectCompositionWitness, PaintEntry};
use vello::kurbo::Affine;
/// T12: the layered path draws solid layers only, so a video is refused, never omitted.
pub(super) fn refuse_layered_video(
    plan: &crate::object_render_scene::ObjectPaintPlan,
) -> Result<(), CompositorError> {
    if plan.video_slots().is_empty() {
        return Ok(());
    }
    Err(CompositorError::new(
        "native video layered effects are unavailable",
    ))
}
impl Compositor {
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    pub(super) fn retire_raster_cache(
        &mut self,
        prepared: &ObjectCompositionWitness,
        dimensions: (u32, u32),
    ) -> Result<(), CompositorError> {
        if self.retained_object_reference.as_ref().is_some_and(|old| {
            old.has_raster_images() && !old.reference_matches_plan(prepared.plan())
        }) {
            self.retire_object_reference_for(prepared.plan(), dimensions)?;
        }
        Ok(())
    }
    pub(super) fn emit_object_paint_sequence(
        &mut self,
        prepared: &ObjectCompositionWitness,
    ) -> Result<(), CompositorError> {
        #[cfg(all(feature = "application", feature = "project_media_codec"))]
        {
            if prepared.has_scene_images() {
                self.retained_object_reference = Some(prepared.without_media());
            }
            // Vello can replace an image atlas on a solid-only pass. Every live native asset is
            // explicitly restored before use; instances of the same asset share one ImageData ID.
            for image in prepared.raster_images().chain(prepared.brush_images()) {
                self.renderer.mark_override_image_dirty(image);
            }
        }
        // Append the whole artwork after all per-item paint, preserving stroke,
        // gradient, glyph and image transforms. No affine is applied twice.
        let camera = prepared.camera_matrix();
        let clip = if camera.is_some() {
            Some(
                prepared
                    .camera_canvas_dimensions()
                    .ok_or_else(|| CompositorError::new("stored camera clip is unavailable"))?,
            )
        } else {
            None
        };
        let mut artwork = vello::Scene::new();
        let scene = if camera.is_some() {
            &mut artwork
        } else {
            &mut self.scene
        };
        for entry in ordered_entries(prepared.plan()) {
            match entry {
                PaintEntry::Contour(command) => {
                    prepare_command(command)?.draw(scene, Affine::IDENTITY, command.path())
                }
                PaintEntry::Text(source) => source.draw(scene),
                PaintEntry::Component(source) => {
                    source.prepared.draw(scene, Affine::IDENTITY, &source.path)
                }
                // T12: a video slot draws only from its decoded frame, never blank.
                PaintEntry::Video(index) => {
                    let (image, transform, opacity) = prepared
                        .video_image(index)
                        .ok_or_else(|| CompositorError::new("native video frame is unavailable"))?;
                    let brush =
                        vello::peniko::ImageBrush::new(image.clone()).multiply_alpha(opacity);
                    scene.draw_image(&brush, transform);
                }
                PaintEntry::Duplicator(source) => {
                    source.prepared.draw(scene, Affine::IDENTITY, &source.path)
                }
                #[cfg(all(feature = "application", feature = "project_media_codec"))]
                PaintEntry::Brush(index) => {
                    if let Some((image, transform, opacity)) = prepared
                        .brush_image(index)
                        .map_err(|_| CompositorError::new("admitted brush image unavailable"))?
                    {
                        let brush =
                            vello::peniko::ImageBrush::new(image.clone()).multiply_alpha(opacity);
                        scene.draw_image(&brush, transform);
                    }
                }
                PaintEntry::Raster(index) => {
                    #[cfg(all(feature = "application", feature = "project_media_codec"))]
                    {
                        if !prepared
                            .replay_raster_mesh(index, scene)
                            .map_err(CompositorError::new)?
                        {
                            let (image, transform, opacity) =
                                prepared.raster_image(index).ok_or_else(|| {
                                    CompositorError::new("native raster resource is unavailable")
                                })?;
                            let brush = vello::peniko::ImageBrush::new(image.clone())
                                .multiply_alpha(opacity);
                            scene.draw_image(&brush, transform);
                        }
                    }
                    #[cfg(not(all(feature = "application", feature = "project_media_codec")))]
                    {
                        let _ = index;
                        return Err(CompositorError::new(
                            "native raster resources require project media support",
                        ));
                    }
                }
            }
        }
        if let Some(matrix) = camera {
            let (width, height) = clip.expect("camera clip preflighted");
            self.scene.push_clip_layer(
                vello::peniko::Fill::NonZero,
                Affine::IDENTITY,
                &vello::kurbo::Rect::new(0., 0., f64::from(width), f64::from(height)),
            );
            self.scene.append(&artwork, Some(Affine::new(matrix)));
            self.scene.pop_layer();
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "application", feature = "project_media_codec"))]
mod mesh_pixels {
    use super::*;
    use crate::compositor::object_compositor::ObjectOutputSpec;
    use crate::export_job::{
        export_one_object_frame, ExportArtifact, JobStatus, ObjectHostExportRequest,
        StagedArtifactPort,
    };
    use crate::history::NativeObjectHistory;
    use crate::object_codec::decode_storage_project;
    use crate::object_document::{
        AssetId, AssetInventory, AssetKind, AssetRecord, AssetSource, FrameScopeKind,
        ProjectEnvelope, ProjectMetadata,
    };
    use crate::object_frame_packet::{prepare_object_frame, ObjectFrameSelector};
    use crate::object_render_scene::{
        prepare_object_composition, prepare_object_navigated_viewport_composition,
        prepare_object_paint_plan, prepare_object_render_scene,
        prepare_object_viewport_composition,
    };
    use crate::request_receipts::OpacityRequest;
    use crate::revision::ObjectSnapshot;
    use crate::scheduler::{FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec};
    use serde_json::{json, Value};
    use std::{io::Cursor, sync::Arc};

    #[derive(Default)]
    struct Png(Vec<u8>);
    impl StagedArtifactPort for Png {
        fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
            Ok(())
        }
        fn write_frame(&mut self, _: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
            assert_eq!(name, "frame_0001.png");
            assert!(self.0.is_empty());
            self.0.extend_from_slice(bytes);
            Ok(())
        }
        fn publish(
            &mut self,
            _: &str,
            target: &str,
            files: &[String],
        ) -> Result<ExportArtifact, String> {
            assert_eq!(files, ["frame_0001.png"]);
            assert!(self.0.starts_with(b"\x89PNG\r\n\x1a\n"));
            Ok(ExportArtifact {
                target: target.into(),
                files: files.to_vec(),
            })
        }
        fn cleanup(&mut self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }
    fn edit(owner: &mut NativeObjectHistory, id: &str, payload: Value) {
        let request: OpacityRequest = serde_json::from_value(json!({"apiVersion":2,"requestId":id,
            "instanceId":owner.instance_id(),"documentId":owner.document_id(),"expectedRevision":owner.content_revision(),
            "operation":"command.document.object.image.mesh.edit","payload":payload})).unwrap();
        let response = owner.dispatch_object_command(&request).unwrap().unwrap();
        assert_eq!(serde_json::to_value(response).unwrap()["ok"], true);
    }
    fn composition(
        snapshot: &ObjectSnapshot,
        frame: u32,
        viewport: bool,
        zoom: f64,
    ) -> ObjectCompositionWitness {
        let selector = ObjectFrameSelector {
            instance_id: snapshot.instance_id(),
            document_id: snapshot.document_id(),
            content_revision: snapshot.content_revision(),
            document_snapshot_id: snapshot.snapshot_id(),
            context_id: "scene-root",
            scope_kind: FrameScopeKind::Authored,
            frame,
        };
        let packet = prepare_object_frame(snapshot, &selector).unwrap();
        let scene = prepare_object_render_scene(&packet, &selector).unwrap();
        let plan = prepare_object_paint_plan(&scene, &selector).unwrap();
        if viewport && zoom != 1. {
            prepare_object_navigated_viewport_composition(
                &plan,
                &selector,
                None,
                false,
                zoom,
                [0., 0.],
            )
            .unwrap()
        } else if viewport {
            prepare_object_viewport_composition(&plan, &selector, None).unwrap()
        } else {
            prepare_object_composition(&plan, &selector).unwrap()
        }
    }
    fn pixels(gpu: &mut Compositor, witness: &ObjectCompositionWitness) -> Vec<u8> {
        let image = gpu
            .compose_object(
                witness,
                ObjectOutputSpec::new(1920, 1080, "white-srgb-normal-rgba8-area").unwrap(),
            )
            .unwrap();
        gpu.readback_object_rgba8(&image).unwrap().bytes().to_vec()
    }
    fn rgb(bytes: &[u8], x: usize, y: usize, expected: [u8; 3]) {
        let offset = (y * 1920 + x) * 4;
        for channel in 0..3 {
            assert!(
                bytes[offset + channel].abs_diff(expected[channel]) <= 2,
                "pixel {x},{y} channel {channel}: {} != {}",
                bytes[offset + channel],
                expected[channel]
            );
        }
    }
    #[test]
    fn original_mesh_pixels_reach_the_real_metal_viewport_compositor_and_png_at_three_frames() {
        let baseline: Value =
            serde_json::from_str(include_str!("../../tests/fixtures/mesh/project.json")).unwrap();
        let mut mesh = baseline["imageMeshes"]["im_1"].clone();
        mesh["offsets"][8] = json!([0, 0]);
        let target = |frame| {
            json!({"contextId":"scene-root","layerUid":"L0",
            "frameScope":{"kind":"authored","frame":frame},"strokeId":format!("raster-{frame}")})
        };
        let scopes: Vec<_> = (0..24).map(|frame| json!({"frameScope":{"kind":"authored","frame":frame},
            "elements":[{"layerUid":"L0","strokeId":format!("raster-{frame}"),"naturalPosition":0,"order":{}}]})).collect();
        let drawing = json!({"format":"nemo.native-object-document","formatVersion":2,"totalFrames":24,
            "layers":[{"layerUid":"L0"}],"objects":[],"items":{"raster":(0..24).map(|frame|
                json!({"target":target(frame),"item":{"assetId":"img-1","x":320,"y":180,"width":200,"height":200}})).collect::<Vec<_>>()},
            "compositionOrder":{"format":"nemo.native-object-order-plan","formatVersion":1,"documentFormatVersion":2,
                "migration":"explicit-plan-required","referencePolicy":"unresolved","contexts":[{"contextId":"scene-root",
                "layers":[{"layerUid":"L0","naturalPosition":0,"order":{}}],"scopes":scopes}]}});
        let source = AssetSource::admitted_embedded(
            "image/png".into(),
            Arc::new(include_bytes!("../../tests/fixtures/mesh/assets/image.png").to_vec()),
            1 << 20,
        )
        .unwrap();
        let asset = Arc::new(
            AssetRecord::new(
                AssetId::new("img-1".into()).unwrap(),
                "image.png".into(),
                AssetKind::Image,
                source,
            )
            .unwrap(),
        );
        let inventory = AssetInventory::new(vec![asset], Vec::new(), 16, 0).unwrap();
        let project = ProjectEnvelope::no_media(
            ProjectMetadata::new(1920, 1080, 24.).unwrap(),
            decode_storage_project(drawing.to_string().as_bytes()).unwrap(),
        )
        .with_asset_inventory(Some(inventory));
        let mut owner = NativeObjectHistory::from_admitted_project("mesh-gpu", project).unwrap();
        let mut gpu = Compositor::new().expect("the original pixel criterion requires real Metal");
        assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
        let plain = pixels(
            &mut gpu,
            &composition(&owner.acquire_snapshot(0).unwrap(), 0, true, 1.),
        );
        let plain_zoomed = pixels(
            &mut gpu,
            &composition(&owner.acquire_snapshot(0).unwrap(), 0, true, 0.75),
        );
        for (stage, action) in [
            (
                "rest",
                json!({"command":"object.image.mesh.edit","action":{"kind":"attach","target":target(13),"mesh":mesh}}),
            ),
            (
                "deformed",
                json!({"command":"object.image.mesh.edit","meshId":"im_1","action":{"kind":"setOffset","index":8,"du":0.1,"dv":0}}),
            ),
            (
                "mask",
                json!({"command":"object.image.mesh.edit","meshId":"im_1","action":{"kind":"setOffset","index":0,"du":0.25,"dv":0.25}}),
            ),
        ] {
            edit(&mut owner, stage, action);
            let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
            for frame in [0, 12, 23] {
                let viewport = composition(&snapshot, frame, true, 1.);
                assert!(
                    viewport.raster_image(0).is_none(),
                    "a native mesh cannot expose the flat-image draw"
                );
                let rendered = pixels(&mut gpu, &viewport);
                assert_eq!(rendered.len(), 1920 * 1080 * 4);
                if stage == "rest" {
                    let differing = plain
                        .chunks_exact(4)
                        .zip(rendered.chunks_exact(4))
                        .filter(|(a, b)| a != b)
                        .count();
                    assert!(
                        differing <= 52,
                        "original rest tolerance: {differing}/2073600 pixels"
                    );
                    assert!(plain
                        .iter()
                        .zip(&rendered)
                        .all(|(a, b)| a.abs_diff(*b) <= 1));
                }
                rgb(&rendered, 240, 180, [220, 30, 30]);
                rgb(&rendered, 400, 180, [30, 60, 220]);
                rgb(&rendered, 150, 180, [255, 255, 255]);
                rgb(
                    &rendered,
                    330,
                    180,
                    if stage == "rest" {
                        [30, 60, 220]
                    } else {
                        [220, 30, 30]
                    },
                );
                if stage == "mask" {
                    rgb(&rendered, 230, 90, [255, 255, 255]);
                }
                if frame == 0 {
                    let zoomed = pixels(&mut gpu, &composition(&snapshot, frame, true, 0.75));
                    if stage == "rest" {
                        let differing = plain_zoomed
                            .chunks_exact(4)
                            .zip(zoomed.chunks_exact(4))
                            .filter(|(a, b)| a != b)
                            .count();
                        assert!(differing <= 52, "zoomed rest seam tolerance: {differing}");
                        assert!(plain_zoomed
                            .iter()
                            .zip(&zoomed)
                            .all(|(a, b)| a.abs_diff(*b) <= 1));
                    }
                    // z=.75 about canvas centre (960,540), applied once.
                    rgb(&zoomed, 420, 270, [220, 30, 30]);
                    rgb(&zoomed, 540, 270, [30, 60, 220]);
                    rgb(&zoomed, 353, 270, [255, 255, 255]);
                    rgb(
                        &zoomed,
                        488,
                        270,
                        if stage == "rest" {
                            [30, 60, 220]
                        } else {
                            [220, 30, 30]
                        },
                    );
                    if stage == "mask" {
                        rgb(&zoomed, 413, 203, [255, 255, 255]);
                    }
                }
                let mut scheduler = FrameScheduler::new();
                let mut port = Png::default();
                let outcome = export_one_object_frame(
                    &owner,
                    &mut scheduler,
                    &mut port,
                    &mut gpu,
                    ObjectHostExportRequest {
                        request_id: format!("mesh-{stage}-{frame}"),
                        target: "png".into(),
                        pins: ObjectSchedulePins {
                            instance_id: snapshot.instance_id().into(),
                            document_id: snapshot.document_id().into(),
                            content_revision: snapshot.content_revision(),
                            document_snapshot_id: snapshot.snapshot_id().into(),
                            context_id: "scene-root".into(),
                            frame,
                            scope: ObjectScheduleScope::Authored,
                        },
                        output: OutputSpec::new("frame", "rgba8", 1920, 1080, "srgb", "straight")
                            .unwrap(),
                        maximum_encoded_bytes: 4 << 20,
                    },
                    composition(&snapshot, frame, false, 1.),
                );
                assert_eq!(outcome.receipt.unwrap().status, JobStatus::Succeeded);
                assert!(outcome.release.cleanup_complete);
                assert_eq!(scheduler.lease_counters().live(), 0);
                let mut decoder = png::Decoder::new(Cursor::new(port.0)).read_info().unwrap();
                let mut decoded = vec![0; decoder.output_buffer_size().unwrap()];
                let info = decoder.next_frame(&mut decoded).unwrap();
                assert_eq!((info.width, info.height), (1920, 1080));
                assert_eq!(rendered, decoded[..info.buffer_size()]);
            }
        }
    }
}
/// A layered GPU result needs its own final whole-image camera pass; do not
/// pretend that transforming only the flat source paints covers that path.
pub(super) fn check_layered_camera(
    prepared: &ObjectCompositionWitness,
) -> Result<(), CompositorError> {
    if prepared.camera_matrix().is_some() {
        return Err(CompositorError::new(
            "stored camera with layered effects is unavailable",
        ));
    }
    Ok(())
}

#[cfg(all(feature = "application", feature = "project_media_codec"))]
impl Compositor {
    /// Existing renderer/device/cache authority. Construct first: failure retains
    /// the previous renderer, scene, witness and charged immutable resources.
    pub(super) fn reset_object_image_cache(&mut self) -> Result<(), CompositorError> {
        let renderer = vello::Renderer::new(
            &self.device,
            vello::RendererOptions {
                antialiasing_support: vello::AaSupport::area_only(),
                use_cpu: true,
                ..Default::default()
            },
        )
        .map_err(|error| CompositorError::new(format!("reset Vello image cache: {error}")))?;
        self.renderer = renderer;
        self.scene.reset();
        self.retained_object_reference = None;
        self.last_reference_image = None;
        Ok(())
    }
    /// Called only on an admitted brush cache miss, before charged replacement
    /// decode/bake. An absent retained brush witness proves no brush mirrors:
    /// every normal image draw retains its complete witness until this reset.
    pub fn retire_object_brush_resources(
        &mut self,
        ids: &[String],
        dimensions: (u32, u32),
    ) -> Result<(), CompositorError> {
        crate::compositor::object_compositor::output_budget(
            dimensions.0,
            dimensions.1,
            self.device.limits().max_texture_dimension_2d,
        )?;
        if ids.is_empty() || ids.iter().any(|id| id.is_empty()) {
            return Err(CompositorError::new(
                "brush retirement requires actual resource identities",
            ));
        }
        if self
            .retained_object_reference
            .as_ref()
            .is_some_and(|old| old.plan().has_brushes())
        {
            self.reset_object_image_cache()?;
        }
        Ok(())
    }
}
