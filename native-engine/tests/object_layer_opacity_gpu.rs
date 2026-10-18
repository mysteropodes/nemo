//! Real Metal viewport texture versus owner-job PNG, not an installed surface claim.
//! Authored UNRUN. No fallback, ignored test, synthetic pixels, or new renderer.
use super::{alpha, close, owner, pins, plan, source};
use crate::compositor::Compositor;
use crate::desktop_viewport::{
    AcquiredSurfaceFrame, CssBounds, DesktopViewportHost, ObjectViewportFrame, PhysicalExtent,
    SettledSurfaceFrame, SurfaceAttempt, SurfacePort, SurfacePresentation, SurfaceRecoveryError,
    ViewportMapping, ViewportStatus,
};
use crate::export_job::{
    export_one_object_frame, ExportArtifact, JobStatus, ObjectHostExportRequest, StagedArtifactPort,
};
use crate::history::NativeObjectHistory;
use crate::object_codec::encode_storage_project;
use crate::object_render_scene::{
    prepare_object_composition, prepare_object_viewport_composition,
    prepare_scheduled_object_composition_from_witness, ObjectCompositionWitness,
};
use crate::render_scene::ScheduledFrameIdentity;
use crate::request_receipts::OpacityRequest;
use crate::revision::ObjectSnapshot;
use crate::scheduler::{
    EvaluationKey, FrameScheduler, ObjectSchedulePins, ObjectScheduleScope, OutputSpec,
};
use serde_json::{json, Value};
use std::{cell::RefCell, io::Cursor, rc::Rc, sync::mpsc, time::Duration};

const SIZE: u32 = 64;
const BLUE: &str = "89504e470d0a1a0a0000000d4948445200000002000000010806000000f4227f8a0000000d49444154789c636060f80f060014f405fbb956037d0000000049454e44ae426082";

fn metal() -> Compositor {
    let gpu = Compositor::new().expect("raw opacity requires real Metal, never a skipped pass");
    assert_eq!(gpu.adapter().get_info().backend, wgpu::Backend::Metal);
    gpu
}
fn output() -> OutputSpec {
    OutputSpec::new("frame", "rgba8", SIZE, SIZE, "srgb", "straight").unwrap()
}
fn scheduled(s: &ObjectSnapshot, frame: u32) -> ObjectSchedulePins {
    ObjectSchedulePins {
        instance_id: s.instance_id().into(),
        document_id: s.document_id().into(),
        content_revision: s.content_revision(),
        document_snapshot_id: s.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame,
        scope: ObjectScheduleScope::Authored,
    }
}

#[derive(Default)]
struct Capture {
    identity: Option<ScheduledFrameIdentity>,
    pixels: Vec<u8>,
    settled: usize,
}
struct TexturePort(Rc<RefCell<Capture>>);
struct TextureFrame(Rc<RefCell<Capture>>);
impl AcquiredSurfaceFrame for TextureFrame {
    fn settle(self) -> SettledSurfaceFrame {
        self.0.borrow_mut().settled += 1;
        SettledSurfaceFrame::Presented
    }
}
impl SurfacePort for TexturePort {
    type Frame<'port, 'source> = TextureFrame;
    fn acquire<'port, 'source>(
        &'port mut self,
        source: SurfacePresentation<'source>,
    ) -> SurfaceAttempt<Self::Frame<'port, 'source>> {
        assert_eq!(
            source.source_extent,
            PhysicalExtent {
                width: SIZE,
                height: SIZE
            }
        );
        assert_eq!(source.surface_extent, source.source_extent);
        let mut captured = self.0.borrow_mut();
        assert!(
            captured.identity.is_none(),
            "one actual surface acquisition"
        );
        captured.pixels = read_texture(source.device, source.queue, source.texture);
        captured.identity = Some(source.identity.clone());
        SurfaceAttempt::Acquired(TextureFrame(self.0.clone()))
    }
    fn recreate(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        panic!("GPU loss is a test failure, not successful recovery")
    }
    fn reconfigure(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        panic!("unexpected surface reconfiguration")
    }
    fn dispose(&mut self) {}
    fn simulated_acquire<'port>(
        &'port mut self,
        _: &ScheduledFrameIdentity,
    ) -> SurfaceAttempt<Self::Frame<'port, 'port>> {
        panic!("opacity pixels must come from the actual Metal texture")
    }
}

