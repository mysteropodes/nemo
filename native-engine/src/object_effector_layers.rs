//! Effector layers of a context resolved into effectors (L04-L, I09).
//!
//! The document side of `effectorLayersFor` / `resolvedEffectorsFor`
//! (`src/js/effector-layer.js` at baseline `fbe4b8a3`). The record leaf (L04-G)
//! stores what an effector layer owns: its target, falloff and world anchor.
//! Everything else it is made of is an ordinary Motion property of the same
//! layer, so this join walks the context's layers in layer order, keeps the
//! visible ones whose record points at the duplicator, asks the Motion port
//! for the layer's position delta and the ten `eff*` properties, and hands
//! the samples to the L04-C kernel (`object_effector_layer`), which owns the
//! channel table, the property defaults and the "nothing nonzero resolves to
//! nothing" rule.
//!
//! Sampling follows the baseline call order exactly: the Motion position
//! first, then the seven offset channels in table order, and only when some
//! channel is nonzero the radius, the strength and the angle. A layer whose
//! offsets are all untouched therefore never asks for those three.
//!
//! Motion sampling itself is NOT here (Root D10/D11, Cyrill expressions): the
//! caller supplies a [`MotionPort`]. A property the provider cannot serve is a
//! typed [`Unavailable`], never a zero.

use crate::evaluation::object_component_kernels::object_effector_falloff::Effector;
use crate::evaluation::object_component_kernels::object_effector_layer::{
    resolve_layer_effector, resolved_effectors_for, EffectorLayer, EffectorLayerSamples,
    MotionDelta, Sampled,
};
use crate::object_document::object_component_record_kernels::object_duplicator_records::{
    DuplicatorSet, EffectorLayerRecord,
};
use crate::object_document::ObjectLayer;
use serde_json::Number;

/// Motion property keys of an effector layer (`P_RADIUS`, `P_STRENGTH`, `P_ANGLE`).
pub(crate) const KEY_RADIUS: &str = "effRadius";
pub(crate) const KEY_STRENGTH: &str = "effStrength";
pub(crate) const KEY_ANGLE: &str = "effAngle";
/// One offset property per channel, in the order of the baseline `CHANNELS`.
pub(crate) const KEY_OFF_POSITION: &str = "effOffPosition";
pub(crate) const KEY_OFF_ROTATION: &str = "effOffRotation";
pub(crate) const KEY_OFF_SCALE: &str = "effOffScale";
pub(crate) const KEY_OFF_OPACITY: &str = "effOffOpacity";
pub(crate) const KEY_OFF_POSITION_Z: &str = "effOffPositionZ";
pub(crate) const KEY_OFF_ROTATION_X: &str = "effOffRotationX";
pub(crate) const KEY_OFF_ROTATION_Y: &str = "effOffRotationY";

/// What a Motion provider cannot serve yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unavailable {
    /// `SMMotion.valueAtFrame(ld, key, frame)` for this property key.
    Property(&'static str),
    /// `SMMotion.layerMotionAt(li, frame)`: the resolved position delta.
    LayerMotion,
}

/// The two Motion lookups the baseline makes, for one frame the caller pinned.
pub(crate) trait MotionPort {
    /// `valueAtFrame(layer, key, frame)`: `Ok(None)` is the baseline's `null`.
    fn value(&self, layer_uid: &str, key: &'static str) -> Result<Sampled, Unavailable>;
    /// Exact element identity in the same provider-owned context and frame.
    /// Providers that only own layer lookups cannot impersonate element values.
    fn element_value(
        &self,
        _layer_uid: &str,
        _stroke_id: &str,
        key: &'static str,
    ) -> Result<Sampled, Unavailable> {
        Err(Unavailable::Property(key))
    }
    /// `layerMotionAt(layerIndex, frame)`: `Ok(None)` is the baseline's `null`.
    fn motion_delta(&self, layer_uid: &str) -> Result<Option<MotionDelta>, Unavailable>;
}

/// The layers an effector lookup walks.
#[derive(Clone, Copy)]
pub(crate) struct EffectorScope<'a> {
    pub context_id: &'a str,
    /// The context's layers in layer order (`state.layers`).
    pub layers: &'a [ObjectLayer],
    pub set: &'a DuplicatorSet,
    pub canvas_w: f64,
    pub canvas_h: f64,
}

