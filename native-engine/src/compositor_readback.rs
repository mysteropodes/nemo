//! Controlled readback payloads; Vello/target/driver allocations remain separate.
use crate::compositor::{Compositor, CompositorError};
use std::{
    ops::Deref,
    sync::{mpsc, Arc},
    time::Duration,
};

#[cfg(all(feature = "application", feature = "project_media_codec"))]
use crate::render_scene::{reserve_readback_bytes, ReadbackReservation};

// Preserve separately compiled pre-media engine targets. The actual desktop
// application enables both features and cannot enter this unmetered path.
#[cfg(not(all(feature = "application", feature = "project_media_codec")))]
#[derive(Debug)]
struct ReadbackReservation(usize);
#[cfg(not(all(feature = "application", feature = "project_media_codec")))]
fn reserve_readback_bytes(bytes: usize) -> Result<ReadbackReservation, &'static str> {
    Ok(ReadbackReservation(bytes))
}
#[cfg(not(all(feature = "application", feature = "project_media_codec")))]
impl ReadbackReservation {
    fn allocate(&mut self) -> Result<Vec<u8>, &'static str> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(self.0)
            .map_err(|_| "readback pixel allocation refused")?;
        bytes.resize(self.0, 0);
        Ok(bytes)
    }
}

// Drop order is part of the contract: actual storage precedes its reservation.
#[derive(Debug)]
struct PackedPixels {
    bytes: Vec<u8>,
    _reservation: Option<ReadbackReservation>,
}
/// Read-only pixel storage. Clones share its allocation and retain its charge
/// through the last owner; borrowing bytes cannot detach the owned storage.
#[derive(Debug, Clone)]
pub struct ReadbackBytes(Arc<PackedPixels>);
impl TryFrom<Vec<u8>> for ReadbackBytes {
    type Error = CompositorError;

    /// Admit already allocated caller storage by its actual retained capacity.
    /// This does not pre-admit the caller's earlier allocation. Feature-off engine
    /// targets preserve their existing unmetered readback domain.
    fn try_from(bytes: Vec<u8>) -> Result<Self, Self::Error> {
        Self::admit_with(bytes, reserve_readback_bytes)
    }
}
impl ReadbackBytes {
    fn admit_with(
        bytes: Vec<u8>,
        reserve: impl FnOnce(usize) -> Result<ReadbackReservation, &'static str>,
    ) -> Result<Self, CompositorError> {
        let reservation = if bytes.capacity() == 0 {
            None
        } else {
            Some(reserve(bytes.capacity()).map_err(CompositorError::new)?)
        };
        Ok(Self(Arc::new(PackedPixels {
            bytes,
            _reservation: reservation,
        })))
    }
}
impl Deref for ReadbackBytes {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0.bytes
    }
}
impl PartialEq for ReadbackBytes {
    fn eq(&self, other: &Self) -> bool {
        self.deref() == other.deref()
    }
}
impl Eq for ReadbackBytes {}

