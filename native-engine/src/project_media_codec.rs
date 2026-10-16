//! Strict private project wire; drawing admission always sees original bytes.
use crate::object_document::{Presence, ProjectEnvelope, ProjectMetadata, ProjectReferenceState};
#[cfg(feature = "project_media_codec")]
use crate::reference_image_asset::{ImageAssetError, ImageAssetLimits};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;

#[cfg(feature = "project_media_codec")]
#[path = "project_asset_wire.rs"]
mod asset_wire;
#[cfg(feature = "project_media_codec")]
pub(crate) use asset_wire::{prepare_project_wire, DecodedProjectWire, PreparedProjectWire};

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
#[cfg(feature = "project_media_codec")]
impl From<ImageAssetError> for ProjectWireError {
    fn from(error: ImageAssetError) -> Self {
        Self::Png(error)
    }
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
    #[serde(skip_serializing_if = "Presence::missing")]
    canvas_bg: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    wa_in: Presence<f64>,
    #[serde(skip_serializing_if = "Presence::missing")]
    wa_out: Presence<f64>,
}
// Derived decoding alone admits positional arrays; reuse the drawing's
// object-only visitor with strict duplicate/unknown/required-member checks.
crate::object_document::object_deserialize!(Metadata {
    canvas_w: u32,
    canvas_h: u32,
    fps: f64,
    #[serde(default)] canvas_bg: Presence<String>,
    #[serde(default)] wa_in: Presence<f64>,
    #[serde(default)] wa_out: Presence<f64>
});

impl Metadata {
    fn into_metadata(self) -> Result<ProjectMetadata, ProjectWireError> {
        ProjectMetadata::new(self.canvas_w, self.canvas_h, self.fps)
            .and_then(|value| {
                value.with_canvas_background(self.canvas_bg.value().map(String::as_str))
            })
            .and_then(|value| {
                value.with_work_area(self.wa_in.value().copied(), self.wa_out.value().copied())
            })
            .map_err(|_| ProjectWireError::Invalid)
    }
    fn from_metadata(value: &ProjectMetadata) -> Self {
        let (canvas_w, canvas_h) = value.canvas_dimensions();
        let (wa_in, wa_out) = value.work_area();
        Self {
            canvas_w,
            canvas_h,
            fps: value.fps(),
            canvas_bg: value
                .canvas_background()
                .map(|v| Presence::Present(v.to_owned()))
                .unwrap_or(Presence::Missing),
            wa_in: wa_in.map(Presence::Present).unwrap_or(Presence::Missing),
            wa_out: wa_out.map(Presence::Present).unwrap_or(Presence::Missing),
        }
    }
}

pub(crate) fn decode_project(
    bytes: &[u8],
    limits: ProjectWireLimits,
) -> Result<ProjectEnvelope, ProjectWireError> {
    decode_allocated_project(bytes, limits, |predicted| {
        let mut encoded = Vec::new();
        encoded
            .try_reserve_exact(predicted)
            .map_err(|_| ProjectWireError::Budget)?;
        encoded.resize(predicted, 0);
        Ok(encoded)
    })
}
pub(crate) fn decode_allocated_project(
    bytes: &[u8],
    limits: ProjectWireLimits,
    allocate: impl FnMut(usize) -> Result<Vec<u8>, ProjectWireError>,
) -> Result<ProjectEnvelope, ProjectWireError> {
    decode_validated_project(
        bytes,
        limits,
        allocate,
        #[cfg(feature = "project_media_codec")]
        |source, png| {
            crate::reference_image_asset::validate_png(source, png)
                .map(|_| ())
                .map_err(ProjectWireError::Png)
        },
    )
}
pub(crate) fn decode_validated_project(
    bytes: &[u8],
    limits: ProjectWireLimits,
    mut allocate: impl FnMut(usize) -> Result<Vec<u8>, ProjectWireError>,
    #[cfg(feature = "project_media_codec")] validate: impl FnMut(
        &[u8],
        ImageAssetLimits,
    ) -> Result<(), ProjectWireError>,
) -> Result<ProjectEnvelope, ProjectWireError> {
    #[cfg(feature = "project_media_codec")]
    {
        return prepare_project_wire(bytes, limits)?
            .decode(allocate, validate)
            .map(|v| v.into_project());
    }
    #[cfg(not(feature = "project_media_codec"))]
    {
        if bytes.len() > limits.envelope_bytes {
            return Err(ProjectWireError::Budget);
        }
        if bytes.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'{') {
            return Err(ProjectWireError::Invalid);
        }
        let wire: Input<'_> =
            serde_json::from_slice(bytes).map_err(|_| ProjectWireError::Invalid)?;
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
        let metadata = wire.project.into_metadata()?;
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
                    &mut allocate,
                    validate,
                )?
            }
        };
        Ok(ProjectEnvelope::no_media(metadata, drawing).with_reference(reference))
    }
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
    allocate: &mut impl FnMut(usize) -> Result<Vec<u8>, ProjectWireError>,
    validate: impl FnOnce(&[u8], ImageAssetLimits) -> Result<(), ProjectWireError>,
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
    let mut encoded = allocate(predicted)?;
    if encoded.len() != predicted {
        return Err(ProjectWireError::Budget);
    }
    let mut reader = base64::read::DecoderReader::new(
        data.as_bytes(),
        &base64::engine::general_purpose::STANDARD,
    );
    std::io::Read::read_exact(&mut reader, &mut encoded).map_err(|_| ProjectWireError::Invalid)?;
    if std::io::Read::read(&mut reader, &mut [0]).map_err(|_| ProjectWireError::Invalid)? != 0 {
        return Err(ProjectWireError::Invalid);
    }
    // Admission reuses decoder checks, not a pixel/encoded-copy result.
    validate(&encoded, png)?;
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

#[path = "project_media_output.rs"]
mod output;
pub(crate) use output::encode_project;
#[cfg(test)]
use output::CappedWriter;

#[cfg(test)]
#[path = "../tests/project_media_codec.rs"]
mod tests;
