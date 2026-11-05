//! #2371 U11: the Solo view of a paint plan. Baseline 1065e9d8 `layerIsEffectivelyVisible` (app.js): while ANY layer of the shown
//! context is soloed only the soloed layers paint, whatever their eye says, and the children of a soloed folder layer count as
//! soloed (one level, `_layerFolderParent`). This plan filter can only LEAVE OUT layers of a plan that already holds them: a layer
//! whose eye is off never reaches the plan unless the frame packet was built with the Solo view open
//! (`object_frame_packet::solo_view::enter`, #2371 C-solo-2), which is also what makes a non-soloed effect, matte or mask layer
//! stop applying. Every caller that builds a packet for a soloed frame or export opens it. The flag is a session view fact, never document data: this derives a plan that
//! leaves the other layers' items out of `mixed` and `indices` (the two lists every painter and exporter walks) and shares the
//! scene, masks and dabs, so no source, resource or revision is copied or changed.
use crate::object_render_scene::{ObjectPaintPlan, PaintIndex};

impl ObjectPaintPlan {
    /// The layer (index into the packet's source layers) an entry paints for; a placed component's items belong to their host.
    fn layer_of(&self, index: &PaintIndex) -> Option<usize> {
        let scene = &self.scene;
        match index {
            PaintIndex::Contour(i) => scene.entry(*i).map(|e| e.source().source_layer_index()),
            PaintIndex::Raster(i) => scene.rasters.get(*i).map(|s| s.layer_index),
            PaintIndex::Text(i) => scene.texts.entries.get(*i).map(|s| s.layer_index),
            PaintIndex::Component(i) => scene.components.paints.get(*i).map(|p| p.host_layer_index),
            PaintIndex::Video(i) => scene.videos.get(*i).map(|v| v.layer_index),
            PaintIndex::Duplicator(i) => scene.duplicators.paints.get(*i).map(|p| p.layer_index),
            #[cfg(all(feature = "application", feature = "project_media_codec"))]
            PaintIndex::Brush(i) => scene
                .brushes
                .as_ref()
                .and_then(|b| b.paints.get(*i))
                .map(|p| p.layer_index),
            PaintIndex::Dab(_, layer) => Some(*layer),
        }
    }

    /// Which source layers stay visible under this Solo set; `None` when no soloed layer belongs to this plan's context
    /// (baseline `anyLayerSoloed` only reads the layers of the shown context, so the flag then changes nothing).
    pub(crate) fn solo_visibility(&self, soloed: &[String]) -> Option<Vec<bool>> {
        crate::object_frame_packet::solo_view::solo_visibility(self.packet().source_layers(), soloed)
    }

    /// This plan with only the Solo-visible layers' items; `None` when the Solo set does not apply to this context. An entry
    /// whose layer cannot be resolved is kept, never silently dropped.
    pub(crate) fn with_solo(&self, soloed: &[String]) -> Option<ObjectPaintPlan> {
        let visible = self.solo_visibility(soloed)?;
        let keeps = |index: &PaintIndex| {
            self.layer_of(index)
                .is_none_or(|layer| visible.get(layer).copied().unwrap_or(true))
        };
        Some(ObjectPaintPlan {
            scene: self.scene.clone(),
            masks: self.masks.clone(),
            indices: self
                .indices
                .iter()
                .copied()
                .filter(|i| keeps(&PaintIndex::Contour(*i)))
                .collect(),
            mixed: self.mixed.iter().copied().filter(|i| keeps(i)).collect(),
            dabs: self.dabs.clone(),
        })
    }
}
