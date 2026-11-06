//! #2257: the production join of a witness to its layer-composition and effect-stack plan.
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, ProducerOutput, ScenePin,
};
use crate::object_render_scene::scene_effect_kernels::scene_effect_scene_join::{
    plan_scene_effects, JoinOptions, SceneEffectJoinError,
};
use crate::object_render_scene::{ObjectCompositionWitness, ObjectSceneError};
use std::sync::Arc;

impl ObjectCompositionWitness {
    /// Join THIS witness' packet to its layer-composition and effect-stack plan, so the viewport and the export select
    /// the same compositor path (`select_composition_path`). Every selected record is claimed by the compositor itself
    /// (the paint plan draws each of them, or refuses the frame), so a scene that needs none of the layered features
    /// stays `Flat` and keeps the accepted fast path. An input the join cannot sample (an animated `blend` or `matteOn`
    /// track, keyed effect parameters, an item-level effect) is a typed refusal here, never a neutral value.
    pub(crate) fn with_joined_layers(self) -> Result<Self, ObjectSceneError> {
        let packet = self.plan().packet();
        let refused = |reason| ObjectSceneError::UnsupportedGeometry {
            source_object_index: 0,
            reason,
        };
        let pin = ScenePin::of_packet(packet);
        let mut claims = ProducerOutput::new("object-compositor", pin.clone());
        for (source, _) in packet.selected_source_indices() {
            claims = claims.with_item(source, ());
        }
        let joined = join_scene(packet, &pin, vec![claims])
            .map_err(|_| refused("layer composition order unavailable"))?;
        let zoom = self.camera_matrix().map_or(1., |m| m[0].hypot(m[1]));
        let effects = plan_scene_effects(packet, &joined, &JoinOptions::new(zoom, &|_| false))
            .map_err(|error| match error {
                SceneEffectJoinError::MaskInputs(inner) => inner,
                SceneEffectJoinError::EffectParametersUnavailable { .. } => {
                    refused("animated layer effect parameters are unavailable")
                }
                SceneEffectJoinError::LayerCompositionPropertyUnavailable { property, .. } => {
                    refused(match property {
                        "itemEffects" => "element effects are unavailable",
                        "matteOn" => "an animated matte switch is unavailable",
                        _ => "an animated blend mode is unavailable",
                    })
                }
                _ => refused("layer composition plan unavailable"),
            })?;
        self.with_scene_effects(Arc::new(effects))
    }
}