// Same copy/map sequence as existing viewport_scene_support, for the texture
// actually handed to SurfacePort. At 64 pixels RGBA8, each row is aligned.
fn read_texture(device: &wgpu::Device, queue: &wgpu::Queue, texture: &wgpu::Texture) -> Vec<u8> {
    let row = SIZE * 4;
    assert_eq!(row % wgpu::COPY_BYTES_PER_ROW_ALIGNMENT, 0);
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("opacity-presented-readback"),
        size: u64::from(row * SIZE),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("opacity-presented-copy"),
    });
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
                bytes_per_row: Some(row),
                rows_per_image: Some(SIZE),
            },
        },
        wgpu::Extent3d {
            width: SIZE,
            height: SIZE,
            depth_or_array_layers: 1,
        },
    );
    queue.submit(Some(encoder.finish()));
    let (sender, receiver) = mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |mapped| {
            let _ = sender.send(mapped.map_err(|error| error.to_string()));
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver
        .recv_timeout(Duration::from_secs(10))
        .unwrap()
        .unwrap();
    let mapped = buffer.slice(..).get_mapped_range();
    let pixels = mapped.to_vec();
    drop(mapped);
    buffer.unmap();
    assert_eq!(pixels.len(), (SIZE * SIZE * 4) as usize);
    pixels
}

fn presented(gpu: &mut Compositor, s: &ObjectSnapshot, w: &ObjectCompositionWitness) -> Vec<u8> {
    let mut scheduler = FrameScheduler::new();
    let resources = w
        .scene_resources()
        .map(|set| set.declared_versions())
        .unwrap_or_default();
    let lease_count = 1 + resources.len() as u64;
    let key = EvaluationKey::new(
        s.snapshot_id(),
        "scene-root",
        w.frame(),
        "final",
        output(),
        resources,
    )
    .unwrap();
    let work = scheduler
        .schedule_object(s.clone(), scheduled(s, w.frame()), key)
        .unwrap();
    let live = prepare_scheduled_object_composition_from_witness(&scheduler, &work, w).unwrap();
    let frame = ObjectViewportFrame::compose(gpu, &live).unwrap();
    let captured = Rc::new(RefCell::new(Capture::default()));
    let extent = PhysicalExtent {
        width: SIZE,
        height: SIZE,
    };
    let mapping = ViewportMapping::new(
        CssBounds {
            x: 0.0,
            y: 0.0,
            width: 64.0,
            height: 64.0,
        },
        extent,
        extent,
        1.0,
    )
    .unwrap();
    let mut host = DesktopViewportHost::new(TexturePort(captured.clone()), mapping);
    host.register(frame.identity().clone()).unwrap();
    let receipt = frame.present(&mut host, gpu, &live).unwrap();
    assert_eq!(receipt.status(), &ViewportStatus::Presented);
    assert_eq!(receipt.identity(), frame.identity());
    assert_eq!(captured.borrow().identity.as_ref(), Some(frame.identity()));
    assert_eq!(captured.borrow().settled, 1);
    drop(live);
    scheduler.succeed(work.work_id()).unwrap();
    let counters = scheduler.lease_counters();
    assert_eq!(
        (counters.acquired(), counters.released(), counters.live()),
        (lease_count, lease_count, 0)
    );
    let pixels = captured.borrow().pixels.clone();
    pixels
}

