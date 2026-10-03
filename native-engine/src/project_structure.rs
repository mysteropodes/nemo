//! Read-only normalization of the legacy layers/frames envelope.
//!
//! This is a shape check, not native document or format-version admission. All
//! other fields (including geometry, components and media) remain opaque JSON.
//! No document identity, revision, history or application state is allocated.
//! Parity is semantic over the characterized safe-number JSON corpus; byte
//! formatting, arbitrary-precision numbers and JavaScript object identity are
//! not preserved.

use serde_json::{json, Value};
use std::fmt::{Display, Formatter};

/// An owned, structurally validated envelope with no mutable public accessor.
#[derive(Debug, Clone, PartialEq)]
pub struct ProjectStructure {
    value: Value,
}

impl ProjectStructure {
    pub fn value(&self) -> &Value {
        &self.value
    }

    /// Serialize retained JSON values, without claiming byte-for-byte fidelity.
    pub fn to_json(&self) -> String {
        self.value.to_string()
    }
}

/// Indices are zero-based; legacy display messages use one-based indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectStructureError {
    InvalidJson,
    InvalidRoot,
    Layers,
    LayerFrames {
        layer_index: usize,
    },
    FrameStrokes {
        layer_index: usize,
        frame_index: usize,
    },
}

impl Display for ProjectStructureError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidJson => formatter.write_str("Invalid JSON"),
            Self::InvalidRoot => formatter.write_str("Invalid"),
            Self::Layers => formatter.write_str("Fichier invalide (layers)"),
            Self::LayerFrames { layer_index } => {
                write!(formatter, "Fichier invalide (calque {})", layer_index + 1)
            }
            Self::FrameStrokes {
                layer_index,
                frame_index,
            } => write!(
                formatter,
                "Fichier invalide (calque {}, frame {})",
                layer_index + 1,
                frame_index + 1
            ),
        }
    }
}

impl std::error::Error for ProjectStructureError {}

/// Match `src/js/project-document.js:parse` for the characterized JSON envelope.
/// A success cannot be used as admission to the native opacity codec or editor.
pub fn parse_project_structure(json: &str) -> Result<ProjectStructure, ProjectStructureError> {
    let mut value: Value =
        serde_json::from_str(json).map_err(|_| ProjectStructureError::InvalidJson)?;
    let root = value
        .as_object_mut()
        .ok_or(ProjectStructureError::InvalidRoot)?;
    let has_layers = root.get("layers").is_some_and(js_truthy);
    let has_frames = root.get("frames").is_some_and(js_truthy);
    if !has_layers && !has_frames {
        return Err(ProjectStructureError::InvalidRoot);
    }
    if !has_layers {
        // Keep the original top-level frames and all other unknown properties.
        // Presence alone is insufficient: false, zero, null and "" migrate too.
        root.insert(
            "layers".into(),
            json!([{
                "name": "Layer 1", "visible": true, "locked": false,
                "frames": root.get("frames")
            }]),
        );
    }
    let layers = root
        .get("layers")
        .and_then(Value::as_array)
        .filter(|layers| !layers.is_empty())
        .ok_or(ProjectStructureError::Layers)?;
    for (layer_index, layer) in layers.iter().enumerate() {
        let frames = layer
            .get("frames")
            .and_then(Value::as_array)
            .ok_or(ProjectStructureError::LayerFrames { layer_index })?;
        for (frame_index, frame) in frames.iter().enumerate() {
            if !frame.get("strokes").is_some_and(Value::is_array) {
                return Err(ProjectStructureError::FrameStrokes {
                    layer_index,
                    frame_index,
                });
            }
        }
    }
    Ok(ProjectStructure { value })
}

// JSON has neither undefined nor NaN. Empty arrays and objects are JS-truthy.
fn js_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_some_and(|number| number != 0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}
