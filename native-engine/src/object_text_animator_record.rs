//! Layer-owned native TextAnimator descriptors. Glyphs are derived from these records,
//! so editing text cannot orphan baked per-glyph copies. Absent stays absent on save.
#[cfg(feature = "evaluation")]
use crate::evaluation::property_text_animator::TextAnimator;
use crate::object_document::{object_deserialize, Presence, PropertyRecord};
const MAX_TEXT_ANIMATORS: usize = 256;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[path = "object_text_animator_props.rs"]
mod props;
#[path = "object_text_animator_selector.rs"]
mod selector;
use props::PropsRecord;
use selector::SelectorRecord;

pub(crate) const SELECTOR_KEYS: [&str; 11] = [
    "start",
    "end",
    "offset",
    "amount",
    "easeHigh",
    "easeLow",
    "smooth",
    "wiggleMin",
    "wiggleMax",
    "wigglesPerSec",
    "correlation",
];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnimatorRecord {
    pub(crate) id: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    enabled: Presence<bool>,
    selector: SelectorRecord,
    #[serde(skip_serializing_if = "Presence::missing")]
    props: Presence<PropsRecord>,
    #[serde(skip_serializing_if = "Presence::missing")]
    properties: Presence<Vec<PropertyRecord>>,
}
struct Wire {
    id: String,
    enabled: Presence<bool>,
    selector: SelectorRecord,
    props: Presence<PropsRecord>,
    properties: Presence<Vec<PropertyRecord>>,
}
object_deserialize!(Wire {
    id: String, #[serde(default)] enabled: Presence<bool>, selector: SelectorRecord,
    #[serde(default)] props: Presence<PropsRecord>,
    #[serde(default)] properties: Presence<Vec<PropertyRecord>>
});
impl<'de> Deserialize<'de> for AnimatorRecord {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let w = Wire::deserialize(d)?;
        let record = Self {
            id: w.id,
            enabled: w.enabled,
            selector: w.selector,
            props: w.props,
            properties: w.properties,
        };
        record.validate().map_err(serde::de::Error::custom)?;
        Ok(record)
    }
}
impl AnimatorRecord {
    fn validate(&self) -> Result<(), &'static str> {
        if self.id.is_empty() || self.id.len() > 255 || self.id.contains('\0') {
            return Err("invalid animator identity");
        }
        self.selector.validate()?;
        self.props.value().map(PropsRecord::validate).transpose()?;
        let mut names = HashSet::new();
        for property in self.properties.value().into_iter().flatten() {
            if !SELECTOR_KEYS.contains(&property.property()) || !names.insert(property.property()) {
                return Err("invalid or duplicate selector property");
            }
            property
                .validate_scalar()
                .map_err(|_| "invalid selector track")?;
        }
        Ok(())
    }
    #[cfg(feature = "evaluation")]
    pub(crate) fn selector_facts(&self, at: f64) -> Result<serde_json::Value, &'static str> {
        use crate::object_document::object_text_layout_kernels::text_glyph_selector::{
            BasedOn, SelectorType, Units,
        };
        let s = self.resolve(at)?.selector;
        Ok(
            serde_json::json!({"selectorType":if s.selector_type==SelectorType::Wiggly {"wiggly"} else {"range"},
            "units":if s.units==Units::Index {"index"} else {"percent"},"basedOn":match s.based_on {BasedOn::Chars=>"chars",BasedOn::Words=>"words",BasedOn::Lines=>"lines"},
            "start":s.start,"end":s.end,"offset":s.offset,"shape":s.shape,"amount":s.amount,"easeHigh":s.ease_high,"easeLow":s.ease_low,"smooth":s.smooth,
            "wiggleMin":s.wiggle_min,"wiggleMax":s.wiggle_max,"wigglesPerSec":s.wiggles_per_sec,"correlation":s.correlation,"seed":s.seed}),
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
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct TextAnimators(pub(crate) Vec<AnimatorRecord>);
impl<'de> Deserialize<'de> for TextAnimators {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let records = Vec::<AnimatorRecord>::deserialize(d)?;
        let mut ids = HashSet::new();
        if records.len() > MAX_TEXT_ANIMATORS || records.iter().any(|r| !ids.insert(r.id.as_str()))
        {
            return Err(serde::de::Error::custom(
                "too many or duplicate text animators",
            ));
        }
        Ok(Self(records))
    }
}
