//! Original vector texture identity stored in the existing vector extension slot.
use crate::object_document::{object_deserialize, Presence};
use serde::{Deserialize, Serialize};
use serde_json::Number;
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct VectorTextureFields {
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) preset: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) group_id: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) pre_stroke: Presence<Option<String>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) pre_opacity: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) is_copy: Presence<bool>,
}
object_deserialize!(VectorTextureFields {
    #[serde(default)] preset: Presence<String>,
    #[serde(default)] group_id: Presence<String>,
    #[serde(default)] pre_stroke: Presence<Option<String>>,
    #[serde(default)] pre_opacity: Presence<Number>,
    #[serde(default)] is_copy: Presence<bool>
});
impl VectorTextureFields {
    pub(crate) fn valid(&self) -> bool {
        (!self.preset.missing()
            || !self.group_id.missing()
            || !self.pre_stroke.missing()
            || !self.pre_opacity.missing()
            || !self.is_copy.missing())
            && [&self.preset, &self.group_id]
                .into_iter()
                .all(|v| v.value().is_none_or(|s| !s.is_empty() && s.len() <= 255))
            && self.pre_stroke.value().is_none_or(|s| {
                s.as_ref()
                    .is_none_or(|s| crate::object_document::bitmap_brush::rgba(s).is_some())
            })
            && self
                .pre_opacity
                .value()
                .is_none_or(|n| n.as_f64().is_some_and(f64::is_finite))
    }
}
