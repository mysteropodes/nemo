//! Persistent paint-bucket provenance. Geometry remains in the ordinary vector record;
//! these authored values let the same native owner regenerate that geometry after a wall edit.
//! Missing, an explicit zero gap and an authored seed array remain distinct.
use crate::object_document::{object_deserialize, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct FillTrace {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) seed: Presence<[Number; 2]>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) seeds: Presence<Vec<[Number; 2]>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) walls: Presence<Vec<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) gap_px: Presence<Number>,
}
object_deserialize!(FillTrace {
    #[serde(default)] seed: Presence<[Number; 2]>,
    #[serde(default)] seeds: Presence<Vec<[Number; 2]>>,
    #[serde(default)] walls: Presence<Vec<String>>,
    #[serde(default)] gap_px: Presence<Number>
});

impl FillTrace {
    pub(crate) fn valid(&self) -> bool {
        let finite = |n: &Number| n.as_f64().is_some_and(f64::is_finite);
        !(self.seed.missing() && self.seeds.missing())
            && self.seed.value().is_none_or(|s| s.iter().all(finite))
            && self
                .seeds
                .value()
                .is_none_or(|s| s.iter().flatten().all(finite))
            && self
                .walls
                .value()
                .is_none_or(|ids| ids.iter().all(|id| !id.is_empty()))
            && self
                .gap_px
                .value()
                .is_none_or(|n| finite(n) && n.as_f64().unwrap() >= 0.0)
    }

    pub(crate) fn points(&self) -> Vec<[f64; 2]> {
        let convert = |s: &[Number; 2]| [s[0].as_f64().unwrap(), s[1].as_f64().unwrap()];
        match self.seeds.value() {
            Some(seeds) => seeds.iter().map(convert).collect(),
            None => self.seed.value().map(convert).into_iter().collect(),
        }
    }

    pub(crate) fn stamp(seed: [f64; 2], walls: Vec<String>, gap: u32) -> Option<Self> {
        Some(Self {
            seed: Presence::Present([Number::from_f64(seed[0])?, Number::from_f64(seed[1])?]),
            seeds: Presence::Missing,
            walls: if walls.is_empty() {
                Presence::Missing
            } else {
                Presence::Present(walls)
            },
            gap_px: Presence::Present(Number::from(gap)),
        })
    }
}
