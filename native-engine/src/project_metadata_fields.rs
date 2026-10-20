//! Supplied palette, guide and perspective storage. These are project fields,
//! distinct from transient tool selection, viewport overlays or local preferences.
use super::{ProjectMetadata, ProjectMetadataError};
use std::sync::Arc;

type Palette = (String, String, Vec<String>);
type ShadowColor = (String, String);
type GuideAxes = (Option<Arc<[f64]>>, Option<Arc<[f64]>>);
type VanishingPoint = (f64, f64, Option<bool>);

#[derive(Debug, Clone, Default, PartialEq)]
pub(super) struct VisualMetadata {
    palettes: Option<Arc<[Palette]>>,
    active_palette_idx: Option<f64>,
    shadow_palette: Option<Arc<[ShadowColor]>>,
    shadow_active_id: Option<String>,
    guides: Option<GuideAxes>,
    perspective_enabled: Option<bool>,
    perspective_mode: Option<String>,
    perspective_density: Option<f64>,
    // Missing, supplied null and supplied (possibly empty) arrays are distinct.
    perspective_vps: Option<Option<Arc<[VanishingPoint]>>>,
}
fn text(value: &str) -> bool {
    value.len() <= 1024 && !value.contains('\0')
}
fn finite(value: Option<f64>) -> bool {
    value.is_none_or(f64::is_finite)
}
fn invalid() -> ProjectMetadataError {
    ProjectMetadataError::InvalidVisualMetadata
}

impl ProjectMetadata {
    /// UI selection may clamp an index; storage preserves its supplied finite value.
    /// Import order, duplicate names/IDs and colour spelling are retained exactly.
    pub(crate) fn with_palette_metadata(
        mut self,
        palettes: Option<Vec<Palette>>,
        active_palette_idx: Option<f64>,
    ) -> Result<Self, ProjectMetadataError> {
        if !finite(active_palette_idx)
            || palettes.as_ref().is_some_and(|values| {
                values.iter().any(|(id, name, colors)| {
                    !text(id) || !text(name) || colors.iter().any(|c| !text(c))
                })
            })
        {
            return Err(invalid());
        }
        self.visual.palettes = palettes.map(Arc::from);
        self.visual.active_palette_idx = active_palette_idx;
        Ok(self)
    }
    pub(crate) fn palette_metadata(&self) -> (Option<&[Palette]>, Option<f64>) {
        (
            self.visual.palettes.as_deref(),
            self.visual.active_palette_idx,
        )
    }
    pub(crate) fn with_shadow_palette_metadata(
        mut self,
        palette: Option<Vec<ShadowColor>>,
        active_id: Option<String>,
    ) -> Result<Self, ProjectMetadataError> {
        if active_id.as_deref().is_some_and(|v| !text(v))
            || palette
                .as_ref()
                .is_some_and(|values| values.iter().any(|(id, color)| !text(id) || !text(color)))
        {
            return Err(invalid());
        }
        self.visual.shadow_palette = palette.map(Arc::from);
        self.visual.shadow_active_id = active_id;
        Ok(self)
    }
    pub(crate) fn shadow_palette_metadata(&self) -> (Option<&[ShadowColor]>, Option<&str>) {
        (
            self.visual.shadow_palette.as_deref(),
            self.visual.shadow_active_id.as_deref(),
        )
    }
    pub(crate) fn with_guide_metadata(
        mut self,
        guides: Option<(Option<Vec<f64>>, Option<Vec<f64>>)>,
    ) -> Result<Self, ProjectMetadataError> {
        if guides
            .as_ref()
            .is_some_and(|(h, v)| h.iter().chain(v.iter()).flatten().any(|x| !x.is_finite()))
        {
            return Err(invalid());
        }
        self.visual.guides = guides.map(|(h, v)| (h.map(Arc::from), v.map(Arc::from)));
        Ok(self)
    }
    pub(crate) fn guide_metadata(&self) -> Option<(Option<&[f64]>, Option<&[f64]>)> {
        self.visual
            .guides
            .as_ref()
            .map(|(h, v)| (h.as_deref(), v.as_deref()))
    }
    pub(crate) fn with_perspective_metadata(
        mut self,
        enabled: Option<bool>,
        mode: Option<String>,
        density: Option<f64>,
        vps: Option<Option<Vec<VanishingPoint>>>,
    ) -> Result<Self, ProjectMetadataError> {
        if !finite(density)
            || mode.as_deref().is_some_and(|v| !text(v))
            || vps.as_ref().and_then(Option::as_ref).is_some_and(|values| {
                values
                    .iter()
                    .any(|(x, y, _)| !x.is_finite() || !y.is_finite())
            })
        {
            return Err(invalid());
        }
        self.visual.perspective_enabled = enabled;
        self.visual.perspective_mode = mode;
        self.visual.perspective_density = density;
        self.visual.perspective_vps = vps.map(|value| value.map(Arc::from));
        Ok(self)
    }
    pub(crate) fn perspective_metadata(
        &self,
    ) -> (
        Option<bool>,
        Option<&str>,
        Option<f64>,
        Option<Option<&[VanishingPoint]>>,
    ) {
        (
            self.visual.perspective_enabled,
            self.visual.perspective_mode.as_deref(),
            self.visual.perspective_density,
            self.visual
                .perspective_vps
                .as_ref()
                .map(|value| value.as_deref()),
        )
    }
}
#[cfg(test)]
#[path = "../tests/project_metadata_fields.rs"]
mod tests;
