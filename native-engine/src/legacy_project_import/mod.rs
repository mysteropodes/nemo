//! Whole-project reader for the baseline (`1065e9d8`) project JSON. It converts the legacy `exportJSON` document into
//! the bytes of a native project file (`nemo.native-project-document`, drawing format v2) that the one strict codec then
//! admits; it creates no second document owner and never touches the legacy file.
//!
//! Slice 1 owns: version handling, the frames-only/legacyCurve/timeLinkOffsets/effectorChannels/matte migrations, canvas and
//! project settings, drawing layers (name, visibility, lock, colour, shy, parent, `layerUid`), keyframes and plain
//! vector strokes with their `strokeId`. Slice 2 adds the animation families: Motion (`motion`, `motionStatic`,
//! `elementMotion`, `expressions`, `exprControls`, the `order` property), blend and mattes, Null, Guide, Widget and Folder
//! layers with the folder table, the stored range with time link and markers, key lock, 3D and follow path, components
//! (`symbols` and their instance layers), duplicators and effector layers, the StoryBoard, tween records, generated tween
//! frames, motion arcs and the camera track. Slice 3 adds media and the remaining stroke and layer families: raster items over
//! PNG/linked image assets, audio tracks over WAV assets with their placement offsets, the reference image, the image-mesh
//! store, vector text blocks (an unbundled font keeps its outlines), the per-stroke `vectorExt` tags (mask, gradient, box angle,
//! linked fill, parametric shape, paint-bucket trace, brush texture, bitmap anchor, shadow channel), stroke and layer
//! effects, text animators, path effects, rigs and Cmd+G groups. `fields.rs` states the disposition of every baseline member
//! and the field-catalogue test enumerates them. Media in other formats (JPEG, MP3...) is converted by the host's
//! `MediaTranscoder`; without one it refuses. Every field family that holds content with no native owner refuses with the
//! typed `legacy_unsupported_field` naming the JSON path, so nothing is ever lost silently. Editor-only preferences are
//! listed in `ignored`; unknown members and baseline-hidden data are listed in `warnings`.
use serde_json::{json, Map, Value};

#[path = "admit.rs"]
mod admit;
#[path = "assemble.rs"]
mod assemble;
#[path = "classify.rs"]
mod classify;
#[path = "color.rs"]
mod color;
#[path = "components.rs"]
mod components;
#[path = "dup.rs"]
mod dup;
#[path = "fields.rs"]
mod fields;
#[path = "groups.rs"]
mod groups;
#[path = "holders.rs"]
mod holders;
#[path = "kinds.rs"]
mod kinds;
#[path = "layers.rs"]
mod layers;
#[path = "media.rs"]
mod media;
#[path = "migrate.rs"]
mod migrate;
#[path = "project.rs"]
mod project;
#[path = "stroke.rs"]
mod stroke;
#[path = "stroke_brush.rs"]
mod stroke_brush;
#[path = "stroke_ext.rs"]
mod stroke_ext;
#[path = "storyboard.rs"]
mod storyboard;
#[path = "text.rs"]
mod text;
#[path = "tweens.rs"]
mod tweens;
pub use admit::{convert_admitted, convert_admitted_with};
pub use media::{MediaTranscoder, NoTranscoder};

pub const LEGACY_UNSUPPORTED_FIELD: &str = "legacy_unsupported_field";
pub const LEGACY_INVALID: &str = "legacy_invalid";
/// The newest `version` the baseline `exportJSON` wrote; a larger one loads with a warning, as at baseline.
pub const BASELINE_VERSION: f64 = 13.0;

