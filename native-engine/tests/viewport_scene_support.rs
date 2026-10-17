//! Shared real-owner helpers for the viewport_scene_* tests (L08-E, #2114).
//! Expected pixels come from the baseline oracle fixture, never from the code
//! under test; the surface double reads the presented texture itself.
use super::ObjectViewportFrame;
use crate::compositor::Compositor;
use crate::desktop_viewport::{
    AcquiredSurfaceFrame, CssBounds, DesktopViewportHost, PhysicalExtent, SettledSurfaceFrame,
    SurfaceAttempt, SurfacePort, SurfacePresentation, SurfaceRecoveryError, ViewportMapping,
};
use crate::export_job::{ExportArtifact, StagedArtifactPort};
use crate::history::NativeObjectHistory;
use crate::object_codec::decode_storage_project;
use crate::object_render_scene::{paint_test_document, LiveObjectComposition};
use crate::render_scene::ScheduledFrameIdentity;
use crate::revision::ObjectSnapshot;
use crate::scheduler::{ObjectSchedulePins, ObjectScheduleScope};
use serde_json::json;
use std::collections::VecDeque;
use std::io::Cursor;
use std::sync::mpsc;
use std::time::Duration;

pub(super) const ORACLE: &str = include_str!("fixtures/viewport-scene-packet-parity/oracle.txt");
pub(super) const SIZE: u32 = 64;

/// One oracle `px` row: baseline = u8-alpha source-over of the baseline colour
/// function, native = the same layers with the document's float alpha.
pub(super) struct PixelRow {
    pub(super) x: usize,
    pub(super) y: usize,
    pub(super) baseline: [f64; 4],
    pub(super) native: [f64; 4],
}

pub(super) fn oracle_rows(scene: &str) -> Vec<PixelRow> {
    let number = |text: &str| text.parse::<f64>().expect("oracle number");
    ORACLE
        .lines()
        .filter_map(|line| {
            let field: Vec<&str> = line.split_whitespace().collect();
            if field.first() != Some(&"px") || field.get(1) != Some(&scene) {
                return None;
            }
            assert_eq!(field.len(), 12, "{line}");
            let take = |from: usize| -> [f64; 4] {
                [
                    number(field[from]),
                    number(field[from + 1]),
                    number(field[from + 2]),
                    number(field[from + 3]),
                ]
            };
            Some(PixelRow {
                x: field[2].parse().unwrap(),
                y: field[3].parse().unwrap(),
                baseline: take(4),
                native: take(8),
            })
        })
        .collect()
}

pub(super) fn oracle_facts() -> Vec<Vec<String>> {
    ORACLE
        .lines()
        .filter(|line| line.starts_with("fact "))
        .map(|line| line.split_whitespace().skip(1).map(str::to_owned).collect())
        .collect()
}

/// Every oracle pixel of `scene` must match within the accepted one-level
/// tolerance of both the baseline and the native-alpha expectation.
pub(super) fn assert_matches_oracle(bytes: &[u8], scene: &str) {
    let rows = oracle_rows(scene);
    assert_eq!(rows.len(), 6, "{scene}");
    assert_eq!(bytes.len(), (SIZE * SIZE * 4) as usize);
    for row in rows {
        let at = (row.y * SIZE as usize + row.x) * 4;
        for channel in 0..4 {
            let actual = f64::from(bytes[at + channel]);
            for (label, expected) in [("baseline", row.baseline), ("native", row.native)] {
                assert!(
                    (actual - expected[channel]).abs() <= 1.0,
                    "{scene} ({},{}) channel {channel}: {actual} vs {label} {}",
                    row.x,
                    row.y,
                    expected[channel]
                );
            }
        }
    }
}

pub(super) fn first_diff(left: &[u8], right: &[u8]) -> Option<usize> {
    if left.len() != right.len() {
        return Some(left.len().min(right.len()));
    }
    left.iter().zip(right).position(|(a, b)| a != b)
}

pub(super) fn assert_same_bytes(label: &str, left: &[u8], right: &[u8]) {
    assert_eq!(first_diff(left, right), None, "{label}");
}

/// The zero-call guard used by every refusal assertion.
pub(super) fn assert_no_surface_call(before: usize, now: usize, label: &str) {
    assert_eq!(before, now, "{label}: the surface was reached");
}

