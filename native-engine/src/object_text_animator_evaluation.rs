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
        let s = self.resolve(at)?.selector;
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
        let mut selector = self.selector.resolve()?;
        if enabled {
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
                                .sample_scalar_raw(slot.unwrap_or(0.0), at)
                                .map_err(|_| "selector sampling unavailable")?,
                        );
                        continue;
                    }
                };
                *slot = property
                    .sample_scalar_raw(*slot, at)
                    .map_err(|_| "selector sampling unavailable")?;
            }
        }
        Ok(TextAnimator {
            enabled,
            selector,
            props: self.props.value().map(PropsRecord::resolve).transpose()?,
        })
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
