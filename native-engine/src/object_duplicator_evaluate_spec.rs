//! The stored duplicator descriptor read as the kernels' inputs (L04-L, I08).
//!
//! `ld.duplicator` is held by L04-G as a storage record (`Presence` per field,
//! `serde_json::Number` values). The L04-B/C/D kernels take the JavaScript
//! view of the same fields: a numeric field that may be `undefined`, `null` or
//! a number ([`JsNumber`]), a label, a flag. This module is that translation
//! and nothing else: no default is invented, an absent field stays
//! `Undefined`, an explicit `null` stays `Null`, and the kernels apply the
//! baseline's `x || default` reads themselves.

use crate::evaluation::object_component_kernels::duplicator_placement_clone::RandMode;
use crate::evaluation::object_component_kernels::duplicator_placement_modes::{
    DuplicatorSpec, JsNumber,
};
use crate::evaluation::object_component_kernels::duplicator_placement_temporal::TemporalOffset;
use crate::evaluation::object_component_kernels::object_effector_falloff::{
    Channel, Effector, EffectorPos, Falloff,
};
use crate::object_document::object_component_record_kernels::object_duplicator_records::{
    DuplicatorDescriptor, EffectorChannel, InlineEffector,
};
use crate::object_document::Presence;
use serde_json::Number;

/// `Number` as the baseline reads it; a missing or non-finite value is NaN.
fn num(n: &Number) -> f64 {
    n.as_f64().unwrap_or(f64::NAN)
}

/// A numeric field: absent is `undefined`.
fn js(field: &Presence<Number>) -> JsNumber {
    match field.value() {
        Some(n) => JsNumber::Value(num(n)),
        None => JsNumber::Undefined,
    }
}

/// A numeric field that may hold an explicit `null` (`endAngle`).
fn js_nullable(field: &Presence<Option<Number>>) -> JsNumber {
    match field.value() {
        None => JsNumber::Undefined,
        Some(None) => JsNumber::Null,
        Some(Some(n)) => JsNumber::Value(num(n)),
    }
}

fn flag(field: &Presence<bool>) -> bool {
    field.value().copied().unwrap_or(false)
}

fn numbers(list: &[Number]) -> Vec<JsNumber> {
    list.iter().map(|n| JsNumber::Value(num(n))).collect()
}

fn channel(c: &EffectorChannel) -> Channel {
    let value = c.value.value().map(|v| numbers(v)).unwrap_or_default();
    Channel::new(&c.prop, value)
}

/// One inline effector in the kernels' shape. A `null` or absent `channels`
/// stays `None` so the legacy `offset*` fields migrate exactly as
/// `effectorChannels` does; an empty list stays an empty list.
pub(crate) fn inline_effector(e: &InlineEffector) -> Effector {
    Effector {
        pos: e.pos.value().map(|p| EffectorPos {
            x: num(&p.x),
            y: num(&p.y),
        }),
        falloff: Falloff::from_label(e.falloff.value().map(String::as_str)),
        radius: js(&e.radius),
        strength: js(&e.strength),
        angle: js(&e.angle),
        channels: e
            .channels
            .value()
            .and_then(|c| c.as_ref())
            .map(|list| list.iter().map(channel).collect()),
        offset_pos: e.offset_pos.value().map(|v| numbers(v)),
        offset_rot: js(&e.offset_rot),
        offset_scale: e.offset_scale.value().map(|v| numbers(v)),
        offset_opacity: js(&e.offset_opacity),
        from_layer: false,
    }
}

/// Everything `applyLayerDuplicator` reads off `ld.duplicator`.
pub(crate) struct DescriptorView<'a> {
    pub spec: DuplicatorSpec<'a>,
    /// `dup.seed`.
    pub seed: JsNumber,
    /// `dup.staggerRandom || {}`.
    pub rand: RandMode,
    /// `dup.timeOffset`; `None` when absent.
    pub temporal: Option<TemporalOffset<'a>>,
    /// `dup.sourceMode === 'objects'`.
    pub objects_mode: bool,
    /// `dup.sourceLayerUids` (empty when absent).
    pub pool_uids: &'a [String],
    pub inline_effectors: Vec<Effector>,
}

/// The descriptor as the evaluation reads it.
pub(crate) fn describe(d: &DuplicatorDescriptor) -> DescriptorView<'_> {
    let spec = DuplicatorSpec {
        mode: d.mode_label(),
        rows: js(&d.rows),
        cols: js(&d.cols),
        count: js(&d.count),
        spacing_x: js(&d.spacing_x),
        spacing_y: js(&d.spacing_y),
        start_angle: js(&d.start_angle),
        end_angle: js_nullable(&d.end_angle),
        radius: js(&d.radius),
        radial_orient: flag(&d.radial_orient),
        path_align_tangent: flag(&d.path_align_tangent),
    };
    let rand = d
        .stagger_random
        .value()
        .map_or(RandMode::default(), |r| RandMode {
            position: flag(&r.position),
            rotation: flag(&r.rotation),
            scale: flag(&r.scale),
            opacity: flag(&r.opacity),
            hue: flag(&r.hue),
        });
    let temporal = d.time_offset.value().map(|t| TemporalOffset {
        enabled: flag(&t.enabled),
        // `undefined | 0` is 0: an absent frame count leaves the offset off.
        offset_frames: t.offset_frames.value().map_or(f64::NAN, num),
        direction: t.direction.value().map(String::as_str),
    });
    DescriptorView {
        spec,
        seed: js(&d.seed),
        rand,
        temporal,
        objects_mode: d.source_mode.value().map(String::as_str) == Some("objects"),
        pool_uids: d.source_layer_uids(),
        inline_effectors: d.effectors().iter().map(inline_effector).collect(),
    }
}