pub(super) fn scene_owner(instance: &str, alpha: f64) -> NativeObjectHistory {
    // Reuse accepted input construction only; expected pixels come from the oracle.
    let mut source = paint_test_document(&[0, 7]);
    source["objects"]
        .as_array_mut()
        .unwrap()
        .retain(|r| r["target"]["strokeId"] == "a");
    for scope in source["compositionOrder"]["contexts"][0]["scopes"]
        .as_array_mut()
        .unwrap()
    {
        scope["elements"]
            .as_array_mut()
            .unwrap()
            .retain(|e| e["strokeId"] == "a");
    }
    for record in source["objects"].as_array_mut().unwrap() {
        record["geometry"]["segments"] = json!([[4,4],[28,4],[28,28],[4,28]].map(|[x,y]|
            json!({"point":{"x":x,"y":y},"handleIn":{"x":0,"y":0},"handleOut":{"x":0,"y":0}})));
        let color = match record["target"]["layerUid"].as_str().unwrap() {
            "L0" => [1, 0, 0],
            "L1" => [0, 1, 0],
            _ => [0, 0, 1],
        };
        record["fill"] = json!({"kind":"solid","r":color[0],"g":color[1],"b":color[2],"a":alpha});
    }
    NativeObjectHistory::new(
        instance,
        decode_storage_project(&serde_json::to_vec(&source).unwrap()).unwrap(),
    )
    .unwrap()
}

pub(super) fn pins(snapshot: &ObjectSnapshot, frame: u32) -> ObjectSchedulePins {
    ObjectSchedulePins {
        instance_id: snapshot.instance_id().into(),
        document_id: snapshot.document_id().into(),
        content_revision: snapshot.content_revision(),
        document_snapshot_id: snapshot.snapshot_id().into(),
        context_id: "scene-root".into(),
        frame,
        scope: ObjectScheduleScope::Authored,
    }
}

pub(super) fn mapping(size: u32) -> ViewportMapping {
    let extent = PhysicalExtent {
        width: size,
        height: size,
    };
    ViewportMapping::new(
        CssBounds {
            x: 0.,
            y: 0.,
            width: 64.,
            height: 64.,
        },
        extent,
        extent,
        1.,
    )
    .unwrap()
}

/// Copy of the production readback sequence (compositor_readback.rs), applied to
/// the texture the surface was actually handed.
pub(super) fn read_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    texture: &wgpu::Texture,
) -> Vec<u8> {
    let row = SIZE * 4;
    let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let padded = row.div_ceil(alignment) * alignment;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("l08e-presented-readback"),
        size: u64::from(padded) * u64::from(SIZE),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("l08e-presented-copy"),
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
                bytes_per_row: Some(padded),
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
    let mut bytes = Vec::new();
    for chunk in mapped.chunks_exact(padded as usize) {
        bytes.extend_from_slice(&chunk[..row as usize]);
    }
    drop(mapped);
    buffer.unmap();
    bytes
}

pub(super) fn decode_png(bytes: &[u8]) -> Vec<u8> {
    let mut reader = png::Decoder::new(Cursor::new(bytes)).read_info().unwrap();
    assert_eq!(reader.info().color_type, png::ColorType::Rgba);
    assert_eq!(reader.info().bit_depth, png::BitDepth::Eight);
    let mut rgba = vec![0; reader.output_buffer_size().unwrap()];
    let info = reader.next_frame(&mut rgba).unwrap();
    assert_eq!((info.width, info.height), (SIZE, SIZE));
    rgba.truncate(info.buffer_size());
    rgba
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    Present,
    Timeout,
    Occluded,
}

