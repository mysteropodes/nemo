//! Immutable supplied project settings, not a document owner, wire codec or UI command.
//! Supplied settings share the existing project revision owner.

#[path = "project_metadata_fields.rs"]
mod visual;
#[path = "project_metadata_annotations.rs"]
mod annotations;
pub(crate) use annotations::{ProjectCommentRecord,ProjectWorkspaceRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectMetadataError {
    InvalidCanvas,
    InvalidFps,
    InvalidCanvasBackground,
    InvalidWorkArea,
    InvalidCompositionMetadata,
    InvalidVisualMetadata,
    InvalidAnnotations,
}

/// Exact accepted spelling plus straight sRGB RGBA8, including the source alpha.
/// A fixed ASCII buffer retains case, shorthand and optional '#' without an allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CanvasBackground {
    spelling: [u8; 9],
    length: u8,
    rgba: [u8; 4],
}
impl CanvasBackground {
    fn parse(value: &str) -> Result<Self, ProjectMetadataError> {
        let source = value.as_bytes();
        let digits = source.strip_prefix(b"#").unwrap_or(source);
        if !matches!(digits.len(), 3 | 6 | 8) {
            return Err(ProjectMetadataError::InvalidCanvasBackground);
        }
        fn digit(value: u8) -> Result<u8, ProjectMetadataError> {
            match value {
                b'0'..=b'9' => Ok(value - b'0'),
                b'a'..=b'f' => Ok(value - b'a' + 10),
                b'A'..=b'F' => Ok(value - b'A' + 10),
                _ => Err(ProjectMetadataError::InvalidCanvasBackground),
            }
        }
        let mut rgba = [0, 0, 0, 255];
        if digits.len() == 3 {
            for (index, value) in digits.iter().enumerate() {
                rgba[index] = digit(*value)? * 17;
            }
        } else {
            for (index, pair) in digits.chunks_exact(2).enumerate() {
                rgba[index] = digit(pair[0])? * 16 + digit(pair[1])?;
            }
        }
        let mut spelling = [0; 9];
        spelling[..source.len()].copy_from_slice(source);
        Ok(Self {
            spelling,
            length: source.len() as u8,
            rgba,
        })
    }
    fn as_str(&self) -> &str {
        // Private construction admits ASCII hex bytes and one optional leading '#'.
        std::str::from_utf8(&self.spelling[..usize::from(self.length)])
            .expect("validated ASCII canvas background")
    }
}

