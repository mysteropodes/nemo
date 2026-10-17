//! Image-mesh / raster binding kernel (L03-R, F17/F18): which raster records carry which `meshId`,
//! and the reference-counted lifetime of a mesh in the store. It mirrors `propagate`, `attach`,
//! `detach`, `isReferenced` and `releaseIfUnused` of `src/js/image-mesh.js` (baseline fbe4b8a3).
//!
//! The store itself is the L03-J `ImageMeshStore`; a raster is the L06-C `RasterItem`, whose
//! passthrough `meshId` member is the only per-frame tag. Nothing here reads a Paper item, a
//! stroke dict or a live tag: the caller projects one layer into `BindingLayer.frames` (one
//! `Vec<BindingSlot>` per frame, in the stroke order of that frame) and publishes the returned
//! state atomically. Every operation is all-or-nothing: it runs on a detached copy and replaces
//! the state only when the whole request succeeded.
//!
//! Matching rule (baseline, kept): the raster is addressed by its POSITION in the frame, and a
//! frame is tagged only when that same position still holds a raster. A frame whose entry is
//! absent or not a raster is left untouched and counted in `PropagateReport::skipped`. The
//! operation writes EVERY frame of the layer, never only the one being looked at.
//!
//! Deliberate differences from the baseline (each pinned by a test):
//! - a target that does not resolve to a raster slot is a typed refusal; the baseline returns 0 or
//!   null and `attach` may leave an orphan mesh behind;
//! - a tag the L06-C record refuses (over 255 bytes, NUL) is a typed refusal; an empty id clears
//!   the tag exactly like the baseline's falsy `meshId`;
//! - `dangling_mesh_ids` is a native integrity query; the baseline never checks that a tag
//!   resolves in the store;
//! - the id counter is explicit state (`MeshIdSequence`), never re-derived from the store, because
//!   the baseline `_seq` is never lowered by a release.
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::geometry::RasterItemError;
use crate::object_document::object_raster_asset_kernels::object_raster_asset_item::RasterItem;
use crate::object_document::{ImageMesh, ImageMeshAdmissionError, ImageMeshStore, MeshIdSequence};
use serde_json::Value;

/// One stroke of one frame, as far as the mesh binding is concerned. A non-raster stroke
/// (a path, a text block) cannot carry a `meshId` in the native records.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum BindingSlot {
    Raster(RasterItem),
    NonRaster,
}

/// One layer: its frames in order and the video fields of the baseline layer dict.
/// `video_mesh_id` is the baseline `videoMeshId`; it is read for the reference scan whether or
/// not `native_video` is set, exactly like `isReferenced`.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct BindingLayer {
    pub(crate) frames: Vec<Vec<BindingSlot>>,
    pub(crate) native_video: bool,
    pub(crate) video_mesh_id: Option<String>,
}

/// The state the baseline binding functions touch: the mesh store, the id counter and every
/// layer set that can reference a mesh (`sets[0]` the scene's layers, the others one per
/// component, as `allLayerSets`).
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MeshBindingState {
    pub(crate) store: ImageMeshStore,
    pub(crate) sequence: MeshIdSequence,
    pub(crate) sets: Vec<Vec<BindingLayer>>,
}

/// A raster addressed by layer set, layer, frame and position in that frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SlotTarget {
    pub(crate) set: usize,
    pub(crate) layer: usize,
    pub(crate) frame: usize,
    pub(crate) slot: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BindingError {
    UnknownLayer,
    UnknownSlot,
    TargetNotRaster,
    /// The L06-C record refused the rewritten raster (an invalid `meshId` tag).
    Item(RasterItemError),
    /// The store or the id counter refused the new state.
    Mesh(ImageMeshAdmissionError),
    /// The native video layer family (L09 G28) is not provided yet.
    VideoLayerUnavailable,
}

impl BindingError {
    /// Stable machine code of the refusal.
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::UnknownLayer => "binding-unknown-layer",
            Self::UnknownSlot => "binding-unknown-slot",
            Self::TargetNotRaster => "binding-target-not-raster",
            Self::Item(_) => "binding-invalid-mesh-tag",
            Self::Mesh(_) => "binding-mesh-store-refused",
            Self::VideoLayerUnavailable => "binding-video-layer-unavailable",
        }
    }
}

