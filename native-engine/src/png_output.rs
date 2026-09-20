//! Deterministic PNG encoding for the staged native export job.

use crate::compositor::{CompositionResult, Compositor};
use crate::render_scene::{GeometryPaintInput, RenderScene};
use png::{BitDepth, ColorType, Compression, Encoder, Filter};
use std::fmt::{Display, Formatter};

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
    pub bytes: Vec<u8>,
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
            bytes: pixels.bytes().to_vec(),
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
pub fn encode_rgba8(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, PngOutputError> {
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

    let mut output = Vec::new();
    {
        let mut encoder = Encoder::new(&mut output, width, height);
        encoder.set_color(ColorType::Rgba);
        encoder.set_depth(BitDepth::Eight);
        encoder.set_filter(Filter::NoFilter);
        encoder.set_compression(Compression::High);
        let mut writer = encoder
            .write_header()
            .map_err(|error| PngOutputError::new(format!("write PNG header: {error}")))?;
        writer
            .write_image_data(rgba)
            .map_err(|error| PngOutputError::new(format!("write PNG image data: {error}")))?;
        writer
            .finish()
            .map_err(|error| PngOutputError::new(format!("finish PNG: {error}")))?;
    }
    Ok(output)
}