/// Supplied metadata only. Missing optional fields remain missing, never white or
/// a synthesized full-timeline work area. totalFrames remains ObjectDocument-owned.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProjectMetadata {
    canvas_width: u32,
    canvas_height: u32,
    fps: f64,
    canvas_background: Option<CanvasBackground>,
    wa_in: Option<f64>,
    wa_out: Option<f64>,
    markers: Option<std::sync::Arc<[(f64, Option<String>, Option<String>)]>>,
    bpm: Option<f64>,
    bpm_offset: Option<f64>,
    bpm_show: Option<bool>,
    visual: visual::VisualMetadata,
    annotations: annotations::Annotations,
}
impl ProjectMetadata {
    /// The existing constructor API and positive finite storage FPS are preserved.
    pub(crate) fn new(
        canvas_width: u32,
        canvas_height: u32,
        fps: f64,
    ) -> Result<Self, ProjectMetadataError> {
        if canvas_width == 0 || canvas_height == 0 {
            return Err(ProjectMetadataError::InvalidCanvas);
        }
        if !fps.is_finite() || fps <= 0.0 {
            return Err(ProjectMetadataError::InvalidFps);
        }
        Ok(Self {
            canvas_width,
            canvas_height,
            fps,
            canvas_background: None,
            wa_in: None,
            wa_out: None,
            markers: None,
            bpm: None,
            bpm_offset: None,
            bpm_show: None,
            visual: visual::VisualMetadata::default(),
            annotations: annotations::Annotations::default(),
        })
    }
    pub(crate) fn with_canvas_dimensions(
        self,
        width: u32,
        height: u32,
    ) -> Result<Self, ProjectMetadataError> {
        if width == 0 || height == 0 {
            return Err(ProjectMetadataError::InvalidCanvas);
        }
        Ok(Self {
            canvas_width: width,
            canvas_height: height,
            ..self
        })
    }
    pub(crate) fn with_fps(self, fps: f64) -> Result<Self, ProjectMetadataError> {
        if !fps.is_finite() || fps <= 0.0 {
            return Err(ProjectMetadataError::InvalidFps);
        }
        Ok(Self { fps, ..self })
    }
    pub(crate) fn with_canvas_background(
        self,
        value: Option<&str>,
    ) -> Result<Self, ProjectMetadataError> {
        let canvas_background = value.map(CanvasBackground::parse).transpose()?;
        Ok(Self {
            canvas_background,
            ..self
        })
    }
    /// Storage does not apply UI clamps or normalize ranges. The direct legacy
    /// setter/import preserve fractional, negative, inverted and out-of-range numbers.
    pub(crate) fn with_work_area(
        self,
        wa_in: Option<f64>,
        wa_out: Option<f64>,
    ) -> Result<Self, ProjectMetadataError> {
        if wa_in.is_some_and(|value| !value.is_finite())
            || wa_out.is_some_and(|value| !value.is_finite())
        {
            return Err(ProjectMetadataError::InvalidWorkArea);
        }
        Ok(Self {
            wa_in,
            wa_out,
            ..self
        })
    }
    /// Exact supplied root-composition metadata. A shared immutable collection
    /// keeps drawing/media-preserving envelope edits cheap; order is authored order.
    pub(crate) fn with_composition_metadata(
        self,
        markers: Option<Vec<(f64, Option<String>, Option<String>)>>,
        bpm: Option<f64>,
        bpm_offset: Option<f64>,
        bpm_show: Option<bool>,
    ) -> Result<Self, ProjectMetadataError> {
        let text_valid = |text: &Option<String>| {
            text.as_ref()
                .is_none_or(|v| v.len() <= 1024 && !v.contains('\0'))
        };
        if bpm.is_some_and(|v| !v.is_finite())
            || bpm_offset.is_some_and(|v| !v.is_finite())
            || markers.as_ref().is_some_and(|values| {
                values.len() > 4096
                    || values.iter().any(|(frame, name, color)| {
                        !frame.is_finite() || !text_valid(name) || !text_valid(color)
                    })
            })
        {
            return Err(ProjectMetadataError::InvalidCompositionMetadata);
        }
        Ok(Self {
            markers: markers.map(std::sync::Arc::from),
            bpm,
            bpm_offset,
            bpm_show,
            ..self
        })
    }
    pub(crate) fn composition_markers(&self) -> Option<&[(f64, Option<String>, Option<String>)]> {
        self.markers.as_deref()
    }
    pub(crate) fn composition_bpm(&self) -> (Option<f64>, Option<f64>, Option<bool>) {
        (self.bpm, self.bpm_offset, self.bpm_show)
    }
    /// A scalar tempo edit retains the existing shared marker allocation.
    /// Supplied finite values are storage values; UI clamps belong to intent.
    pub(crate) fn with_composition_bpm(
        self,
        bpm: Option<f64>,
        bpm_offset: Option<f64>,
        bpm_show: Option<bool>,
    ) -> Result<Self, ProjectMetadataError> {
        if bpm.is_some_and(|v| !v.is_finite()) || bpm_offset.is_some_and(|v| !v.is_finite()) {
            return Err(ProjectMetadataError::InvalidCompositionMetadata);
        }
        Ok(Self {
            bpm,
            bpm_offset,
            bpm_show,
            ..self
        })
    }
    pub(crate) fn canvas_dimensions(&self) -> (u32, u32) {
        (self.canvas_width, self.canvas_height)
    }
    pub(crate) fn fps(&self) -> f64 {
        self.fps
    }
    pub(crate) fn canvas_background(&self) -> Option<&str> {
        self.canvas_background
            .as_ref()
            .map(CanvasBackground::as_str)
    }
    pub(crate) fn canvas_background_rgba8(&self) -> Option<[u8; 4]> {
        self.canvas_background.map(|value| value.rgba)
    }
    pub(crate) fn work_area(&self) -> (Option<f64>, Option<f64>) {
        (self.wa_in, self.wa_out)
    }
}

#[cfg(test)]
#[path = "../tests/project_settings.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/project_composition_metadata.rs"]
mod project_composition_metadata;
