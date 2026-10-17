//! Video-layer mesh ids and the duplicate-layer remap of the binding kernel (L03-R, F17/F18):
//! `attachToVideoLayer`, `detachFromVideoLayer` and the `remapMeshes` block of `duplicateLayer`
//! (`src/js/timeline.js`) at baseline fbe4b8a3, as methods of `MeshBindingState`.
//!
//! A video layer keeps its mesh id on the layer record (`video_mesh_id`), not in a stroke. The native
//! video layer family (L09 G28) is not provided yet, so the two video operations take a
//! `VideoFamily` and refuse with the typed `VideoLayerUnavailable` while it is `Unavailable`; they
//! never fake a layer and never fall back to another writer. The remap reads the id from the
//! layer record as the baseline does, so it already covers a video id the day the family arrives.
use crate::evaluation::object_image_mesh_kernels::object_image_mesh_binding::{
    item_with_mesh_id, BindingError, BindingSlot, MeshBindingState,
};
use crate::object_document::ImageMesh;

/// Whether the native video layer family exists in the document being edited.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VideoFamily {
    Provided,
    Unavailable,
}

/// The ids a duplicated layer was remapped to. `pairs` are `(old, new)` in first-use order (the
/// order the fresh ids were issued); `dangling` are ids whose mesh is gone and which stay as they
/// were, like the baseline (it leaves the stroke untouched when `duplicate` returns null).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct RemapReport {
    pub(crate) pairs: Vec<(String, String)>,
    pub(crate) dangling: Vec<String>,
}

impl RemapReport {
    pub(crate) fn mapped(&self, old: &str) -> Option<&str> {
        self.pairs
            .iter()
            .find(|pair| pair.0.as_str() == old)
            .map(|pair| pair.1.as_str())
    }
}

impl MeshBindingState {
    /// `attachToVideoLayer`: a fresh mesh on a native video layer; `None` for a layer that is not
    /// one. A video id already on the layer is replaced and its mesh stays in the store.
    pub(crate) fn attach_video(
        &mut self,
        family: VideoFamily,
        set: usize,
        layer: usize,
        mesh: ImageMesh,
    ) -> Result<Option<String>, BindingError> {
        if family == VideoFamily::Unavailable {
            return Err(BindingError::VideoLayerUnavailable);
        }
        if !self.layer_at(set, layer)?.native_video {
            return Ok(None);
        }
        let mut next = self.clone();
        let id = next.insert_new_mesh(mesh)?;
        next.layer_at_mut(set, layer)?.video_mesh_id = Some(id.clone());
        *self = next;
        Ok(Some(id))
    }

    /// `detachFromVideoLayer`: clear the layer's video id and release its mesh when unreferenced.
    /// False (nothing changed) when the layer has no video id.
    pub(crate) fn detach_video(
        &mut self,
        family: VideoFamily,
        set: usize,
        layer: usize,
    ) -> Result<bool, BindingError> {
        if family == VideoFamily::Unavailable {
            return Err(BindingError::VideoLayerUnavailable);
        }
        let Some(id) = self
            .layer_at(set, layer)?
            .video_mesh_id
            .clone()
            .filter(|id| !id.is_empty())
        else {
            return Ok(false);
        };
        let mut next = self.clone();
        next.layer_at_mut(set, layer)?.video_mesh_id = None;
        next.release_if_unused(&id)?;
        *self = next;
        Ok(true)
    }

    /// The `remapMeshes` block of `duplicateLayer`: every distinct mesh id the copied layer
    /// carries (raster frames in order, then the video id) is deep-copied under a fresh id and
    /// rewritten, so editing the copy never moves the original. The caller applies the returned
    /// pairs to the other places that name a mesh (per-vertex motion holders, rig binds).
    pub(crate) fn remap_duplicated_layer(
        &mut self,
        set: usize,
        layer: usize,
    ) -> Result<RemapReport, BindingError> {
        self.layer_at(set, layer)?;
        let mut next = self.clone();
        let mut report = RemapReport::default();
        let frame_count = next.layer_at(set, layer)?.frames.len();
        for frame in 0..frame_count {
            let slot_count = next.layer_at(set, layer)?.frames[frame].len();
            for slot in 0..slot_count {
                let old = match &next.layer_at(set, layer)?.frames[frame][slot] {
                    BindingSlot::Raster(item) => match item.mesh_id() {
                        Some(id) => id.to_string(),
                        None => continue,
                    },
                    BindingSlot::NonRaster => continue,
                };
                if let Some(new) = next.remapped_id(&old, &mut report)? {
                    let rewritten = match &next.layer_at(set, layer)?.frames[frame][slot] {
                        BindingSlot::Raster(item) => item_with_mesh_id(item, Some(&new))?,
                        BindingSlot::NonRaster => continue,
                    };
                    next.layer_at_mut(set, layer)?.frames[frame][slot] =
                        BindingSlot::Raster(rewritten);
                }
            }
        }
        let video = next
            .layer_at(set, layer)?
            .video_mesh_id
            .clone()
            .filter(|id| !id.is_empty());
        if let Some(old) = video {
            if let Some(new) = next.remapped_id(&old, &mut report)? {
                next.layer_at_mut(set, layer)?.video_mesh_id = Some(new);
            }
        }
        *self = next;
        Ok(report)
    }

    /// The fresh id for `old`: the one already issued during this remap, a new deep copy, or
    /// `None` (recorded once in `dangling`) when the source mesh is gone.
    fn remapped_id(
        &mut self,
        old: &str,
        report: &mut RemapReport,
    ) -> Result<Option<String>, BindingError> {
        if let Some(new) = report.mapped(old) {
            return Ok(Some(new.to_string()));
        }
        match self.duplicate_mesh(old)? {
            Some(new) => {
                report.pairs.push((old.to_string(), new.clone()));
                Ok(Some(new))
            }
            None => {
                if !report.dangling.iter().any(|seen| seen == old) {
                    report.dangling.push(old.to_string());
                }
                Ok(None)
            }
        }
    }
}
