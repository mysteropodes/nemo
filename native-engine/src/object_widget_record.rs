//! Native widget layer record (F19): the stored `ld.widget` of `src/js/rig-widget.js` at fbe4b8a3.
//!
//! A widget layer is an ordinary object-document layer that carries an optional `widget` member:
//! `kind` (`joystick` | `slider`), the world anchor `pos`, the pad `size`, the reserved
//! `screenSpace` flag, the `x` axis and (joystick only) the `y` axis. The presence of the member is
//! the baseline `isWidgetLayer` flag. The record stores NO value: each axis points at a number
//! control by key, and that control lives in the layer's own `PropertyHolder.controls`, where the
//! property owners already sample, key and express it. Nothing here is a second control catalog.
//!
//! Kept from the baseline, and pinned against an executed oracle:
//! - member order `kind, pos, size, screenSpace, x, y` and axis order `key, min, max, rest, label`;
//! - absent, `null` and explicit stay three states for `pos`, `size`, `rest` and `label`
//!   (`pos`/`size` null and absent both fall back at evaluation, `rest` null reads as 0 while absent
//!   reads as `min`); `screenSpace` absent differs from `false`;
//! - numbers are kept as authored (integer or float spelling), never rewritten;
//! - an axis key that names no number control is ADMITTED and reported as an unresolved binding,
//!   never rewritten or dropped (the baseline `axisValue` would read a silent 0).
//!
//! Deliberate differences, each pinned by a test: unknown members and malformed shapes never decode
//! (the baseline deep-copies anything), a joystick must carry a `y` axis and a slider must not
//! (creation always produced exactly that; the baseline import never checked it), `y: null` is
//! refused, and a layer that carries a widget may not own drawing objects (the baseline never
//! renders a widget layer, so an object on one could only break the zero-pixel rule).
//!
//! Not provided: creation and write-from-local transactions (L03-M), the geometry kernel join, the
//! locked-layer gesture policy (F06) and the scene/export join (L03-S).

use crate::object_document::{
    object_deserialize, ExpressionControl, ObjectDocument, ObjectLayer, ObjectRecord, Presence,
    PropertyHolder,
};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use std::collections::HashSet;

/// The two widget kinds: exact, case-sensitive strings on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WidgetLayerKind {
    Joystick,
    Slider,
}

impl WidgetLayerKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            WidgetLayerKind::Joystick => "joystick",
            WidgetLayerKind::Slider => "slider",
        }
    }
}

impl Serialize for WidgetLayerKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for WidgetLayerKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match String::deserialize(deserializer)?.as_str() {
            "joystick" => Ok(WidgetLayerKind::Joystick),
            "slider" => Ok(WidgetLayerKind::Slider),
            other => Err(serde::de::Error::unknown_variant(
                other,
                &["joystick", "slider"],
            )),
        }
    }
}

/// Why a widget (or the layers carrying widgets) is not admissible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WidgetAdmissionError {
    JoystickNeedsYAxis,
    SliderHasYAxis,
    WidgetLayerHasObjects,
}

impl WidgetAdmissionError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            WidgetAdmissionError::JoystickNeedsYAxis => "a joystick widget requires a y axis",
            WidgetAdmissionError::SliderHasYAxis => "a slider widget has no y axis",
            WidgetAdmissionError::WidgetLayerHasObjects => "a widget layer carries no objects",
        }
    }
}

/// The axis `rest` field as the baseline reads it: `undefined`, `null` or a number.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum WidgetRest {
    /// Absent: the widget rests at the axis minimum.
    Absent,
    /// Explicit null: reads as zero.
    Null,
    Value(f64),
}

/// One widget axis: the control key and the presentation range over it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct WidgetAxis {
    pub(crate) key: String,
    pub(crate) min: Number,
    pub(crate) max: Number,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) rest: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) label: Presence<Option<String>>,
}
object_deserialize!(WidgetAxis {
    key: String, min: Number, max: Number,
    #[serde(default)] rest: Presence<Option<Number>>,
    #[serde(default)] label: Presence<Option<String>>
});

/// The stored widget of one layer.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct WidgetRecord {
    pub(crate) kind: WidgetLayerKind,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) pos: Presence<Option<[Number; 2]>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) size: Presence<Option<[Number; 2]>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) screen_space: Presence<bool>,
    pub(crate) x: WidgetAxis,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) y: Presence<WidgetAxis>,
}
object_deserialize!(WidgetRecord {
    kind: WidgetLayerKind,
    #[serde(default)] pos: Presence<Option<[Number; 2]>>,
    #[serde(default)] size: Presence<Option<[Number; 2]>>,
    #[serde(default)] screen_space: Presence<bool>,
    x: WidgetAxis,
    #[serde(default)] y: Presence<WidgetAxis>
});

fn pair_value(pair: &Presence<Option<[Number; 2]>>) -> Option<[f64; 2]> {
    let values = pair.value()?.as_ref()?;
    Some([values[0].as_f64()?, values[1].as_f64()?])
}

impl WidgetAxis {
    pub(crate) fn min_value(&self) -> f64 {
        self.min.as_f64().unwrap_or(f64::NAN)
    }

    pub(crate) fn max_value(&self) -> f64 {
        self.max.as_f64().unwrap_or(f64::NAN)
    }

    /// Detached view of `rest`; the three authored states stay apart.
    pub(crate) fn rest_state(&self) -> WidgetRest {
        match self.rest.value() {
            None => WidgetRest::Absent,
            Some(None) => WidgetRest::Null,
            Some(Some(rest)) => WidgetRest::Value(rest.as_f64().unwrap_or(f64::NAN)),
        }
    }