#[derive(Default)]
struct ArtifactPort {
    events: Vec<&'static str>,
    bytes: Vec<u8>,
}
impl StagedArtifactPort for ArtifactPort {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        self.events.push("begin");
        Ok(())
    }
    fn write_frame(&mut self, _: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        assert_eq!(name, "frame_0001.png");
        assert!(self.bytes.is_empty());
        self.events.push("write");
        self.bytes = bytes.to_vec();
        Ok(())
    }
    fn publish(
        &mut self,
        _: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String> {
        assert_eq!(files, ["frame_0001.png"]);
        self.events.push("publish");
        Ok(ExportArtifact {
            target: target.into(),
            files: files.to_vec(),
        })
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        self.events.push("cleanup");
        Ok(())
    }
}
fn parity(
    gpu: &mut Compositor,
    o: &NativeObjectHistory,
    s: &ObjectSnapshot,
    frame: u32,
    prior: Option<&ObjectCompositionWitness>,
) -> (Vec<u8>, ObjectCompositionWitness) {
    let before = encode_storage_project(s.document()).unwrap();
    let history = o.history_depths();
    let p = plan(s, frame);
    let viewport = prepare_object_viewport_composition(&p, &pins(s, frame), prior).unwrap();
    let pixels = presented(gpu, s, &viewport);
    let export = prepare_object_composition(&p, &pins(s, frame)).unwrap();
    assert!(
        !viewport.same_identity_as(&export),
        "viewport and artifact purposes stay distinct"
    );
    assert_eq!(viewport.document_snapshot_id(), s.snapshot_id());
    assert_eq!(export.content_revision(), s.content_revision());
    let mut scheduler = FrameScheduler::new();
    let mut port = ArtifactPort::default();
    let outcome = export_one_object_frame(
        o,
        &mut scheduler,
        &mut port,
        gpu,
        ObjectHostExportRequest {
            request_id: format!("opacity-{}-{frame}", s.content_revision()),
            target: "opacity-artifact".into(),
            pins: scheduled(s, frame),
            output: output(),
            maximum_encoded_bytes: 65536,
        },
        export,
    );
    let receipt = outcome.receipt.unwrap();
    assert_eq!(receipt.status, JobStatus::Succeeded);
    assert_eq!(receipt.pinned_revision, s.content_revision());
    assert_eq!(receipt.document_snapshot_id, s.snapshot_id());
    assert_eq!(receipt.artifact.unwrap().files, ["frame_0001.png"]);
    assert!(outcome.release.cleanup_complete);
    assert_eq!(port.events, ["begin", "write", "publish"]);
    assert_eq!(scheduler.lease_counters().live(), 0);
    let mut reader = png::Decoder::new(Cursor::new(&port.bytes))
        .read_info()
        .unwrap();
    assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);
    let mut decoded = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut decoded).unwrap();
    assert_eq!((info.width, info.height), (SIZE, SIZE));
    decoded.truncate(info.buffer_size());
    assert_eq!(
        decoded, pixels,
        "actual viewport texture and independently encoded PNG"
    );
    assert_eq!(encode_storage_project(s.document()).unwrap(), before);
    assert_eq!(o.history_depths(), history);
    assert_eq!(o.content_revision(), s.content_revision());
    (pixels, viewport)
}
fn pixel(bytes: &[u8], x: usize, y: usize, expected: [u8; 4]) {
    let actual = &bytes[(y * SIZE as usize + x) * 4..][..4];
    // Existing viewport RGBA8 numeric contract: one byte quantization, interior
    // probes only. Full viewport/export comparison above has zero tolerance.
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            actual.abs_diff(expected) <= 1,
            "({x},{y}): {actual} != {expected}"
        );
    }
}
fn ink(bytes: &[u8]) {
    assert!(
        bytes.chunks_exact(4).any(|p| p != [255; 4]),
        "equal blank outputs cannot pass"
    );
}

