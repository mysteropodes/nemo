//! Instance placement records of `object_component_records` (L04-G): the `sym*`
//! fields of a component-instance layer and the outer frame keys that hold a
//! `componentFrame` / `blankOverride`, plus the shared record-level helpers.
//!
//! Every optional field keeps absent, null and explicit apart; evaluation reads
//! them through the accessors, which collapse only what the baseline collapses
//! (`||`, `!= null`). Nothing here normalises a label or invents a default.
use crate::document::{validate_curve_points, CurvePoint};
use crate::object_document::{object_deserialize, ObjectLayer, OrderTrack, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;
use std::collections::HashSet;

/// The context that hosts instance layers; a component context is its symbolId.
pub(crate) const SCENE_CONTEXT: &str = "scene-root";
/// Same decoding bound as `LayerTiming`'s sparse arrays.
pub(crate) const MAX_ENTRIES: usize = 4096;
const MAX_ID_BYTES: usize = 255;
/// The four labels `resolveSymbolFrameIdx` distinguishes (any other reads as loop).
const PLAY_MODES: [&str; 4] = ["loop", "once", "pingpong", "single"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ComponentRecordError {
    Invalid(&'static str),
    Unsupported(&'static str),
    DuplicateId(&'static str),
    Unresolved(&'static str),
    /// An instance whose context is itself a component (`toastNestedComponentsUnsupported`).
    NestedComponent,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct ComponentInstance {
    pub(crate) context_id: String,
    pub(crate) layer_uid: String,
    pub(crate) symbol_id: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) sym_play_mode: Presence<Option<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) sym_speed: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) sym_placed_at: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) sym_single_frame: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) sym_trim_in: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) sym_trim_out: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) sym_matrix: Presence<Option<Vec<Number>>>,
    /// `ld.timeRemap`: the Motion key track the property holders hand to this owner.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) time_remap: Presence<Option<OrderTrack>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) frame_keys: Presence<Vec<FrameKey>>,
}
object_deserialize!(ComponentInstance {
    context_id: String, layer_uid: String, symbol_id: String,
    #[serde(default)] sym_play_mode: Presence<Option<String>>,
    #[serde(default)] sym_speed: Presence<Option<Number>>,
    #[serde(default)] sym_placed_at: Presence<Option<Number>>,
    #[serde(default)] sym_single_frame: Presence<Option<Number>>,
    #[serde(default)] sym_trim_in: Presence<Option<Number>>,
    #[serde(default)] sym_trim_out: Presence<Option<Number>>,
    #[serde(default)] sym_matrix: Presence<Option<Vec<Number>>>,
    #[serde(default)] time_remap: Presence<Option<OrderTrack>>,
    #[serde(default, deserialize_with = "bounded_presence")] frame_keys: Presence<Vec<FrameKey>>
});

/// An outer keyframe of the instance layer that holds a component frame or a blank.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FrameKey {
    pub(crate) frame: u32,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) component_frame: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) blank_override: Presence<Option<bool>>,
}
object_deserialize!(FrameKey {
    frame: u32, #[serde(default)] component_frame: Presence<Option<Number>>,
    #[serde(default)] blank_override: Presence<Option<bool>>
});

// Refuse oversized arrays during decoding, before an unbounded Vec allocation.
pub(crate) fn bounded_entries<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Entries<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Entries<T> {
        type Value = Vec<T>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("at most 4096 component entries")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut entries = Vec::new();
            while let Some(entry) = sequence.next_element()? {
                if entries.len() == MAX_ENTRIES {
                    return Err(serde::de::Error::custom("too many component entries"));
                }
                entries.push(entry);
            }
            Ok(entries)
        }
    }
    deserializer.deserialize_seq(Entries(std::marker::PhantomData))
}
pub(crate) fn bounded_presence<'de, D, T>(deserializer: D) -> Result<Presence<Vec<T>>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    bounded_entries(deserializer).map(Presence::Present)
}

pub(crate) fn valid_id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= MAX_ID_BYTES && !value.contains('\0')
}
pub(crate) fn invalid(message: &'static str) -> ComponentRecordError {
    ComponentRecordError::Invalid(message)
}
fn finite(value: &Number) -> bool {
    value.as_f64().is_some_and(f64::is_finite)
}
fn number(value: &Presence<Option<Number>>) -> Option<f64> {
    value
        .value()
        .and_then(|n| n.as_ref())
        .and_then(Number::as_f64)
}

