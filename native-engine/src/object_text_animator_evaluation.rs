//! Native text animator sampling over stored records and the retained Motion kernel.
use crate::evaluation::property_text_animator::{AnimatorColor, AnimatorProps, TextAnimator};
use crate::object_document::object_text_layout_kernels::animator::{
    props::PropsRecord, AnimatorRecord,
};
use crate::object_document::Presence;
use serde_json::Number;
impl AnimatorRecord {
    #[cfg(feature = "evaluation")]
    pub(crate) fn selector_facts(&self, at: f64) -> Result<serde_json::Value, &'static str> {
        use crate::object_document::object_text_layout_kernels::text_glyph_selector::{
            BasedOn, SelectorType, Units,
        };
        let s = self.selector_at(at)?;
        Ok(
            serde_json::json!({"selectorType":if s.selector_type==SelectorType::Wiggly {"wiggly"} else {"range"},
            "units":if s.units==Units::Index {"index"} else {"percent"},"basedOn":match s.based_on {BasedOn::Chars=>"chars",BasedOn::Words=>"words",BasedOn::Lines=>"lines"},
            "start":s.start,"end":s.end.unwrap_or(100.0),"offset":s.offset,"shape":s.shape,"amount":s.amount.unwrap_or(100.0),"easeHigh":s.ease_high,"easeLow":s.ease_low,"smooth":s.smooth.unwrap_or(100.0),
            "wiggleMin":s.wiggle_min.unwrap_or(-50.0),"wiggleMax":s.wiggle_max.unwrap_or(50.0),"wigglesPerSec":s.wiggles_per_sec.unwrap_or(2.0),"correlation":s.correlation.unwrap_or(50.0),"seed":s.seed}),
        )
    }
    /// Missing producer support is explicit, never a fallback to raw selector values.
    #[cfg(feature = "evaluation")]
    pub(crate) fn resolve(&self, at: f64) -> Result<TextAnimator, &'static str> {
        let enabled = self.enabled.value().copied().unwrap_or(true);
        let selector = if enabled {
            self.selector_at(at)?
        } else {
            self.selector.resolve()?
        };
        Ok(TextAnimator {
            enabled,
            selector,
            props: self.props.value().map(PropsRecord::resolve).transpose()?,
        })
    }

    fn selector_at(
        &self,
        at: f64,
    ) -> Result<
        crate::object_document::object_text_layout_kernels::text_glyph_selector::Selector,
        &'static str,
    > {
        if !at.is_finite() {
            return Err("nonfinite selector frame");
        }
        let mut selector = self.selector.resolve()?;

        for property in self.properties.value().into_iter().flatten() {
            if property.expression.value().is_some_and(|e| e.enabled) {
                return Err("text selector expression evaluation is unavailable");
            }
            let slot = match property.property() {
                "start" => &mut selector.start,
                "offset" => &mut selector.offset,
                "easeHigh" => &mut selector.ease_high,
                "easeLow" => &mut selector.ease_low,
                key => {
                    let slot = match key {
                        "end" => &mut selector.end,
                        "amount" => &mut selector.amount,
                        "smooth" => &mut selector.smooth,
                        "wiggleMin" => &mut selector.wiggle_min,
                        "wiggleMax" => &mut selector.wiggle_max,
                        "wigglesPerSec" => &mut selector.wiggles_per_sec,
                        "correlation" => &mut selector.correlation,
                        _ => return Err("invalid selector property"),
                    };
                    *slot = Some(
                        property
                            .sample_scalar_raw(
                                slot.unwrap_or(match key {
                                    "end" | "amount" | "smooth" => 100.0,
                                    "wiggleMin" => -50.0,
                                    "wiggleMax" | "correlation" => 50.0,
                                    "wigglesPerSec" => 2.0,
                                    _ => 0.0,
                                }),
                                at,
                            )
                            .map_err(|_| "selector sampling unavailable")?,
                    );
                    continue;
                }
            };
            *slot = property
                .sample_scalar_raw(*slot, at)
                .map_err(|_| "selector sampling unavailable")?;
        }

        Ok(selector)
    }
}

use crate::object_document::object_text_layout_kernels::animator::props::{num, pair, scalar};
fn color(v: &Presence<Vec<Option<Number>>>) -> Result<Option<AnimatorColor>, &'static str> {
    v.value()
        .map(|v| {
            if !(v.len() == 3 || v.len() == 4) || v[..3].iter().any(Option::is_none) {
                return Err("invalid animator color");
            }
            Ok(AnimatorColor {
                rgb: [
                    num(v[0].as_ref().unwrap())?,
                    num(v[1].as_ref().unwrap())?,
                    num(v[2].as_ref().unwrap())?,
                ],
                alpha: v.get(3).and_then(Option::as_ref).map(num).transpose()?,
            })
        })
        .transpose()
}
impl PropsRecord {
    #[cfg(feature = "evaluation")]
    pub(crate) fn resolve(&self) -> Result<AnimatorProps, &'static str> {
        Ok(AnimatorProps {
            position: pair(&self.position)?,
            scale: pair(&self.scale)?,
            rotation: scalar(&self.rotation)?,
            opacity: scalar(&self.opacity)?,
            fill_color: color(&self.fill_color)?,
            stroke_color: color(&self.stroke_color)?,
        })
    }
}

use crate::object_document::object_text_layout_kernels::animator::selector::SelectorRecord;
use crate::object_document::object_text_layout_kernels::text_glyph_selector::{
    BasedOn, Selector, SelectorType, Units,
};
impl SelectorRecord {
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