#[test]
fn gpu_static_fifty_once_per_item_has_literal_overlap_and_real_viewport_png_parity() {
    let mut gpu = metal();
    let o = owner(&source(json!([{"property":"opacity","static":[50]}])));
    let s = o.acquire_snapshot(0).unwrap();
    let p = plan(&s, 7);
    assert_eq!(p.packet().source_frame_for_layer(0), Some(0));
    assert_eq!(p.packet().frame(), 7);
    assert_eq!(alpha(&p), [0.375, 0.375]);
    close(1.0 - (1.0 - 0.375) * (1.0 - 0.375), 0.609375);
    for command in p.commands() {
        assert_eq!(
            command.source().record().solid_fill().unwrap().a.as_f64(),
            Some(0.75)
        );
    }
    let (pixels, _) = parity(&mut gpu, &o, &s, 7, None);
    ink(&pixels);
    // Red .75 * .5 twice over white: 255 * (1 - .609375) = 99.609375.
    pixel(&pixels, 25, 30, [255, 100, 100, 255]);
    pixel(&pixels, 0, 0, [255; 4]);
    assert_eq!(o.history_depths(), (0, 0));
}

#[test]
fn gpu_linear_and_hold_use_presentation_time_while_the_drawing_stays_held() {
    let mut gpu = metal();
    for (first, cases) in [
        (
            json!({"frame":0,"v":[0],"curvePoints":[]}),
            vec![(0, 0.0, 255), (2, 0.2, 184), (5, 0.5, 100), (10, 1.0, 16)],
        ),
        (
            json!({"frame":0,"v":[0],"hold":true}),
            vec![(5, 0.0, 255), (9, 0.0, 255), (10, 1.0, 16)],
        ),
    ] {
        let o = owner(&source(
            json!([{"property":"opacity","track":{"keys":[first,{"frame":10,"v":[100]}]}}]),
        ));
        let s = o.acquire_snapshot(0).unwrap();
        for (frame, factor, channel) in cases {
            let p = plan(&s, frame);
            assert_eq!(p.packet().source_frame_for_layer(0), Some(0));
            close(p.packet().layer_opacity(0).unwrap(), factor);
            for a in alpha(&p) {
                close(a, 0.75 * factor);
            }
            let (pixels, _) = parity(&mut gpu, &o, &s, frame, None);
            pixel(&pixels, 25, 30, [255, channel, channel, 255]);
            pixel(&pixels, 0, 0, [255; 4]);
            if factor == 0.0 {
                assert!(pixels.chunks_exact(4).all(|p| p == [255; 4]));
            } else {
                ink(&pixels);
            }
        }
    }
}

fn saved(o: &NativeObjectHistory) -> Vec<u8> {
    let s = o.acquire_snapshot(o.content_revision()).unwrap();
    o.serialize_project_file(o.content_revision(), s.snapshot_id())
        .unwrap()
}
fn item(o: &mut NativeObjectHistory, command: Value) {
    let candidate = o
        .prepare_item_edit_json(command.to_string().as_bytes())
        .unwrap();
    o.commit_item_edit(candidate).unwrap();
}

