//! Effector layers resolved into effectors (L04-C, I09).
//!
//! Mirrors `effectorLayersFor`, `resolveEffector` and `resolvedEffectorsFor`
//! of `src/js/effector-layer.js` at baseline `fbe4b8a3`, and the
//! `dup.effectors.concat(layer effectors)` of `_duplicatorClonePlacement`.
//!
//! An effector layer owns no values: its origin is its stored world anchor
//! plus the layer's resolved Motion position, and its radius, strength, angle
//! and per-channel offsets are ordinary Motion properties. This kernel does
//! NOT sample Motion (Root D10/D11, Cyrill expressions): the caller supplies
//! what `SMMotion.layerMotionAt` and `SMMotion.valueAtFrame` returned as
//! [`EffectorLayer::motion`] and [`EffectorLayerSamples`], with the same
//! "null result" meaning as the baseline. The kernel assembles the channel
//! list (an untouched channel contributes nothing), applies the property
//! defaults and returns the very shape an inline effector has, or nothing
//! when no channel is nonzero (`resolveEffector` returns `null`).

use crate::evaluation::object_component_kernels::duplicator_placement_modes::JsNumber;
use crate::evaluation::object_component_kernels::object_effector_falloff::{
    at, Channel, Effector, EffectorPos, Falloff,
};

/// What `valueAtFrame(ld, key, frame)` returned: `None` for a null result,
/// otherwise the array (elements may be undefined, null or NaN).
pub(crate) type Sampled = Option<Vec<JsNumber>>;

/// The ten Motion properties an effector layer is sampled for.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EffectorLayerSamples {
    pub radius: Sampled,
    pub strength: Sampled,
    pub angle: Sampled,
    pub off_position: Sampled,
    pub off_rotation: Sampled,
    pub off_scale: Sampled,
    pub off_opacity: Sampled,
    pub off_position_z: Sampled,
    pub off_rotation_x: Sampled,
    pub off_rotation_y: Sampled,
}

/// `mm = SMMotion.layerMotionAt(li, frame)`: `dx`/`dy` read as `mm.dx || 0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MotionDelta {
    pub dx: JsNumber,
    pub dy: JsNumber,
}

/// One entry of `state.layers` as the effector lookup reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct EffectorLayer<'a> {
    /// `ld.isEffectorLayer && ld.effector`.
    pub is_effector_layer: bool,
    /// `ld.effector.targetLayerUid`.
    pub target_uid: Option<&'a str>,
    /// `ld.visible`: only an explicit `false` disables the effector.
    pub visible: Option<bool>,
    /// `ld.effector.pos` (a world anchor); `None` centers on the canvas.
    pub pos: Option<[f64; 2]>,
    /// `ld.effector.falloff`.
    pub falloff: Option<&'a str>,
    pub motion: Option<MotionDelta>,
    pub samples: EffectorLayerSamples,
}

/// Channel table of the baseline `CHANNELS`: property and its dimension.
const CHANNELS: [(&str, usize); 7] = [
    ("position", 2),
    ("rotation", 1),
    ("scale", 2),
    ("opacity", 1),
    ("positionZ", 1),
    ("rotationX", 1),
    ("rotationY", 1),
];

/// `(v && v[0] != null) ? v[0] : fallback`.
fn val(s: &Sampled, fallback: f64) -> f64 {
    match s {
        Some(v) => match at(v, 0) {
            JsNumber::Value(x) => x,
            _ => fallback,
        },
        None => fallback,
    }
}

/// `v ? [v[0] || 0, v[1] || 0] : [0, 0]`.
fn val2(s: &Sampled) -> [f64; 2] {
    match s {
        Some(v) => [at(v, 0).or_default(0.0), at(v, 1).or_default(0.0)],
        None => [0.0, 0.0],
    }
}

/// `v[0]` and `v[1] || 0` are both falsy (`!v[0] && !(v[1] || 0)`).
fn untouched(a: f64, b: f64) -> bool {
    (a == 0.0 || a.is_nan()) && (b == 0.0 || b.is_nan())
}

/// `resolveEffector(li, ld, frame)` for one layer; `None` when no channel is
/// nonzero. `canvas_w`/`canvas_h` give the default origin `[W / 2, H / 2]`.
pub(crate) fn resolve_layer_effector(
    layer: &EffectorLayer<'_>,
    canvas_w: f64,
    canvas_h: f64,
) -> Option<Effector> {
    let base = layer.pos.unwrap_or([canvas_w / 2.0, canvas_h / 2.0]);
    let (mut ox, mut oy) = (base[0], base[1]);
    if let Some(mm) = layer.motion {
        ox += mm.dx.or_default(0.0);
        oy += mm.dy.or_default(0.0);
    }
    let s = &layer.samples;
    let channel_samples = [
        &s.off_position,
        &s.off_rotation,
        &s.off_scale,
        &s.off_opacity,
        &s.off_position_z,
        &s.off_rotation_x,
        &s.off_rotation_y,
    ];
    let mut channels = Vec::new();
    for (i, (prop, dim)) in CHANNELS.iter().enumerate() {
        let sample = channel_samples[i];
        if *dim == 2 {
            let v = val2(sample);
            if untouched(v[0], v[1]) {
                continue;
            }
            channels.push(Channel::new(
                prop,
                vec![JsNumber::Value(v[0]), JsNumber::Value(v[1])],
            ));
        } else {
            let v = val(sample, 0.0);
            if untouched(v, 0.0) {
                continue;
            }
            channels.push(Channel::new(prop, vec![JsNumber::Value(v)]));
        }
    }
    if channels.is_empty() {
        return None;
    }
    Some(Effector {
        pos: Some(EffectorPos { x: ox, y: oy }),
        falloff: Falloff::from_label(layer.falloff),
        radius: JsNumber::Value(val(&s.radius, 200.0)),
        strength: JsNumber::Value(val(&s.strength, 100.0)),
        angle: JsNumber::Value(val(&s.angle, 0.0)),
        channels: Some(channels),
        from_layer: true,
        ..Effector::default()
    })
}

/// `resolvedEffectorsFor(dupLd, frame)`: every visible effector layer whose
/// target is the duplicator's `layerUid`, in layer order, resolved. A missing
/// or empty uid yields nothing.
pub(crate) fn resolved_effectors_for(
    dup_uid: Option<&str>,
    layers: &[EffectorLayer<'_>],
    canvas_w: f64,
    canvas_h: f64,
) -> Vec<Effector> {
    let uid = match dup_uid {
        Some(u) if !u.is_empty() => u,
        _ => return Vec::new(),
    };
    layers
        .iter()
        .filter(|l| l.is_effector_layer && l.target_uid == Some(uid) && l.visible != Some(false))
        .filter_map(|l| resolve_layer_effector(l, canvas_w, canvas_h))
        .collect()
}

/// `dup.effectors.concat(layer effectors)`: the inline effectors first, then
/// the layer ones. The list is unchanged when there are no layer effectors.
pub(crate) fn effective_effectors(
    inline: &[Effector],
    from_layers: Vec<Effector>,
) -> Vec<Effector> {
    let mut all = inline.to_vec();
    all.extend(from_layers);
    all
}

#[cfg(test)]
#[path = "../tests/object_effector_layer_controls.rs"]
mod controls_tests;
#[cfg(test)]
#[path = "../tests/object_effector_layer_codec.rs"]
pub(crate) mod test_codec;
#[cfg(test)]
#[path = "../tests/object_effector_layer.rs"]
mod tests;
