//! Original layer FollowPath storage (motion.js fbe4b8a3:3736-3738).
//! The setter writes null or {targetLayerUid, align}; absent align reads false.
//! Resolution/availability belongs to the native consumer, not this DTO.
use crate::object_document::{object_deserialize, Presence};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub(crate) struct FollowPathRecord {
    target_layer_uid: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    align: Presence<bool>,
}
object_deserialize!(FollowPathRecord {
    #[serde(deserialize_with = "target_uid")] target_layer_uid: String,
    #[serde(default)] align: Presence<bool>
});
fn target_uid<'de, D: serde::Deserializer<'de>>(decoder: D) -> Result<String, D::Error> {
    let uid = String::deserialize(decoder)?;
    if uid.len() > 255 || uid.contains('\0') || (!uid.is_empty() && uid.trim().is_empty()) {
        return Err(serde::de::Error::custom(
            "invalid FollowPath target identity",
        ));
    }
    Ok(uid)
}
impl FollowPathRecord {
    pub(crate) fn target_layer_uid(&self) -> &str {
        &self.target_layer_uid
    }
    pub(crate) fn align(&self) -> bool {
        self.align.value().copied().unwrap_or(false)
    }
}
