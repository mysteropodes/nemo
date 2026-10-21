//! Brush paints retain their charged pixels through the ordinary scene witness.
use crate::object_render_scene::scene_effect_kernels::object_scene_resource_set::SceneResourceEntry;
use crate::object_render_scene::{
    ObjectCompositionWitness, ObjectPaintPlan, ObjectSceneError, ObjectSceneResourceSet,
};
use std::sync::Arc;
use vello::{kurbo::Affine, peniko::ImageData};
impl ObjectPaintPlan {
    pub(crate) fn has_brushes(&self) -> bool {
        self.brush_sources()
            .is_some_and(|sources| !sources.paints.is_empty())
    }
    pub(crate) fn first_brush_source(&self) -> Option<usize> {
        self.brush_sources()?.paints.first().map(|p| p.source_index)
    }
    pub(crate) fn brushes_match(&self, plan: &Self) -> bool {
        match (self.brush_sources(), plan.brush_sources()) {
            (None,None) => true,
            (Some(old),Some(new)) => old.paints.len()==new.paints.len() && old.paints.iter().zip(&new.paints).all(|(a,b)| {
                use crate::evaluation::object_scene_style_kernels::object_scene_producer_brush::InkState;
                a.source_index==b.source_index && match (&a.ink,&b.ink) {
                    (InkState::Collapsed,InkState::Collapsed) => true,
                    (InkState::Ink(a),InkState::Ink(b)) => a.bitmap.same_allocation_as(&b.bitmap),
                    _ => false,
                }
            }),
            (None,Some(new)) => new.paints.is_empty(),
            (Some(old),None) => old.paints.is_empty(),
        }
    }
}
pub(super) fn attach(
    mut witness: ObjectCompositionWitness,
) -> Result<ObjectCompositionWitness, ObjectSceneError> {
    let Some(brushes) = witness.plan.brush_sources() else {
        return Ok(witness);
    };
    let entries = witness
        .resources
        .iter()
        .flat_map(|set| set.entries())
        .chain(brushes.resources().entries())
        .map(|entry| {
            SceneResourceEntry::new(
                entry.resource_id(),
                entry.version(),
                entry.kind(),
                entry.charged_bytes(),
                Arc::clone(entry.retention()),
            )
        })
        .collect();
    witness.resources = Some(Arc::new(ObjectSceneResourceSet::new(entries).map_err(
        |_| ObjectSceneError::UnsupportedGeometry {
            source_object_index: 0,
            reason: "mixed brush resource declarations invalid",
        },
    )?));
    Ok(witness)
}
impl ObjectCompositionWitness {
    pub(crate) fn brush_images(&self) -> impl Iterator<Item = &ImageData> {
        self.plan
            .brush_sources()
            .into_iter()
            .flat_map(|sources| sources.images())
    }
    pub(crate) fn brush_image(
        &self,
        index: usize,
    ) -> Result<Option<(&ImageData, Affine, f32)>, ObjectSceneError> {
        let paint = self
            .plan
            .brush_sources()
            .and_then(|sources| sources.paints.get(index))
            .ok_or_else(|| ObjectSceneError::UnsupportedGeometry {
                source_object_index: index,
                reason: "admitted brush image unavailable",
            })?;
        Ok(paint.image())
    }
}
