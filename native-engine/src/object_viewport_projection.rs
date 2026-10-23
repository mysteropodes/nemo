//! Closed session-only viewport choices and proof of the actual painted affine.
use crate::desktop_viewport::PresentationReceipt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum ObjectPreviewView {
    #[default]
    Canvas,
    StoredCamera,
}

/// Desired viewport only; never serialized into a document or used for export.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ObjectPreviewNavigation {
    zoom: f64,
    pan: [f64; 2],
}
impl Default for ObjectPreviewNavigation {
    fn default() -> Self {
        Self {
            zoom: 1.,
            pan: [0., 0.],
        }
    }
}
impl ObjectPreviewNavigation {
    pub fn new(zoom: f64, pan: [f64; 2]) -> Result<Self, String> {
        let value = Self {
            zoom,
            pan: pan.map(|n| if n == 0. { 0. } else { n }),
        };
        value.validate()?;
        Ok(value)
    }
    pub fn validate(self) -> Result<(), String> {
        if !self.zoom.is_finite() || self.zoom <= 0. || self.pan.iter().any(|n| !n.is_finite()) {
            return Err(
                "native preview navigation requires finite positive zoom and finite pan".into(),
            );
        }
        Ok(())
    }
    pub fn zoom(self) -> f64 {
        self.zoom
    }
    pub fn pan(self) -> [f64; 2] {
        self.pan
    }
    /// The producer owns the one canonical cache identity. This downward port is
    /// available exactly when viewport composition's source provider is built.
    #[cfg(all(feature = "application", feature = "project_media_codec"))]
    pub fn quality(self, mode: ObjectPreviewView) -> String {
        crate::object_render_scene::object_viewport_quality(
            mode == ObjectPreviewView::StoredCamera,
            self.zoom,
            self.pan,
        )
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectViewProjection {
    pub mode: ObjectPreviewView,
    pub navigation: ObjectPreviewNavigation,
    pub camera_available: bool,
    pub document_to_composition: [f64; 6],
    pub composition_to_document: [f64; 6],
}
impl ObjectViewProjection {
    pub fn from_transform(
        mode: ObjectPreviewView,
        forward: [f64; 6],
        camera_available: bool,
    ) -> Result<Self, String> {
        Self::from_navigation(
            mode,
            ObjectPreviewNavigation::default(),
            forward,
            camera_available,
        )
    }
    pub fn from_navigation(
        mode: ObjectPreviewView,
        navigation: ObjectPreviewNavigation,
        forward: [f64; 6],
        camera_available: bool,
    ) -> Result<Self, String> {
        navigation.validate()?;
        let [a, b, c, d, e, f] = forward;
        let determinant = a * d - b * c;
        if forward.iter().any(|n| !n.is_finite()) || !determinant.is_finite() || determinant == 0. {
            return Err("native preview coordinate transform is not invertible".into());
        }
        let inverse = [
            d / determinant,
            -b / determinant,
            -c / determinant,
            a / determinant,
            (c * f - d * e) / determinant,
            (b * e - a * f) / determinant,
        ];
        if inverse.iter().any(|n| !n.is_finite()) {
            return Err("native preview inverse transform is unavailable".into());
        }
        Ok(Self {
            mode,
            navigation,
            camera_available,
            document_to_composition: forward,
            composition_to_document: inverse,
        })
    }
}
#[derive(Debug)]
pub struct ObjectPresentedFrame {
    pub receipt: PresentationReceipt,
    pub view: ObjectViewProjection,
}
