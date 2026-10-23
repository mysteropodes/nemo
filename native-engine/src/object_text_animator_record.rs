//! Layer-owned native TextAnimator descriptors. Glyphs are derived from these records,
//! so editing text cannot orphan baked per-glyph copies. Absent stays absent on save.
use crate::object_document::{object_deserialize, Presence, PropertyRecord};
const MAX_TEXT_ANIMATORS: usize = 256;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[path = "object_text_animator_props.rs"]
pub(crate) mod props;
#[path = "object_text_animator_selector.rs"]
pub(crate) mod selector;
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
    pub(crate) enabled: Presence<bool>,
    pub(crate) selector: SelectorRecord,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) props: Presence<PropsRecord>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) properties: Presence<Vec<PropertyRecord>>,
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
        if self.id.trim().is_empty() || self.id.len() > 255 || self.id.contains('\0') {
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
