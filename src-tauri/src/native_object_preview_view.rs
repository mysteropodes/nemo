//! Read-only, object-preview-only session choice and the actual painted affine.
use super::NativePreviewReceipt;
use native_engine::desktop_viewport::PresentationReceipt;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ObjectPreviewView {
    #[default]
    Canvas,
    StoredCamera,
}
impl ObjectPreviewView {
    pub(crate) fn quality(self) -> &'static str {
        match self { Self::Canvas => "final", Self::StoredCamera => "final-camera-view" }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObjectViewProjection {
    pub(crate) mode: ObjectPreviewView,
    pub(crate) camera_available: bool,
    pub(crate) document_to_composition: [f64; 6],
    pub(crate) composition_to_document: [f64; 6],
}
impl ObjectViewProjection {
    pub(crate) fn from_transform(mode: ObjectPreviewView, forward: [f64; 6], camera_available: bool) -> Result<Self, String> {
        let [a,b,c,d,e,f] = forward;
        let determinant = a*d-b*c;
        if forward.iter().any(|n| !n.is_finite()) || !determinant.is_finite() || determinant == 0. {
            return Err("native preview coordinate transform is not invertible".into());
        }
        let inverse = [d/determinant, -b/determinant, -c/determinant, a/determinant,
            (c*f-d*e)/determinant, (b*e-a*f)/determinant];
        if inverse.iter().any(|n| !n.is_finite()) { return Err("native preview inverse transform is unavailable".into()); }
        Ok(Self { mode, camera_available, document_to_composition: forward, composition_to_document: inverse })
    }
}

#[derive(Debug)]
pub(crate) struct ObjectPresentedFrame {
    pub(crate) receipt: PresentationReceipt,
    pub(crate) view: ObjectViewProjection,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ObjectPreviewReceipt {
    #[serde(flatten)]
    pub(crate) preview: NativePreviewReceipt,
    pub(crate) view: ObjectViewProjection,
}
// Existing internal Canvas callers can keep reading the original receipt fields.
impl std::ops::Deref for ObjectPreviewReceipt {
    type Target = NativePreviewReceipt;
    fn deref(&self) -> &Self::Target { &self.preview }
}
