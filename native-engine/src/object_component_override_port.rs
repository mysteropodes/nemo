//! Production [`OverridePort`] of a placed component instance (L04-K, #2243).
//!
//! Baseline `applyOverrides` reads each exposed key off the INSTANCE layer through
//! `SMMotion.valueAtFrame(ld, key, frame)` (exposed-properties.js:41): the keyed track when the
//! stopwatch is on, else the static value, else `PROP_DEFAULT[key] = [ep.default]`. Native, the
//! same sampling is `sample_property_raw` on the scene-root layer holder; the metadata of an
//! exposed key (`object_property_exposed.rs`) carries the declared default. A colour is read as
//! four 0..255 components and decoded to a `SolidFill`. Nothing is invented: a missing default,
//! a failed sample or a channel outside 0..255 is a typed port error.
use crate::evaluation::object_component_kernels::object_component_exposed::{
    ExposedEntry, ExposedValue,
};
use crate::evaluation::object_component_kernels::object_component_exposed_apply::OverridePort;
use crate::object_document::object_component_record_kernels::object_component_records::SCENE_CONTEXT;
use crate::object_document::{is_colour_field, ObjectTarget, SolidFill};
use crate::object_frame_packet::{
    sample_property_raw, ObjectFramePacket, Presence, PropertyHolderTarget,
};
use serde_json::Number;
use std::collections::HashSet;

pub(crate) struct PacketOverrides<'a> {
    packet: &'a ObjectFramePacket,
    colours: HashSet<String>,
}

impl<'a> PacketOverrides<'a> {
    pub(crate) fn new(packet: &'a ObjectFramePacket) -> Self {
        Self {
            packet,
            colours: HashSet::new(),
        }
    }
}

fn channel(value: f64) -> Result<Number, String> {
    if !(0.0..=255.0).contains(&value) {
        return Err("colour override channel is outside 0..255".to_owned());
    }
    Number::from_f64(value / 255.0).ok_or_else(|| "colour override is not finite".to_owned())
}

fn decode_colour(values: &[f64]) -> Result<SolidFill, String> {
    let [r, g, b, a] = values else {
        return Err("colour override needs four components".to_owned());
    };
    Ok(SolidFill {
        kind: "solid".to_owned(),
        r: channel(*r)?,
        g: channel(*g)?,
        b: channel(*b)?,
        a: channel(*a)?,
    })
}

impl OverridePort for PacketOverrides<'_> {
    fn available(&mut self) -> bool {
        true
    }
    fn register(&mut self, prop: &ExposedEntry) {
        if is_colour_field(&prop.target_field) {
            self.colours.insert(prop.key.clone());
        }
    }
    fn read(
        &mut self,
        layer_uid: &str,
        key: &str,
        frame: u32,
    ) -> Result<Vec<ExposedValue>, String> {
        let target = PropertyHolderTarget {
            context_id: SCENE_CONTEXT.to_owned(),
            layer_uid: layer_uid.to_owned(),
            stroke_id: Presence::Missing,
        };
        let values = sample_property_raw(self.packet.snapshot(), &target, key, f64::from(frame))
            .map_err(|error| format!("exposed property sample failed: {error:?}"))?;
        if self.colours.contains(key) {
            return Ok(vec![ExposedValue::Paint(Some(decode_colour(&values)?))]);
        }
        Ok(values.into_iter().map(ExposedValue::Number).collect())
    }
}

/// An ordered source without an evaluated shape is legitimate only when the override evaluator
/// itself reported it hidden; a `__visible` declaration alone never excuses a missing shape.
pub(crate) fn absent_source_allowed(
    hidden: &[ObjectTarget],
    target: &ObjectTarget,
) -> Result<(), &'static str> {
    if hidden.iter().any(|t| t == target) {
        Ok(())
    } else {
        Err("component order and evaluated shapes disagree")
    }
}
