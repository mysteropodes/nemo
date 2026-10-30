//! Retained layer input projection, including real own mask-instance metadata.
//!
//! Sampled from the packet: stored blend/matte/matte-source/extra-matte names, layer effect stacks with their
//! static `p1..p8` (a stack with enabled keyed parameters is refused), visibility, kind flags, parent UID and the
//! own mask count/feather. Refused, never neutral: a `matteOn` or `blend` property track or enabled expression on
//! the layer (`LayerCompositionPropertyUnavailable`) and enabled item-level effects on any of its objects
//! (`itemEffects`). They need the raw layer-property consumer to evaluate them at the frame; until it is joined the
//! plan would otherwise be built on the static value, so the join stays honest by returning a typed error.
//! Canvas-background layers are not document layers and carry no input here.
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
        if let Some(property) = unsampled_composition_property(packet, layer.layer_uid()) {
            return Err(SceneEffectJoinError::LayerCompositionPropertyUnavailable {
                layer: index,
                property,
            });
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

/// The first `matteOn`/`blend` record that is animated (keyed or expressed), as `(property, animated)` pairs.
pub(crate) fn first_animated_composition_property<'a>(
    records: impl IntoIterator<Item = (&'a str, bool)>,
) -> Option<&'static str> {
    records.into_iter().find_map(|(property, animated)| {
        let name = ["matteOn", "blend"].into_iter().find(|n| *n == property)?;
        animated.then_some(name)
    })
}

/// Composition inputs the join cannot sample yet: a keyed or expressed `matteOn`/`blend` property, or an
/// enabled item-level effect on a non-mask object. Packet admission refuses `matteOn` holders today, so the
/// property arm is the join's own guard for the day the raw layer-property consumer admits them.
fn unsampled_composition_property(packet: &ObjectFramePacket, uid: &str) -> Option<&'static str> {
    let document = packet.snapshot().document();
    let animated = first_animated_composition_property(
        document
            .property_holders()
            .unwrap_or(&[])
            .iter()
            .filter(|holder| {
                let target = holder.target();
                target.context_id() == packet.context_id()
                    && target.layer_uid() == uid
                    && target.stroke_id().is_none()
            })
            .flat_map(|holder| holder.properties())
            .map(|record| {
                let keyed = record.track.value().is_some_and(|t| !t.keys.is_empty());
                let expressed = record.expression.value().is_some_and(|e| e.enabled);
                (record.property(), keyed || expressed)
            }),
    );
    animated.or_else(|| {
        packet
            .source_objects()
            .iter()
            .any(|record| {
                record.target().layer_uid() == uid
                    && !record.is_mask()
                    && record.has_enabled_effects()
            })
            .then_some("itemEffects")
    })
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
