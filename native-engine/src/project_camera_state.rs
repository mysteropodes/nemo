//! Persistent camera inputs. Evaluation remains in the original camera kernel.
use crate::object_document::Presence;
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectCameraKey {
    pub(crate) frame: f64,
    pub(crate) x: f64,
    pub(crate) y: f64,
    pub(crate) w: f64,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) rot: Presence<f64>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) h_out: Presence<[f64; 2]>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) h_in: Presence<[f64; 2]>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) ease: Presence<[f64; 4]>,
}
crate::object_document::object_deserialize!(ProjectCameraKey {
    frame:f64, x:f64, y:f64, w:f64,
    #[serde(default)] rot:Presence<f64>,
    #[serde(default)] h_out:Presence<[f64;2]>,
    #[serde(default)] h_in:Presence<[f64;2]>,
    #[serde(default)] ease:Presence<[f64;4]>
});
impl ProjectCameraKey {
    fn valid(&self) -> bool {
        [self.frame,self.x,self.y,self.w].into_iter()
            .chain(self.rot.value().copied())
            .chain(self.h_out.value().into_iter().flatten().copied())
            .chain(self.h_in.value().into_iter().flatten().copied())
            .chain(self.ease.value().into_iter().flatten().copied())
            .all(f64::is_finite)
    }
}

/// Missing and explicit empty keys remain distinct. cameraView is session state
/// and is deliberately not stored as document metadata.
#[derive(Debug, Default, Clone, PartialEq)]
pub(crate) struct ProjectCameraState {
    pub(crate) keys: Option<Arc<[ProjectCameraKey]>>,
    pub(crate) layer_on: Option<bool>,
}
impl ProjectCameraState {
    pub(crate) fn new(keys: Option<Vec<ProjectCameraKey>>, layer_on: Option<bool>) -> Result<Self, ()> {
        if keys.as_ref().is_some_and(|keys| keys.iter().any(|key| !key.valid())) {
            return Err(());
        }
        Ok(Self { keys:keys.map(Arc::from), layer_on })
    }
}
