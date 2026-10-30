//! Retire and reuse the reference composition within the existing preview owner.
use crate::native_dispatch::NativeObjectHost;
use native_engine::{
    compositor::Compositor,
    object_render_scene::{ObjectCompositionWitness, ObjectPaintPlan},
};

impl NativeObjectHost {
    pub(super) fn prepare_reference_preview(
        &mut self,
        plan: &ObjectPaintPlan,
        dimensions: (u32, u32),
        compositor: &mut Compositor,
        decode: impl FnOnce(
            Option<&ObjectCompositionWitness>,
        ) -> Result<ObjectCompositionWitness, String>,
    ) -> Result<ObjectCompositionWitness, String> {
        // Both Vello and the host retain charged pixels. A failed reset leaves
        // both intact; a failed decode leaves neither stale cache and is retryable.
        compositor
            .retire_object_reference_for(plan, dimensions)
            .map_err(|error| error.to_string())?;
        if self
            .preview
            .reference_cache
            .as_ref()
            .is_some_and(|previous| !previous.reference_matches_plan(plan))
        {
            self.preview.reference_cache = None;
        }
        let composition = decode(self.preview.reference_cache.as_ref())?;
        if composition.has_scene_images() {
            self.preview.reference_cache = Some(composition.clone());
        } else if !self
            .preview
            .reference_cache
            .as_ref()
            .is_some_and(ObjectCompositionWitness::has_viewport_reference)
        {
            // A hidden (or zero-opacity) reference leaves this composition without
            // scene images, but the plan-matched cached raster stays so an undo or
            // re-show reuses it; a replaced/removed one was already cleared above.
            self.preview.reference_cache = None;
        }
        Ok(composition)
    }
}
