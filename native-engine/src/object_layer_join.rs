//! #2257: the production join of a witness to its layer-composition and effect-stack plan.
use crate::evaluation::object_scene_style_kernels::object_scene_producer_pin::{
    join_scene, ProducerOutput, ScenePin,
};
use crate::object_render_scene::scene_effect_kernels::scene_effect_scene_join::{
    effect_specs, plan_scene_effects, ExtraItemEffects, JoinOptions, SceneEffectJoinError,
};
use crate::object_render_scene::{ObjectCompositionWitness, ObjectSceneError};
use std::sync::Arc;

impl ObjectCompositionWitness {
    /// Join THIS witness' packet to its layer-composition and effect-stack plan, so the viewport and the export select
    /// the same compositor path (`select_composition_path`). Every selected record is claimed by the compositor itself
    /// (the paint plan draws each of them, or refuses the frame), so a scene that needs none of the layered features
    /// stays `Flat` and keeps the accepted fast path. Keyed effect parameters and a keyed `matteOn` are sampled at the
    /// frame; an input the join cannot sample (an expressed `matteOn`, a `blend` track) is a typed refusal here,
    /// never a neutral value.
    ///
    /// `effect_zoom` is the baseline `Viewport::effect_zoom` that scales every pixel-space effect radius: the session
    /// navigation zoom of a viewport witness and 1 for an export, which renders document pixels 1:1. The stored
    /// camera is NOT part of it: the baseline applies the camera to the items and leaves `effect_zoom` alone.
    pub(crate) fn with_joined_layers(self, effect_zoom: f64) -> Result<Self, ObjectSceneError> {
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
        // A placed component's inner records live in the symbol's context, so the join's packet-wide object table does
        // not hold them: their enabled stacks are sampled here, at the host frame (baseline `sceneEffectsOf(c.data)`),
        // and filed on the host layer under the key of `component_item_key`.
        let frame = f64::from(packet.frame());
        let mut extras = Vec::new();
        for paint in self.plan().component_paints() {
            if !paint.record.has_enabled_effects() {
                continue;
            }
            let key = self.plan().component_item_key(paint);
            let specs = effect_specs(paint.record.effects().unwrap_or(&[]), frame);
            let (Some(key), Some(specs)) = (key, specs) else {
                return Err(refused("a keyed effect parameter could not be sampled"));
            };
            extras.push(ExtraItemEffects {
                source_layer_index: paint.host_layer_index,
                key,
                specs,
            });
        }
        // A duplicator copy carries its source's stack onto the HOST layer, whose own object table may not hold the
        // source (a hidden pool layer): filed under the source's object index, one stack for all of its copies.
        for copy in self.plan().duplicator_paints() {
            let Some(source) = packet.source_objects().get(copy.source_index) else {
                continue;
            };
            if source.is_mask() || !source.has_enabled_effects() {
                continue;
            }
            let Some(specs) = effect_specs(source.effects().unwrap_or(&[]), frame) else {
                return Err(refused("a keyed effect parameter could not be sampled"));
            };
            extras.push(ExtraItemEffects {
                source_layer_index: copy.layer_index,
                key: copy.source_index,
                specs,
            });
        }
        let options = JoinOptions::new(effect_zoom, &|_| false).with_extra_item_effects(&extras);
        let effects = plan_scene_effects(packet, &joined, &options)
            .map_err(|error| match error {
                SceneEffectJoinError::MaskInputs(inner) => inner,
                SceneEffectJoinError::EffectParametersUnavailable { .. } => {
                    refused("a keyed effect parameter could not be sampled")
                }
                SceneEffectJoinError::LayerCompositionPropertyUnavailable { property, .. } => {
                    refused(match property {
                        "matteOn" => "an expressed matte switch is unavailable",
                        _ => "a blend mode track is unavailable",
                    })
                }
                _ => refused("layer composition plan unavailable"),
            })?;
        self.with_scene_effects(Arc::new(effects))
    }
}
