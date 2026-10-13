//! Private bounded PNG bytes from the existing live scheduled-object readback.
use crate::compositor::{Compositor, ScheduledObjectCompositionResult};
use crate::object_render_scene::{LiveObjectComposition, ScheduledObjectComposition};
use crate::png_output::{encode_rgba8_into, PngOutputError};
use crate::render_scene::ScheduledFrameIdentity;
use std::io::{self, Write};

/// This is a per-call output limit, not another resource-accounting owner.
struct BoundedPngBytes {
    bytes: Vec<u8>,
    maximum: usize,
}
impl BoundedPngBytes {
    fn new(maximum: usize) -> Result<Self, PngOutputError> {
        if maximum == 0 {
            return Err(PngOutputError::new("PNG byte budget must be positive"));
        }
        Ok(Self {
            bytes: Vec::new(),
            maximum,
        })
    }
}
impl Write for BoundedPngBytes {
    fn write(&mut self, chunk: &[u8]) -> io::Result<usize> {
        if self
            .bytes
            .len()
            .checked_add(chunk.len())
            .is_none_or(|size| size > self.maximum)
        {
            return Err(io::Error::other("PNG byte budget exceeded"));
        }
        // Request only the checked remaining growth, never a geometric reserve
        // that exceeds the caller's limit. Allocator failure returns no artifact.
        self.bytes
            .try_reserve_exact(chunk.len())
            .map_err(|_| io::Error::other("PNG byte allocation failed"))?;
        self.bytes.extend_from_slice(chunk);
        Ok(chunk.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn encode_scheduled_object_png(
    compositor: &Compositor,
    result: &ScheduledObjectCompositionResult,
    live: &LiveObjectComposition<'_>,
    maximum_encoded_bytes: usize,
) -> Result<ScheduledObjectPng, PngOutputError> {
    if live.prepared().composition().is_viewport() {
        return Err(PngOutputError::new(
            "viewport reference content is not an artwork export",
        ));
    }
    let mut output = BoundedPngBytes::new(maximum_encoded_bytes)?;
    let pixels = compositor
        .readback_scheduled_object_rgba8(result, live)
        .map_err(|error| PngOutputError::new(error.to_string()))?;
    let (width, height) = pixels.dimensions();
    let rgba = pixels
        .bytes(live)
        .map_err(|error| PngOutputError::new(error.to_string()))?;
    encode_rgba8_into(width, height, rgba, &mut output)?;
    Ok(ScheduledObjectPng {
        bytes: output.bytes,
        prepared: live.prepared().clone(),
    })
}

/// No destination or publication authority. Retained bytes require fresh live
/// work at consumption time, including after a successful earlier encoding.
pub(crate) struct ScheduledObjectPng {
    bytes: Vec<u8>,
    prepared: ScheduledObjectComposition,
}
impl ScheduledObjectPng {
    pub(crate) fn identity(&self) -> &ScheduledFrameIdentity {
        self.prepared.identity()
    }
    pub(crate) fn dimensions(&self) -> (u32, u32) {
        self.prepared.dimensions()
    }
    pub(crate) fn bytes<'a>(
        &'a self,
        live: &'a LiveObjectComposition<'_>,
    ) -> Result<&'a [u8], PngOutputError> {
        live.validate_result(&self.prepared).map_err(|error| {
            PngOutputError::new(format!("PNG work is not consumable: {error:?}"))
        })?;
        Ok(&self.bytes)
    }
}

#[cfg(test)]
#[path = "../tests/object_png_output.rs"]
mod tests;