/// `written` frames carried a raster at the position and were (re)tagged; `skipped` did not.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PropagateReport {
    pub(crate) written: usize,
    pub(crate) skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttachReport {
    pub(crate) mesh_id: String,
    /// The mesh the raster pointed at before; the baseline leaves it in the store.
    pub(crate) replaced: Option<String>,
    pub(crate) propagate: PropagateReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DetachReport {
    /// The tag of the addressed raster before the detach.
    pub(crate) mesh_id: Option<String>,
    pub(crate) propagate: PropagateReport,
    /// The return value of `releaseIfUnused` (true even when the id was not in the store).
    pub(crate) released: bool,
}

/// A falsy baseline `meshId` (null, undefined, empty) is no tag.
fn tag(mesh_id: Option<&str>) -> Option<&str> {
    mesh_id.filter(|id| !id.is_empty())
}

/// The raster with exactly its `meshId` member replaced (or removed), re-decoded through the strict
/// L06-C codec so every other member keeps its authored value and number spelling.
pub(crate) fn item_with_mesh_id(
    item: &RasterItem,
    mesh_id: Option<&str>,
) -> Result<RasterItem, BindingError> {
    let wanted = tag(mesh_id);
    if item.mesh_id() == wanted {
        return Ok(item.clone());
    }
    let text = item.encode().map_err(BindingError::Item)?;
    let Ok(Value::Object(mut members)) = serde_json::from_str::<Value>(&text) else {
        return Err(BindingError::Item(RasterItemError::Malformed));
    };
    match wanted {
        Some(id) => {
            members.insert("meshId".to_string(), Value::String(id.to_string()));
        }
        None => {
            members.remove("meshId");
        }
    }
    RasterItem::decode(&Value::Object(members).to_string()).map_err(BindingError::Item)
}

impl MeshBindingState {
    /// A state whose counter is what a fresh `load()` of the store leaves behind.
    pub(crate) fn new(
        store: ImageMeshStore,
        sets: Vec<Vec<BindingLayer>>,
    ) -> Result<Self, BindingError> {
        let sequence = store.id_sequence().map_err(BindingError::Mesh)?;
        Ok(Self {
            store,
            sequence,
            sets,
        })
    }

    pub(crate) fn layer_at(&self, set: usize, layer: usize) -> Result<&BindingLayer, BindingError> {
        self.sets
            .get(set)
            .and_then(|layers| layers.get(layer))
            .ok_or(BindingError::UnknownLayer)
    }

    pub(crate) fn layer_at_mut(
        &mut self,
        set: usize,
        layer: usize,
    ) -> Result<&mut BindingLayer, BindingError> {
        self.sets
            .get_mut(set)
            .and_then(|layers| layers.get_mut(layer))
            .ok_or(BindingError::UnknownLayer)
    }

    /// The raster record a target resolves to.
    pub(crate) fn raster_at(&self, target: SlotTarget) -> Result<&RasterItem, BindingError> {
        let layer = self.layer_at(target.set, target.layer)?;
        let slot = layer
            .frames
            .get(target.frame)
            .and_then(|frame| frame.get(target.slot))
            .ok_or(BindingError::UnknownSlot)?;
        match slot {
            BindingSlot::Raster(item) => Ok(item),
            BindingSlot::NonRaster => Err(BindingError::TargetNotRaster),
        }
    }

    /// The tag of the addressed raster (the baseline live `raster.data.meshId`).
    pub(crate) fn mesh_id_at(&self, target: SlotTarget) -> Result<Option<&str>, BindingError> {
        Ok(self.raster_at(target)?.mesh_id())
    }

    /// `isReferenced`: any raster of any set, or any layer's video id, carries the id.
    pub(crate) fn is_referenced(&self, mesh_id: &str) -> bool {
        self.sets.iter().flatten().any(|layer| {
            layer.video_mesh_id.as_deref() == Some(mesh_id)
                || layer.frames.iter().flatten().any(|slot| {
                    matches!(slot, BindingSlot::Raster(item) if item.mesh_id() == Some(mesh_id))
                })
        })
    }

    /// Ids that a raster or a video field names but the store does not hold, in scan order.
    pub(crate) fn dangling_mesh_ids(&self) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        for layer in self.sets.iter().flatten() {
            let video = layer.video_mesh_id.as_deref().filter(|id| !id.is_empty());
            let tags = layer.frames.iter().flatten().filter_map(|slot| match slot {
                BindingSlot::Raster(item) => item.mesh_id(),
                BindingSlot::NonRaster => None,
            });
            for id in video.into_iter().chain(tags) {
                if self.store.get(id).is_none() && !found.iter().any(|seen| seen == id) {
                    found.push(id.to_string());
                }
            }
        }
        found
    }

    /// `propagate`: write (or, for no/empty id, clear) the tag on the raster at the same position
    /// of EVERY frame of the layer. The target must be a raster of the layer.
    pub(crate) fn propagate(
        &mut self,
        target: SlotTarget,
        mesh_id: Option<&str>,
    ) -> Result<PropagateReport, BindingError> {
        self.raster_at(target)?;
        let layer = self.layer_at(target.set, target.layer)?;
        let mut edits: Vec<(usize, RasterItem)> = Vec::new();
        for (frame, strokes) in layer.frames.iter().enumerate() {
            if let Some(BindingSlot::Raster(item)) = strokes.get(target.slot) {
                edits.push((frame, item_with_mesh_id(item, mesh_id)?));
            }
        }
        let written = edits.len();
        let skipped = layer.frames.len() - written;
        let layer = self.layer_at_mut(target.set, target.layer)?;
        for (frame, item) in edits {
            layer.frames[frame][target.slot] = BindingSlot::Raster(item);
        }
        Ok(PropagateReport { written, skipped })
    }

    /// `releaseIfUnused`: drop the mesh once nothing references it. An empty id and a still
    /// referenced id keep the store; any other id returns true, in the store or not.
    pub(crate) fn release_if_unused(&mut self, mesh_id: &str) -> Result<bool, BindingError> {
        if mesh_id.is_empty() || self.is_referenced(mesh_id) {
            return Ok(false);
        }
        let entries: Vec<(String, ImageMesh)> = self
            .store
            .entries()
            .iter()
            .filter(|entry| entry.0.as_str() != mesh_id)
            .cloned()
            .collect();
        self.store = ImageMeshStore::try_new(entries).map_err(BindingError::Mesh)?;
        Ok(true)
    }

    /// `attach`: issue the next id, store the freshly created mesh under it and tag every frame.
    /// A raster that already pointed at a mesh keeps that mesh in the store (reported, not released).
    pub(crate) fn attach(
        &mut self,
        target: SlotTarget,
        mesh: ImageMesh,
    ) -> Result<AttachReport, BindingError> {
        let replaced = self.raster_at(target)?.mesh_id().map(str::to_string);
        let mut next = self.clone();
        let id = next.insert_new_mesh(mesh)?;
        let propagate = next.propagate(target, Some(&id))?;
        *self = next;
        Ok(AttachReport {
            mesh_id: id,
            replaced,
            propagate,
        })
    }

    /// `detach`: clear the tag on every frame, then release the mesh the raster pointed at.
    /// With no tag on the addressed raster the frames are still cleared and nothing is released.
    pub(crate) fn detach(&mut self, target: SlotTarget) -> Result<DetachReport, BindingError> {
        let mesh_id = self.raster_at(target)?.mesh_id().map(str::to_string);
        let mut next = self.clone();
        let propagate = next.propagate(target, None)?;
        let released = match &mesh_id {
            Some(id) => next.release_if_unused(id)?,
            None => false,
        };
        *self = next;
        Ok(DetachReport {
            mesh_id,
            propagate,
            released,
        })
    }

    /// Issues the next id (skipping taken ones, never lowering the counter) and appends the mesh.
    pub(crate) fn insert_new_mesh(&mut self, mesh: ImageMesh) -> Result<String, BindingError> {
        let mut sequence = self.sequence;
        let id = sequence.issue(&self.store).map_err(BindingError::Mesh)?;
        let mut entries = self.store.entries().to_vec();
        entries.push((id.clone(), mesh));
        let store = ImageMeshStore::try_new(entries).map_err(BindingError::Mesh)?;
        self.sequence = sequence;
        self.store = store;
        Ok(id)
    }

    /// `NS.duplicate`: a deep copy under a fresh id, or `None` (counter untouched) when the source is gone.
    pub(crate) fn duplicate_mesh(&mut self, mesh_id: &str) -> Result<Option<String>, BindingError> {
        let Some(source) = self.store.get(mesh_id).cloned() else {
            return Ok(None);
        };
        self.insert_new_mesh(source).map(Some)
    }
}

#[cfg(test)]
#[path = "../tests/object_image_mesh_binding_rows.rs"]
mod rows;

#[cfg(test)]
#[path = "../tests/object_image_mesh_binding.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_image_mesh_binding_oracle.rs"]
mod oracle_tests;

#[cfg(test)]
#[path = "../tests/object_image_mesh_binding_controls.rs"]
mod controls_tests;
