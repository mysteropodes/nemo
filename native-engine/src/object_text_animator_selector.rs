//! Presence-aware stored selector, resolved by native Motion sampling at evaluation time.
#[cfg(feature = "evaluation")]
use crate::object_document::object_text_layout_kernels::text_glyph_selector::{
    BasedOn, Selector, SelectorType, Units,
};
use crate::object_document::{object_deserialize, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SelectorRecord {
    #[serde(skip_serializing_if = "Presence::missing")]
    selector_type: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    units: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    based_on: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    start: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    end: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    offset: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    shape: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    amount: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    ease_high: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    ease_low: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    smooth: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    wiggle_min: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    wiggle_max: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    wiggles_per_sec: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    correlation: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    seed: Presence<Option<Number>>,
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
    #[cfg(feature = "evaluation")]
    pub(crate) fn resolve(&self) -> Result<Selector, &'static str> {
        let text = |v: &Presence<String>,
                    allowed: &[&str],
                    default: &str|
         -> Result<String, &'static str> {
            let s = v.value().map(String::as_str).unwrap_or(default);
            if allowed.contains(&s) {
                Ok(s.to_string())
            } else {
                Err("invalid selector enum")
            }
        };
        let number = |v: &Presence<Option<Number>>,
                      default: Option<f64>|
         -> Result<Option<f64>, &'static str> {
            match v {
                Presence::Missing => Ok(default),
                Presence::Present(None) => Ok(None),
                Presence::Present(Some(n)) => n
                    .as_f64()
                    .filter(|v| v.is_finite())
                    .map(Some)
                    .ok_or("nonfinite selector"),
            }
        };
        let mut out = Selector::default_selector();
        out.selector_type =
            SelectorType::from_baseline(&text(&self.selector_type, &["range", "wiggly"], "range")?);
        out.units = Units::from_baseline(&text(&self.units, &["percent", "index"], "percent")?);
        out.based_on = BasedOn::from_baseline(&text(
            &self.based_on,
            &["chars", "words", "lines"],
            "chars",
        )?);
        out.start = number(&self.start, Some(out.start))?.unwrap_or(0.0);
        out.end = number(&self.end, out.end)?;
        out.offset = number(&self.offset, Some(out.offset))?.unwrap_or(0.0);
        out.shape = number(&self.shape, Some(out.shape))?.unwrap_or(0.0);
        out.amount = number(&self.amount, out.amount)?;
        out.ease_high = number(&self.ease_high, Some(out.ease_high))?.unwrap_or(0.0);
        out.ease_low = number(&self.ease_low, Some(out.ease_low))?.unwrap_or(0.0);
        out.smooth = number(&self.smooth, out.smooth)?;
        out.wiggle_min = number(&self.wiggle_min, out.wiggle_min)?;
        out.wiggle_max = number(&self.wiggle_max, out.wiggle_max)?;
        out.wiggles_per_sec = number(&self.wiggles_per_sec, out.wiggles_per_sec)?;
        out.correlation = number(&self.correlation, out.correlation)?;
        out.seed = number(&self.seed, Some(out.seed))?.unwrap_or(0.0);
        Ok(out)
    }
}
