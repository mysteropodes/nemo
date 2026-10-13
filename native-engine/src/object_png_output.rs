//! Private bounded PNG bytes from the existing live scheduled-object readback.
use crate::compositor::{Compositor, ScheduledObjectCompositionResult};
use crate::object_render_scene::{LiveObjectComposition, ScheduledObjectComposition};
use crate::png_output::{encode_rgba8_into, PngOutputError};
use crate::render_scene::ScheduledFrameIdentity;
use std::io::{self, Write};

#[cfg(all(feature = "application", feature = "project_media_codec"))]
use crate::render_scene::{reserve_readback_bytes, ReadbackReservation};

// Preserve the separately compiled pre-media targets. The desktop application
// enables both features and always uses the existing shared resource domain.
#[cfg(not(all(feature = "application", feature = "project_media_codec")))]
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
            .map_err(|_| "PNG byte allocation failed")?;
        bytes.resize(self.0, 0);
        Ok(bytes)
    }
}

/// The per-call limit and shared charge have distinct jobs. Field order destroys
/// actual bytes before releasing their capacity charge, including retained output.
struct BoundedPngBytes {
    bytes: Vec<u8>,
    reservation: Option<ReadbackReservation>,
    maximum: usize,
}
impl BoundedPngBytes {
    fn new(maximum: usize) -> Result<Self, PngOutputError> {
        if maximum == 0 {
            return Err(PngOutputError::new("PNG byte budget must be positive"));
        }
        Ok(Self {
            bytes: Vec::new(),
            reservation: None,
            maximum,
        })
    }

    fn write_using(
        &mut self,
        chunk: &[u8],
        reserve: impl FnOnce(usize) -> Result<ReadbackReservation, &'static str>,
        allocate: impl FnOnce(&mut ReadbackReservation) -> Result<Vec<u8>, &'static str>,
    ) -> io::Result<usize> {
        let required = self
            .bytes
            .len()
            .checked_add(chunk.len())
            .filter(|&size| size <= self.maximum)
            .ok_or_else(|| io::Error::other("PNG byte budget exceeded"))?;
        if required > self.bytes.capacity() {
            // Bounded geometric growth avoids copying every emitted PNG chunk.
            // Reserve the full replacement while old storage is still charged.
            let capacity = required
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.maximum);
            let mut reservation = reserve(capacity).map_err(io::Error::other)?;
            let mut bytes = allocate(&mut reservation).map_err(io::Error::other)?;
            if bytes.len() != capacity || bytes.capacity() != capacity {
                return Err(io::Error::other("PNG byte allocation capacity mismatch"));
            }
            bytes.truncate(self.bytes.len());
            bytes.copy_from_slice(&self.bytes);
            // No fallible operation between these moves. Drop old bytes BEFORE
            // releasing their charge; the replacement charge is already held.
            self.bytes = bytes;
            self.reservation = Some(reservation);
        }
        // Capacity was admitted above; this append cannot allocate.
        self.bytes.extend_from_slice(chunk);
        Ok(chunk.len())
    }
}
impl Write for BoundedPngBytes {
    fn write(&mut self, chunk: &[u8]) -> io::Result<usize> {
        self.write_using(chunk, reserve_readback_bytes, ReadbackReservation::allocate)
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
        bytes: output,
        prepared: live.prepared().clone(),
    })
}

/// No destination or publication authority. Retained bytes require fresh live
/// work at consumption time, including after a successful earlier encoding.
pub(crate) struct ScheduledObjectPng {
    bytes: BoundedPngBytes,
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
        Ok(&self.bytes.bytes)
    }
}

#[cfg(test)]
#[path = "../tests/object_png_output.rs"]
mod tests;
