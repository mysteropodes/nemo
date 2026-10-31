//! The closed generic property intents of `object.property.edit` and their strict map-only decoding.
//! D18-B control declaration intents are exposed to their owner as a borrowed `ControlOp`.
use crate::object_document::{OrderCurvePoint, OrderExpression, Presence};
use serde::Deserialize;
use serde_json::Number;

/// The closed control intents; the key travels as the request's `property`.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ControlOp<'a> {
    Add { kind: &'a str, name: &'a str },
    Rename { name: &'a str },
    Remove,
    Move { direction: i64 },
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum PropertyAction {
    // Private producer only. Public property intents keep refusing expression edits.
    #[serde(skip_deserializing)]
    SetExpression {
        expression: Option<OrderExpression>,
    },
    SetValue {
        frame: Number,
        values: Vec<Number>,
    },
    SetKey {
        frame: Number,
        values: Vec<Number>,
        #[serde(default, rename = "curvePoints")]
        curve_points: Presence<Option<Vec<OrderCurvePoint>>>,
    },
    RemoveKey {
        frame: Number,
    },
    SetAnimated {
        frame: Number,
        animated: bool,
    },
    // D18-B: declaration edits of the holder's expression controls; `property` is the control key.
    #[serde(rename_all = "camelCase")]
    AddControl {
        control_type: String,
        name: String,
    },
    RenameControl {
        name: String,
    },
    RemoveControl {},
    MoveControl {
        direction: i64,
    },
}
impl PropertyAction {
    pub(crate) fn control(&self) -> Option<ControlOp<'_>> {
        Some(match self {
            Self::AddControl { control_type, name } => ControlOp::Add {
                kind: control_type,
                name,
            },
            Self::RenameControl { name } => ControlOp::Rename { name },
            Self::RemoveControl {} => ControlOp::Remove,
            Self::MoveControl { direction } => ControlOp::Move {
                direction: *direction,
            },
            _ => return None,
        })
    }
}
// Internally tagged enum derives also admit positional sequences. Require a
// map before the derive sees original fields, preserving duplicate rejection.
pub(crate) fn deserialize_action<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct MapOnly<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for MapOnly<T> {
        type Value = T;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a property action object")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(MapOnly(std::marker::PhantomData))
}