/// Typed refusal: the current document is never touched, the message names the JSON path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyImportError {
    code: &'static str,
    path: String,
    reason: String,
}
impl LegacyImportError {
    pub(crate) fn unsupported(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            code: LEGACY_UNSUPPORTED_FIELD,
            path: path.into(),
            reason: reason.into(),
        }
    }
    pub(crate) fn invalid(path: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            code: LEGACY_INVALID,
            path: path.into(),
            reason: reason.into(),
        }
    }
    pub fn code(&self) -> &'static str {
        self.code
    }
    pub fn path(&self) -> &str {
        &self.path
    }
    pub fn message(&self) -> String {
        format!("legacy project field {}: {}", self.path, self.reason)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegacyWarning {
    pub path: String,
    pub code: &'static str,
    pub message: String,
}

/// Field-path loss report returned with the opened document.
#[derive(Debug, Clone, PartialEq)]
pub struct LegacyReport {
    pub source_version: Option<f64>,
    pub migrations: Vec<String>,
    pub warnings: Vec<LegacyWarning>,
    pub ignored: Vec<String>,
    pub layers: usize,
    pub keyframes: usize,
    pub strokes: usize,
}
/// The wire report lists at most this many warnings; `warningsTotal` always carries the full count.
pub const MAX_REPORT_WARNINGS: usize = 50;

impl LegacyReport {
    pub fn to_json(&self) -> Value {
        json!({"sourceVersion": self.source_version, "migrations": self.migrations,
            "warnings": self.warnings.iter().take(MAX_REPORT_WARNINGS).map(|w| json!({"path": w.path, "code": w.code, "message": w.message})).collect::<Vec<_>>(),
            "warningsTotal": self.warnings.len(),
            "ignored": self.ignored,
            "counts": {"layers": self.layers, "keyframes": self.keyframes, "strokes": self.strokes}})
    }
}

pub struct Converted {
    pub project_file: Vec<u8>,
    pub report: LegacyReport,
    pub(crate) stroke_paths: admit::StrokePaths,
}

/// Mutable state shared by the conversion passes.
#[derive(Default)]
pub(crate) struct Ctx<'a> {
    pub(crate) warnings: Vec<LegacyWarning>,
    pub(crate) migrations: Vec<String>,
    pub(crate) ignored: Vec<String>,
    pub(crate) omit: Vec<&'static str>,
    pub(crate) keyframes: usize,
    pub(crate) strokes: usize,
    /// Baseline JSON path of every converted scene or component stroke, keyed by (layer uid, frame, stroke id).
    pub(crate) stroke_paths: std::collections::HashMap<(String, u32, String), String>,
    /// Catalog assets in first-use order, with the payload keys that deduplicate them.
    pub(crate) assets: Vec<Value>,
    pub(crate) asset_keys: std::collections::HashMap<String, String>,
    pub(crate) placements: Vec<Value>,
    /// Raster item entries of every context.
    pub(crate) raster: Vec<Value>,
    /// Innermost group leaf tags of vector strokes: (layer uid, stroke id, group id).
    pub(crate) group_tags: Vec<(String, String, String)>,
    pub(crate) transcoder: Option<&'a dyn MediaTranscoder>,
    /// Text item entries of every context.
    pub(crate) text: Vec<Value>,
}
impl Ctx<'_> {
    pub(crate) fn warn(
        &mut self,
        path: impl Into<String>,
        code: &'static str,
        message: impl Into<String>,
    ) {
        self.warnings.push(LegacyWarning {
            path: path.into(),
            code,
            message: message.into(),
        });
    }
}

/// Optional metadata groups, the order the host bisects them in when the native codec refuses the whole file.
pub const OPTIONAL_GROUPS: &[&str] = project::PASS_THROUGH;

/// A baseline project: a JSON object without a `format` member that holds `layers` or the oldest `frames`.
pub fn looks_like_legacy_project(bytes: &[u8]) -> bool {
    match serde_json::from_slice::<Value>(bytes) {
        Ok(Value::Object(map)) => {
            !map.contains_key("format")
                && (map.get("layers").is_some_and(Value::is_array)
                    || map.get("frames").is_some_and(Value::is_array))
        }
        _ => false,
    }
}

pub fn convert(bytes: &[u8]) -> Result<Converted, LegacyImportError> {
    convert_omitting(bytes, &[], &NoTranscoder)
}