impl ComponentInstance {
    /// `symbols` are the component ids of the same table, `scene_layers` the
    /// layers of the scene context the instance is placed on.
    pub(crate) fn validate(
        &self,
        symbols: &HashSet<&str>,
        scene_layers: &[ObjectLayer],
    ) -> Result<(), ComponentRecordError> {
        if self.context_id != SCENE_CONTEXT {
            return Err(if symbols.contains(self.context_id.as_str()) {
                ComponentRecordError::NestedComponent
            } else {
                ComponentRecordError::Unresolved("instance context is not a known context")
            });
        }
        if !symbols.contains(self.symbol_id.as_str()) {
            return Err(ComponentRecordError::Unresolved(
                "instance symbolId names no component",
            ));
        }
        let host = scene_layers
            .iter()
            .find(|layer| layer.layer_uid() == self.layer_uid)
            .ok_or(ComponentRecordError::Unresolved(
                "instance layer is not a scene layer",
            ))?;
        if let Some(Some(label)) = self.sym_play_mode.value() {
            if !PLAY_MODES.contains(&label.as_str()) {
                return Err(invalid("symPlayMode is not loop, once, pingpong or single"));
            }
        }
        if let Some(Some(matrix)) = self.sym_matrix.value() {
            if matrix.len() != 6 || !matrix.iter().all(finite) {
                return Err(invalid("symMatrix needs six finite numbers"));
            }
        }
        if let Some(Some(track)) = self.time_remap.value() {
            validate_remap(track)?;
        }
        let mut previous: Option<u32> = None;
        for key in self.frame_keys.value().into_iter().flatten() {
            if previous.is_some_and(|p| key.frame <= p) {
                return Err(invalid("frameKeys must strictly increase"));
            }
            previous = Some(key.frame);
            if key.component_frame.missing() && key.blank_override.missing() {
                return Err(invalid(
                    "a frame key carries neither componentFrame nor blankOverride",
                ));
            }
            // Baseline writes both fields only on `isKeyframe` outer frames.
            let on_key = host
                .timing
                .value()
                .is_some_and(|timing| timing.exposure(key.frame).0 == "key");
            if !on_key {
                return Err(invalid("a frame key must sit on a timing key of its layer"));
            }
        }
        Ok(())
    }
    pub(crate) fn context_id(&self) -> &str {
        &self.context_id
    }
    pub(crate) fn layer_uid(&self) -> &str {
        &self.layer_uid
    }
    pub(crate) fn symbol_id(&self) -> &str {
        &self.symbol_id
    }
    /// `None` for absent and for null; the label is NOT normalised (`loop` stays distinct from absent).
    pub(crate) fn play_mode(&self) -> Option<&str> {
        self.sym_play_mode.value().and_then(|m| m.as_deref())
    }
    pub(crate) fn speed(&self) -> Option<f64> {
        number(&self.sym_speed)
    }
    pub(crate) fn placed_at(&self) -> Option<f64> {
        number(&self.sym_placed_at)
    }
    pub(crate) fn single_frame(&self) -> Option<f64> {
        number(&self.sym_single_frame)
    }
    pub(crate) fn trim_in(&self) -> Option<f64> {
        number(&self.sym_trim_in)
    }
    pub(crate) fn trim_out(&self) -> Option<f64> {
        number(&self.sym_trim_out)
    }
    pub(crate) fn matrix(&self) -> Option<&[Number]> {
        self.sym_matrix.value().and_then(|m| m.as_deref())
    }
    pub(crate) fn time_remap(&self) -> Option<&OrderTrack> {
        self.time_remap.value().and_then(|t| t.as_ref())
    }
    pub(crate) fn frame_keys(&self) -> &[FrameKey] {
        match self.frame_keys.value() {
            Some(keys) => keys.as_slice(),
            None => &[],
        }
    }
}

impl FrameKey {
    pub(crate) fn frame(&self) -> u32 {
        self.frame
    }
    /// `componentFrame != null`: absent and null both read as no held frame.
    pub(crate) fn component_frame(&self) -> Option<f64> {
        number(&self.component_frame)
    }
    /// Explicit `true` / `false`; absent and null are `None` (only `Some(true)` hides).
    pub(crate) fn blank_override(&self) -> Option<bool> {
        self.blank_override.value().and_then(|b| *b)
    }
}

/// The key rules the property holders apply to a one-dimensional track
/// (`object_properties.rs`), for the Time Remap track they hand to this owner.
pub(crate) fn validate_remap(track: &OrderTrack) -> Result<(), ComponentRecordError> {
    let mut previous: Option<f64> = None;
    for key in &track.keys {
        let frame = key
            .frame
            .as_f64()
            .filter(|f| f.is_finite())
            .ok_or(invalid("timeRemap key frame must be finite"))?;
        if previous.is_some_and(|p| frame <= p) {
            return Err(invalid("timeRemap keys must strictly increase"));
        }
        previous = Some(frame);
        if key.v.len() != 1 || !key.v.iter().all(finite) {
            return Err(invalid("timeRemap values are one finite number"));
        }
        for handle in [key.h_in.value(), key.h_out.value()].into_iter().flatten() {
            if handle.len() != 2 || !handle.iter().all(finite) {
                return Err(invalid("timeRemap handles are two finite numbers"));
            }
        }
        if let Some(Some(points)) = key.curve_points.value() {
            let points: Vec<CurvePoint> = points
                .iter()
                .map(|p| CurvePoint {
                    x: p.x.clone(),
                    y: p.y.clone(),
                    tx: p.tx.value().cloned(),
                    ty: p.ty.value().cloned(),
                })
                .collect();
            validate_curve_points(&points).map_err(ComponentRecordError::Invalid)?;
        }
    }
    Ok(())
}
