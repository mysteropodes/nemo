//! Strict private project wire; drawing admission always sees original bytes.
use crate::object_document::{ProjectEnvelope, ProjectMetadata, ProjectReferenceState};
#[cfg(feature = "project_media_codec")]
use crate::reference_image_asset::{ImageAssetError, ImageAssetLimits};
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
    #[cfg(feature = "project_media_codec")]
    Png(ImageAssetError),
}

/// Explicit caller limits cannot exceed the staged file ceilings.
#[derive(Debug, Clone, Copy)]
pub(crate) struct ProjectWireLimits {
    envelope_bytes: usize,
    drawing_bytes: usize,
    #[cfg(feature = "project_media_codec")]
    png: Option<(usize, ImageAssetLimits)>,
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
            #[cfg(feature = "project_media_codec")]
            png: None,
        })
    }
    #[cfg(feature = "project_media_codec")]
    pub(crate) fn with_png_limits(
        mut self,
        encoded: usize,
        rgba: usize,
        scratch: usize,
        width: u32,
        height: u32,
    ) -> Result<Self, ProjectWireError> {
        let png = ImageAssetLimits::new(encoded, rgba, scratch, width, height)
            .map_err(|_| ProjectWireError::Budget)?;
        self.png = Some((encoded, png));
        Ok(self)
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
    let asset = asset_entry(wire.assets)?;
    if wire.viewport_reference.get() == "null" && asset.is_some() {
        return Err(ProjectWireError::Unsupported);
    }
    #[cfg(not(feature = "project_media_codec"))]
    if wire.viewport_reference.get() != "null" {
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
    let reference = if wire.viewport_reference.get() == "null" {
        if asset.is_some() {
            return Err(ProjectWireError::Unsupported);
        }
        ProjectReferenceState::NoMedia
    } else {
        #[cfg(not(feature = "project_media_codec"))]
        {
            return Err(ProjectWireError::Unsupported);
        }
        #[cfg(feature = "project_media_codec")]
        {
            decode_reference(
                wire.viewport_reference,
                asset.ok_or(ProjectWireError::Invalid)?,
                limits,
            )?
        }
    };
    Ok(ProjectEnvelope::no_media(metadata, drawing).with_reference(reference))
}

// At most one borrowed asset: a long input table never builds a large Vec.
fn asset_entry(raw: &RawValue) -> Result<Option<&RawValue>, ProjectWireError> {
    struct One;
    impl<'de> serde::de::Visitor<'de> for One {
        type Value = Option<&'de RawValue>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("zero or one asset")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            let first = seq.next_element()?;
            if seq.next_element::<&RawValue>()?.is_some() {
                return Err(serde::de::Error::custom("extra asset"));
            }
            Ok(first)
        }
    }
    serde::Deserializer::deserialize_seq(&mut serde_json::Deserializer::from_str(raw.get()), One)
        .map_err(|_| ProjectWireError::Invalid)
}

#[cfg(feature = "project_media_codec")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Reference<'a> {
    #[serde(borrow)]
    kind: &'a RawValue,
    #[serde(borrow)]
    asset_id: &'a RawValue,
    #[serde(borrow)]
    name: &'a RawValue,
    visible: bool,
    opacity: f64,
    offset_frames: i64,
}
#[cfg(feature = "project_media_codec")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Asset<'a> {
    #[serde(borrow)]
    id: &'a RawValue,
    #[serde(borrow)]
    media_type: &'a RawValue,
    #[serde(borrow)]
    encoding: &'a RawValue,
    #[serde(borrow)]
    data: &'a RawValue,
}
#[cfg(feature = "project_media_codec")]
fn object<'a, T: Deserialize<'a>>(raw: &'a RawValue) -> Result<T, ProjectWireError> {
    if !raw.get().starts_with('{') {
        return Err(ProjectWireError::Invalid);
    }
    serde_json::from_str(raw.get()).map_err(|_| ProjectWireError::Invalid)
}
#[cfg(feature = "project_media_codec")]
fn text(raw: &RawValue, limit: usize) -> Result<String, ProjectWireError> {
    // JSON escapes expand spelling by at most six bytes per decoded byte.
    if raw.get().len()
        > limit
            .checked_mul(6)
            .and_then(|n| n.checked_add(2))
            .ok_or(ProjectWireError::Budget)?
    {
        return Err(ProjectWireError::Budget);
    }
    let value: String = serde_json::from_str(raw.get()).map_err(|_| ProjectWireError::Invalid)?;
    if value.len() > limit {
        return Err(ProjectWireError::Budget);
    }
    Ok(value)
}
#[cfg(feature = "project_media_codec")]
fn decode_reference(
    reference: &RawValue,
    asset: &RawValue,
    limits: ProjectWireLimits,
) -> Result<ProjectReferenceState, ProjectWireError> {
    let (cap, png) = limits.png.ok_or(ProjectWireError::Budget)?;
    let r: Reference<'_> = object(reference)?;
    let a: Asset<'_> = object(asset)?;
    if text(r.kind, 11)? != "still-image"
        || text(a.media_type, 9)? != "image/png"
        || text(a.encoding, 6)? != "base64"
    {
        return Err(ProjectWireError::Unsupported);
    }
    let id = text(r.asset_id, 255)?;
    let name = text(r.name, 255)?;
    if id != text(a.id, 255)?
        || id.is_empty()
        || id.contains('\0')
        || name.is_empty()
        || name.contains(['/', '\\', '\0'])
        || matches!(name.as_str(), "." | "..")
        || !r.opacity.is_finite()
        || !(0.0..=1.0).contains(&r.opacity)
        || r.offset_frames.unsigned_abs() > 9_007_199_254_740_991
    {
        return Err(ProjectWireError::Invalid);
    }
    let chars = base64::encoded_len(cap, true).ok_or(ProjectWireError::Budget)?;
    let data = text(a.data, chars)?;
    if data.is_empty() || data.len() % 4 != 0 {
        return Err(ProjectWireError::Invalid);
    }
    let padding = data.bytes().rev().take_while(|b| *b == b'=').count();
    if padding > 2 {
        return Err(ProjectWireError::Invalid);
    }
    let predicted = (data.len() / 4)
        .checked_mul(3)
        .and_then(|n| n.checked_sub(padding))
        .ok_or(ProjectWireError::Budget)?;
    if predicted == 0 || predicted > cap {
        return Err(ProjectWireError::Budget);
    }
    let mut encoded = Vec::new();
    encoded
        .try_reserve_exact(predicted)
        .map_err(|_| ProjectWireError::Budget)?;
    encoded.resize(predicted, 0);
    let mut reader = base64::read::DecoderReader::new(
        data.as_bytes(),
        &base64::engine::general_purpose::STANDARD,
    );
    std::io::Read::read_exact(&mut reader, &mut encoded).map_err(|_| ProjectWireError::Invalid)?;
    if std::io::Read::read(&mut reader, &mut [0]).map_err(|_| ProjectWireError::Invalid)? != 0 {
        return Err(ProjectWireError::Invalid);
    }
    // The decoded RGBA and helper's encoded copy NEVER enter the document.
    let admitted =
        crate::reference_image_asset::decode_png(&encoded, png).map_err(ProjectWireError::Png)?;
    drop(admitted);
    ProjectReferenceState::admitted_still(
        id,
        name,
        r.visible,
        r.opacity,
        r.offset_frames,
        encoded,
        cap,
    )
    .map_err(|_| ProjectWireError::Invalid)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Output<'a, R, A> {
    format: &'static str,
    format_version: u32,
    project: Metadata,
    drawing: &'a RawValue,
    viewport_reference: R,
    assets: A,
}