    /// `axisLabel`: the live control name wins when the key resolves and the name is non-empty,
    /// then the stored label, then the empty string.
    pub(crate) fn display_label(&self, layer_uid: &str, holders: &[PropertyHolder]) -> String {
        if !self.key.is_empty() {
            let controls = layer_controls(holders, layer_uid);
            if let Some(control) = controls.into_iter().find(|c| c.key == self.key) {
                if !control.name.is_empty() {
                    return control.name.clone();
                }
            }
        }
        match self.label.value() {
            Some(Some(label)) => label.clone(),
            _ => String::new(),
        }
    }
}

impl WidgetRecord {
    /// Axis-count rules shared by decode and encode. Numbers need no check: a `Number` can only
    /// hold finite values, so a non-finite coordinate can neither parse nor be constructed.
    pub(crate) fn validate(&self) -> Result<(), WidgetAdmissionError> {
        match (self.kind, self.y.value()) {
            (WidgetLayerKind::Joystick, None) => {
                return Err(WidgetAdmissionError::JoystickNeedsYAxis)
            }
            (WidgetLayerKind::Slider, Some(_)) => return Err(WidgetAdmissionError::SliderHasYAxis),
            _ => {}
        }
        Ok(())
    }

    /// Explicit world anchor; `None` for absent or null (the baseline then uses the canvas centre).
    pub(crate) fn pos_value(&self) -> Option<[f64; 2]> {
        pair_value(&self.pos)
    }

    /// Explicit pad size; `None` for absent or null (the baseline then uses the per-kind default).
    pub(crate) fn size_value(&self) -> Option<[f64; 2]> {
        pair_value(&self.size)
    }

    /// Reserved flag; absent reads as false and nothing branches on it yet.
    pub(crate) fn screen_space_value(&self) -> bool {
        self.screen_space.value().copied().unwrap_or(false)
    }

    pub(crate) fn y_axis(&self) -> Option<&WidgetAxis> {
        self.y.value()
    }

    /// Resolve both axes against the layer's own holders.
    pub(crate) fn bindings(&self, layer_uid: &str, holders: &[PropertyHolder]) -> WidgetBindings {
        let controls = layer_controls(holders, layer_uid);
        WidgetBindings {
            x: resolve_key(&controls, &self.x.key),
            y: self.y.value().map(|axis| resolve_key(&controls, &axis.key)),
        }
    }
}

/// Why an axis key does not reach a number control.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AxisUnresolved {
    EmptyKey,
    NoSuchControl,
    NotNumberControl,
}

/// Whether an axis key reaches a number control of the widget layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AxisBinding {
    Resolved,
    Unresolved(AxisUnresolved),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WidgetBindings {
    pub(crate) x: AxisBinding,
    pub(crate) y: Option<AxisBinding>,
}

/// Controls declared by the layer-level holders of `layer_uid`, in authored order.
fn layer_controls<'a>(
    holders: &'a [PropertyHolder],
    layer_uid: &str,
) -> Vec<&'a ExpressionControl> {
    holders
        .iter()
        .filter(|holder| holder.target.layer_uid == layer_uid && holder.target.stroke_id.missing())
        .flat_map(|holder| holder.controls.value().into_iter().flatten())
        .collect()
}

/// The FIRST control with `key` decides, like the baseline `controlByKey`.
fn resolve_key(controls: &[&ExpressionControl], key: &str) -> AxisBinding {
    if key.is_empty() {
        return AxisBinding::Unresolved(AxisUnresolved::EmptyKey);
    }
    match controls.iter().find(|control| control.key == key) {
        None => AxisBinding::Unresolved(AxisUnresolved::NoSuchControl),
        Some(control) if control.kind == "number" => AxisBinding::Resolved,
        Some(_) => AxisBinding::Unresolved(AxisUnresolved::NotNumberControl),
    }
}

impl ObjectLayer {
    /// The stored widget; its presence is the baseline `isWidgetLayer` flag.
    pub(crate) fn widget(&self) -> Option<&WidgetRecord> {
        self.widget.value()
    }
}

impl ObjectDocument {
    /// Axis bindings of the widget on `layer_uid`; `None` when the layer is missing or carries no widget.
    pub(crate) fn widget_bindings(&self, layer_uid: &str) -> Option<WidgetBindings> {
        let layer = self.layers.iter().find(|l| l.layer_uid == layer_uid)?;
        let widget = layer.widget()?;
        Some(widget.bindings(layer_uid, self.property_holders().unwrap_or(&[])))
    }
}

/// Document-level admission: every widget is well formed and no object targets a widget layer.
pub(crate) fn validate_widget_layers(
    layers: &[ObjectLayer],
    objects: &[ObjectRecord],
) -> Result<(), WidgetAdmissionError> {
    let mut widget_layers = HashSet::new();
    for layer in layers {
        if let Some(widget) = layer.widget() {
            widget.validate()?;
            widget_layers.insert(layer.layer_uid.as_str());
        }
    }
    if objects
        .iter()
        .any(|record| widget_layers.contains(record.target.layer_uid.as_str()))
    {
        return Err(WidgetAdmissionError::WidgetLayerHasObjects);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/object_widget_record_rows.rs"]
mod oracle_rows;

#[cfg(test)]
#[path = "../tests/object_widget_record.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/object_widget_record_controls.rs"]
mod controls_tests;
