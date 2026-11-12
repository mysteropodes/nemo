//! Retained layer input projection, including real own mask-instance metadata.
//!
//! Sampled from the packet: stored blend/matte/matte-source/extra-matte names, layer effect stacks with their
//! static `p1..p8` (a stack with enabled keyed parameters is refused), visibility, kind flags, parent UID and the
//! own mask count/feather, and the enabled effect stacks of the layer's selected non-mask objects (`item_effects`,
//! keyed by source object index: the baseline `item.effects`). Refused, never neutral: a `blend` property track
//! or enabled expression on the layer (the layer's own hold-only `blendKeys` ARE sampled, at the frame), and an expressed `matteOn` the owner's expression port did not evaluate (`LayerCompositionPropertyUnavailable`). They need the raw
//! layer-property consumer to evaluate them at the frame; until it is joined the plan would otherwise be built on
//! the static value, so the join stays honest by returning a typed error.
//! Canvas-background layers are not document layers and carry no input here.
use crate::object_frame_packet::{
    sample_property_raw, sample_scalar_keys_raw, ObjectFramePacket, Presence, PropertyHolderTarget,
};
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
    // #2371: the Solo view decides which layers count as visible, so a non-soloed effect, matte or mask layer stops applying.
    let visible = crate::object_frame_packet::solo_view::layer_visibility(packet.context_id(), source);
    let selected: std::collections::HashSet<usize> = packet
        .selected_source_indices()
        .map(|(object, _)| object)
        .collect();
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
        if let Some(property) = unsampled_composition_property(packet, layer.layer_uid()) {
            return Err(SceneEffectJoinError::LayerCompositionPropertyUnavailable {
                layer: index,
                property,
            });
        }
        let frame = f64::from(packet.frame());
        let unavailable = || SceneEffectJoinError::EffectParametersUnavailable { layer: index };
        let effects = effect_specs(layer.effects().unwrap_or(&[]), frame).ok_or_else(unavailable)?;
        let mut item_effects = Vec::new();
        for (object, record) in packet.source_objects().iter().enumerate() {
            if record.target().layer_uid() != layer.layer_uid()
                || record.is_mask()
                || !record.has_enabled_effects()
                || !selected.contains(&object)
            {
                continue;
            }
            let stack = effect_specs(record.effects().unwrap_or(&[]), frame).ok_or_else(unavailable)?;
            item_effects.push((object, stack));
        }
        inputs.push(JoinLayerInput {
            layer_uid: layer.layer_uid().to_owned(),
            source_layer_index: index,
            visible: visible[index],
            is_widget: layer.widget.value().is_some(),
            is_effect: layer.is_effect_layer(),
            is_folder: layer.is_folder_layer(),
            // Baseline `layerBlendModeAt`: hold-only keys when the layer has them, else the stored blend.
            blend_mode: layer.blend_mode_at(packet.frame()).map(str::to_owned),
            matte_mode: matte_mode_at(packet, layer, index)?,
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
            item_effect_enabled: !item_effects.is_empty(),
            item_effects,
        });
    }
    if inputs.len() != source.len() {
        return Err(SceneEffectJoinError::InvalidOrder);
    }
    Ok(inputs)
}

/// The stack as the baseline `sceneEffectsOf(ld, frame)` hands it to the renderer: each `p1..p8` is the static value,
/// or `SMMotion.evalTrack` of its key track at `frame` when that parameter is keyed (`effectParamValueAt`). `None`
/// when a keyed parameter of an ENABLED entry cannot be sampled (never a static substitute).
pub(crate) fn effect_specs(stack: &[crate::object_frame_packet::LayerEffect], frame: f64) -> Option<Vec<EffectSpec>> {
    stack
        .iter()
        .map(|entry| {
            let mut params = [0usize, 1, 2, 3, 4, 5, 6, 7].map(|slot| entry.param(slot).map(|v| v as f32));
            for (name, track) in entry.keys.value().and_then(Option::as_ref).map_or(&[][..], |keys| keys.entries()) {
                let slot = ["p1", "p2", "p3", "p4", "p5", "p6", "p7", "p8"].iter().position(|n| n == name);
                let (Some(slot), Some(track)) = (slot, track.as_ref().filter(|t| !t.keys.is_empty())) else {
                    continue;
                };
                match sample_scalar_keys_raw(&track.keys, frame) {
                    Some(value) => params[slot] = Some(value as f32),
                    None if entry.enabled => return None,
                    None => {}
                }
            }
            Some(EffectSpec { effect_type: entry.effect_type.clone(), enabled: entry.enabled, params })
        })
        .collect()
}

/// The layer's matte mode at the frame: the stored name, dropped (`undefined` on the wire) on the frames its
/// `matteOn` switches the matte off. Only an effective matte mode reads the property.
fn matte_mode_at(
    packet: &ObjectFramePacket,
    layer: &crate::object_frame_packet::ObjectLayer,
    index: usize,
) -> Result<Option<String>, SceneEffectJoinError> {
    let Some(mode) = layer.matte_mode().map(str::to_owned) else {
        return Ok(None);
    };
    if mode.is_empty() || mode == "none" {
        return Ok(Some(mode));
    }
    match matte_on(packet, layer.layer_uid()) {
        Some(true) => Ok(Some(mode)),
        Some(false) => Ok(None),
        None => Err(SceneEffectJoinError::LayerCompositionPropertyUnavailable {
            layer: index,
            property: "matteOn",
        }),
    }
}

/// The first `matteOn`/`blend` record that is animated, as `(property, animated)` pairs.
pub(crate) fn first_animated_composition_property<'a>(
    records: impl IntoIterator<Item = (&'a str, bool)>,
) -> Option<&'static str> {
    records.into_iter().find_map(|(property, animated)| {
        let name = ["matteOn", "blend"].into_iter().find(|n| *n == property)?;
        animated.then_some(name)
    })
}

/// Composition inputs the join cannot sample: an ENABLED EXPRESSION on `matteOn` the owner's expression port did not
/// evaluate for this packet (a keyed or static `matteOn` is sampled raw by [`matte_on`], an expressed one is the
/// packet's evaluated receipt), and any `blend` property track, which the document cannot author today (the
/// baseline's hold-only `blendKeys` are the layer's own record, sampled by `blend_mode_at`).
fn unsampled_composition_property(packet: &ObjectFramePacket, uid: &str) -> Option<&'static str> {
    let document = packet.snapshot().document();
    first_animated_composition_property(
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
                let evaluated = record.property() == "matteOn" && packet.evaluated_matte_on(uid).is_some();
                (
                    record.property(),
                    (expressed && !evaluated) || (keyed && record.property() != "matteOn"),
                )
            }),
    )
}

/// Baseline `matteOnAt(layer, frame)`: the layer is matted unless its `matteOn` (the evaluated value when an
/// expression drives it, else the raw track, else the static value, else 100) is below 50 at the frame. A layer with
/// no such property is matted.
fn matte_on(packet: &ObjectFramePacket, uid: &str) -> Option<bool> {
    if let Some(value) = packet.evaluated_matte_on(uid) {
        return Some(value >= 50.);
    }
    let target = PropertyHolderTarget {
        context_id: packet.context_id().to_owned(),
        layer_uid: uid.to_owned(),
        stroke_id: Presence::Missing,
    };
    let values = sample_property_raw(packet.snapshot(), &target, "matteOn", f64::from(packet.frame())).ok()?;
    Some(values.first().is_none_or(|value| *value >= 50.))
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
