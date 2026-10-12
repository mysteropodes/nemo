//! Strict private NoMedia wire; drawing admission always sees original bytes.
use crate::object_document::{ProjectEnvelope, ProjectMetadata, ProjectReferenceState};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

const FORMAT: &str = "nemo.native-project-document";
const MAX_ENVELOPE: usize = 32 * 1024 * 1024;
const MAX_DRAWING: usize = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProjectWireError {
    Budget,
    Invalid,
    Unsupported,
    Drawing(crate::object_codec::ObjectCodecError),
}

/// Explicit caller limits cannot exceed the staged file ceilings.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProjectWireLimits {
    envelope_bytes: usize,
    drawing_bytes: usize,
}
impl ProjectWireLimits {
    pub(crate) fn new(
        envelope_bytes: usize,
        drawing_bytes: usize,
    ) -> Result<Self, ProjectWireError> {
        if envelope_bytes == 0
            || envelope_bytes > MAX_ENVELOPE
            || drawing_bytes == 0
            || drawing_bytes > MAX_DRAWING
        {
            return Err(ProjectWireError::Budget);
        }
        Ok(Self {
            envelope_bytes,
            drawing_bytes,
        })
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Input<'a> {
    format: String,
    format_version: u32,
    project: Metadata,
    #[serde(borrow)]
    drawing: &'a RawValue,
    #[serde(borrow)]
    viewport_reference: &'a RawValue,
    #[serde(borrow)]
    assets: &'a RawValue,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Metadata {
    canvas_w: u32,
    canvas_h: u32,
    fps: f64,
}
// Derived decoding alone admits positional arrays; reuse the drawing's
// object-only visitor with strict duplicate/unknown/required-member checks.
crate::object_document::object_deserialize!(Metadata {
    canvas_w: u32,
    canvas_h: u32,
    fps: f64
});

pub(crate) fn decode_project(
    bytes: &[u8],
    limits: ProjectWireLimits,
) -> Result<ProjectEnvelope, ProjectWireError> {
    if bytes.len() > limits.envelope_bytes {
        return Err(ProjectWireError::Budget);
    }
    if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
        return Err(ProjectWireError::Invalid);
    }
    let wire: Input<'_> = serde_json::from_slice(bytes).map_err(|_| ProjectWireError::Invalid)?;
    if wire.format != FORMAT || wire.format_version != 1 {
        return Err(ProjectWireError::Unsupported);
    }
    if wire.drawing.get().len() > limits.drawing_bytes {
        return Err(ProjectWireError::Budget);
    }
    let assets: Vec<serde::de::IgnoredAny> =
        serde_json::from_str(wire.assets.get()).map_err(|_| ProjectWireError::Invalid)?;
    if wire.viewport_reference.get() != "null" || !assets.is_empty() {
        return Err(ProjectWireError::Unsupported);
    }
    let metadata = ProjectMetadata::new(
        wire.project.canvas_w,
        wire.project.canvas_h,
        wire.project.fps,
    )
    .map_err(|_| ProjectWireError::Invalid)?;
    let drawing = crate::object_codec::decode_storage_project(wire.drawing.get().as_bytes())
        .map_err(ProjectWireError::Drawing)?;
    Ok(ProjectEnvelope::no_media(metadata, drawing))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Output<'a> {
    format: &'static str,
    format_version: u32,
    project: Metadata,
    drawing: &'a RawValue,
    viewport_reference: (),
    assets: [(); 0],
}

// Check each write BEFORE reserving/copying it. Growth is capped too, so
// neither serialized length nor Vec capacity can exceed the caller limit.
struct CappedWriter {
    bytes: Vec<u8>,
    cap: usize,
    refused: bool,
}
impl std::io::Write for CappedWriter {
    fn write(&mut self, chunk: &[u8]) -> std::io::Result<usize> {
        let needed = self.bytes.len().checked_add(chunk.len());
        if needed.is_none_or(|n| n > self.cap) {
            self.refused = true;
            return Err(std::io::Error::other("project wire byte budget"));
        }
        let needed = needed.unwrap();
        if needed > self.bytes.capacity() {
            let capacity = needed
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.cap);
            if self
                .bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .is_err()
            {
                self.refused = true;
                return Err(std::io::Error::other("project wire reservation"));
            }
        }
        self.bytes.extend_from_slice(chunk);
        Ok(chunk.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn bounded_json(value: &impl Serialize, cap: usize) -> Result<Vec<u8>, ProjectWireError> {
    let mut writer = CappedWriter {
        bytes: Vec::new(),
        cap,
        refused: false,
    };
    serde_json::to_writer(&mut writer, value).map_err(|_| {
        if writer.refused {
            ProjectWireError::Budget
        } else {
            ProjectWireError::Invalid
        }
    })?;
    Ok(writer.bytes)
}

pub(crate) fn encode_project(
    envelope: &ProjectEnvelope,
    limits: ProjectWireLimits,
) -> Result<Vec<u8>, ProjectWireError> {
    if envelope.reference_state() != ProjectReferenceState::NoMedia {
        return Err(ProjectWireError::Unsupported);
    }
    crate::object_codec::validate_storage(envelope.drawing()).map_err(ProjectWireError::Drawing)?;
    let drawing = bounded_json(envelope.drawing(), limits.drawing_bytes)?;
    let raw =
        serde_json::from_slice::<&RawValue>(&drawing).map_err(|_| ProjectWireError::Invalid)?;
    let (canvas_w, canvas_h) = envelope.project().canvas_dimensions();
    let wire = Output {
        format: FORMAT,
        format_version: 1,
        project: Metadata {
            canvas_w,
            canvas_h,
            fps: envelope.project().fps(),
        },
        drawing: raw,
        viewport_reference: (),
        assets: [],
    };
    bounded_json(&wire, limits.envelope_bytes)
}

#[cfg(test)]
#[path = "../tests/project_media_codec.rs"]
mod tests;