struct ReadbackBuffer {
    buffer: Option<wgpu::Buffer>,
    reservation: Option<ReadbackReservation>,
    queue: wgpu::Queue,
    submitted: bool,
    completed: bool,
}
impl ReadbackBuffer {
    fn buffer(&self) -> &wgpu::Buffer {
        self.buffer.as_ref().expect("owned readback buffer")
    }
}
impl Drop for ReadbackBuffer {
    fn drop(&mut self) {
        let buffer = self.buffer.take().expect("owned readback buffer");
        let reservation = self.reservation.take().expect("owned readback reservation");
        let retire = move || {
            // No raw buffer escapes the private producer. Destruction invalidates
            // the allocation before its final owned handle and quota are released.
            buffer.destroy();
            drop(buffer);
            drop(reservation);
        };
        if self.submitted && !self.completed {
            // This hook owns cleanup even if the caller returns or unwinds before
            // polling. wgpu runs it only after the submitted copy and map callbacks.
            // No device/queue is captured, avoiding a queue -> callback -> queue cycle.
            self.queue.on_submitted_work_done(retire);
        } else {
            retire();
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ReadbackLayout {
    row: usize,
    padded_row: u32,
    gpu_bytes: usize,
    packed_bytes: usize,
}
impl ReadbackLayout {
    fn new(width: u32, height: u32) -> Result<Self, CompositorError> {
        let invalid = || CompositorError::new("RGBA8 readback dimensions exceed capacity");
        if width == 0 || height == 0 {
            return Err(invalid());
        }
        let row = width.checked_mul(4).ok_or_else(invalid)?;
        let alignment = wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let padded_row =
            row.checked_add(alignment - 1).ok_or_else(invalid)? / alignment * alignment;
        let size = |stride: u32| -> Result<usize, CompositorError> {
            u64::from(stride)
                .checked_mul(u64::from(height))
                .and_then(|bytes| usize::try_from(bytes).ok())
                .filter(|bytes| *bytes <= isize::MAX as usize)
                .ok_or_else(invalid)
        };
        Ok(Self {
            row: row as usize,
            padded_row,
            gpu_bytes: size(padded_row)?,
            packed_bytes: size(row)?,
        })
    }
}

impl Compositor {
    pub(super) fn readback_texture(
        &self,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
    ) -> Result<ReadbackBytes, CompositorError> {
        self.readback_with(
            texture,
            width,
            height,
            reserve_readback_bytes,
            |_, _| Ok(()),
        )
    }

    // Production and injected controls share the actual allocation/copy/map path.
    fn readback_with(
        &self,
        texture: &wgpu::Texture,
        width: u32,
        height: u32,
        reserve: impl Fn(usize) -> Result<ReadbackReservation, &'static str>,
        mut checkpoint: impl FnMut(&'static str, Option<&wgpu::Buffer>) -> Result<(), CompositorError>,
    ) -> Result<ReadbackBytes, CompositorError> {
        let layout = ReadbackLayout::new(width, height)?;
        // Acquire both overlapping payloads before creating either allocation.
        let gpu_reservation = reserve(layout.gpu_bytes).map_err(CompositorError::new)?;
        let mut packed_reservation = reserve(layout.packed_bytes).map_err(CompositorError::new)?;
        checkpoint("reserved", None)?;
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("n12-native-rgba8-readback"),
            size: layout.gpu_bytes as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut gpu = ReadbackBuffer {
            buffer: Some(buffer),
            reservation: Some(gpu_reservation),
            queue: self.queue.clone(),
            submitted: false,
            completed: false,
        };
        checkpoint("gpu-created", Some(gpu.buffer()))?;
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("n12-native-rgba8-copy"),
            });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: gpu.buffer(),
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(layout.padded_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
        );
        let commands = encoder.finish();
        // Set before submit: unwind inside submit must conservatively retain the
        // buffer until the queue's own completion hook, never return quota early.
        gpu.submitted = true;
        self.queue.submit(Some(commands));
        checkpoint("submitted", Some(gpu.buffer()))?;
        let (sender, receiver) = mpsc::channel();
        gpu.buffer()
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |mapped| {
                let _ = sender.send(mapped.map_err(|error| error.to_string()));
            });
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|error| CompositorError::new(format!("wait for RGBA8 readback: {error}")))?;
        gpu.completed = true;
        receiver
            .recv_timeout(Duration::from_secs(10))
            .map_err(|error| CompositorError::new(format!("receive RGBA8 readback: {error}")))?
            .map_err(|error| CompositorError::new(format!("map RGBA8 readback: {error}")))?;
        let mapped = gpu.buffer().slice(..).get_mapped_range();
        if mapped.len() != layout.gpu_bytes {
            return Err(CompositorError::new("RGBA8 readback mapped size changed"));
        }
        checkpoint("mapped", Some(gpu.buffer()))?;
        let mut bytes = packed_reservation
            .allocate()
            .map_err(CompositorError::new)?;
        checkpoint("packed-created", Some(gpu.buffer()))?;
        for (row, target) in mapped
            .chunks_exact(layout.padded_row as usize)
            .zip(bytes.chunks_exact_mut(layout.row))
        {
            target.copy_from_slice(&row[..layout.row]);
        }
        drop(mapped);
        gpu.buffer().unmap();
        checkpoint("packed-copied", Some(gpu.buffer()))?;
        Ok(ReadbackBytes(Arc::new(PackedPixels {
            bytes,
            _reservation: Some(packed_reservation),
        })))
    }
}

#[cfg(all(test, feature = "application", feature = "project_media_codec"))]
#[path = "../tests/compositor_readback.rs"]
mod tests;
