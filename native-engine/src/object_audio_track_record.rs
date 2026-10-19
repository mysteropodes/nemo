//! Persistent AudioTrack metadata in the existing native document, never a second owner.
//! Baseline dataB64 is the referenced retained asset; offsetFrames is the SOLE
//! AssetPlacement offset. No encoded copy or duplicate offset lives in this table.
use crate::object_document::{object_deserialize, OrderTrack, Presence};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Number;
use std::collections::{BTreeMap, HashSet};

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct AudioTrackRecord {
    pub(crate) asset_id: String,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) audio_id: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) name: Presence<String>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) volume: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) muted: Presence<bool>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) trim_start: Presence<Number>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) trim_end: Presence<Option<Number>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) motion: Presence<Option<BTreeMap<String, OrderTrack>>>,
    #[serde(skip_serializing_if = "Presence::missing")]
    pub(crate) motion_static: Presence<Option<BTreeMap<String, Vec<Number>>>>,
}
object_deserialize!(AudioTrackRecord {
    asset_id: String, #[serde(default)] audio_id: Presence<String>,
    #[serde(default)] name: Presence<String>, #[serde(default)] volume: Presence<Number>,
    #[serde(default)] muted: Presence<bool>, #[serde(default)] trim_start: Presence<Number>,
    #[serde(default)] trim_end: Presence<Option<Number>>,
    #[serde(default, deserialize_with = "unique_optional_map")] motion: Presence<Option<BTreeMap<String, OrderTrack>>>,
    #[serde(default, deserialize_with = "unique_optional_map")] motion_static: Presence<Option<BTreeMap<String, Vec<Number>>>>
});

// Deserialize original maps without collapsing duplicate property names.
fn unique_optional_map<'de, D, T>(
    deserializer: D,
) -> Result<Presence<Option<BTreeMap<String, T>>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    struct Unique<T>(BTreeMap<String, T>);
    impl<'de, T: Deserialize<'de>> Deserialize<'de> for Unique<T> {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct Visitor<T>(std::marker::PhantomData<T>);
            impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Visitor<T> {
                type Value = Unique<T>;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("unique audio Motion properties")
                }
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    mut map: A,
                ) -> Result<Self::Value, A::Error> {
                    let mut out = BTreeMap::new();
                    while let Some(key) = map.next_key::<String>()? {
                        if out.contains_key(&key) {
                            return Err(serde::de::Error::custom(
                                "duplicate audio Motion property",
                            ));
                        }
                        if out.len() >= 64 {
                            return Err(serde::de::Error::custom("audio Motion property bound"));
                        }
                        out.insert(key, map.next_value()?);
                    }
                    Ok(Unique(out))
                }
            }
            d.deserialize_map(Visitor(std::marker::PhantomData))
        }
    }
    Option::<Unique<T>>::deserialize(deserializer).map(|v| Presence::Present(v.map(|m| m.0)))
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub(crate) struct AudioTracks(Vec<AudioTrackRecord>);
#[path = "object_audio_track_edit.rs"]
pub(crate) mod edit;
impl<'de> Deserialize<'de> for AudioTracks {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let records = Vec::<AudioTrackRecord>::deserialize(deserializer)?;
        if records.len() > 1024 {
            return Err(serde::de::Error::custom(
                "audio track count exceeds catalog bound",
            ));
        }
        Ok(Self(records))
    }
}
pub(crate) fn valid_audio_id(id: &str) -> bool {
    !id.trim().is_empty() && id.len() <= 255 && !id.contains('\0')
}
impl AudioTracks {
    pub(crate) fn records(&self) -> &[AudioTrackRecord] {
        &self.0
    }
    pub(crate) fn get(&self, id: &str) -> Option<&AudioTrackRecord> {
        self.0
            .iter()
            .find(|t| t.audio_id.value().is_some_and(|a| a == id))
    }
    /// The existing storage codec supplies its SAME scalar-track validator.
    pub(crate) fn validate<E>(
        &self,
        mut scalar_track: impl FnMut(&OrderTrack) -> Result<(), E>,
    ) -> Result<(), &'static str> {
        let mut ids = HashSet::new();
        for track in &self.0 {
            if !valid_audio_id(&track.asset_id) {
                return Err("invalid audio source id");
            }
            if let Some(id) = track.audio_id.value() {
                if !valid_audio_id(id) || !ids.insert(id) {
                    return Err("invalid or duplicate audio id");
                }
            }
            if track
                .name
                .value()
                .is_some_and(|n| n.len() > 255 || n.contains('\0'))
            {
                return Err("invalid audio name");
            }
            // Number is finite JSON; do not clamp authored legacy volume or trims.
            if let Some(Some(motion)) = track.motion.value() {
                for (property, keys) in motion {
                    if property != "volume" {
                        return Err("unsupported audio Motion property");
                    }
                    scalar_track(keys).map_err(|_| "invalid audio volume track")?;
                }
            }
            if let Some(Some(values)) = track.motion_static.value() {
                if values
                    .iter()
                    .any(|(property, value)| property != "volume" || value.len() != 1)
                {
                    return Err("invalid audio static volume");
                }
            }
        }
        Ok(())
    }
}

#[cfg(feature = "project_media_codec")]
#[path = "object_audio_track_baseline.rs"]
pub(crate) mod baseline;

#[cfg(feature = "evaluation")]
#[path = "object_audio_track_sampling.rs"]
mod sampling;
#[cfg(test)]
#[path = "../tests/object_audio_track_record.rs"]
mod tests;

#[cfg(all(test, feature = "history", feature = "project_media_codec"))]
#[path = "../tests/object_audio_track_owner.rs"]
mod owner_tests;
