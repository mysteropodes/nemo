//! Ordered effect parameter tracks, stored by LayerEffect in the existing document.
//! Track scalar/curve semantics belong to the shared order codec, not this container.
use crate::object_document::OrderTrack;
use serde::de::{Error as _, MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashSet;
use std::fmt;
use std::io::{self, Write};

const MAX_ENTRIES: usize = 4096;
const MAX_KEY_BYTES: usize = 1024;
const MAX_STORE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EffectKeyTracksError {
    Invalid,
}

/// A map with authored Object.keys order. Empty/unknown parameter names are opaque data.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct EffectKeyTracks {
    entries: Vec<(String, Option<OrderTrack>)>,
}

// Count the ACTUAL encoded key/track bytes without first allocating a serialized map/track.
struct EncodedBudget {
    remaining: usize,
}
impl EncodedBudget {
    fn new() -> Self {
        Self {
            remaining: MAX_STORE_BYTES - 2,
        }
    }
    fn reserve(&mut self, count: usize) -> Result<(), EffectKeyTracksError> {
        self.remaining = self
            .remaining
            .checked_sub(count)
            .ok_or(EffectKeyTracksError::Invalid)?;
        Ok(())
    }
    fn encoded<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<(), EffectKeyTracksError> {
        serde_json::to_writer(self, value).map_err(|_| EffectKeyTracksError::Invalid)
    }
    fn key(&mut self, key: &str, index: usize) -> Result<(), EffectKeyTracksError> {
        if key.len() > MAX_KEY_BYTES {
            return Err(EffectKeyTracksError::Invalid);
        }
        self.reserve(1 + usize::from(index > 0))?; // colon + comma; braces already reserved
        self.encoded(key)
    }
}
impl Write for EncodedBudget {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.reserve(bytes.len()).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "effect key map exceeds encoded bound",
            )
        })?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl EffectKeyTracks {
    pub(crate) fn entries(&self) -> &[(String, Option<OrderTrack>)] {
        &self.entries
    }
    pub(crate) fn entries_mut(&mut self) -> &mut [(String, Option<OrderTrack>)] {
        &mut self.entries
    }

    /// Container bounds only. Call the existing shared scalar track predicate separately.
    pub(crate) fn checked(&self) -> Result<(), EffectKeyTracksError> {
        if self.entries.len() > MAX_ENTRIES {
            return Err(EffectKeyTracksError::Invalid);
        }
        let mut seen = HashSet::new();
        let mut budget = EncodedBudget::new();
        for (index, (key, track)) in self.entries.iter().enumerate() {
            if !seen.insert(key) {
                return Err(EffectKeyTracksError::Invalid);
            }
            budget.key(key, index)?;
            budget.encoded(track)?;
        }
        Ok(())
    }
}
impl Serialize for EffectKeyTracks {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.checked()
            .map_err(|_| serde::ser::Error::custom("invalid bounded effect key map"))?;
        let mut map = serializer.serialize_map(Some(self.entries.len()))?;
        for (key, track) in &self.entries {
            map.serialize_entry(key, track)?;
        }
        map.end()
    }
}
impl<'de> Deserialize<'de> for EffectKeyTracks {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Tracks;
        impl<'de> Visitor<'de> for Tracks {
            type Value = EffectKeyTracks;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("an ordered bounded effect key object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut entries = Vec::new();
                let mut seen = HashSet::new();
                let mut budget = EncodedBudget::new();
                while let Some(key) = map.next_key::<String>()? {
                    // Reject duplicate identity before reading or overwriting its value.
                    if entries.len() >= MAX_ENTRIES || !seen.insert(key.clone()) {
                        return Err(A::Error::custom("duplicate or excessive effect key entry"));
                    }
                    budget
                        .key(&key, entries.len())
                        .map_err(|_| A::Error::custom("invalid bounded effect key map"))?;
                    let track = map.next_value::<Option<OrderTrack>>()?;
                    budget
                        .encoded(&track)
                        .map_err(|_| A::Error::custom("invalid bounded effect key map"))?;
                    entries.push((key, track));
                }
                Ok(EffectKeyTracks { entries })
            }
        }
        deserializer.deserialize_map(Tracks)
    }
}
#[cfg(test)]
#[path = "../tests/object_effect_key_tracks.rs"]
mod tests;
