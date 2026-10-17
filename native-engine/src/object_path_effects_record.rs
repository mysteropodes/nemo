//! L01-E (G13): the per-layer ordered `pathFx` stack record (baseline `ld.pathFx`, path-fx.js). Storage
//! order IS evaluation order; an entry is `{type, enabled?, p1?, p2?, p3?}` and nothing else.
//!
//! `enabled` is absent, true or false (the baseline only reads `enabled !== false`); each of `p1..p3` is
//! absent, an explicit null (both read as the type default through `paramValue`) or a number, and the three
//! stay distinct in storage. A type the baseline `typeDef` does not know is refused at admission: the
//! baseline `activeStack` would silently drop it, which is a stored effect that never runs. Duplicate
//! types are allowed (the baseline allows them). Evaluation lives in object_path_effects_kernel.rs (L01-A)
//! and the evaluation join (L01-H); this file only stores and admits.
use crate::object_document::{object_deserialize, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;

/// The ids of the baseline `TYPES` table, in table order.
pub(crate) const PATH_FX_TYPE_IDS: [&str; 2] = ["zigzag", "roughen"];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PathFxRecord {
    #[serde(rename = "type")]
    pub(crate) fx_type: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) enabled: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) p1: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) p2: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) p3: Presence<Option<Number>>,
}
object_deserialize!(PathFxRecord {
    #[serde(rename = "type")] fx_type: String,
    #[serde(default)] enabled: Presence<bool>,
    #[serde(default)] p1: Presence<Option<Number>>,
    #[serde(default)] p2: Presence<Option<Number>>,
    #[serde(default)] p3: Presence<Option<Number>>
});

impl PathFxRecord {
    /// `None` when `enabled` is absent; only an explicit false disables the entry.
    pub(crate) fn enabled_flag(&self) -> Option<bool> {
        self.enabled.value().copied()
    }
    /// The stored number of `p<index+1>`; an absent member and an explicit null both give `None`.
    pub(crate) fn param(&self, index: usize) -> Option<f64> {
        let slot = match index {
            0 => &self.p1,
            1 => &self.p2,
            2 => &self.p3,
            _ => return None,
        };
        slot.value()
            .and_then(Option::as_ref)
            .and_then(Number::as_f64)
    }
}
