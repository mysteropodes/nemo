//! Strict admission and serialization for the bounded opacity document.

use crate::document::{
    CurvePoint, OpacityDocument, OpacityKey, OPACITY_DOCUMENT_FORMAT,
    OPACITY_DOCUMENT_FORMAT_VERSION,
};
use serde_json::{Number, Value};
use std::collections::HashSet;
use std::fmt::{Display, Formatter};

const DEFAULT_CURVE: [[f64; 2]; 5] = [
    [0.0, 0.0],
    [0.25, 0.156],
    [0.5, 0.5],
    [0.75, 0.844],
    [1.0, 1.0],
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecErrorKind {
    Parse,
    Invalid,
    Unsupported,
    DuplicateId,
    Encode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodecError {
    kind: CodecErrorKind,
    message: String,
}

impl CodecError {
    pub fn kind(&self) -> CodecErrorKind {
        self.kind
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    fn new(kind: CodecErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl Display for CodecError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CodecError {}

/// Parse and validate a complete candidate before returning an admitted value.
pub fn decode_project(bytes: &[u8]) -> Result<OpacityDocument, CodecError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|error| {
        CodecError::new(CodecErrorKind::Parse, format!("project JSON: {error}"))
    })?;
    let document: OpacityDocument = serde_json::from_value(value).map_err(structure_error)?;
    validate(&document)?;
    Ok(document)
}

/// Serialize only a value that still satisfies the admission invariant.
pub fn encode_project(document: &OpacityDocument) -> Result<Vec<u8>, CodecError> {
    validate(document)?;
    serde_json::to_vec(document)
        .map_err(|error| CodecError::new(CodecErrorKind::Encode, error.to_string()))
}

fn structure_error(error: serde_json::Error) -> CodecError {
    let message = error.to_string();
    let kind = if message.contains("unknown field") {
        CodecErrorKind::Unsupported
    } else {
        CodecErrorKind::Invalid
    };
    CodecError::new(kind, message)
}

fn validate(document: &OpacityDocument) -> Result<(), CodecError> {
    if document.format != OPACITY_DOCUMENT_FORMAT
        || document.format_version != OPACITY_DOCUMENT_FORMAT_VERSION
    {
        return unsupported("unsupported opacity document format or formatVersion");
    }
    if document.total_frames == 0 {
        return invalid("totalFrames must be positive");
    }
    if document.layers.is_empty() {
        return invalid("at least one identified opacity layer is required");
    }

    let mut layer_ids = HashSet::with_capacity(document.layers.len());
    for layer in &document.layers {
        if layer.layer_uid.is_empty() {
            return invalid("layerUid must be an opaque non-empty string");
        }
        if !layer_ids.insert(layer.layer_uid.as_str()) {
            return Err(CodecError::new(
                CodecErrorKind::DuplicateId,
                format!("duplicate layerUid: {}", layer.layer_uid),
            ));
        }
        if layer.motion_static.is_none() && layer.motion.is_none() {
            return invalid("each layer must store static or keyed opacity");
        }
        if let Some(motion) = &layer.motion_static {
            validate_opacity(&motion.opacity[0], "motionStatic.opacity[0]")?;
        }
        if let Some(motion) = &layer.motion {
            validate_track(&motion.opacity.keys, document.total_frames)?;
        }
    }
    Ok(())
}

fn validate_track(keys: &[OpacityKey], total_frames: u32) -> Result<(), CodecError> {
    if keys.is_empty() {
        return invalid("motion.opacity.keys must not be empty");
    }
    let mut previous = None;
    for key in keys {
        if key.frame >= total_frames {
            return invalid(format!(
                "opacity key frame {} is outside totalFrames {}",
                key.frame, total_frames
            ));
        }
        if let Some(frame) = previous {
            if key.frame == frame {
                return invalid(format!("duplicate opacity key frame: {}", key.frame));
            }
            if key.frame < frame {
                return invalid("opacity key frames must be strictly increasing");
            }
        }
        previous = Some(key.frame);
        validate_opacity(&key.v[0], "motion.opacity.keys[].v[0]")?;
        validate_curve(&key.curve_points)?;
        validate_zero_handle(&key.h_out, "hOut")?;
        validate_zero_handle(&key.h_in, "hIn")?;
    }
    Ok(())
}

fn validate_opacity(value: &Number, context: &str) -> Result<(), CodecError> {
    let Some(value) = value.as_f64() else {
        return invalid(format!("{context} must be a finite number"));
    };
    if !(0.0..=100.0).contains(&value) {
        return invalid(format!("{context} must be in the range 0..100"));
    }
    Ok(())
}

fn validate_curve(points: &[CurvePoint]) -> Result<(), CodecError> {
    if points.len() != DEFAULT_CURVE.len() {
        return invalid("curvePoints must contain the five characterized points");
    }
    for (index, (point, expected)) in points.iter().zip(DEFAULT_CURVE).enumerate() {
        let actual = [
            number(&point.x, "curvePoints.x")?,
            number(&point.y, "curvePoints.y")?,
        ];
        if actual != expected {
            return unsupported(format!(
                "curvePoints[{index}] differs from the characterized opacity curve"
            ));
        }
    }
    Ok(())
}

fn validate_zero_handle(handle: &[Number; 2], name: &str) -> Result<(), CodecError> {
    let values = [number(&handle[0], name)?, number(&handle[1], name)?];
    if values != [0.0, 0.0] {
        return unsupported(format!("non-zero opacity {name} is unsupported"));
    }
    Ok(())
}

fn number(value: &Number, context: &str) -> Result<f64, CodecError> {
    value.as_f64().ok_or_else(|| {
        CodecError::new(CodecErrorKind::Invalid, format!("{context} must be finite"))
    })
}

fn invalid<T>(message: impl Into<String>) -> Result<T, CodecError> {
    Err(CodecError::new(CodecErrorKind::Invalid, message))
}

fn unsupported<T>(message: impl Into<String>) -> Result<T, CodecError> {
    Err(CodecError::new(CodecErrorKind::Unsupported, message))
}