#[test]
fn gpu_raster_opacity_preserves_encoded_resource_charge_old_witness_and_reopened_pixels() {
    let mut gpu = metal();
    let file = json!({"format":"nemo.native-project-document","formatVersion":1,
        "project":{"canvasW":64,"canvasH":64,"fps":24,"canvasBg":"#ffffff"},
        "viewportReference":null,"assets":[],
        "drawing":source(json!([{"property":"opacity","static":[50]}]))});
    let mut o =
        NativeObjectHistory::from_project_file("opacity-raster", file.to_string().as_bytes())
            .unwrap();
    let s = o.acquire_snapshot(0).unwrap();
    let png: Vec<_> = BLUE
        .as_bytes()
        .chunks_exact(2)
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect();
    let request = OpacityRequest::history_stage(
        "opacity-raster-import",
        o.instance_id(),
        o.document_id(),
        Some(0),
        "command.document.object.asset.attach",
        json!({"documentSnapshotId":s.snapshot_id(),"lifecycleGeneration":1,"name":"still.png","mediaType":"image/png",
            "source":{"kind":"file","path":"/admitted/still.png"},"placement":{"kind":"newLayer"}}),
    );
    let reply = o.dispatch_image_import(&request, &png).unwrap();
    assert!(reply.is_ok(), "{reply:?}");
    let target = serde_json::to_value(reply).unwrap()["result"]["target"].clone();
    item(
        &mut o,
        json!({"command":"object.raster.resize","stableTarget":target,"width":16,"height":8}),
    );
    item(
        &mut o,
        json!({"command":"object.raster.move","stableTarget":target,"x":48,"y":52}),
    );
    let old_s = o.acquire_snapshot(o.content_revision()).unwrap();
    let old_bytes = saved(&o);
    let (before, old) = parity(&mut gpu, &o, &old_s, 0, None);
    pixel(&before, 42, 52, [0, 0, 255, 255]);
    pixel(&before, 52, 52, [255; 4]);
    let versions = old.scene_resources().unwrap().declared_versions();
    assert_eq!(versions.len(), 1);
    assert_eq!(old.scene_resources().unwrap().total_charged_bytes(), 8);
    let (image, _, opacity) = old.raster_image(0).unwrap();
    assert_eq!(opacity, 1.0);
    let held = image.data.clone();
    assert_eq!(held.as_ref(), &[0, 0, 255, 255, 255, 255, 255, 255]);
    let command = json!({"command":"object.property.edit","target":{"contextId":"scene-root","layerUid":target["layerUid"]},
        "property":"opacity","action":{"kind":"setValue","frame":0,"values":[25]}});
    let candidate = o
        .prepare_property_edit_json(command.to_string().as_bytes())
        .unwrap();
    o.commit_property_edit(candidate).unwrap();
    let s = o.acquire_snapshot(o.content_revision()).unwrap();
    let (after, changed) = parity(&mut gpu, &o, &s, 0, Some(&old));
    ink(&after);
    pixel(&after, 42, 52, [191, 191, 255, 255]);
    pixel(&after, 25, 30, [255, 100, 100, 255]);
    assert_ne!(before, after);
    assert_eq!(changed.raster_image(0).unwrap().2, 0.25);
    assert_eq!(changed.raster_image(0).unwrap().0.data.id(), held.id());
    assert_eq!(
        changed.scene_resources().unwrap().declared_versions(),
        versions
    );
    assert_eq!(changed.scene_resources().unwrap().total_charged_bytes(), 8);
    assert_eq!(old.raster_image(0).unwrap().2, 1.0);
    assert_eq!(presented(&mut gpu, &old_s, &old), before);
    assert_eq!(
        o.serialize_project_file(old_s.content_revision(), old_s.snapshot_id())
            .unwrap(),
        old_bytes,
        "the immutable retained revision remains saveable"
    );
    assert_ne!(saved(&o), old_bytes);
    let bytes = saved(&o);
    let reopened = NativeObjectHistory::from_project_file("opacity-reopened", &bytes).unwrap();
    let r = reopened
        .acquire_snapshot(reopened.content_revision())
        .unwrap();
    let (restored, w) = parity(&mut gpu, &reopened, &r, 0, None);
    assert_eq!(restored, after);
    assert_eq!(w.scene_resources().unwrap().declared_versions(), versions);
    assert_eq!(w.scene_resources().unwrap().total_charged_bytes(), 8);
    assert_eq!(saved(&o), bytes);
    drop(o);
    drop(reopened);
    drop(changed);
    drop(old);
    drop(w);
    drop(s);
    drop(old_s);
    drop(r);
    assert_eq!(held.as_ref(), &[0, 0, 255, 255, 255, 255, 255, 255]);
}