pub(crate) fn convert_omitting(
    bytes: &[u8],
    omit: &[&'static str],
    transcoder: &dyn MediaTranscoder,
) -> Result<Converted, LegacyImportError> {
    let value: Value = serde_json::from_slice(bytes)
        .map_err(|_| LegacyImportError::invalid("$", "not a JSON document"))?;
    let Value::Object(mut doc) = value else {
        return Err(LegacyImportError::invalid(
            "$",
            "a project is a JSON object",
        ));
    };
    let mut ctx = Ctx {
        omit: omit.to_vec(),
        transcoder: Some(transcoder),
        ..Ctx::default()
    };
    let source_version = doc.get("version").and_then(Value::as_f64);
    if source_version.is_some_and(|v| v > BASELINE_VERSION) {
        ctx.warn("version", "newer_format_version", "this project was saved by a newer version of the app; fields it added are not carried over");
    }
    migrate::run(&mut doc, &mut ctx)?;
    let layer_values = doc
        .get("layers")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| LegacyImportError::invalid("layers", "layers must be a non-empty array"))?;
    let settings = project::read(&doc, &layer_values, &mut ctx)?;
    let mut drawing = assemble::drawing(&doc, &layer_values, settings.total, &mut ctx)?;
    if let Some(meshes) = media::image_meshes(&doc)? {
        drawing["imageMeshes"] = meshes;
    }
    if let Some(tracks) = media::audio_tracks(&doc, &mut ctx)? {
        drawing["audioTracks"] = tracks;
    }
    if !ctx.raster.is_empty() || !ctx.text.is_empty() {
        let mut items = Map::new();
        if !ctx.raster.is_empty() {
            items.insert("raster".into(), Value::Array(std::mem::take(&mut ctx.raster)));
        }
        if !ctx.text.is_empty() {
            items.insert("text".into(), Value::Array(std::mem::take(&mut ctx.text)));
        }
        drawing["items"] = Value::Object(items);
    }
    let reference = media::reference(&doc, &mut ctx)?;
    media::library(&doc, &mut ctx);
    classify::project_members(&doc, &mut ctx)?;
    let mut file = json!({"format": "nemo.native-project-document", "formatVersion": 1, "project": settings.metadata,
        "drawing": drawing, "viewportReference": reference, "assets": []});
    if !ctx.assets.is_empty() {
        let ids: Vec<Value> = ctx.assets.iter().map(|a| a["id"].clone()).collect();
        file["formatVersion"] = json!(2);
        file["assets"] = Value::Array(std::mem::take(&mut ctx.assets));
        file["assetInventory"] = json!({"assetIds": ids, "placements": std::mem::take(&mut ctx.placements)});
    }
    let project_file = serde_json::to_vec(&file)
        .map_err(|_| LegacyImportError::invalid("$", "converted project cannot be encoded"))?;
    let stroke_paths = std::mem::take(&mut ctx.stroke_paths);
    let report = LegacyReport {
        source_version,
        migrations: ctx.migrations,
        warnings: ctx.warnings,
        ignored: ctx.ignored,
        layers: layer_values.len(),
        keyframes: ctx.keyframes,
        strokes: ctx.strokes,
    };
    Ok(Converted {
        project_file,
        report,
        stroke_paths,
    })
}

pub(crate) fn object<'a>(
    value: &'a Value,
    path: &str,
) -> Result<&'a Map<String, Value>, LegacyImportError> {
    value
        .as_object()
        .ok_or_else(|| LegacyImportError::invalid(path, "an object is required"))
}

#[cfg(test)]
#[path = "../../tests/legacy_project_import_animation.rs"]
mod animation_tests;
#[cfg(test)]
#[path = "../../tests/legacy_project_import_families.rs"]
mod families_tests;
#[cfg(test)]
#[path = "../../tests/legacy_project_import_fields.rs"]
mod fields_tests;
#[cfg(test)]
#[path = "../../tests/legacy_project_import_media.rs"]
mod media_tests;
#[cfg(test)]
#[path = "../../tests/legacy_project_import_refusals.rs"]
mod refusal_tests;
#[cfg(test)]
#[path = "../../tests/legacy_project_import_unit.rs"]
mod unit_tests;