#[cfg(feature = "project_media_codec")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceOutput<'a> {
    kind: &'static str,
    asset_id: &'a str,
    name: &'a str,
    visible: bool,
    opacity: f64,
    offset_frames: i64,
}
#[cfg(feature = "project_media_codec")]
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AssetOutput<'a> {
    id: &'a str,
    media_type: &'static str,
    encoding: &'static str,
    data: Encoded<'a>,
}
#[cfg(feature = "project_media_codec")]
struct Encoded<'a>(&'a [u8]);
#[cfg(feature = "project_media_codec")]
impl Serialize for Encoded<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&base64::display::Base64Display::new(
            self.0,
            &base64::engine::general_purpose::STANDARD,
        ))
    }
}

// Cap logical bytes and reservation REQUESTS before copying. Allocator
// over-provisioning/actual Vec capacity is not bounded by reserve_exact.
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
    let reference = envelope.reference_state();
    #[cfg(not(feature = "project_media_codec"))]
    if reference != ProjectReferenceState::NoMedia {
        return Err(ProjectWireError::Unsupported);
    }
    #[cfg(feature = "project_media_codec")]
    if let ProjectReferenceState::StillImage(data) = &reference {
        let (cap, _) = limits.png.ok_or(ProjectWireError::Budget)?;
        if data.encoded_png().len() > cap
            || base64::encoded_len(data.encoded_png().len(), true)
                .ok_or(ProjectWireError::Budget)?
                > limits.envelope_bytes
        {
            return Err(ProjectWireError::Budget);
        }
    }
    crate::object_codec::validate_storage(envelope.drawing()).map_err(ProjectWireError::Drawing)?;
    let drawing = bounded_json(envelope.drawing(), limits.drawing_bytes)?;
    let raw =
        serde_json::from_slice::<&RawValue>(&drawing).map_err(|_| ProjectWireError::Invalid)?;
    let (canvas_w, canvas_h) = envelope.project().canvas_dimensions();
    let project = Metadata {
        canvas_w,
        canvas_h,
        fps: envelope.project().fps(),
    };
    #[cfg(feature = "project_media_codec")]
    if let ProjectReferenceState::StillImage(data) = &reference {
        let viewport_reference = ReferenceOutput {
            kind: "still-image",
            asset_id: data.id(),
            name: data.name(),
            visible: data.visible(),
            opacity: data.opacity(),
            offset_frames: data.offset_frames(),
        };
        let assets = [AssetOutput {
            id: data.id(),
            media_type: "image/png",
            encoding: "base64",
            data: Encoded(data.encoded_png()),
        }];
        return bounded_json(
            &Output {
                format: FORMAT,
                format_version: 1,
                project,
                drawing: raw,
                viewport_reference,
                assets,
            },
            limits.envelope_bytes,
        );
    }
    let wire = Output {
        format: FORMAT,
        format_version: 1,
        project,
        drawing: raw,
        viewport_reference: (),
        assets: [(); 0],
    };
    bounded_json(&wire, limits.envelope_bytes)
}

#[cfg(test)]
#[path = "../tests/project_media_codec.rs"]
mod tests;
