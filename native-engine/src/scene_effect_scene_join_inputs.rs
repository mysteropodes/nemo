//! Retained layer input projection, including real own mask-instance metadata.
use crate::object_frame_packet::ObjectFramePacket;
use crate::object_render_scene::mask_inputs::mask_summary;
use crate::object_render_scene::scene_effect_kernels::scene_effect_params::EffectSpec;
use crate::object_render_scene::scene_effect_kernels::scene_effect_scene_join::{
    JoinLayerInput, SceneEffectJoinError, SceneEffectPlan,
};

/// Document layers in paint order, read from the packet that carries the pinned snapshot.
pub(crate) fn layer_inputs(
    packet: &ObjectFramePacket,
) -> Result<Vec<JoinLayerInput>, SceneEffectJoinError> {
    let order = packet
        .numeric_order()
        .ok_or(SceneEffectJoinError::NoNumericOrder)?;
    let masks = mask_summary(packet).map_err(SceneEffectJoinError::MaskInputs)?;
    let source = packet.source_layers();
    let mut seen = vec![false; source.len()];
    let mut inputs = Vec::with_capacity(source.len());
    for placed in order.layers() {
        let index = placed.source_layer_index();
        let layer = source
            .get(index)
            .ok_or(SceneEffectJoinError::InvalidOrder)?;
        if std::mem::replace(&mut seen[index], true) {
            return Err(SceneEffectJoinError::InvalidOrder);
        }
        if layer
            .effects()
            .is_some_and(|stack| stack.iter().any(|e| e.enabled && e.has_keyed_params()))
        {
            return Err(SceneEffectJoinError::EffectParametersUnavailable { layer: index });
        }
        let effects = layer
            .effects()
            .unwrap_or(&[])
            .iter()
            .map(|entry| EffectSpec {
                effect_type: entry.effect_type.clone(),
                enabled: entry.enabled,
                params: [0usize, 1, 2, 3, 4, 5, 6, 7]
                    .map(|slot| entry.param(slot).map(|v| v as f32)),
            })
            .collect();
        inputs.push(JoinLayerInput {
            layer_uid: layer.layer_uid().to_owned(),
            source_layer_index: index,
            visible: layer.visible(),
            is_widget: layer.widget.value().is_some(),
            is_effect: layer.is_effect_layer(),
            is_folder: layer.is_folder_layer(),
            blend_mode: layer.blend_mode().map(str::to_owned),
            matte_mode: layer.matte_mode().map(str::to_owned),
            matte_source_uid: layer.matte_source_uid().map(str::to_owned),
            mattes_more: layer
                .mattes_more()
                .iter()
                .map(|extra| (extra.uid.clone(), extra.mode.clone()))
                .collect(),
            effects,
            effects_from: layer.effects_from().map(str::to_owned),
            parent_uid: layer
                .parent_layer_uid
                .value()
                .and_then(|parent| parent.clone()),
            mask_count: masks.get(&index).map_or(0, |value| value.0),
            mask_feather: masks.get(&index).map_or(0., |value| value.1),
            item_effect_enabled: false,
        });
    }
    if inputs.len() != source.len() {
        return Err(SceneEffectJoinError::InvalidOrder);
    }
    Ok(inputs)
}

impl SceneEffectPlan {
    pub(crate) fn mask_count(&self, source_layer_index: usize) -> Option<usize> {
        self.plans
            .layers
            .iter()
            .find(|layer| layer.source_layer_index == source_layer_index)
            .map(|layer| layer.mask_count)
    }
    pub(crate) fn mask_feather(&self, source_layer_index: usize) -> Option<f64> {
        self.plans
            .layers
            .iter()
            .find(|layer| layer.source_layer_index == source_layer_index)
            .map(|layer| layer.mask_feather)
    }
}
