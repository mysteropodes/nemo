//! L01-E (G22): the `paramShape` record of an `editable-vector-path` record: the declared intent of a
//! dynamic rectangle, ellipse or star/polygon (baseline `data.paramShape`, tools.js `applyParamShapeRect`,
//! `applyParamShapeEllipse`, `applyParamShapeStar`, `stampParamShapeBox`). The record's cubic geometry
//! already holds the built curve; this is the metadata the Coins/Arc/Star panels and per-corner Motion
//! read back, not a second source of truth for the geometry.
//!
//! Every kind-specific member is optional because the baseline builders read them with defaults
//! (`ps.tl||0`, `ps.sweep!==undefined?ps.sweep:359.9`, `ps.pointCount||5`): an absent radius is not a zero
//! radius and stays distinct. A member that belongs to another kind is refused, not dropped.
use crate::object_document::{object_deserialize, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;

pub(crate) const PARAM_SHAPE_KINDS: [&str; 3] = ["rect", "ellipse", "star"];

/// The shape's stable generator frame (`paramShape.box`), axis-aligned and un-rotated.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ParamBox {
    pub(crate) x1: Number,
    pub(crate) y1: Number,
    pub(crate) x2: Number,
    pub(crate) y2: Number,
}
object_deserialize!(ParamBox {
    x1: Number,
    y1: Number,
    x2: Number,
    y2: Number
});

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ParamShape {
    pub(crate) kind: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) tl: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) tr: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) br: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) bl: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) start_angle: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) sweep: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) inner_radius: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) point_count: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) inner_ratio: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) corner_radius: Presence<Number>,
    #[serde(rename = "box", skip_serializing_if = "Presence::missing")]
    pub(crate) frame: Presence<ParamBox>,
}
object_deserialize!(ParamShape {
    kind: String,
    #[serde(default)] tl: Presence<Number>,
    #[serde(default)] tr: Presence<Number>,
    #[serde(default)] br: Presence<Number>,
    #[serde(default)] bl: Presence<Number>,
    #[serde(default)] start_angle: Presence<Number>,
    #[serde(default)] sweep: Presence<Number>,
    #[serde(default)] inner_radius: Presence<Number>,
    #[serde(default)] point_count: Presence<Number>,
    #[serde(default)] inner_ratio: Presence<Number>,
    #[serde(default)] corner_radius: Presence<Number>,
    #[serde(default, rename = "box")] frame: Presence<ParamBox>
});
