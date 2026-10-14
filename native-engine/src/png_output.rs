//! Deterministic PNG encoding for the staged native export job.

use crate::compositor::{CompositionResult, Compositor, ReadbackBytes};
use crate::render_scene::{GeometryPaintInput, RenderScene};
use png::{BitDepth, ColorType, Compression, Encoder, Filter};
use std::fmt::{Display, Formatter};
use std::io::Write;

#[path = "object_png_output.rs"]
mod object;
use crate::object_render_scene::LiveObjectComposition;
pub use object::EncodedPngBytes;
pub(crate) use object::{encode_scheduled_object_png, ScheduledObjectPng};

/// Borrow the caller's original compositor; no GPU or publication owner is made.
pub(crate) trait ObjectPngEncoder {
    fn encode_object_png(
        &mut self,
        live: &LiveObjectComposition<'_>,
        maximum_encoded_bytes: usize,
    ) -> Result<ScheduledObjectPng, String>;
}
impl ObjectPngEncoder for Compositor {
    fn encode_object_png(
        &mut self,
        live: &LiveObjectComposition<'_>,
        maximum_encoded_bytes: usize,
    ) -> Result<ScheduledObjectPng, String> {
        let result = self
            .compose_scheduled_object(live)
            .map_err(|e| e.to_string())?;
        encode_scheduled_object_png(self, &result, live, maximum_encoded_bytes)
            .map_err(|e| e.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PngOutputError {
    message: String,
}

impl PngOutputError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl Display for PngOutputError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for PngOutputError {}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportFrameInput(pub(crate) u32, pub(crate) GeometryPaintInput);

impl ExportFrameInput {
    pub fn new(source_frame: u32, geometry: GeometryPaintInput) -> Self {
        Self(source_frame, geometry)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ExportBegin {
    pub request_id: String,
    pub expected_revision: u64,
    pub context_id: String,
    pub quality: String,
    pub target: String,
    pub frames: Vec<ExportFrameInput>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportArtifact {
    pub target: String,
    pub files: Vec<String>,
}

/// Private-stage/publication boundary; N14 never writes a destination directly.
pub trait StagedArtifactPort {
    fn begin_staging(&mut self, job_id: &str, target: &str) -> Result<(), String>;
    fn write_frame(&mut self, job_id: &str, name: &str, bytes: &[u8]) -> Result<(), String>;
    fn publish(
        &mut self,
        job_id: &str,
        target: &str,
        files: &[String],
    ) -> Result<ExportArtifact, String>;
    fn cleanup(&mut self, job_id: &str) -> Result<(), String>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportReadback {
    pub width: u32,
    pub height: u32,
    pub bytes: ReadbackBytes,
    pub document_snapshot_id: String,
    pub document_id: String,
    pub content_revision: u64,
    pub context_id: String,
    pub source_frame: u32,
    pub quality: String,
}

/// Injectable N12 seam. The production implementation delegates to the sole
/// native compositor's compose and readback methods.
pub trait ExportCompositor {
    type Composition;
    fn compose(&mut self, scene: &RenderScene) -> Result<Self::Composition, String>;
    fn readback_rgba8(&self, result: &Self::Composition) -> Result<ExportReadback, String>;
}

impl ExportCompositor for Compositor {
    type Composition = CompositionResult;
    fn compose(&mut self, scene: &RenderScene) -> Result<Self::Composition, String> {
        Compositor::compose(self, scene).map_err(|error| error.to_string())
    }
    fn readback_rgba8(&self, result: &Self::Composition) -> Result<ExportReadback, String> {
        let pixels = Compositor::readback_rgba8(self, result).map_err(|error| error.to_string())?;
        let (width, height) = pixels.dimensions();
        Ok(ExportReadback {
            width,
            height,
            bytes: pixels.retained_bytes(),
            document_snapshot_id: pixels.document_snapshot_id().to_owned(),
            document_id: pixels.document_id().to_owned(),
            content_revision: pixels.content_revision(),
            context_id: pixels.context_id().to_owned(),
            source_frame: pixels.frame(),
            quality: pixels.quality().to_owned(),
        })
    }
}

pub(crate) fn validate_readback(
    pixels: &ExportReadback,
    scene: &RenderScene,
) -> Result<(), String> {
    if (pixels.width, pixels.height) != (320, 180) {
        return Err("RGBA8 readback dimensions differ from the prepared scene".into());
    }
    if pixels.bytes.len() != 320 * 180 * 4
        || pixels.document_snapshot_id != scene.document_snapshot_id()
        || pixels.document_id != scene.document_id()
        || pixels.content_revision != scene.content_revision()
        || pixels.context_id != scene.context_id()
        || pixels.source_frame != scene.frame()
        || pixels.quality != scene.quality()
    {
        return Err(
            "RGBA8 readback byte length or identity differs from the prepared scene".into(),
        );
    }
    Ok(())
}

/// Encode one tightly packed RGBA8 image with a fixed, metadata-free profile.
///
/// Determinism is guaranteed only for this locked encoder/configuration. It is
/// not a promise of byte equality with the legacy JavaScript encoder.
pub fn encode_rgba8(
    width: u32,
    height: u32,
    rgba: &[u8],
) -> Result<EncodedPngBytes, PngOutputError> {
    // Keep the existing shared allocation quota rather than adding an image-size
    // policy. Feature-off targets retain their explicitly unmetered domain.
    let mut output = object::BoundedPngBytes::new(isize::MAX as usize)?;
    encode_rgba8_into(width, height, rgba, &mut output)?;
    Ok(output.into_shared())
}

/// One encoder/profile for existing opacity output and bounded object output.
fn encode_rgba8_into(
    width: u32,
    height: u32,
    rgba: &[u8],
    output: impl Write,
) -> Result<(), PngOutputError> {
    if width == 0 || height == 0 {
        return Err(PngOutputError::new(
            "PNG dimensions must be greater than zero",
        ));
    }
    let expected = u64::from(width)
        .checked_mul(u64::from(height))
        .and_then(|pixels| pixels.checked_mul(4))
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or_else(|| PngOutputError::new("PNG RGBA8 byte length overflows"))?;
    if rgba.len() != expected {
        return Err(PngOutputError::new(format!(
            "PNG RGBA8 input has {} bytes; expected {expected}",
            rgba.len()
        )));
    }

    {
        let mut encoder = Encoder::new(output, width, height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        encoder.set_filter(Filter::NoFilter);
        encoder.set_compression(Compression::High);
        let mut writer = encoder
            .write_header()
            .map_err(|error| PngOutputError::new(format!("write PNG header: {error}")))?;
        {
            // Stream IDAT chunks instead of first retaining the whole compressed
            // image inside png::Writer. Row/compressor storage remains separate.
            let mut stream = writer
                .stream_writer_with_size(4096)
                .map_err(|error| PngOutputError::new(format!("start PNG stream: {error}")))?;
            stream
                .write_all(rgba)
                .map_err(|error| PngOutputError::new(format!("write PNG image data: {error}")))?;
            stream
                .finish()
                .map_err(|error| PngOutputError::new(format!("finish PNG stream: {error}")))?;
        }
        writer
            .finish()
            .map_err(|error| PngOutputError::new(format!("finish PNG: {error}")))?;
    }
    Ok(())
}

#[cfg(test)]
mod streaming_tests {
    use super::*;
    use std::io::{self, Cursor};

    #[derive(Default)]
    struct Sink {
        bytes: Vec<u8>,
        largest_write: usize,
        fail_after: Option<usize>,
    }
    impl Write for Sink {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            self.largest_write = self.largest_write.max(bytes.len());
            if bytes.len() > 4096 {
                return Err(io::Error::other("whole-image write exceeds 4 KiB"));
            }
            if self
                .fail_after
                .is_some_and(|limit| self.bytes.len() + bytes.len() > limit)
            {
                return Err(io::Error::other("injected PNG sink failure"));
            }
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    fn noisy_rgba() -> Vec<u8> {
        let mut seed = 0x1234_5678u32;
        let mut pixels = vec![0; 256 * 64 * 4];
        for pixel in pixels.chunks_exact_mut(4) {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            pixel.copy_from_slice(&seed.to_le_bytes());
        }
        // Preserve hidden straight RGB at zero alpha, plus half and full alpha.
        pixels[..12].copy_from_slice(&[17, 34, 51, 0, 17, 34, 51, 127, 99, 88, 77, 255]);
        pixels
    }

    fn assert_pixels(encoded: &[u8], width: u32, height: u32, expected: &[u8]) {
        let mut reader = png::Decoder::new(Cursor::new(encoded)).read_info().unwrap();
        assert_eq!((reader.info().width, reader.info().height), (width, height));
        assert_eq!(reader.info().color_type, ColorType::Rgba);
        assert_eq!(reader.info().bit_depth, BitDepth::Eight);
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!(&pixels[..info.buffer_size()], expected);
        reader.finish().unwrap();
    }

    #[test]
    fn cpu_streaming_png_incompressible_output_respects_actual_sink_bound() {
        let pixels = noisy_rgba();
        let mut sink = Sink::default();
        encode_rgba8_into(256, 64, &pixels, &mut sink).unwrap();
        assert_eq!(sink.largest_write, 4096);
        assert!(
            sink.bytes.len() > 4096 * 8,
            "fixture must exercise many chunks"
        );
        assert_pixels(&sink.bytes, 256, 64, &pixels);
        assert_eq!(&*encode_rgba8(256, 64, &pixels).unwrap(), sink.bytes);
        assert_eq!(&*encode_rgba8(256, 64, &pixels).unwrap(), sink.bytes);
    }

    #[test]
    fn cpu_streaming_png_tiny_terminal_chunk_preserves_straight_alpha() {
        let pixels = [17, 34, 51, 0, 17, 34, 51, 127, 99, 88, 77, 255];
        let mut sink = Sink::default();
        encode_rgba8_into(3, 1, &pixels, &mut sink).unwrap();
        assert!(sink.largest_write < 4096);
        assert_pixels(&sink.bytes, 3, 1, &pixels);
    }

    #[test]
    fn cpu_streaming_png_invalid_shapes_refuse_before_any_output() {
        for (width, height, pixels) in [
            (0, 1, &[][..]),
            (1, 0, &[][..]),
            (1, 1, &[0, 1, 2][..]),
            (1, 1, &[0, 1, 2, 3, 4][..]),
            (u32::MAX, u32::MAX, &[][..]),
        ] {
            let mut sink = Sink::default();
            assert!(encode_rgba8_into(width, height, pixels, &mut sink).is_err());
            assert!(sink.bytes.is_empty());
            assert_eq!(sink.largest_write, 0);
        }
    }

    #[test]
    fn cpu_streaming_png_propagates_header_write_stream_finish_and_iend_errors() {
        let noisy = noisy_rgba();
        let tiny = [17, 34, 51, 0];
        let complete = encode_rgba8(1, 1, &tiny).unwrap();
        for (width, height, pixels, limit, stage) in [
            (1, 1, tiny.as_slice(), 0, "write PNG header:"),
            (256, 64, noisy.as_slice(), 33, "write PNG image data:"),
            (1, 1, tiny.as_slice(), 33, "finish PNG stream:"),
            (1, 1, tiny.as_slice(), complete.len() - 1, "finish PNG:"),
        ] {
            let mut sink = Sink {
                fail_after: Some(limit),
                ..Sink::default()
            };
            let error = encode_rgba8_into(width, height, pixels, &mut sink).unwrap_err();
            assert!(error.to_string().starts_with(stage), "{error}");
            assert!(error.to_string().contains("injected PNG sink failure"));
            assert!(sink.bytes.len() <= limit);
        }
    }
}
