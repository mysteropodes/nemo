//! Presence-aware stored selector, resolved by native Motion sampling at evaluation time.

use crate::object_document::{object_deserialize, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelectorRecord {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) selector_type: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) units: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) based_on: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) start: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) end: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) offset: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) shape: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) amount: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) ease_high: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) ease_low: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) smooth: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) wiggle_min: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) wiggle_max: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) wiggles_per_sec: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) correlation: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) seed: Presence<Option<Number>>,
}
object_deserialize!(SelectorRecord {
    #[serde(default)] selector_type: Presence<String>,
    #[serde(default)] units: Presence<String>,
    #[serde(default)] based_on: Presence<String>,
    #[serde(default)] start: Presence<Option<Number>>,
    #[serde(default)] end: Presence<Option<Number>>,
    #[serde(default)] offset: Presence<Option<Number>>,
    #[serde(default)] shape: Presence<Option<Number>>,
    #[serde(default)] amount: Presence<Option<Number>>,
    #[serde(default)] ease_high: Presence<Option<Number>>,
    #[serde(default)] ease_low: Presence<Option<Number>>,
    #[serde(default)] smooth: Presence<Option<Number>>,
    #[serde(default)] wiggle_min: Presence<Option<Number>>,
    #[serde(default)] wiggle_max: Presence<Option<Number>>,
    #[serde(default)] wiggles_per_sec: Presence<Option<Number>>,
    #[serde(default)] correlation: Presence<Option<Number>>,
    #[serde(default)] seed: Presence<Option<Number>>
});
impl SelectorRecord {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        for (v, allowed) in [
            (&self.selector_type, &["range", "wiggly"][..]),
            (&self.units, &["percent", "index"][..]),
            (&self.based_on, &["chars", "words", "lines"][..]),
        ] {
            if v.value().is_some_and(|s| !allowed.contains(&s.as_str())) {
                return Err("invalid selector enum");
            }
        }
        if self
            .start
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|n| n.as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err("nonfinite selector");
        }
        if self
            .end
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|n| n.as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err("nonfinite selector");
        }
        if self
            .offset
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|n| n.as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err("nonfinite selector");
        }
        if self
            .shape
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|n| n.as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err("nonfinite selector");
        }
        if self
            .amount
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|n| n.as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err("nonfinite selector");
        }
        if self
            .ease_high
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|n| n.as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err("nonfinite selector");
        }
        if self
            .ease_low
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|n| n.as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err("nonfinite selector");
        }
        if self
            .smooth
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|n| n.as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err("nonfinite selector");
        }
        if self
            .wiggle_min
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|n| n.as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err("nonfinite selector");
        }
        if self
            .wiggle_max
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|n| n.as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err("nonfinite selector");
        }
        if self
            .wiggles_per_sec
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|n| n.as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err("nonfinite selector");
        }
        if self
            .correlation
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|n| n.as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err("nonfinite selector");
        }
        if self
            .seed
            .value()
            .and_then(Option::as_ref)
            .is_some_and(|n| n.as_f64().is_none_or(|n| !n.is_finite()))
        {
            return Err("nonfinite selector");
        }
        Ok(())
    }
}