/// Surface double on the exact Compositor GPU. It counts every acquire, records
/// the frame identity it was handed, and copies the presented texture's bytes.
pub(super) struct CapturePort {
    context: (wgpu::Instance, wgpu::Adapter, wgpu::Device, wgpu::Queue),
    texture: wgpu::Texture,
    pub(super) steps: VecDeque<Step>,
    pub(super) calls: usize,
    pub(super) frames: usize,
    pub(super) settlements: usize,
    pub(super) captured: Vec<Vec<u8>>,
    pub(super) identities: Vec<ScheduledFrameIdentity>,
}
impl CapturePort {
    pub(super) fn new(
        gpu: &Compositor,
        frame: &ObjectViewportFrame,
        live: &LiveObjectComposition<'_>,
    ) -> Self {
        let source = gpu.scheduled_object_surface(&frame.result, live).unwrap();
        Self {
            context: (
                gpu.instance().clone(),
                gpu.adapter().clone(),
                gpu.device().clone(),
                gpu.queue().clone(),
            ),
            texture: source.texture.clone(),
            steps: VecDeque::new(),
            calls: 0,
            frames: 0,
            settlements: 0,
            captured: Vec::new(),
            identities: Vec::new(),
        }
    }
}
pub(super) struct Frame<'a> {
    settlements: &'a mut usize,
    accepted: bool,
}
impl AcquiredSurfaceFrame for Frame<'_> {
    fn settle(self) -> SettledSurfaceFrame {
        if self.accepted {
            *self.settlements += 1;
            SettledSurfaceFrame::Presented
        } else {
            SettledSurfaceFrame::ValidationFailure
        }
    }
}
impl SurfacePort for CapturePort {
    type Frame<'p, 's> = Frame<'p>;
    fn acquire<'p, 's>(
        &'p mut self,
        source: SurfacePresentation<'s>,
    ) -> SurfaceAttempt<Self::Frame<'p, 's>> {
        self.calls += 1;
        self.identities.push(source.identity.clone());
        if source.instance != &self.context.0
            || source.adapter != &self.context.1
            || source.device != &self.context.2
            || source.queue != &self.context.3
        {
            return SurfaceAttempt::Acquired(Frame {
                settlements: &mut self.settlements,
                accepted: false,
            });
        }
        assert_eq!(source.texture, &self.texture);
        assert_eq!(
            source.source_extent,
            PhysicalExtent {
                width: SIZE,
                height: SIZE
            }
        );
        match self.steps.pop_front().unwrap_or(Step::Present) {
            Step::Timeout => return SurfaceAttempt::Timeout,
            Step::Occluded => return SurfaceAttempt::Occluded,
            Step::Present => {}
        }
        self.captured
            .push(read_texture(source.device, source.queue, source.texture));
        self.frames += 1;
        SurfaceAttempt::Acquired(Frame {
            settlements: &mut self.settlements,
            accepted: true,
        })
    }
    fn recreate(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn reconfigure(&mut self, _: ViewportMapping) -> Result<(), SurfaceRecoveryError> {
        Ok(())
    }
    fn dispose(&mut self) {}
    /// The test-only permit-bypass entry; it still counts as reaching the surface.
    fn simulated_acquire<'p>(
        &'p mut self,
        identity: &ScheduledFrameIdentity,
    ) -> SurfaceAttempt<Self::Frame<'p, 'p>> {
        self.calls += 1;
        self.identities.push(identity.clone());
        SurfaceAttempt::Acquired(Frame {
            settlements: &mut self.settlements,
            accepted: true,
        })
    }
}

pub(super) fn host_for(
    gpu: &Compositor,
    frame: &ObjectViewportFrame,
    live: &LiveObjectComposition<'_>,
    steps: &[Step],
) -> DesktopViewportHost<CapturePort> {
    let mut port = CapturePort::new(gpu, frame, live);
    port.steps.extend(steps.iter().copied());
    let mut host = DesktopViewportHost::new(port, mapping(SIZE));
    host.register(frame.identity().clone()).unwrap();
    host
}

/// In-memory staging port: bytes of the one written frame, nothing on disk.
#[derive(Default)]
pub(super) struct MemoryPort {
    pub(super) names: Vec<String>,
    pub(super) bytes: Vec<u8>,
    pub(super) published: bool,
}
impl StagedArtifactPort for MemoryPort {
    fn begin_staging(&mut self, _: &str, _: &str) -> Result<(), String> {
        Ok(())
    }
    fn write_frame(&mut self, _: &str, name: &str, bytes: &[u8]) -> Result<(), String> {
        self.names.push(name.into());
        self.bytes = bytes.to_vec();
        Ok(())
    }
    fn publish(
        &mut self,
        _: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String> {
        self.published = true;
        Ok(ExportArtifact {
            target: target.into(),
            files: files.to_vec(),
        })
    }
    fn cleanup(&mut self, _: &str) -> Result<(), String> {
        Ok(())
    }
}
