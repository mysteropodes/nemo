//! Inline-effector and effector-layer records of `object_duplicator_records`
//! (L04-G, I07), with the shared record-level helpers and the error type.
//!
//! An effector LAYER owns no values: its radius, strength, angle and offsets
//! are ordinary Motion properties of the same layer (property holders, D10), so
//! only its target, falloff and world anchor live here. Falloff and channel
//! labels are the closed sets the UI writes (`SMMotion.DUP_TARGET_PROPS`).
use crate::object_document::{object_deserialize, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;

pub(crate) const MAX_ENTRIES: usize = 4096;
const MAX_ID_BYTES: usize = 255;
const FALLOFFS: [&str; 3] = ["radial", "linear", "step"];
const CHANNEL_PROPS: [&str; 8] = [
    "position",
    "positionZ",
    "rotation",
    "rotationX",
    "rotationY",
    "scale",
    "opacity",
    "hue",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DuplicatorRecordError {
    Invalid(&'static str),
    Unsupported(&'static str),
    DuplicateId(&'static str),
    Unresolved(&'static str),
    /// A duplicator that would read another duplicator (or itself).
    Chained(&'static str),
}

/// `dup.effectors[i]`, current (`channels`) or legacy (`offset*`) shape.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct InlineEffector {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) pos: Presence<EffectorPos>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) radius: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) falloff: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) angle: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) strength: Presence<Number>,
    /// Null reads like absent (`if (eff.channels)`): the legacy fields migrate.
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) channels: Presence<Option<Vec<EffectorChannel>>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) offset_pos: Presence<Vec<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) offset_rot: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) offset_scale: Presence<Vec<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) offset_opacity: Presence<Number>,
}
object_deserialize!(InlineEffector {
    #[serde(default)] pos: Presence<EffectorPos>, #[serde(default)] radius: Presence<Number>,
    #[serde(default)] falloff: Presence<String>, #[serde(default)] angle: Presence<Number>,
    #[serde(default)] strength: Presence<Number>,
    #[serde(default)] channels: Presence<Option<Vec<EffectorChannel>>>,
    #[serde(default)] offset_pos: Presence<Vec<Number>>,
    #[serde(default)] offset_rot: Presence<Number>,
    #[serde(default)] offset_scale: Presence<Vec<Number>>,
    #[serde(default)] offset_opacity: Presence<Number>
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EffectorPos {
    pub(crate) x: Number,
    pub(crate) y: Number,
}
object_deserialize!(EffectorPos {
    x: Number,
    y: Number
});

/// `{ prop, value }`: `value` is read as `ch.value || []`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EffectorChannel {
    pub(crate) prop: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) value: Presence<Vec<Number>>,
}
object_deserialize!(EffectorChannel { prop: String, #[serde(default)] value: Presence<Vec<Number>> });

/// The layer that carries `ld.isEffectorLayer` and `ld.effector`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EffectorLayerRecord {
    pub(crate) context_id: String,
    pub(crate) layer_uid: String,
    pub(crate) effector: EffectorTarget,
}
object_deserialize!(EffectorLayerRecord {
    context_id: String,
    layer_uid: String,
    effector: EffectorTarget
});

/// `ld.effector`: `pos` is a two-number world anchor array here, unlike the inline `{x, y}`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EffectorTarget {
    pub(crate) target_layer_uid: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) falloff: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) pos: Presence<Vec<Number>>,
}
object_deserialize!(EffectorTarget {
    target_layer_uid: String, #[serde(default)] falloff: Presence<String>,
    #[serde(default)] pos: Presence<Vec<Number>>
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
            f.write_str("at most 4096 duplicator entries")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut sequence: A,
        ) -> Result<Self::Value, A::Error> {
            let mut entries = Vec::new();
            while let Some(entry) = sequence.next_element()? {
                if entries.len() == MAX_ENTRIES {
                    return Err(serde::de::Error::custom("too many duplicator entries"));
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
pub(crate) fn invalid(message: &'static str) -> DuplicatorRecordError {
    DuplicatorRecordError::Invalid(message)
}
pub(crate) fn label_in(value: &Presence<String>, set: &[&str]) -> bool {
    value
        .value()
        .is_none_or(|label| set.contains(&label.as_str()))
}

impl InlineEffector {
    pub(crate) fn validate(&self) -> Result<(), DuplicatorRecordError> {
        if !label_in(&self.falloff, &FALLOFFS) {
            return Err(invalid("effector falloff is not radial, linear or step"));
        }
        for legacy in [self.offset_pos.value(), self.offset_scale.value()]
            .into_iter()
            .flatten()
        {
            if legacy.len() > 2 {
                return Err(invalid("legacy effector offset has at most two numbers"));
            }
        }
        for channel in self.channels.value().into_iter().flatten().flatten() {
            if !CHANNEL_PROPS.contains(&channel.prop.as_str()) {
                return Err(invalid(
                    "effector channel prop is not a duplicator target property",
                ));
            }
            if channel.value.value().is_some_and(|v| v.len() > 2) {
                return Err(invalid("effector channel value has at most two numbers"));
            }
        }
        Ok(())
    }
}

impl EffectorTarget {
    pub(crate) fn validate(&self, own_layer_uid: &str) -> Result<(), DuplicatorRecordError> {
        if !valid_id(&self.target_layer_uid) || self.target_layer_uid == own_layer_uid {
            return Err(invalid("effector target must be another layer"));
        }
        if !label_in(&self.falloff, &FALLOFFS) {
            return Err(invalid("effector falloff is not radial, linear or step"));
        }
        if self.pos.value().is_some_and(|p| p.len() != 2) {
            return Err(invalid("effector pos is a two-number world anchor"));
        }
        Ok(())
    }
}