/// `Number` as the baseline reads it; a missing or non-finite value is NaN.
fn num(n: &Number) -> f64 {
    n.as_f64().unwrap_or(f64::NAN)
}

/// `effectorLayersFor(targetUid)`: every visible effector layer pointing at
/// `target_uid`, in layer order, with its record. An empty uid yields nothing;
/// only an explicit `visible === false` disables a layer.
pub(crate) fn effector_layers_for<'a>(
    scope: &EffectorScope<'a>,
    target_uid: &str,
) -> Vec<(&'a ObjectLayer, &'a EffectorLayerRecord)> {
    if target_uid.is_empty() {
        return Vec::new();
    }
    let mut found = Vec::new();
    for layer in scope.layers {
        let record = scope
            .set
            .effector_layers
            .iter()
            .find(|e| e.context_id == scope.context_id && e.layer_uid == layer.layer_uid());
        if let Some(record) = record {
            if record.effector.target_layer_uid == target_uid && layer.visible() {
                found.push((layer, record));
            }
        }
    }
    found
}

/// One layer as the L04-C kernel reads it, with the samples the baseline
/// would have asked for. The radius, strength and angle stay unsampled when
/// the offsets alone already resolve to nothing.
fn sampled_view<'a>(
    layer: &ObjectLayer,
    record: &'a EffectorLayerRecord,
    port: &dyn MotionPort,
    canvas: [f64; 2],
) -> Result<EffectorLayer<'a>, Unavailable> {
    let uid = layer.layer_uid();
    let motion = port.motion_delta(uid)?;
    let pos = record.effector.pos.value().map(|p| {
        let at = |i: usize| p.get(i).map_or(f64::NAN, num);
        [at(0), at(1)]
    });
    let mut view = EffectorLayer {
        is_effector_layer: true,
        target_uid: Some(record.effector.target_layer_uid.as_str()),
        visible: Some(layer.visible()),
        pos,
        falloff: record.effector.falloff.value().map(String::as_str),
        motion,
        samples: EffectorLayerSamples::default(),
    };
    view.samples.off_position = port.value(uid, KEY_OFF_POSITION)?;
    view.samples.off_rotation = port.value(uid, KEY_OFF_ROTATION)?;
    view.samples.off_scale = port.value(uid, KEY_OFF_SCALE)?;
    view.samples.off_opacity = port.value(uid, KEY_OFF_OPACITY)?;
    view.samples.off_position_z = port.value(uid, KEY_OFF_POSITION_Z)?;
    view.samples.off_rotation_x = port.value(uid, KEY_OFF_ROTATION_X)?;
    view.samples.off_rotation_y = port.value(uid, KEY_OFF_ROTATION_Y)?;
    if resolve_layer_effector(&view, canvas[0], canvas[1]).is_none() {
        return Ok(view);
    }
    view.samples.radius = port.value(uid, KEY_RADIUS)?;
    view.samples.strength = port.value(uid, KEY_STRENGTH)?;
    view.samples.angle = port.value(uid, KEY_ANGLE)?;
    Ok(view)
}

/// `resolvedEffectorsFor(dupLd, frame)`: the effectors of every visible effector
/// layer aimed at the duplicator `dup_uid`, in layer order, in the shape of an
/// inline effector. Empty when there is none or no layer has a nonzero offset.
pub(crate) fn resolved_effectors(
    scope: &EffectorScope<'_>,
    dup_uid: &str,
    port: &dyn MotionPort,
) -> Result<Vec<Effector>, Unavailable> {
    let canvas = [scope.canvas_w, scope.canvas_h];
    let mut views = Vec::new();
    for (layer, record) in effector_layers_for(scope, dup_uid) {
        views.push(sampled_view(layer, record, port, canvas)?);
    }
    Ok(resolved_effectors_for(
        Some(dup_uid),
        &views,
        scope.canvas_w,
        scope.canvas_h,
    ))
}

#[cfg(test)]
#[path = "../tests/object_effector_layers_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_effector_layers.rs"]
mod tests;
