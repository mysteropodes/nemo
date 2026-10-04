//! Atomic admission and semantic JSON round-trip for staged native objects.
#[cfg(feature = "commands")]
pub(crate) use crate::object_document::object_deserialize;
// The codec admission port exposes the immutable types consumed by reads.
pub use crate::object_document::{ObjectDocument, ObjectRecord, ObjectTarget};
use crate::object_document::{OBJECT_DOCUMENT_FORMAT, OBJECT_DOCUMENT_FORMAT_VERSION};
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectCodecErrorKind {
    Parse,
    Invalid,
    Unsupported,
    DuplicateId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectCodecError {
    kind: ObjectCodecErrorKind,
    message: String,
}
impl ObjectCodecError {
    pub fn kind(&self) -> ObjectCodecErrorKind {
        self.kind
    }
    pub fn message(&self) -> &str {
        &self.message
    }
    pub(crate) fn new(kind: ObjectCodecErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for ObjectCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for ObjectCodecError {}

/// Deserialize directly into strict structs: a Value intermediate would erase
/// duplicate JSON member keys before admission could reject them.
pub fn decode_project(bytes: &[u8]) -> Result<ObjectDocument, ObjectCodecError> {
    let document: ObjectDocument = serde_json::from_slice(bytes).map_err(|error| {
        let kind = if error.to_string().contains("unknown field") {
            ObjectCodecErrorKind::Unsupported
        } else if error.is_syntax() || error.is_eof() {
            ObjectCodecErrorKind::Parse
        } else {
            ObjectCodecErrorKind::Invalid
        };
        ObjectCodecError::new(kind, error.to_string())
    })?;
    validate(&document)?;
    Ok(document)
}

pub fn encode_project(document: &ObjectDocument) -> Result<Vec<u8>, ObjectCodecError> {
    validate(document)?;
    serde_json::to_vec(document)
        .map_err(|error| ObjectCodecError::new(ObjectCodecErrorKind::Invalid, error.to_string()))
}

pub(crate) fn validate(document: &ObjectDocument) -> Result<(), ObjectCodecError> {
    use ObjectCodecErrorKind::{DuplicateId, Invalid, Unsupported};
    let reject = |kind, message: &str| ObjectCodecError::new(kind, message);
    if document.format != OBJECT_DOCUMENT_FORMAT
        || document.format_version != OBJECT_DOCUMENT_FORMAT_VERSION
    {
        return Err(reject(
            Unsupported,
            "unsupported object format or formatVersion",
        ));
    }
    if document.total_frames == 0 || document.layers.is_empty() {
        return Err(reject(
            Invalid,
            "positive totalFrames and identified layers are required",
        ));
    }
    let mut layers = HashSet::new();
    for layer in &document.layers {
        if layer.layer_uid.is_empty() {
            return Err(reject(Invalid, "empty layerUid"));
        }
        if !layers.insert(layer.layer_uid.as_str()) {
            return Err(reject(DuplicateId, "duplicate layerUid"));
        }
    }
    let mut targets = HashSet::new();
    for record in &document.objects {
        if record.schema_version != 1 || record.family != "closed-solid-fill-cubic-path" {
            return Err(reject(Unsupported, "unsupported object schema or family"));
        }
        if !record.target.valid() || record.target.frame_scope.frame >= document.total_frames {
            return Err(reject(Invalid, "invalid scoped object target or frame"));
        }
        if !layers.contains(record.target.layer_uid.as_str()) {
            return Err(reject(Invalid, "object layer reference is unresolved"));
        }
        if !targets.insert(&record.target) {
            return Err(reject(DuplicateId, "duplicate scoped object key"));
        }
        let geometry = &record.geometry;
        if geometry.kind != "cubic-path"
            || !geometry.closed
            || geometry.coordinate_space != "document-world"
            || geometry.handle_space != "relative"
        {
            return Err(reject(Unsupported, "unsupported cubic geometry"));
        }
        if !(2..=256).contains(&geometry.segments.len()) {
            return Err(reject(Invalid, "cubic geometry requires 2..256 segments"));
        }
        for segment in &geometry.segments {
            for point in [&segment.point, &segment.handle_in, &segment.handle_out] {
                if ![&point.x, &point.y]
                    .into_iter()
                    .all(|n| n.as_f64().is_some_and(f64::is_finite))
                {
                    return Err(reject(
                        Invalid,
                        "cubic coordinates must be finite f64 values",
                    ));
                }
            }
        }
        let fill = &record.fill;
        if fill.kind != "solid" {
            return Err(reject(Unsupported, "only solid fill is supported"));
        }
        if ![&fill.r, &fill.g, &fill.b, &fill.a].into_iter().all(|n| {
            n.as_f64()
                .is_some_and(|v| v.is_finite() && (0.0..=1.0).contains(&v))
        }) {
            return Err(reject(
                Invalid,
                "solid fill channels must be finite and within 0..1",
            ));
        }
    }
    Ok(())
}
